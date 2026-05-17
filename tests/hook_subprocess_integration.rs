//! Subprocess-level end-to-end tests for `run_hook`'s adoption-log dispatch
//! sites.
//!
//! Follow-up to the in-crate tests added in `src/cli/hook.rs` by the
//! daemon-and-sidecar tentacle (swarm-2). Those tests pin the metric
//! rendering + the counter wire-up from `record_hook_outcome` into
//! `ADOPTION_LOG_FILE` by calling `record_hook_outcome` directly. That
//! leaves the three dispatch sites inside `run_hook` itself
//! code-review-guarded: someone could remove a `record_hook_outcome*` call
//! and the in-crate tests would still pass.
//!
//! These tests spawn the real `symforge` binary in a tempdir and pin each
//! of the three sites end-to-end:
//!
//!   1. `no_sidecar` — port file missing and daemon fallback fails.
//!      Exercises `record_hook_outcome_with_detail(NoSidecar,
//!      reason="sidecar_port_missing")`.
//!   2. `stale_port` — port file present but the listener never accepts,
//!      so the subprocess's 50ms HTTP read times out. Exercises
//!      `record_hook_outcome_with_detail(NoSidecar,
//!      reason="sidecar_port_stale")`.
//!   3. `routed_success` — port file points at a minimal in-test TCP
//!      responder that returns `HTTP/1.1 200 OK`. Exercises the plain
//!      `record_hook_outcome(Routed)` call on the success path.
//!
//! All three assert against the tab-separated substring format written by
//! `append_hook_adoption_event*`: `<session>\t<workflow>\t<outcome>`. The
//! session id is left unpinned (normalized to `-` when no daemon session
//! file is present), leaving only the `(workflow, outcome)` pair checked.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::Path;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use tempfile::TempDir;

/// Mirrors `ADOPTION_LOG_FILE` in `src/cli/hook.rs`. Intentionally
/// duplicated: if the constant is renamed, the in-crate test
/// `test_record_hook_outcome_writes_to_adoption_log_file_constant`
/// catches it; if the constant's consumer inside `run_hook` drops its
/// call site, these tests catch it. The pair pins the full chain.
const ADOPTION_LOG_RELATIVE: &str = ".symforge/hook-adoption.log";

/// Mirrors `PORT_FILE` in `src/cli/hook.rs`. Any rename of either side
/// without updating this constant breaks the stale-port and routed
/// tests loudly.
const PORT_FILE_RELATIVE: &str = ".symforge/sidecar.port";

/// Minimal PostToolUse/Read payload for the stdin-routing path. The
/// `.rs` extension keeps `should_fail_open_read` from downgrading the
/// workflow to PassThrough (which skips `record_hook_outcome` and would
/// turn every test in this file into a no-op).
const READ_PAYLOAD: &str = r#"{"tool_name":"Read","tool_input":{"file_path":"src/foo.rs"}}"#;

/// Pin site 1: no sidecar, no daemon fallback.
#[test]
fn run_hook_no_sidecar_writes_source_read_no_sidecar_event() {
    let tmp = TempDir::new().expect("tempdir creation");
    let contents = run_hook_in_tempdir(tmp.path(), READ_PAYLOAD);
    assert!(
        contents.contains("\tsource-read\tno-sidecar"),
        "log must contain a tab-separated `source-read\\tno-sidecar` entry \
         (regression: record_hook_outcome_with_detail removed from the \
         port-file-missing dispatch branch); got:\n{contents}"
    );
}

/// Pin site 2: port file present, HTTP read times out.
#[test]
fn run_hook_stale_port_writes_source_read_no_sidecar_event() {
    let tmp = TempDir::new().expect("tempdir creation");
    std::fs::create_dir_all(tmp.path().join(".symforge")).expect("create .symforge dir");

    // Bind an ephemeral port and HOLD the listener for the entire test —
    // never accept. Subprocess's TCP connect may succeed (SYN queued) or
    // fail depending on backlog; either way the 50ms read timeout in
    // `sync_http_get_with_timeout` trips, producing an `Err` that drives
    // `run_hook` into the stale-port branch.
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind stale-port listener");
    let stale_port = listener.local_addr().expect("stale-port local_addr").port();
    std::fs::write(tmp.path().join(PORT_FILE_RELATIVE), stale_port.to_string())
        .expect("write stale port file");

    let contents = run_hook_in_tempdir(tmp.path(), READ_PAYLOAD);
    drop(listener);

    assert!(
        contents.contains("\tsource-read\tno-sidecar"),
        "log must contain a tab-separated `source-read\\tno-sidecar` entry \
         (regression: record_hook_outcome_with_detail removed from the \
         stale-port dispatch branch); got:\n{contents}"
    );
}

/// Pin site 3: port file points at a responder; HTTP call succeeds.
#[test]
fn run_hook_routed_success_writes_source_read_routed_event() {
    // Bind first so the port is known before the subprocess launches.
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock sidecar listener");
    let port = listener
        .local_addr()
        .expect("mock sidecar local_addr")
        .port();

    // Minimal single-shot HTTP responder. Started BEFORE the subprocess
    // spawns so the accept loop is already waiting when the subprocess
    // connects — the 50ms HTTP_TIMEOUT leaves no room for thread start-up
    // races. Writes a fixed 200-OK response and drops the stream, which
    // closes the connection and lets the subprocess's `read_to_string`
    // return.
    let mock = thread::spawn(move || {
        if let Ok((mut stream, _)) = listener.accept() {
            let _ = stream.set_read_timeout(Some(Duration::from_millis(500)));
            let mut buf = [0u8; 2048];
            let _ = stream.read(&mut buf);
            let _ = stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n");
        }
    });

    let tmp = TempDir::new().expect("tempdir creation");
    std::fs::create_dir_all(tmp.path().join(".symforge")).expect("create .symforge dir");
    std::fs::write(tmp.path().join(PORT_FILE_RELATIVE), port.to_string())
        .expect("write mock port file");

    let contents = run_hook_in_tempdir(tmp.path(), READ_PAYLOAD);

    // Best-effort join: if the subprocess served successfully, the mock
    // has already exited. If it failed early, the accept thread may still
    // block; we don't want to hang the test runner, so the JoinHandle is
    // consumed with a non-blocking check and otherwise detached — the
    // thread dies when the test binary process exits.
    drop(mock);

    assert!(
        contents.contains("\tsource-read\trouted"),
        "log must contain a tab-separated `source-read\\trouted` entry \
         (regression: record_hook_outcome removed from the success \
         dispatch branch); got:\n{contents}"
    );
}

/// Spawn `symforge hook` in `cwd`, pipe `payload` on stdin, wait for exit,
/// and return the adoption log contents. Panics with a clear message if
/// the subprocess doesn't exit, exits non-zero, or doesn't create the
/// log file. Shared across all three site tests.
fn run_hook_in_tempdir(cwd: &Path, payload: &str) -> String {
    let bin = env!("CARGO_BIN_EXE_symforge");
    let mut child = Command::new(bin)
        .arg("hook")
        .current_dir(cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("symforge binary should spawn");

    child
        .stdin
        .as_mut()
        .expect("piped stdin")
        .write_all(payload.as_bytes())
        .expect("write hook payload to child stdin");
    drop(child.stdin.take());

    let status = wait_with_timeout(&mut child, Duration::from_secs(15))
        .expect("hook subprocess should exit within 15s")
        .expect("hook subprocess status readable");
    assert!(
        status.success(),
        "symforge hook exited non-zero: {status:?}"
    );

    let log_path = cwd.join(ADOPTION_LOG_RELATIVE);
    assert!(
        log_path.exists(),
        "run_hook must append to {ADOPTION_LOG_RELATIVE} under the child's cwd; \
         missing at {}. This usually means a record_hook_outcome* call was \
         removed from the run_hook dispatch branch being exercised.",
        log_path.display()
    );

    std::fs::read_to_string(&log_path).expect("log readable")
}

/// Poll the child for exit with a timeout. `Ok(Some)` on clean exit,
/// `Ok(None)` on timeout (after killing the child), `Err` on wait
/// failure. Local to avoid pulling in an async runtime just for this.
fn wait_with_timeout(child: &mut Child, timeout: Duration) -> std::io::Result<Option<ExitStatus>> {
    let start = Instant::now();
    loop {
        match child.try_wait()? {
            Some(status) => return Ok(Some(status)),
            None => {
                if start.elapsed() >= timeout {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Ok(None);
                }
                thread::sleep(Duration::from_millis(25));
            }
        }
    }
}
