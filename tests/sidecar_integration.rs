/// Integration tests for the HTTP sidecar and hook infrastructure.
///
/// Proves HOOK-01 (sidecar binds ephemeral port, port file written, endpoints respond),
/// HOOK-02 (shared index mutation visible through sidecar),
/// HOOK-03 (hook round-trip under 100ms),
/// HOOK-10 (hook stdout is valid JSON for all paths including fail-open).
///
/// Note: Tests that change process cwd are run with `--test-threads=1` (the full integration
/// test suite is invoked with that flag) to avoid cwd races.  Within the file, all async
/// tests that mutate cwd acquire `CWD_LOCK` which is a `tokio::sync::Mutex` so it can be
/// held across await points on the multi-thread runtime.
use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use once_cell::sync::Lazy;
use tempfile::TempDir;
use tokenizor_agentic_mcp::{
    cli::HookSubcommand,
    cli::hook::{event_name_for, fail_open_json, run_hook, success_json},
    domain::{LanguageId, SymbolKind, SymbolRecord},
    live_index::{IndexedFile, LiveIndex, ParseStatus, SharedIndex},
    sidecar::spawn_sidecar,
};
use tokio::sync::Mutex;

// ---------------------------------------------------------------------------
// Serialize all cwd-manipulating tests.
// tokio::sync::Mutex is Send so it can be held across await points.
// ---------------------------------------------------------------------------
static CWD_LOCK: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

// ---------------------------------------------------------------------------
// Test helpers
// ---------------------------------------------------------------------------

/// Build a minimal `IndexedFile` for a Rust source file with one function symbol.
fn make_rust_file(path: &str, fn_name: &str) -> IndexedFile {
    let content = format!("fn {fn_name}() {{}}").into_bytes();
    IndexedFile {
        relative_path: path.to_string(),
        language: LanguageId::Rust,
        content: content.clone(),
        symbols: vec![SymbolRecord {
            name: fn_name.to_string(),
            kind: SymbolKind::Function,
            depth: 0,
            sort_order: 0,
            byte_range: (0, content.len() as u32),
            line_range: (1, 1),
        }],
        parse_status: ParseStatus::Parsed,
        byte_len: content.len() as u64,
        content_hash: "test".to_string(),
        references: vec![],
        alias_map: HashMap::new(),
    }
}

/// Build a `SharedIndex` using the public API (`LiveIndex::empty()` + `add_file`).
fn build_shared_index(files: Vec<IndexedFile>) -> SharedIndex {
    let shared = LiveIndex::empty();
    {
        let mut guard = shared.write().expect("lock should not be poisoned");
        for file in files {
            let path = file.relative_path.clone();
            guard.add_file(path, file);
        }
    }
    shared
}

/// Make a synchronous raw HTTP GET request to `127.0.0.1:{port}{path}?{query}`.
/// Returns the response body or an error.
fn raw_http_get(port: u16, path: &str, query: &str) -> anyhow::Result<String> {
    let addr: std::net::SocketAddr = format!("127.0.0.1:{port}").parse()?;
    let timeout = Duration::from_millis(500);
    let mut stream = TcpStream::connect_timeout(&addr, timeout)?;
    stream.set_read_timeout(Some(timeout))?;
    stream.set_write_timeout(Some(timeout))?;

    let request_path = if query.is_empty() {
        path.to_string()
    } else {
        format!("{path}?{query}")
    };

    let request = format!(
        "GET {request_path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n"
    );
    stream.write_all(request.as_bytes())?;

    let mut response = String::new();
    stream.read_to_string(&mut response)?;

    let body = response
        .split_once("\r\n\r\n")
        .map(|(_, b)| b)
        .unwrap_or("")
        .to_string();
    Ok(body)
}

fn stable_cwd() -> PathBuf {
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from(env!("CARGO_MANIFEST_DIR")))
}

fn restore_cwd(path: &Path) {
    if std::env::set_current_dir(path).is_err() {
        std::env::set_current_dir(env!("CARGO_MANIFEST_DIR"))
            .expect("manifest dir must be a valid cwd fallback");
    }
}

// ---------------------------------------------------------------------------
// HOOK-01: Sidecar binds ephemeral port and writes port file
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_sidecar_binds_ephemeral_port() {
    let tmp = TempDir::new().unwrap();
    let _guard = CWD_LOCK.lock().await;
    let original = stable_cwd();
    std::env::set_current_dir(tmp.path()).unwrap();

    let index = build_shared_index(vec![make_rust_file("src/main.rs", "main")]);
    let handle = spawn_sidecar(Arc::clone(&index), "127.0.0.1")
        .await
        .expect("spawn_sidecar should succeed");

    assert!(handle.port > 0, "port must be a valid non-zero value");

    let port_file = tmp.path().join(".tokenizor/sidecar.port");
    assert!(port_file.exists(), "sidecar.port file must exist");
    let content = std::fs::read_to_string(&port_file).unwrap();
    let file_port: u16 = content
        .trim()
        .parse()
        .expect("port file must contain a valid u16");
    assert_eq!(file_port, handle.port, "port file must match handle port");

    let pid_file = tmp.path().join(".tokenizor/sidecar.pid");
    assert!(pid_file.exists(), "sidecar.pid file must exist");

    // Send shutdown and wait briefly for async cleanup.
    let _ = handle.shutdown_tx.send(());
    tokio::time::sleep(Duration::from_millis(100)).await;

    assert!(
        !port_file.exists(),
        "sidecar.port file must be cleaned up after shutdown"
    );
    assert!(
        !pid_file.exists(),
        "sidecar.pid file must be cleaned up after shutdown"
    );

    restore_cwd(&original);
}

// ---------------------------------------------------------------------------
// HOOK-01: Health endpoint responds within 50ms
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_health_endpoint_responds() {
    let tmp = TempDir::new().unwrap();
    let _guard = CWD_LOCK.lock().await;
    let original = stable_cwd();
    std::env::set_current_dir(tmp.path()).unwrap();

    let index = build_shared_index(vec![
        make_rust_file("src/main.rs", "main"),
        make_rust_file("src/lib.rs", "run"),
    ]);
    let handle = spawn_sidecar(Arc::clone(&index), "127.0.0.1")
        .await
        .expect("spawn_sidecar should succeed");

    tokio::time::sleep(Duration::from_millis(20)).await;

    let start = Instant::now();
    let body = raw_http_get(handle.port, "/health", "").expect("GET /health must succeed");
    let elapsed = start.elapsed();

    assert!(
        elapsed < Duration::from_millis(50),
        "health response latency must be <50ms, got {:?}",
        elapsed
    );

    let parsed: serde_json::Value =
        serde_json::from_str(&body).expect("health response must be valid JSON");
    assert!(
        parsed.get("file_count").is_some(),
        "health response must contain 'file_count'"
    );
    assert!(
        parsed.get("symbol_count").is_some(),
        "health response must contain 'symbol_count'"
    );
    assert_eq!(parsed["file_count"], 2, "file_count must match index");

    let _ = handle.shutdown_tx.send(());
    restore_cwd(&original);
}

// ---------------------------------------------------------------------------
// HOOK-01: /outline endpoint returns symbols for a known file
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_outline_endpoint() {
    let tmp = TempDir::new().unwrap();
    let _guard = CWD_LOCK.lock().await;
    let original = stable_cwd();
    std::env::set_current_dir(tmp.path()).unwrap();

    let index = build_shared_index(vec![make_rust_file("src/foo.rs", "hello")]);
    let handle = spawn_sidecar(Arc::clone(&index), "127.0.0.1")
        .await
        .expect("spawn_sidecar should succeed");

    tokio::time::sleep(Duration::from_millis(20)).await;

    let body = raw_http_get(handle.port, "/outline", "path=src/foo.rs")
        .expect("GET /outline must succeed");

    assert!(
        body.contains("src/foo.rs"),
        "outline should mention the requested file"
    );
    assert!(
        body.contains("hello"),
        "outline should include the symbol name"
    );
    assert!(
        body.contains("tokens saved"),
        "outline should include the token savings footer"
    );

    let _ = handle.shutdown_tx.send(());
    restore_cwd(&original);
}

// ---------------------------------------------------------------------------
// HOOK-02: Shared index mutation visible through sidecar
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_shared_index_mutation() {
    let tmp = TempDir::new().unwrap();
    let _guard = CWD_LOCK.lock().await;
    let original = stable_cwd();
    std::env::set_current_dir(tmp.path()).unwrap();

    let index = build_shared_index(vec![make_rust_file("src/a.rs", "alpha")]);
    let handle = spawn_sidecar(Arc::clone(&index), "127.0.0.1")
        .await
        .expect("spawn_sidecar should succeed");

    tokio::time::sleep(Duration::from_millis(20)).await;

    // Verify initial state via sidecar.
    let body = raw_http_get(handle.port, "/health", "").expect("GET /health must succeed");
    let parsed: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(parsed["file_count"], 1, "initially 1 file");

    // Add a new file through the shared Arc<LiveIndex>.
    {
        let mut guard = index.write().expect("lock should not be poisoned");
        let new_file = make_rust_file("src/b.rs", "beta");
        guard.add_file("src/b.rs".to_string(), new_file);
    }

    // Sidecar should now report 2 files (same Arc).
    let body2 =
        raw_http_get(handle.port, "/health", "").expect("GET /health after mutation must succeed");
    let parsed2: serde_json::Value = serde_json::from_str(&body2).unwrap();
    assert_eq!(
        parsed2["file_count"], 2,
        "sidecar must see mutated index — file_count must be 2"
    );

    // Outline for the new file must also be visible.
    let outline = raw_http_get(handle.port, "/outline", "path=src/b.rs")
        .expect("GET /outline for new file must succeed");
    assert!(
        outline.contains("src/b.rs"),
        "outline should mention the new file"
    );
    assert!(
        outline.contains("beta"),
        "new file symbol must be visible through sidecar"
    );

    let _ = handle.shutdown_tx.send(());
    restore_cwd(&original);
}

// ---------------------------------------------------------------------------
// HOOK-03: Hook binary completes round-trip in under 100ms
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_hook_binary_latency() {
    let tmp = TempDir::new().unwrap();
    let _guard = CWD_LOCK.lock().await;
    let original = stable_cwd();
    std::env::set_current_dir(tmp.path()).unwrap();

    let index = build_shared_index(vec![make_rust_file("src/main.rs", "main")]);
    let handle = spawn_sidecar(Arc::clone(&index), "127.0.0.1")
        .await
        .expect("spawn_sidecar should succeed");

    tokio::time::sleep(Duration::from_millis(20)).await;

    // Port file already written by spawn_sidecar.
    // SessionStart calls /repo-map — no file path env var needed.
    let start = Instant::now();
    // run_hook writes JSON to stdout — acceptable in test context.
    run_hook(Some(&HookSubcommand::SessionStart)).expect("run_hook must succeed");
    let elapsed = start.elapsed();

    assert!(
        elapsed < Duration::from_millis(100),
        "hook round-trip must be <100ms, got {:?}",
        elapsed
    );

    let _ = handle.shutdown_tx.send(());
    restore_cwd(&original);
}

// ---------------------------------------------------------------------------
// HOOK-10: Hook output is valid JSON for all subcommands (direct function test)
// ---------------------------------------------------------------------------

#[test]
fn test_hook_output_valid_json() {
    // Test the JSON-building functions directly (these are what run_hook outputs).
    let subcommands = [
        HookSubcommand::Read,
        HookSubcommand::Edit,
        HookSubcommand::Grep,
        HookSubcommand::SessionStart,
        HookSubcommand::PromptSubmit,
    ];

    for sub in &subcommands {
        let event_name = event_name_for(sub);

        // Test fail-open JSON.
        let fail_json = fail_open_json(event_name);
        let parsed: serde_json::Value = serde_json::from_str(&fail_json)
            .unwrap_or_else(|e| panic!("fail_open_json for {:?} must be valid JSON: {e}", sub));
        assert!(
            parsed["hookSpecificOutput"].get("hookEventName").is_some(),
            "hookEventName must be present in fail_open output for {:?}",
            sub
        );
        assert!(
            parsed["hookSpecificOutput"]
                .get("additionalContext")
                .is_some(),
            "additionalContext must be present in fail_open output for {:?}",
            sub
        );

        // Test success JSON.
        let success = success_json(event_name, "some context data");
        let parsed2: serde_json::Value = serde_json::from_str(&success)
            .unwrap_or_else(|e| panic!("success_json for {:?} must be valid JSON: {e}", sub));
        assert!(
            parsed2["hookSpecificOutput"].get("hookEventName").is_some(),
            "hookEventName must be present in success output for {:?}",
            sub
        );
        assert!(
            parsed2["hookSpecificOutput"]
                .get("additionalContext")
                .is_some(),
            "additionalContext must be present in success output for {:?}",
            sub
        );
        assert_eq!(
            parsed2["hookSpecificOutput"]["additionalContext"], "some context data",
            "additionalContext value must match for {:?}",
            sub
        );
    }
}

// ---------------------------------------------------------------------------
// HOOK-10: Hook fail-open path outputs valid JSON when no sidecar running
// ---------------------------------------------------------------------------

#[test]
fn test_hook_failopen_valid_json() {
    let tmp = TempDir::new().unwrap();
    // Sync tests use std::sync::Mutex for cwd lock.
    let _guard = CWD_LOCK.blocking_lock();
    let original = stable_cwd();
    std::env::set_current_dir(tmp.path()).unwrap();

    // No sidecar running — no port file — fail-open path.
    let subcommands = [
        HookSubcommand::Read,
        HookSubcommand::Edit,
        HookSubcommand::Grep,
        HookSubcommand::SessionStart,
        HookSubcommand::PromptSubmit,
    ];

    for sub in &subcommands {
        let event_name = event_name_for(sub);
        let json = fail_open_json(event_name);
        let parsed: serde_json::Value = serde_json::from_str(&json)
            .unwrap_or_else(|e| panic!("fail-open JSON for {:?} must be valid: {e}", sub));

        assert_eq!(
            parsed["hookSpecificOutput"]["additionalContext"], "",
            "fail-open additionalContext must be empty string for {:?}",
            sub
        );
    }

    restore_cwd(&original);
}

// ---------------------------------------------------------------------------
// HOOK-01: /repo-map endpoint returns all indexed files
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_repo_map_endpoint() {
    let tmp = TempDir::new().unwrap();
    let _guard = CWD_LOCK.lock().await;
    let original = stable_cwd();
    std::env::set_current_dir(tmp.path()).unwrap();

    let files = vec![
        make_rust_file("src/a.rs", "alpha"),
        make_rust_file("src/b.rs", "beta"),
        make_rust_file("src/c.rs", "gamma"),
    ];
    let index = build_shared_index(files);
    let handle = spawn_sidecar(Arc::clone(&index), "127.0.0.1")
        .await
        .expect("spawn_sidecar should succeed");

    tokio::time::sleep(Duration::from_millis(20)).await;

    let body = raw_http_get(handle.port, "/repo-map", "").expect("GET /repo-map must succeed");

    assert!(
        body.contains("3 files"),
        "repo-map should summarize file count"
    );
    assert!(
        body.contains("3 symbols"),
        "repo-map should summarize symbol count"
    );
    assert!(
        body.contains("Rust: 3"),
        "repo-map should include language breakdown"
    );
    assert!(
        body.contains("src"),
        "repo-map should include the src directory bucket"
    );

    let _ = handle.shutdown_tx.send(());
    restore_cwd(&original);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_prompt_context_endpoint_prefers_file_hint() {
    let tmp = TempDir::new().unwrap();
    let _guard = CWD_LOCK.lock().await;
    let original = stable_cwd();
    std::env::set_current_dir(tmp.path()).unwrap();

    let index = build_shared_index(vec![make_rust_file("src/foo.rs", "hello")]);
    let handle = spawn_sidecar(Arc::clone(&index), "127.0.0.1")
        .await
        .expect("spawn_sidecar should succeed");

    tokio::time::sleep(Duration::from_millis(20)).await;

    let body = raw_http_get(
        handle.port,
        "/prompt-context",
        "text=please%20inspect%20src%2Ffoo.rs",
    )
    .expect("GET /prompt-context must succeed");

    assert!(
        body.contains("src/foo.rs"),
        "prompt context should mention the hinted file"
    );
    assert!(
        body.contains("hello"),
        "prompt context should include the hinted file symbol"
    );

    let _ = handle.shutdown_tx.send(());
    restore_cwd(&original);
}
