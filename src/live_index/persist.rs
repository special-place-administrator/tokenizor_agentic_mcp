/// LiveIndex persistence: serialize on shutdown, load on startup.
///
/// Uses postcard (compact binary) for fast round-trips.
/// Atomic write (tmp → rename) to prevent corruption on crash.
/// Background verification corrects stale entries after loading a snapshot.
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use crate::domain::{FileClassification, LanguageId, ReferenceRecord, SymbolRecord};
use crate::live_index::store::{
    CircuitBreakerState, IndexLoadSource, IndexedFile, LiveIndex, ParseStatus, SnapshotVerifyState,
};

// ── Constants ─────────────────────────────────────────────────────────────────

const CURRENT_VERSION: u32 = 2;
const INDEX_FILENAME: &str = "index.bin";
const TOKENIZOR_DIR: &str = ".tokenizor";

// ── Snapshot types ────────────────────────────────────────────────────────────

/// Serializable snapshot of all per-file data in a `LiveIndex`.
///
/// Does NOT include non-serializable fields (Instant, AtomicUsize, RwLock).
/// Reverse index and trigram index are rebuilt from snapshot on load.
#[derive(Serialize, Deserialize)]
pub struct IndexSnapshot {
    pub version: u32,
    pub files: HashMap<String, IndexedFileSnapshot>,
}

/// Serializable snapshot of a single indexed file.
#[derive(Serialize, Deserialize, Clone)]
pub struct IndexedFileSnapshot {
    pub relative_path: String,
    pub language: LanguageId,
    pub classification: FileClassification,
    pub content: Vec<u8>,
    pub symbols: Vec<SymbolRecord>,
    pub parse_status: ParseStatus,
    pub byte_len: u64,
    pub content_hash: String,
    pub references: Vec<ReferenceRecord>,
    pub alias_map: HashMap<String, String>,
    /// Seconds since UNIX epoch of the file's last modification time at index time.
    /// Used by stat_check_files for mtime comparison.
    pub mtime_secs: i64,
}

// ── Result type for stat checking ─────────────────────────────────────────────

/// Result of a stat-based freshness check of the loaded index.
pub struct StatCheckResult {
    /// Files whose on-disk mtime or size differs from the indexed values.
    pub changed: Vec<String>,
    /// Files in the index that no longer exist on disk.
    pub deleted: Vec<String>,
    /// Files on disk that are not in the index (new since snapshot was taken).
    pub new_files: Vec<String>,
}

// ── Public API ─────────────────────────────────────────────────────────────────

/// Serialize `index` to `.tokenizor/index.bin` inside `project_root`.
///
/// Uses an atomic write pattern (write to tmp, then rename) so a crash during
/// write never leaves a partially-written file.
///
/// Returns `Ok(())` on success. Non-fatal — caller logs and continues.
pub fn serialize_index(index: &LiveIndex, project_root: &Path) -> anyhow::Result<()> {
    let snapshot_input = capture_snapshot_build_input(index);
    serialize_captured_snapshot(snapshot_input, project_root)
}

fn capture_snapshot_build_input(index: &LiveIndex) -> SnapshotBuildInput {
    SnapshotBuildInput {
        files: index.files.clone(),
    }
}

fn serialize_captured_snapshot(
    snapshot_input: SnapshotBuildInput,
    project_root: &Path,
) -> anyhow::Result<()> {
    let snapshot = build_snapshot(snapshot_input, project_root);
    write_snapshot(snapshot, project_root)
}

pub fn serialize_shared_index(
    shared: &crate::live_index::store::SharedIndex,
    project_root: &Path,
) -> anyhow::Result<()> {
    let snapshot_input = {
        let guard = shared.read().expect("lock not poisoned");
        capture_snapshot_build_input(&guard)
    };
    serialize_captured_snapshot(snapshot_input, project_root)
}

fn write_snapshot(snapshot: IndexSnapshot, project_root: &Path) -> anyhow::Result<()> {
    // Serialize with postcard
    let bytes = postcard::to_stdvec(&snapshot)?;

    // Ensure .tokenizor directory exists
    let dir = project_root.join(TOKENIZOR_DIR);
    std::fs::create_dir_all(&dir)?;

    // Atomic write: tmp file then rename
    let final_path = dir.join(INDEX_FILENAME);
    let tmp_path = dir.join(format!("{INDEX_FILENAME}.tmp"));

    std::fs::write(&tmp_path, &bytes)?;
    std::fs::rename(&tmp_path, &final_path)?;

    info!(
        bytes = bytes.len(),
        files = snapshot.files.len(),
        "index serialized to .tokenizor/index.bin"
    );

    Ok(())
}

/// Load an `IndexSnapshot` from `.tokenizor/index.bin`.
///
/// Returns `None` (not panic) on:
/// - file not found (first run or crash)
/// - version mismatch (schema upgrade)
/// - corrupt / truncated bytes
pub fn load_snapshot(project_root: &Path) -> Option<IndexSnapshot> {
    let path = project_root.join(TOKENIZOR_DIR).join(INDEX_FILENAME);

    let bytes = match std::fs::read(&path) {
        Ok(b) => b,
        Err(_) => {
            // File not found is the normal case on first run
            return None;
        }
    };

    let snapshot: IndexSnapshot = match postcard::from_bytes(&bytes) {
        Ok(s) => s,
        Err(e) => {
            warn!("failed to deserialize index snapshot (corrupt?): {e}");
            return None;
        }
    };

    if snapshot.version != CURRENT_VERSION {
        warn!(
            "index snapshot version mismatch: got {}, expected {} — will re-index",
            snapshot.version, CURRENT_VERSION
        );
        return None;
    }

    Some(snapshot)
}

/// Convert an `IndexSnapshot` into a live `LiveIndex`.
///
/// Rebuilds the reverse index and trigram index from the snapshot data.
/// Sets `loaded_at`, `loaded_at_system`, `is_empty = false`.
pub fn snapshot_to_live_index(snapshot: IndexSnapshot) -> LiveIndex {
    let mut files: HashMap<String, Arc<IndexedFile>> = HashMap::with_capacity(snapshot.files.len());

    for (path, snap_file) in snapshot.files {
        let indexed_file = IndexedFile {
            relative_path: snap_file.relative_path,
            language: snap_file.language,
            classification: snap_file.classification,
            content: snap_file.content,
            symbols: snap_file.symbols,
            parse_status: snap_file.parse_status,
            byte_len: snap_file.byte_len,
            content_hash: snap_file.content_hash,
            references: snap_file.references,
            alias_map: snap_file.alias_map,
        };
        files.insert(path, Arc::new(indexed_file));
    }

    let trigram_index = super::trigram::TrigramIndex::build_from_files(&files);

    let mut index = LiveIndex {
        files,
        loaded_at: Instant::now(),
        loaded_at_system: SystemTime::now(),
        load_duration: Duration::ZERO,
        cb_state: CircuitBreakerState::new(0.20),
        is_empty: false,
        load_source: IndexLoadSource::SnapshotRestore,
        snapshot_verify_state: SnapshotVerifyState::Pending,
        reverse_index: HashMap::new(),
        files_by_basename: HashMap::new(),
        files_by_dir_component: HashMap::new(),
        trigram_index,
    };
    index.rebuild_reverse_index();
    index.rebuild_path_indices();
    index
}

/// Stat-check all files in the index against disk to find changed/deleted/new files.
///
/// Compares `byte_len` and `mtime_secs` stored in the snapshot against current
/// filesystem metadata. Files with differing size or mtime are in `changed`.
/// Files with `ENOENT` go to `deleted`. Files on disk not in the index go to `new_files`.
pub fn stat_check_files(
    index: &LiveIndex,
    snapshot_mtimes: &HashMap<String, i64>,
    root: &Path,
) -> StatCheckResult {
    let verify_view = capture_verify_view(index);
    stat_check_files_from_view(&verify_view, snapshot_mtimes, root)
}

fn stat_check_files_from_view(
    verify_view: &VerifyIndexView,
    snapshot_mtimes: &HashMap<String, i64>,
    root: &Path,
) -> StatCheckResult {
    let known_paths: std::collections::HashSet<&str> = verify_view
        .files
        .iter()
        .map(|file| file.relative_path.as_str())
        .collect();
    let mut changed = Vec::new();
    let mut deleted = Vec::new();

    // Check each indexed file against disk
    for file in &verify_view.files {
        let abs_path = root.join(
            file.relative_path
                .replace('/', std::path::MAIN_SEPARATOR_STR),
        );
        match std::fs::metadata(&abs_path) {
            Ok(meta) => {
                let on_disk_size = meta.len();
                let on_disk_mtime = meta
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                    .map(|d| d.as_secs() as i64)
                    .unwrap_or(0);

                let stored_mtime = snapshot_mtimes
                    .get(&file.relative_path)
                    .copied()
                    .unwrap_or(0);

                if on_disk_size != file.byte_len || on_disk_mtime != stored_mtime {
                    changed.push(file.relative_path.clone());
                }
            }
            Err(_) => {
                // File gone
                deleted.push(file.relative_path.clone());
            }
        }
    }

    // Find new files (on disk but not in index)
    let new_files = match crate::discovery::discover_files(root) {
        Ok(discovered) => discovered
            .into_iter()
            .filter(|df| !known_paths.contains(df.relative_path.as_str()))
            .map(|df| df.relative_path)
            .collect(),
        Err(e) => {
            warn!("stat_check_files: discover_files failed: {e}");
            Vec::new()
        }
    };

    StatCheckResult {
        changed,
        deleted,
        new_files,
    }
}

/// Select approximately `sample_pct` of files and check their content hashes.
///
/// Returns paths of files whose on-disk content hash differs from the index.
/// Default: 10% (pass 0.10).
pub fn spot_verify_sample(index: &LiveIndex, root: &Path, sample_pct: f64) -> Vec<String> {
    let verify_view = capture_verify_view(index);
    spot_verify_sample_from_view(&verify_view, root, sample_pct)
}

fn spot_verify_sample_from_view(
    verify_view: &VerifyIndexView,
    root: &Path,
    sample_pct: f64,
) -> Vec<String> {
    if verify_view.files.is_empty() {
        return Vec::new();
    }

    // Deterministic pseudo-random sample: every Nth file
    let total = verify_view.files.len();
    let sample_size = ((total as f64 * sample_pct).ceil() as usize)
        .max(1)
        .min(total);
    let step = if sample_size == 0 {
        1
    } else {
        total / sample_size
    };
    let step = step.max(1);

    let mut mismatches = Vec::new();

    for file in verify_view.files.iter().step_by(step) {
        let abs_path = root.join(
            file.relative_path
                .replace('/', std::path::MAIN_SEPARATOR_STR),
        );
        let bytes = match std::fs::read(&abs_path) {
            Ok(b) => b,
            Err(_) => continue,
        };

        let on_disk_hash = crate::hash::digest_hex(&bytes);
        if on_disk_hash != file.content_hash {
            mismatches.push(file.relative_path.clone());
        }
    }

    mismatches
}

// ── Private helpers ───────────────────────────────────────────────────────────

#[derive(Clone)]
pub(crate) struct SnapshotBuildInput {
    files: HashMap<String, Arc<IndexedFile>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct VerifyFileView {
    relative_path: String,
    byte_len: u64,
    content_hash: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct VerifyIndexView {
    files: Vec<VerifyFileView>,
}

fn capture_verify_view(index: &LiveIndex) -> VerifyIndexView {
    let mut files: Vec<VerifyFileView> = index
        .files
        .iter()
        .map(|(path, file)| VerifyFileView {
            relative_path: path.clone(),
            byte_len: file.byte_len,
            content_hash: file.content_hash.clone(),
        })
        .collect();
    files.sort_by(|a, b| a.relative_path.cmp(&b.relative_path));
    VerifyIndexView { files }
}

/// Convert captured live-index data to `IndexSnapshot`.
fn build_snapshot(snapshot_input: SnapshotBuildInput, project_root: &Path) -> IndexSnapshot {
    let mut snap_files = HashMap::with_capacity(snapshot_input.files.len());

    for (path, file) in snapshot_input.files {
        // Try to get mtime from disk for the snapshot
        let abs_path = project_root.join(path.replace('/', std::path::MAIN_SEPARATOR_STR));
        let mtime_secs = std::fs::metadata(&abs_path)
            .ok()
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);

        snap_files.insert(
            path.clone(),
            IndexedFileSnapshot {
                relative_path: file.relative_path.clone(),
                language: file.language.clone(),
                classification: file.classification,
                content: file.content.clone(),
                symbols: file.symbols.clone(),
                parse_status: file.parse_status.clone(),
                byte_len: file.byte_len,
                content_hash: file.content_hash.clone(),
                references: file.references.clone(),
                alias_map: file.alias_map.clone(),
                mtime_secs,
            },
        );
    }

    IndexSnapshot {
        version: CURRENT_VERSION,
        files: snap_files,
    }
}

/// Background task: verify a loaded index against disk and re-parse stale files.
///
/// Run after `snapshot_to_live_index` to bring the index to current disk state.
/// Non-blocking for queries — writes are protected by the index's RwLock.
pub async fn background_verify(
    index: crate::live_index::store::SharedIndex,
    root: std::path::PathBuf,
    snapshot_mtimes: HashMap<String, i64>,
) {
    index.mark_snapshot_verify_running();

    // 1. Stat-check all files (fast: just metadata reads)
    let verify_view = {
        let guard = index.read().expect("lock not poisoned");
        capture_verify_view(&guard)
    };
    let stat_result = stat_check_files_from_view(&verify_view, &snapshot_mtimes, &root);

    let changed_count = stat_result.changed.len();
    let deleted_count = stat_result.deleted.len();
    let new_count = stat_result.new_files.len();

    // 2. Remove deleted files
    if !stat_result.deleted.is_empty() {
        for path in &stat_result.deleted {
            index.remove_file(path);
        }
    }

    // 3. Re-parse changed files
    let to_reparse: Vec<String> = stat_result
        .changed
        .into_iter()
        .chain(stat_result.new_files.into_iter())
        .collect();

    for rel_path in &to_reparse {
        let abs_path = root.join(rel_path.replace('/', std::path::MAIN_SEPARATOR_STR));
        let bytes = match std::fs::read(&abs_path) {
            Ok(b) => b,
            Err(e) => {
                warn!("background_verify: failed to read {rel_path}: {e}");
                continue;
            }
        };

        // Detect language from path
        let ext = std::path::Path::new(rel_path)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");
        let language = match crate::domain::LanguageId::from_extension(ext) {
            Some(lang) => lang,
            None => continue,
        };

        let result = crate::parsing::process_file_with_classification(
            rel_path,
            &bytes,
            language,
            FileClassification::for_code_path(rel_path),
        );
        let indexed_file = IndexedFile::from_parse_result(result, bytes);

        index.update_file(rel_path.clone(), indexed_file);
    }

    // 4. Spot-verify sample (10%) for content hash mismatches
    let verify_view = {
        let guard = index.read().expect("lock not poisoned");
        capture_verify_view(&guard)
    };
    let spot_mismatches = spot_verify_sample_from_view(&verify_view, &root, 0.10);

    let spot_count = spot_mismatches.len();

    // Re-parse spot-check mismatches
    for rel_path in &spot_mismatches {
        let abs_path = root.join(rel_path.replace('/', std::path::MAIN_SEPARATOR_STR));
        let bytes = match std::fs::read(&abs_path) {
            Ok(b) => b,
            Err(e) => {
                warn!("background_verify spot-check: failed to read {rel_path}: {e}");
                continue;
            }
        };

        let ext = std::path::Path::new(rel_path)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");
        let language = match crate::domain::LanguageId::from_extension(ext) {
            Some(lang) => lang,
            None => continue,
        };

        let result = crate::parsing::process_file_with_classification(
            rel_path,
            &bytes,
            language,
            FileClassification::for_code_path(rel_path),
        );
        let indexed_file = IndexedFile::from_parse_result(result, bytes);

        index.update_file(rel_path.clone(), indexed_file);
    }

    index.mark_snapshot_verify_completed();

    info!(
        "background verify complete: {} changed, {} deleted, {} new, {} spot-check mismatches",
        changed_count, deleted_count, new_count, spot_count
    );
}

// ── Unit tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{LanguageId, ReferenceKind, ReferenceRecord, SymbolKind, SymbolRecord};
    use crate::live_index::store::{
        IndexLoadSource, IndexedFile, ParseStatus, SnapshotVerifyState,
    };
    use std::collections::HashMap;
    use std::sync::Arc;
    use std::time::{Duration, Instant, SystemTime};
    use tempfile::TempDir;

    // ── Helpers ───────────────────────────────────────────────────────────────

    fn make_symbol(name: &str) -> SymbolRecord {
        SymbolRecord {
            name: name.to_string(),
            kind: SymbolKind::Function,
            depth: 0,
            sort_order: 0,
            byte_range: (0, 10),
            line_range: (0, 1),
            doc_byte_range: None,
        }
    }

    fn make_reference(name: &str) -> ReferenceRecord {
        ReferenceRecord {
            name: name.to_string(),
            qualified_name: None,
            kind: ReferenceKind::Call,
            byte_range: (5, 10),
            line_range: (0, 0),
            enclosing_symbol_index: None,
        }
    }

    fn make_indexed_file(path: &str, content: &[u8]) -> IndexedFile {
        let mut alias_map = HashMap::new();
        alias_map.insert("Alias".to_string(), "Original".to_string());
        IndexedFile {
            relative_path: path.to_string(),
            language: LanguageId::Rust,
            classification: crate::domain::FileClassification::for_code_path(path),
            content: content.to_vec(),
            symbols: vec![make_symbol("my_func")],
            parse_status: ParseStatus::Parsed,
            byte_len: content.len() as u64,
            content_hash: crate::hash::digest_hex(content),
            references: vec![make_reference("other_func")],
            alias_map,
        }
    }

    fn make_live_index_with_files(files: Vec<(&str, &[u8])>) -> LiveIndex {
        let mut file_map: HashMap<String, Arc<IndexedFile>> = HashMap::new();
        for (path, content) in files {
            file_map.insert(path.to_string(), Arc::new(make_indexed_file(path, content)));
        }
        let trigram_index = crate::live_index::trigram::TrigramIndex::build_from_files(&file_map);
        let mut index = LiveIndex {
            files: file_map,
            loaded_at: Instant::now(),
            loaded_at_system: SystemTime::now(),
            load_duration: Duration::ZERO,
            cb_state: CircuitBreakerState::new(0.20),
            is_empty: false,
            load_source: IndexLoadSource::FreshLoad,
            snapshot_verify_state: SnapshotVerifyState::NotNeeded,
            reverse_index: HashMap::new(),
            files_by_basename: HashMap::new(),
            files_by_dir_component: HashMap::new(),
            trigram_index,
        };
        index.rebuild_reverse_index();
        index.rebuild_path_indices();
        index
    }

    // ── Round-trip tests ──────────────────────────────────────────────────────

    #[test]
    fn test_round_trip_preserves_files_symbols_references_content() {
        let tmp = TempDir::new().unwrap();
        let content = b"fn my_func() { other_func(); }";
        let index =
            make_live_index_with_files(vec![("tests/generated/main.generated.rs", content)]);

        // Serialize
        serialize_index(&index, tmp.path()).expect("serialize should succeed");

        // Load
        let snapshot = load_snapshot(tmp.path()).expect("snapshot should load");
        let loaded = snapshot_to_live_index(snapshot);

        // Verify
        assert_eq!(loaded.files.len(), 1);
        let file = loaded
            .files
            .get("tests/generated/main.generated.rs")
            .expect("file should be present");
        assert_eq!(file.content, content);
        assert_eq!(file.symbols.len(), 1);
        assert_eq!(file.symbols[0].name, "my_func");
        assert_eq!(file.references.len(), 1);
        assert_eq!(file.references[0].name, "other_func");
        assert!(file.classification.is_code());
        assert!(file.classification.is_test);
        assert!(file.classification.is_generated);
        assert_eq!(
            file.alias_map.get("Alias").map(|s| s.as_str()),
            Some("Original")
        );
    }

    #[test]
    fn test_round_trip_empty_index() {
        let tmp = TempDir::new().unwrap();
        let index = make_live_index_with_files(vec![]);

        serialize_index(&index, tmp.path()).expect("serialize empty index should succeed");

        let snapshot = load_snapshot(tmp.path()).expect("snapshot should load");
        let loaded = snapshot_to_live_index(snapshot);

        assert_eq!(loaded.files.len(), 0);
    }

    #[test]
    fn test_snapshot_to_live_index_marks_snapshot_restore_pending_verify() {
        let tmp = TempDir::new().unwrap();
        let index = make_live_index_with_files(vec![("src/main.rs", b"fn main() {}")]);

        serialize_index(&index, tmp.path()).expect("serialize should succeed");
        let snapshot = load_snapshot(tmp.path()).expect("snapshot should load");
        let loaded = snapshot_to_live_index(snapshot);

        assert_eq!(loaded.load_source(), IndexLoadSource::SnapshotRestore);
        assert_eq!(loaded.snapshot_verify_state(), SnapshotVerifyState::Pending);
    }

    #[tokio::test]
    async fn test_background_verify_marks_snapshot_verify_completed() {
        let tmp = TempDir::new().unwrap();
        let file_path = tmp.path().join("src").join("main.rs");
        std::fs::create_dir_all(file_path.parent().unwrap()).unwrap();
        std::fs::write(&file_path, b"fn main() {}\n").unwrap();

        let index = make_live_index_with_files(vec![("src/main.rs", b"fn main() {}\n")]);
        serialize_index(&index, tmp.path()).expect("serialize should succeed");

        let snapshot = load_snapshot(tmp.path()).expect("snapshot should load");
        let snapshot_mtimes = snapshot
            .files
            .iter()
            .map(|(path, file)| (path.clone(), file.mtime_secs))
            .collect::<HashMap<_, _>>();
        let loaded = snapshot_to_live_index(snapshot);
        let shared = crate::live_index::SharedIndexHandle::shared(loaded);

        {
            let guard = shared.read().unwrap();
            assert_eq!(guard.load_source(), IndexLoadSource::SnapshotRestore);
            assert_eq!(guard.snapshot_verify_state(), SnapshotVerifyState::Pending);
        }

        background_verify(shared.clone(), tmp.path().to_path_buf(), snapshot_mtimes).await;

        let guard = shared.read().unwrap();
        assert_eq!(guard.load_source(), IndexLoadSource::SnapshotRestore);
        assert_eq!(
            guard.snapshot_verify_state(),
            SnapshotVerifyState::Completed
        );
        drop(guard);

        let published = shared.published_state();
        assert_eq!(
            published.snapshot_verify_state,
            SnapshotVerifyState::Completed
        );
        assert!(
            published.generation >= 2,
            "expected published generation to advance through verify transitions"
        );
    }

    #[test]
    fn test_build_snapshot_resolves_mtime_against_project_root() {
        let tmp = TempDir::new().unwrap();
        let project_root = tmp.path().join("project");
        let file_path = project_root.join("src").join("main.rs");
        std::fs::create_dir_all(file_path.parent().unwrap()).unwrap();
        std::fs::write(&file_path, b"fn main() {}\n").unwrap();

        let index = make_live_index_with_files(vec![("src/main.rs", b"fn main() {}\n")]);
        let snapshot = build_snapshot(capture_snapshot_build_input(&index), &project_root);

        let expected_mtime = std::fs::metadata(&file_path)
            .unwrap()
            .modified()
            .unwrap()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        assert_eq!(
            snapshot.files.get("src/main.rs").unwrap().mtime_secs,
            expected_mtime
        );
    }

    #[test]
    fn test_capture_verify_view_sorts_paths() {
        let index = make_live_index_with_files(vec![
            ("src/z.rs", b"fn z() {}\n"),
            ("src/a.rs", b"fn a() {}\n"),
            ("src/m.rs", b"fn m() {}\n"),
        ]);

        let view = capture_verify_view(&index);
        let paths: Vec<&str> = view
            .files
            .iter()
            .map(|file| file.relative_path.as_str())
            .collect();

        assert_eq!(paths, vec!["src/a.rs", "src/m.rs", "src/z.rs"]);
    }

    #[test]
    fn test_round_trip_multiple_files() {
        let tmp = TempDir::new().unwrap();
        let index = make_live_index_with_files(vec![
            ("a.rs", b"fn alpha() {}"),
            ("b.rs", b"fn beta() {}"),
            ("c.py", b"def gamma(): pass"),
        ]);

        serialize_index(&index, tmp.path()).expect("serialize should succeed");

        let snapshot = load_snapshot(tmp.path()).expect("snapshot should load");
        let loaded = snapshot_to_live_index(snapshot);

        assert_eq!(loaded.files.len(), 3);
        assert!(loaded.files.contains_key("a.rs"));
        assert!(loaded.files.contains_key("b.rs"));
        assert!(loaded.files.contains_key("c.py"));
    }

    #[test]
    fn test_round_trip_preserves_parse_status_variants() {
        let tmp = TempDir::new().unwrap();
        let mut file_map: HashMap<String, Arc<IndexedFile>> = HashMap::new();

        // Parsed
        file_map.insert(
            "ok.rs".to_string(),
            Arc::new(IndexedFile {
                relative_path: "ok.rs".to_string(),
                language: LanguageId::Rust,
                classification: crate::domain::FileClassification::for_code_path("ok.rs"),
                content: b"fn foo() {}".to_vec(),
                symbols: vec![],
                parse_status: ParseStatus::Parsed,
                byte_len: 11,
                content_hash: "hash1".to_string(),
                references: vec![],
                alias_map: HashMap::new(),
            }),
        );

        // PartialParse
        file_map.insert(
            "partial.rs".to_string(),
            Arc::new(IndexedFile {
                relative_path: "partial.rs".to_string(),
                language: LanguageId::Rust,
                classification: crate::domain::FileClassification::for_code_path("partial.rs"),
                content: b"fn bad(".to_vec(),
                symbols: vec![],
                parse_status: ParseStatus::PartialParse {
                    warning: "syntax error".to_string(),
                },
                byte_len: 7,
                content_hash: "hash2".to_string(),
                references: vec![],
                alias_map: HashMap::new(),
            }),
        );

        // Failed
        file_map.insert(
            "fail.rb".to_string(),
            Arc::new(IndexedFile {
                relative_path: "fail.rb".to_string(),
                language: LanguageId::Ruby,
                classification: crate::domain::FileClassification::for_code_path("fail.rb"),
                content: b"garbage".to_vec(),
                symbols: vec![],
                parse_status: ParseStatus::Failed {
                    error: "parse error".to_string(),
                },
                byte_len: 7,
                content_hash: "hash3".to_string(),
                references: vec![],
                alias_map: HashMap::new(),
            }),
        );

        let trigram_index = crate::live_index::trigram::TrigramIndex::build_from_files(&file_map);
        let mut index = LiveIndex {
            files: file_map,
            loaded_at: Instant::now(),
            loaded_at_system: SystemTime::now(),
            load_duration: Duration::ZERO,
            cb_state: CircuitBreakerState::new(0.20),
            is_empty: false,
            load_source: IndexLoadSource::FreshLoad,
            snapshot_verify_state: SnapshotVerifyState::NotNeeded,
            reverse_index: HashMap::new(),
            files_by_basename: HashMap::new(),
            files_by_dir_component: HashMap::new(),
            trigram_index,
        };
        index.rebuild_reverse_index();
        index.rebuild_path_indices();

        serialize_index(&index, tmp.path()).expect("serialize should succeed");
        let snapshot = load_snapshot(tmp.path()).expect("load should succeed");
        let loaded = snapshot_to_live_index(snapshot);

        assert_eq!(
            loaded.files.get("ok.rs").unwrap().parse_status,
            ParseStatus::Parsed
        );
        assert!(matches!(
            loaded.files.get("partial.rs").unwrap().parse_status,
            ParseStatus::PartialParse { .. }
        ));
        assert!(matches!(
            loaded.files.get("fail.rb").unwrap().parse_status,
            ParseStatus::Failed { .. }
        ));
    }

    // ── Version mismatch / corrupt data tests ─────────────────────────────────

    #[test]
    fn test_version_mismatch_returns_none() {
        let tmp = TempDir::new().unwrap();

        // Build a snapshot with a wrong version and serialize it manually
        let snapshot = IndexSnapshot {
            version: 999,
            files: HashMap::new(),
        };
        let bytes = postcard::to_stdvec(&snapshot).unwrap();
        let dir = tmp.path().join(".tokenizor");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("index.bin"), &bytes).unwrap();

        // load_snapshot must return None, not panic
        let result = load_snapshot(tmp.path());
        assert!(result.is_none(), "version mismatch must return None");
    }

    #[test]
    fn test_corrupt_bytes_returns_none_no_panic() {
        let tmp = TempDir::new().unwrap();

        // Write random garbage
        let dir = tmp.path().join(".tokenizor");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("index.bin"),
            b"not valid postcard data xyzzy 12345",
        )
        .unwrap();

        let result = load_snapshot(tmp.path());
        assert!(
            result.is_none(),
            "corrupt bytes must return None, not panic"
        );
    }

    #[test]
    fn test_truncated_bytes_returns_none_no_panic() {
        let tmp = TempDir::new().unwrap();

        // Serialize a real snapshot, then truncate it to half
        let index = make_live_index_with_files(vec![("a.rs", b"fn foo() {}")]);
        serialize_index(&index, tmp.path()).expect("serialize should succeed");

        let bin_path = tmp.path().join(".tokenizor").join("index.bin");
        let full_bytes = std::fs::read(&bin_path).unwrap();
        let truncated = &full_bytes[..full_bytes.len() / 2];
        std::fs::write(&bin_path, truncated).unwrap();

        let result = load_snapshot(tmp.path());
        assert!(
            result.is_none(),
            "truncated bytes must return None, not panic"
        );
    }

    #[test]
    fn test_missing_file_returns_none() {
        let tmp = TempDir::new().unwrap();
        // No .tokenizor/index.bin exists
        let result = load_snapshot(tmp.path());
        assert!(result.is_none(), "missing file must return None");
    }

    // ── stat_check_files tests ────────────────────────────────────────────────

    #[test]
    fn test_stat_check_identifies_changed_file_by_size() {
        let tmp = TempDir::new().unwrap();
        let file_path = tmp.path().join("a.rs");
        std::fs::write(&file_path, b"fn foo() {}").unwrap();

        // Build index with wrong byte_len to simulate a changed file
        let mut file_map: HashMap<String, Arc<IndexedFile>> = HashMap::new();
        file_map.insert(
            "a.rs".to_string(),
            Arc::new(IndexedFile {
                relative_path: "a.rs".to_string(),
                language: LanguageId::Rust,
                classification: crate::domain::FileClassification::for_code_path("a.rs"),
                content: b"fn foo() {}".to_vec(),
                symbols: vec![],
                parse_status: ParseStatus::Parsed,
                byte_len: 999, // wrong size — simulates change
                content_hash: "old_hash".to_string(),
                references: vec![],
                alias_map: HashMap::new(),
            }),
        );
        let trigram_index = crate::live_index::trigram::TrigramIndex::build_from_files(&file_map);
        let mut index = LiveIndex {
            files: file_map,
            loaded_at: Instant::now(),
            loaded_at_system: SystemTime::now(),
            load_duration: Duration::ZERO,
            cb_state: CircuitBreakerState::new(0.20),
            is_empty: false,
            load_source: IndexLoadSource::FreshLoad,
            snapshot_verify_state: SnapshotVerifyState::NotNeeded,
            reverse_index: HashMap::new(),
            files_by_basename: HashMap::new(),
            files_by_dir_component: HashMap::new(),
            trigram_index,
        };
        index.rebuild_reverse_index();
        index.rebuild_path_indices();

        // mtime from disk
        let mtime = std::fs::metadata(&file_path)
            .unwrap()
            .modified()
            .unwrap()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        let mut mtimes = HashMap::new();
        mtimes.insert("a.rs".to_string(), mtime);

        let result = stat_check_files(&index, &mtimes, tmp.path());
        assert!(
            result.changed.contains(&"a.rs".to_string()),
            "changed by size mismatch"
        );
        assert!(result.deleted.is_empty());
    }

    #[test]
    fn test_stat_check_identifies_deleted_file() {
        let tmp = TempDir::new().unwrap();

        // Index has a file that doesn't exist on disk
        let mut file_map: HashMap<String, Arc<IndexedFile>> = HashMap::new();
        file_map.insert(
            "ghost.rs".to_string(),
            Arc::new(IndexedFile {
                relative_path: "ghost.rs".to_string(),
                language: LanguageId::Rust,
                classification: crate::domain::FileClassification::for_code_path("ghost.rs"),
                content: b"fn ghost() {}".to_vec(),
                symbols: vec![],
                parse_status: ParseStatus::Parsed,
                byte_len: 13,
                content_hash: "hash".to_string(),
                references: vec![],
                alias_map: HashMap::new(),
            }),
        );
        let trigram_index = crate::live_index::trigram::TrigramIndex::build_from_files(&file_map);
        let mut index = LiveIndex {
            files: file_map,
            loaded_at: Instant::now(),
            loaded_at_system: SystemTime::now(),
            load_duration: Duration::ZERO,
            cb_state: CircuitBreakerState::new(0.20),
            is_empty: false,
            load_source: IndexLoadSource::FreshLoad,
            snapshot_verify_state: SnapshotVerifyState::NotNeeded,
            reverse_index: HashMap::new(),
            files_by_basename: HashMap::new(),
            files_by_dir_component: HashMap::new(),
            trigram_index,
        };
        index.rebuild_reverse_index();
        index.rebuild_path_indices();

        let result = stat_check_files(&index, &HashMap::new(), tmp.path());
        assert!(
            result.deleted.contains(&"ghost.rs".to_string()),
            "missing file should be in deleted"
        );
    }

    #[test]
    fn test_stat_check_identifies_new_file() {
        let tmp = TempDir::new().unwrap();
        // Write a file on disk that's not in the index
        std::fs::write(tmp.path().join("new.rs"), b"fn new_func() {}").unwrap();

        // Empty index
        let index = make_live_index_with_files(vec![]);

        let result = stat_check_files(&index, &HashMap::new(), tmp.path());
        assert!(
            result.new_files.contains(&"new.rs".to_string()),
            "new file should be detected"
        );
    }

    // ── spot_verify_sample tests ──────────────────────────────────────────────

    #[test]
    fn test_spot_verify_catches_content_hash_mismatch() {
        let tmp = TempDir::new().unwrap();
        let file_path = tmp.path().join("a.rs");
        // On-disk content is different from what's in the index
        std::fs::write(&file_path, b"fn modified() {}").unwrap();

        let mut file_map: HashMap<String, Arc<IndexedFile>> = HashMap::new();
        file_map.insert(
            "a.rs".to_string(),
            Arc::new(IndexedFile {
                relative_path: "a.rs".to_string(),
                language: LanguageId::Rust,
                classification: crate::domain::FileClassification::for_code_path("a.rs"),
                content: b"fn original() {}".to_vec(), // old content
                symbols: vec![],
                parse_status: ParseStatus::Parsed,
                byte_len: 16,
                content_hash: crate::hash::digest_hex(b"fn original() {}"), // stale hash
                references: vec![],
                alias_map: HashMap::new(),
            }),
        );
        let trigram_index = crate::live_index::trigram::TrigramIndex::build_from_files(&file_map);
        let mut index = LiveIndex {
            files: file_map,
            loaded_at: Instant::now(),
            loaded_at_system: SystemTime::now(),
            load_duration: Duration::ZERO,
            cb_state: CircuitBreakerState::new(0.20),
            is_empty: false,
            load_source: IndexLoadSource::FreshLoad,
            snapshot_verify_state: SnapshotVerifyState::NotNeeded,
            reverse_index: HashMap::new(),
            files_by_basename: HashMap::new(),
            files_by_dir_component: HashMap::new(),
            trigram_index,
        };
        index.rebuild_reverse_index();
        index.rebuild_path_indices();

        // Sample 100% to ensure the file is included
        let mismatches = spot_verify_sample(&index, tmp.path(), 1.0);
        assert!(
            mismatches.contains(&"a.rs".to_string()),
            "hash mismatch should be detected"
        );
    }

    #[test]
    fn test_spot_verify_no_mismatch_when_hashes_match() {
        let tmp = TempDir::new().unwrap();
        let content = b"fn current() {}";
        let file_path = tmp.path().join("a.rs");
        std::fs::write(&file_path, content).unwrap();

        let hash = crate::hash::digest_hex(content);
        let mut file_map: HashMap<String, Arc<IndexedFile>> = HashMap::new();
        file_map.insert(
            "a.rs".to_string(),
            Arc::new(IndexedFile {
                relative_path: "a.rs".to_string(),
                language: LanguageId::Rust,
                classification: crate::domain::FileClassification::for_code_path("a.rs"),
                content: content.to_vec(),
                symbols: vec![],
                parse_status: ParseStatus::Parsed,
                byte_len: content.len() as u64,
                content_hash: hash,
                references: vec![],
                alias_map: HashMap::new(),
            }),
        );
        let trigram_index = crate::live_index::trigram::TrigramIndex::build_from_files(&file_map);
        let mut index = LiveIndex {
            files: file_map,
            loaded_at: Instant::now(),
            loaded_at_system: SystemTime::now(),
            load_duration: Duration::ZERO,
            cb_state: CircuitBreakerState::new(0.20),
            is_empty: false,
            load_source: IndexLoadSource::FreshLoad,
            snapshot_verify_state: SnapshotVerifyState::NotNeeded,
            reverse_index: HashMap::new(),
            files_by_basename: HashMap::new(),
            files_by_dir_component: HashMap::new(),
            trigram_index,
        };
        index.rebuild_reverse_index();
        index.rebuild_path_indices();

        let mismatches = spot_verify_sample(&index, tmp.path(), 1.0);
        assert!(mismatches.is_empty(), "no mismatch when hash is current");
    }

    #[test]
    fn test_spot_verify_empty_index_returns_empty() {
        let tmp = TempDir::new().unwrap();
        let index = make_live_index_with_files(vec![]);
        let mismatches = spot_verify_sample(&index, tmp.path(), 0.10);
        assert!(mismatches.is_empty(), "empty index returns empty vec");
    }

    // ── Snapshot atomicity test ───────────────────────────────────────────────

    #[test]
    fn test_serialize_creates_tokenizor_dir() {
        let tmp = TempDir::new().unwrap();
        let index = make_live_index_with_files(vec![("src/lib.rs", b"fn lib() {}")]);

        serialize_index(&index, tmp.path()).expect("serialize should succeed");

        assert!(
            tmp.path().join(".tokenizor").join("index.bin").exists(),
            ".tokenizor/index.bin should be created"
        );
    }

    #[test]
    fn test_serialize_idempotent() {
        let tmp = TempDir::new().unwrap();
        let index = make_live_index_with_files(vec![("a.rs", b"fn a() {}")]);

        // Serialize twice — should succeed both times (no leftover .tmp)
        serialize_index(&index, tmp.path()).expect("first serialize should succeed");
        serialize_index(&index, tmp.path()).expect("second serialize should succeed");

        assert!(tmp.path().join(".tokenizor").join("index.bin").exists());
        // No tmp file should remain
        assert!(!tmp.path().join(".tokenizor").join("index.bin.tmp").exists());
    }
}
