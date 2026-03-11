---
phase: 05-http-sidecar-hook-infrastructure
verified: 2026-03-10T21:55:00Z
status: passed
score: 16/16 must-haves verified
re_verification: false
---

# Phase 5: HTTP Sidecar + Hook Infrastructure Verification Report

**Phase Goal:** HTTP sidecar + Claude Code hook infrastructure — axum server sharing Arc<LiveIndex>, CLI hook binary with fail-open JSON, tokenizor init command
**Verified:** 2026-03-10T21:55:00Z
**Status:** PASSED
**Re-verification:** No — initial verification

---

## Goal Achievement

### Observable Truths

All truths are drawn from the `must_haves` frontmatter across plans 01, 02, and 03.

#### Plan 01 Truths (HOOK-01, HOOK-02)

| # | Truth | Status | Evidence |
|---|-------|--------|---------|
| 1 | Sidecar binds to an OS-assigned ephemeral port and writes it to .tokenizor/sidecar.port | VERIFIED | `server.rs` calls `TcpListener::bind("{host}:0")`, extracts port, calls `port_file::write_port_file(port)`. Integration test `test_sidecar_binds_ephemeral_port` confirms file exists and matches handle port. |
| 2 | Sidecar writes PID to .tokenizor/sidecar.pid and cleans up both files on shutdown | VERIFIED | `spawn_sidecar` calls `write_pid_file(std::process::id())`. Cleanup is called inside spawned task after `axum::serve` completes. Integration test confirms cleanup after shutdown signal. |
| 3 | All 5 endpoints (/health, /outline, /impact, /symbol-context, /repo-map) return valid JSON | VERIFIED | `router.rs` wires all 5 GET routes. `handlers.rs` implements all 5 with JSON responses. Tests `test_health_endpoint_responds`, `test_outline_endpoint`, `test_repo_map_endpoint` confirm live JSON responses. |
| 4 | Sidecar receives the same Arc<RwLock<LiveIndex>> as MCP tools — zero data duplication | VERIFIED | `main.rs` calls `sidecar::spawn_sidecar(Arc::clone(&index), &bind_host)` — the same `index` Arc used for the MCP server. Integration test `test_shared_index_mutation` mutates via one Arc reference and reads the change via sidecar. |
| 5 | Stale port/PID detection works via /health check on stored port | VERIFIED | `port_file::check_stale()` uses `TcpStream::connect_timeout(200ms)`. Unit test `test_check_stale_cleans_up_when_port_is_closed` verifies cleanup of stale files. `spawn_sidecar` calls `check_stale` before binding. |

#### Plan 02 Truths (HOOK-03, HOOK-10)

| # | Truth | Status | Evidence |
|---|-------|--------|---------|
| 6 | tokenizor hook read|edit|grep|session-start outputs valid JSON to stdout and nothing else | VERIFIED | `run_hook` only calls `println!` for the final JSON line. No tracing initialization in hook path. Unit tests `test_fail_open_json_is_valid`, `test_success_json_is_valid` confirm JSON validity. Integration test `test_hook_output_valid_json` validates all 4 subcommands. |
| 7 | When sidecar is unreachable (port file missing or connection refused), hook returns empty additionalContext JSON — fail-open | VERIFIED | `run_hook` reads port file first; on any error `println!("{}", fail_open_json(event_name))` and returns `Ok(())`. Integration test `test_hook_failopen_valid_json` confirms empty `additionalContext`. |
| 8 | Hook binary uses sync I/O (no tokio runtime) and completes in well under 100ms | VERIFIED | `src/cli/hook.rs` uses `std::net::TcpStream::connect_timeout` throughout — zero async, zero tokio. `main()` is sync; tokio runtime only created in `run_mcp_server()`. Integration test `test_hook_binary_latency` asserts round-trip < 100ms. |
| 9 | tokenizor init merges PostToolUse hook entries into ~/.claude/settings.json without overwriting existing hooks | VERIFIED | `merge_event_entries` filters out existing tokenizor entries by marker, appends fresh ones, keeps non-tokenizor entries. Integration test `test_init_preserves_other_hooks` confirms existing hooks survive. |
| 10 | Running tokenizor init twice produces identical settings.json (idempotent) | VERIFIED | `is_tokenizor_entry` identifies entries by `"tokenizor hook"` substring — old entries removed, fresh ones appended, net count stays same. Integration test `test_init_idempotent` confirms byte-identical output on second run. |

#### Plan 03 Truths (HOOK-01, HOOK-02, HOOK-03, HOOK-10)

| # | Truth | Status | Evidence |
|---|-------|--------|---------|
| 11 | The sidecar starts on an OS-assigned ephemeral port and /health responds within 50ms | VERIFIED | `test_health_endpoint_responds` asserts `elapsed < 50ms`. Passes on current hardware. |
| 12 | Sidecar and MCP tools share the same Arc<LiveIndex> — mutation through one is visible through the other | VERIFIED | `test_shared_index_mutation` adds file via `index.write()`, then confirms sidecar `/health` reports incremented `file_count` and `/outline` returns new symbols. |
| 13 | Hook binary completes a full round-trip (spawn + HTTP + response) in under 100ms | VERIFIED | `test_hook_binary_latency` times `run_hook(SessionStart)` against live sidecar. Passes with generous margin. |
| 14 | Hook binary stdout is valid JSON for all subcommands including fail-open paths | VERIFIED | `test_hook_output_valid_json` (all 4 subcommands, success + fail paths). |
| 15 | tokenizor init produces valid settings.json with all 4 hook entries | VERIFIED | `test_init_writes_hooks` confirms 3 PostToolUse + 1 SessionStart entries. |
| 16 | MCP server stdout purity (RELY-04) still holds after sidecar and CLI additions | VERIFIED | `test_stdout_purity` integration test passes unchanged. |

**Score:** 16/16 truths verified

---

### Required Artifacts

| Artifact | Expected | Status | Details |
|----------|----------|--------|---------|
| `Cargo.toml` | axum 0.8, clap 4 (derive), dirs 6, tokio sync, once_cell (dev) | VERIFIED | All 5 deps present: `axum = "0.8"`, `clap = { version = "4", features = ["derive"] }`, `dirs = "6"`, tokio has `"sync"` feature, `once_cell = "1"` (dev) |
| `src/sidecar/mod.rs` | SidecarHandle struct, pub use re-exports | VERIFIED | Exports `SidecarHandle { port: u16, shutdown_tx }` and `pub use server::spawn_sidecar` |
| `src/sidecar/port_file.rs` | write_port_file, write_pid_file, read_port, cleanup_files, check_stale | VERIFIED | All 5 functions implemented and exported; 9 unit tests all pass |
| `src/sidecar/server.rs` | spawn_sidecar async function | VERIFIED | Full implementation with ephemeral bind, port/PID writes, graceful shutdown, cleanup |
| `src/sidecar/router.rs` | build_router wiring 5 GET routes | VERIFIED | All 5 routes wired with `.with_state(index)` |
| `src/sidecar/handlers.rs` | 5 async handler functions + response structs | VERIFIED | All 5 handlers, 10+ unit tests, all pass |
| `src/cli/mod.rs` | Cli, Commands, HookSubcommand clap types | VERIFIED | All 3 types with correct derive macros |
| `src/cli/hook.rs` | run_hook + sync HTTP + fail-open JSON | VERIFIED | Complete implementation; 11 unit tests pass |
| `src/cli/init.rs` | run_init + merge_hooks_into_settings | VERIFIED | Both public functions; 8 unit tests + 3 integration tests pass |
| `src/main.rs` | CLI dispatch: None -> MCP+sidecar, Init -> run_init, Hook -> run_hook | VERIFIED | Sync `main()` with explicit tokio runtime only in `run_mcp_server()` |
| `tests/sidecar_integration.rs` | 8 integration tests for HOOK-01/02/03/10 | VERIFIED | 8 tests, all pass, min_lines > 80 (427 lines) |
| `tests/init_integration.rs` | 3 init idempotency integration tests | VERIFIED | 3 tests, all pass, 145 lines |

---

### Key Link Verification

| From | To | Via | Status | Details |
|------|----|-----|--------|---------|
| `src/sidecar/server.rs` | `src/sidecar/router.rs` | `router::build_router()` | WIRED | `let app = router::build_router(index);` at line 45 |
| `src/sidecar/server.rs` | `src/sidecar/port_file.rs` | `port_file::write_port_file/write_pid_file` | WIRED | Both calls present lines 39-40 |
| `src/sidecar/handlers.rs` | `src/live_index/query.rs` | `index.read()` | WIRED | All 5 handlers acquire `index.read()` guard; owned data extracted before drop |
| `src/cli/hook.rs` | `src/sidecar/port_file.rs` (convention) | `read_port_file()` reads `.tokenizor/sidecar.port` | WIRED | `read_port_file()` in hook.rs reads same file path convention; independent implementation matching contract |
| `src/cli/init.rs` | `~/.claude/settings.json` | serde_json Value read-modify-write | WIRED | `merge_hooks_into_settings` reads, merges, writes via `serde_json` |
| `src/cli/hook.rs` | stdout | `println!` of hookSpecificOutput JSON | WIRED | Only `println!` call is the final JSON line; tracing never initialized in hook path |
| `src/main.rs` | `src/cli/mod.rs` | `Cli::parse()` dispatch | WIRED | `let cli = cli::Cli::parse()` + match on `cli.command` at lines 8-13 |
| `src/main.rs` | `src/sidecar/server.rs` | `sidecar::spawn_sidecar()` | WIRED | `sidecar::spawn_sidecar(Arc::clone(&index), &bind_host).await?` at line 87 |
| `tests/sidecar_integration.rs` | `src/sidecar/server.rs` | `spawn_sidecar` in test setup | WIRED | `use tokenizor_agentic_mcp::sidecar::spawn_sidecar` + called in 6 of 8 tests |

---

### Requirements Coverage

| Requirement | Source Plan(s) | Description | Status | Evidence |
|-------------|---------------|-------------|--------|---------|
| HOOK-01 | 05-01, 05-03 | HTTP sidecar (axum) on localhost:0, port written to .tokenizor/sidecar.port | SATISFIED | `spawn_sidecar` binds `:0`, writes port file. `test_sidecar_binds_ephemeral_port`, `test_health_endpoint_responds`, `test_outline_endpoint`, `test_repo_map_endpoint` all prove endpoint behavior. |
| HOOK-02 | 05-01, 05-03 | Sidecar shares Arc<LiveIndex> with MCP tools — zero data duplication | SATISFIED | `main.rs` passes `Arc::clone(&index)` to both `TokenizorServer` and `spawn_sidecar`. `test_shared_index_mutation` proves the mutation is visible. |
| HOOK-03 | 05-02, 05-03 | Hook response latency <100ms total | SATISFIED | `run_hook` uses sync `TcpStream` with 50ms timeout. `test_hook_binary_latency` asserts < 100ms. |
| HOOK-10 | 05-02, 05-03 | Hook stdout is valid JSON only — no debug output corruption | SATISFIED | `run_hook` has exactly one `println!`. No tracing init in hook path. `test_hook_output_valid_json` + `test_hook_failopen_valid_json` confirm all paths. `test_stdout_purity` (RELY-04) continues to pass. |

**Orphaned requirements check:** REQUIREMENTS.md Traceability table maps HOOK-01, HOOK-02, HOOK-03, HOOK-10 to Phase 5. All four are claimed by plans 05-01, 05-02, 05-03. No orphaned requirements.

---

### Anti-Patterns Found

| File | Line | Pattern | Severity | Impact |
|------|------|---------|----------|--------|
| `tests/sidecar_integration.rs` | 361 | `let _guard = std::sync::Mutex::new(());` — creates a fresh mutex per test call instead of locking the shared `CWD_LOCK` | INFO | Does not cause failures because the test calls only `fail_open_json()` (pure function) and the suite runs at `--test-threads=1`. However, the cwd isolation is not actually enforced. Could cause flaky failures if thread count is ever increased. |

No blockers or warnings found in production code paths.

---

### Human Verification Required

None — all goal-critical behaviors are verified programmatically via passing unit and integration tests.

The following are low-value optional checks a human could do but are not blocking:

1. **Binary invocation sanity check** — run `cargo build` then invoke `./target/debug/tokenizor.exe hook read` in a directory without a `.tokenizor/sidecar.port` file and confirm the output is valid JSON. The integration tests cover this path, but an operator smoke-test confirms the compiled binary works end-to-end.

2. **Real Claude Code hook installation** — run `tokenizor init` against an actual `~/.claude/settings.json` and confirm the hook entries appear in the Claude Code UI settings panel. The `init_integration.rs` tests prove the merge logic with temp directories; the real-path scenario differs only in directory resolution.

---

### Gaps Summary

No gaps. All 16 observable truths verified. All 12 artifacts exist, are substantive, and are wired. All 4 phase requirements (HOOK-01, HOOK-02, HOOK-03, HOOK-10) are satisfied with integration test evidence. RELY-04 (stdout purity) is unbroken.

The one noted anti-pattern (`test_hook_failopen_valid_json` using a locally-scoped mutex instead of the shared `CWD_LOCK`) is INFO-level only — it does not affect correctness given the `--test-threads=1` convention documented in the test file header.

---

*Verified: 2026-03-10T21:55:00Z*
*Verifier: Claude (gsd-verifier)*
