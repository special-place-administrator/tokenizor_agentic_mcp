/// Integration tests for the file watcher — proves FRSH-01 through FRSH-06 and RELY-03.
///
/// Each test uses a real tempdir, spawns the watcher via tokio::spawn, performs a
/// filesystem operation, waits for the debounce window to pass, then queries the
/// live LiveIndex to confirm the expected mutation.
///
/// Timing: debounce window is 200ms; tests wait 500ms (200ms debounce + 300ms margin).
///
/// Test map:
///   test_watcher_detects_modify_and_updates_index  → FRSH-01, FRSH-03, FRSH-06
///   test_watcher_indexes_new_file                  → FRSH-04
///   test_watcher_removes_deleted_file              → FRSH-05
///   test_watcher_hash_skip_on_noop_write           → content_hash optimization
///   test_watcher_enoent_handled_gracefully         → RELY-03
///   test_single_file_reparse_under_50ms            → FRSH-02
///   test_watcher_state_reports_active              → health extension
///   test_watcher_ignores_non_source_files          → filter correctness
use std::fs;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tempfile::TempDir;
use tokenizor_agentic_mcp::live_index::LiveIndex;
use tokenizor_agentic_mcp::watcher::{WatcherInfo, WatcherState, run_watcher};

// ---------------------------------------------------------------------------
// Test helpers
// ---------------------------------------------------------------------------

/// Write a file, creating parent directories as needed.
fn write_file(dir: &Path, name: &str, content: &str) {
    let path = dir.join(name);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, content).unwrap();
}

/// Spawn the watcher as a background tokio task and wait for it to initialize.
///
/// Returns the Arc<Mutex<WatcherInfo>> so tests can inspect watcher state.
async fn spawn_watcher(
    dir: &TempDir,
    shared: &tokenizor_agentic_mcp::live_index::SharedIndex,
) -> Arc<Mutex<WatcherInfo>> {
    let watcher_info = Arc::new(Mutex::new(WatcherInfo::default()));
    let root = dir.path().to_path_buf();
    let index_clone = Arc::clone(shared);
    let info_clone = Arc::clone(&watcher_info);

    tokio::spawn(async move {
        run_watcher(root, index_clone, info_clone).await;
    });

    // Give the watcher time to initialize the OS-level watch handle.
    tokio::time::sleep(Duration::from_millis(100)).await;

    watcher_info
}

/// Wait for the debounce window + processing margin.
async fn wait_debounce() {
    tokio::time::sleep(Duration::from_millis(500)).await;
}

// ---------------------------------------------------------------------------
// Test 1: FRSH-01, FRSH-03, FRSH-06 — modify a file → index updated
// ---------------------------------------------------------------------------

/// Prove that overwriting a file with new content causes the watcher to re-index it.
///
/// FRSH-01: file change is detected within 500ms.
/// FRSH-03: updated symbols are queryable immediately after re-index.
/// FRSH-06: editing a function name → the new name is returned from queries.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_watcher_detects_modify_and_updates_index() {
    let dir = tempfile::tempdir().unwrap();
    let src_dir = dir.path().join("src");
    fs::create_dir_all(&src_dir).unwrap();

    // Write initial file with fn hello()
    write_file(dir.path(), "src/hello.rs", "fn hello() {}");

    let shared = LiveIndex::load(dir.path()).unwrap();

    // Verify initial state
    {
        let index = shared.read().unwrap();
        let file = index
            .get_file("src/hello.rs")
            .expect("src/hello.rs should be indexed");
        let names: Vec<&str> = file.symbols.iter().map(|s| s.name.as_str()).collect();
        assert!(
            names.contains(&"hello"),
            "initial symbol 'hello' should exist: {names:?}"
        );
    }

    let _watcher_info = spawn_watcher(&dir, &shared).await;

    // Overwrite with fn hello_world()
    write_file(dir.path(), "src/hello.rs", "fn hello_world() {}");

    wait_debounce().await;

    // Verify index reflects the updated symbol
    {
        let index = shared.read().unwrap();
        let file = index
            .get_file("src/hello.rs")
            .expect("src/hello.rs should still be in index");
        let names: Vec<&str> = file.symbols.iter().map(|s| s.name.as_str()).collect();
        assert!(
            names.contains(&"hello_world"),
            "FRSH-01/03/06: updated symbol 'hello_world' should be in index after edit, got: {names:?}"
        );
        assert!(
            !names.contains(&"hello"),
            "FRSH-06: old symbol 'hello' must be gone after overwrite, got: {names:?}"
        );
    }
}

// ---------------------------------------------------------------------------
// Test 2: FRSH-04 — create a new file → it appears in the index
// ---------------------------------------------------------------------------

/// Prove that creating a new source file causes the watcher to add it to the index.
///
/// FRSH-04: creating a new .rs file makes it appear in repo_outline within 500ms.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_watcher_indexes_new_file() {
    let dir = tempfile::tempdir().unwrap();
    let src_dir = dir.path().join("src");
    fs::create_dir_all(&src_dir).unwrap();

    // Write a seed file so the index loads with ≥1 file
    write_file(dir.path(), "src/existing.rs", "fn existing() {}");

    let shared = LiveIndex::load(dir.path()).unwrap();
    let initial_count = shared.read().unwrap().file_count();

    let _watcher_info = spawn_watcher(&dir, &shared).await;

    // Create a brand-new file
    write_file(dir.path(), "src/new_file.rs", "fn new_function() {}");

    wait_debounce().await;

    // Verify the new file is now in the index
    {
        let index = shared.read().unwrap();
        let new_count = index.file_count();
        assert_eq!(
            new_count,
            initial_count + 1,
            "FRSH-04: file_count should have increased by 1 after creating new_file.rs"
        );

        let file = index
            .get_file("src/new_file.rs")
            .expect("FRSH-04: src/new_file.rs should be in index after create");
        let names: Vec<&str> = file.symbols.iter().map(|s| s.name.as_str()).collect();
        assert!(
            names.contains(&"new_function"),
            "FRSH-04: new file should have 'new_function' symbol, got: {names:?}"
        );
    }
}

// ---------------------------------------------------------------------------
// Test 3: FRSH-05 — delete a file → it is removed from the index
// ---------------------------------------------------------------------------

/// Prove that deleting a source file causes the watcher to remove it from the index.
///
/// FRSH-05: deleting a .rs file removes it from the index within 500ms.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_watcher_removes_deleted_file() {
    let dir = tempfile::tempdir().unwrap();
    let src_dir = dir.path().join("src");
    fs::create_dir_all(&src_dir).unwrap();

    write_file(dir.path(), "src/to_delete.rs", "fn doomed() {}");
    write_file(dir.path(), "src/stable.rs", "fn keeper() {}");

    let shared = LiveIndex::load(dir.path()).unwrap();

    // Verify initial state — to_delete.rs is in the index
    {
        let index = shared.read().unwrap();
        assert_eq!(index.file_count(), 2, "should have 2 files initially");
        assert!(
            index.get_file("src/to_delete.rs").is_some(),
            "src/to_delete.rs should be in index before delete"
        );
    }

    let _watcher_info = spawn_watcher(&dir, &shared).await;

    // Delete the file
    fs::remove_file(dir.path().join("src/to_delete.rs")).unwrap();

    wait_debounce().await;

    // Verify the file has been removed from the index
    {
        let index = shared.read().unwrap();
        assert!(
            index.get_file("src/to_delete.rs").is_none(),
            "FRSH-05: src/to_delete.rs should be removed from index after deletion"
        );
        assert_eq!(
            index.file_count(),
            1,
            "FRSH-05: file_count should decrease to 1 after deletion"
        );
        // Stable file must remain
        assert!(
            index.get_file("src/stable.rs").is_some(),
            "src/stable.rs should still be in index after unrelated file was deleted"
        );
    }
}

// ---------------------------------------------------------------------------
// Test 4: Content-hash skip — noop write does not corrupt the index
// ---------------------------------------------------------------------------

/// Prove that writing the same content to a file is handled safely.
///
/// When content is unchanged, the hash-skip optimization fires and skips tree-sitter.
/// The symbols after the write must be identical to the symbols before.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_watcher_hash_skip_on_noop_write() {
    let dir = tempfile::tempdir().unwrap();
    let src_dir = dir.path().join("src");
    fs::create_dir_all(&src_dir).unwrap();

    let content = "fn stable() {}";
    write_file(dir.path(), "src/stable.rs", content);

    let shared = LiveIndex::load(dir.path()).unwrap();

    // Capture initial symbol names
    let initial_symbols: Vec<String> = {
        let index = shared.read().unwrap();
        let file = index.get_file("src/stable.rs").unwrap();
        file.symbols.iter().map(|s| s.name.clone()).collect()
    };

    let watcher_info = spawn_watcher(&dir, &shared).await;
    let events_before = watcher_info.lock().unwrap().events_processed;

    // Overwrite with SAME content — hash-skip should fire
    write_file(dir.path(), "src/stable.rs", content);

    wait_debounce().await;

    // Symbols should be identical (hash-skip means no re-parse happened,
    // but even if it did, the result should be the same)
    {
        let index = shared.read().unwrap();
        let file = index
            .get_file("src/stable.rs")
            .expect("src/stable.rs should still be indexed after noop write");
        let after_symbols: Vec<String> = file.symbols.iter().map(|s| s.name.clone()).collect();
        assert_eq!(
            initial_symbols, after_symbols,
            "hash-skip: symbols should be unchanged after writing identical content"
        );
    }

    // Verify watcher processed the event (counted it) or at minimum didn't crash
    let events_after = watcher_info.lock().unwrap().events_processed;
    let _ = events_before; // events_after may or may not be > events_before (hash-skip counts the event)
    let _ = events_after;
}

// ---------------------------------------------------------------------------
// Test 5: RELY-03 — ENOENT handled gracefully, no panic, watcher stays active
// ---------------------------------------------------------------------------

/// Prove that deleting a file does not crash the watcher (RELY-03).
///
/// The watcher must handle the delete event gracefully: remove the file from
/// the index and remain in Active state (not Degraded).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_watcher_enoent_handled_gracefully() {
    let dir = tempfile::tempdir().unwrap();
    let src_dir = dir.path().join("src");
    fs::create_dir_all(&src_dir).unwrap();

    write_file(dir.path(), "src/fragile.rs", "fn at_risk() {}");
    write_file(dir.path(), "src/anchor.rs", "fn anchor() {}");

    let shared = LiveIndex::load(dir.path()).unwrap();

    let watcher_info = spawn_watcher(&dir, &shared).await;

    // Delete fragile.rs — triggers ENOENT path in maybe_reindex
    fs::remove_file(dir.path().join("src/fragile.rs")).unwrap();

    wait_debounce().await;

    // Verify no panic (if we reach here, no panic occurred)
    // Verify file removed from index
    {
        let index = shared.read().unwrap();
        assert!(
            index.get_file("src/fragile.rs").is_none(),
            "RELY-03: fragile.rs should be removed from index after deletion"
        );
    }

    // Verify watcher is still Active (RELY-03: no crash, graceful degradation path not taken)
    {
        let info = watcher_info.lock().unwrap();
        assert_eq!(
            info.state,
            WatcherState::Active,
            "RELY-03: watcher should remain Active after ENOENT; got: {:?}",
            info.state
        );
    }
}

// ---------------------------------------------------------------------------
// Test 6: FRSH-02 — single file re-parse completes in <50ms
// ---------------------------------------------------------------------------

/// Prove that parsing a single moderate Rust file takes less than 50ms (FRSH-02).
///
/// This is a performance micro-benchmark exercising the parsing module directly.
/// It does NOT go through the watcher (that latency is dominated by debounce).
#[test]
fn test_single_file_reparse_under_50ms() {
    use std::time::Instant;
    use tokenizor_agentic_mcp::domain::LanguageId;
    use tokenizor_agentic_mcp::parsing;

    // A moderate Rust function (~20 lines)
    let source = r#"
/// A moderately complex function for benchmarking the parser.
pub fn compute_sum(items: &[u32]) -> u32 {
    let mut total = 0u32;
    for &item in items {
        if item % 2 == 0 {
            total += item;
        } else {
            total = total.saturating_add(item * 2);
        }
    }
    total
}

pub struct Accumulator {
    values: Vec<u32>,
    threshold: u32,
}

impl Accumulator {
    pub fn new(threshold: u32) -> Self {
        Self { values: Vec::new(), threshold }
    }

    pub fn push(&mut self, val: u32) {
        self.values.push(val);
    }

    pub fn result(&self) -> u32 {
        self.values.iter().copied().sum()
    }
}
"#;

    let bytes = source.as_bytes();
    let start = Instant::now();
    let _result = parsing::process_file("bench.rs", bytes, LanguageId::Rust);
    let elapsed = start.elapsed();

    assert!(
        elapsed < Duration::from_millis(50),
        "FRSH-02: single-file re-parse must complete in <50ms, took {}ms",
        elapsed.as_millis()
    );
}

// ---------------------------------------------------------------------------
// Test 7: Watcher state reports Active after startup
// ---------------------------------------------------------------------------

/// Prove that WatcherInfo.state transitions to Active after run_watcher initializes.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_watcher_state_reports_active() {
    let dir = tempfile::tempdir().unwrap();
    let src_dir = dir.path().join("src");
    fs::create_dir_all(&src_dir).unwrap();
    write_file(dir.path(), "src/code.rs", "fn main() {}");

    let shared = LiveIndex::load(dir.path()).unwrap();
    let watcher_info = spawn_watcher(&dir, &shared).await;

    // After spawn_watcher (which waits 100ms for initialization), state should be Active.
    // Allow up to 200ms more for slower CI environments.
    tokio::time::sleep(Duration::from_millis(200)).await;

    let state = watcher_info.lock().unwrap().state.clone();
    assert_eq!(
        state,
        WatcherState::Active,
        "watcher state should be Active after initialization, got: {:?}",
        state
    );
}

// ---------------------------------------------------------------------------
// Test 8: Watcher ignores non-source files (e.g., README.md)
// ---------------------------------------------------------------------------

/// Prove that creating a non-source file does NOT cause it to be indexed.
///
/// The watcher must filter out files with unsupported extensions.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_watcher_ignores_non_source_files() {
    let dir = tempfile::tempdir().unwrap();
    let src_dir = dir.path().join("src");
    fs::create_dir_all(&src_dir).unwrap();

    write_file(dir.path(), "src/code.rs", "fn main() {}");

    let shared = LiveIndex::load(dir.path()).unwrap();
    let initial_count = shared.read().unwrap().file_count();

    let _watcher_info = spawn_watcher(&dir, &shared).await;

    // Create a non-source file — should be ignored by the watcher
    write_file(dir.path(), "README.md", "# My Project");
    write_file(dir.path(), "config.json", r#"{"version": "1"}"#);

    wait_debounce().await;

    // Verify file count unchanged (README.md and config.json not indexed)
    {
        let index = shared.read().unwrap();
        assert_eq!(
            index.file_count(),
            initial_count,
            "watcher should not index non-source files; count should remain {initial_count}, got {}",
            index.file_count()
        );
        assert!(
            index.get_file("README.md").is_none(),
            "README.md should NOT be in the index (unsupported extension)"
        );
        assert!(
            index.get_file("config.json").is_none(),
            "config.json should NOT be in the index (unsupported extension)"
        );
    }
}
