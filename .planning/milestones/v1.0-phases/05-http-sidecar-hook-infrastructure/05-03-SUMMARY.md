---
phase: 05-http-sidecar-hook-infrastructure
plan: 03
subsystem: infra
tags: [axum, clap, tokio, sidecar, hooks, cli, integration-tests]

# Dependency graph
requires:
  - phase: 05-01
    provides: spawn_sidecar, SidecarHandle, port_file, router, 5 HTTP endpoint handlers
  - phase: 05-02
    provides: cli/mod.rs (Cli/Commands/HookSubcommand), cli/hook.rs (run_hook), cli/init.rs (merge_tokenizor_hooks)
provides:
  - CLI dispatch in main.rs (Init/Hook/None routes)
  - Sidecar spawned between watcher and MCP serve on None path
  - Sidecar shutdown signal sent after MCP server exits
  - merge_hooks_into_settings() public function in cli/init.rs
  - 8 sidecar integration tests proving HOOK-01/02/03/10
  - 3 init integration tests proving idempotent hook installation
affects:
  - phase-06-hook-stdin-parsing
  - any phase that extends the CLI or sidecar

# Tech tracking
tech-stack:
  added: [once_cell (dev-dependency for Lazy tokio::sync::Mutex in tests)]
  patterns:
    - Sync main() with explicit tokio runtime build for MCP server path only
    - tokio::sync::Mutex as static CWD_LOCK in async integration tests
    - build_shared_index via LiveIndex::empty() + add_file() for test isolation

key-files:
  created:
    - tests/sidecar_integration.rs
    - tests/init_integration.rs
  modified:
    - src/main.rs
    - src/cli/init.rs
    - Cargo.toml

key-decisions:
  - "main() is sync (no #[tokio::main]); run_mcp_server() builds explicit tokio runtime — avoids runtime overhead for tokenizor init and tokenizor hook subcommands"
  - "merge_hooks_into_settings(settings_path, binary_path) extracted as public function — enables integration tests to use temp dirs instead of real ~/.claude/settings.json"
  - "tokio::sync::Mutex for CWD_LOCK in async tests — std::sync::MutexGuard is not Send and cannot be held across .await points in multi_thread runtime"
  - "once_cell Lazy for static tokio::sync::Mutex — simplest way to initialize async mutex as a static in Rust stable"

patterns-established:
  - "Integration test cwd isolation: acquire tokio CWD_LOCK, set_current_dir to TempDir, run test body with awaits, restore cwd at end"
  - "Sidecar test helper: build_shared_index via empty() + add_file() avoids pub(crate) field access from external test crates"

requirements-completed: [HOOK-01, HOOK-02, HOOK-03, HOOK-10]

# Metrics
duration: 8min
completed: 2026-03-10
---

# Phase 5 Plan 03: Wire Main + Integration Tests Summary

**CLI dispatch wired into main.rs with sidecar spawn, 11 integration tests proving all Phase 5 requirements end-to-end**

## Performance

- **Duration:** 8 min
- **Started:** 2026-03-10T20:39:50Z
- **Completed:** 2026-03-10T20:47:57Z
- **Tasks:** 2
- **Files modified:** 5

## Accomplishments
- main.rs rewritten: sync `main()` dispatches Init/Hook/None, `run_mcp_server()` builds explicit tokio runtime, sidecar spawns after watcher and shuts down after MCP server exits
- 8 sidecar integration tests cover HOOK-01 (ephemeral port + port file + endpoint responses), HOOK-02 (shared index mutation visible through sidecar), HOOK-03 (round-trip < 100ms), HOOK-10 (valid JSON output for all subcommands and fail-open path)
- 3 init integration tests cover writing all 4 hook entries, idempotency, and preservation of non-tokenizor hooks
- RELY-04 stdout purity test confirmed passing after all changes

## Task Commits

1. **Task 1: Rewrite main.rs for CLI dispatch with sidecar spawn** - `d9e4d83` (feat)
2. **Task 2: Integration tests for sidecar, hooks, and init** - `32c6d00` (feat)

**Plan metadata:** (next commit after this SUMMARY)

## Files Created/Modified
- `src/main.rs` - Sync main() with CLI dispatch; run_mcp_server() builds tokio runtime, spawns sidecar after watcher
- `src/cli/init.rs` - Added public merge_hooks_into_settings() function for test isolation
- `tests/sidecar_integration.rs` - 8 integration tests proving HOOK-01/02/03/10
- `tests/init_integration.rs` - 3 integration tests proving idempotent hook installation
- `Cargo.toml` - Added once_cell as dev-dependency

## Decisions Made
- `main()` is sync with no `#[tokio::main]` — avoids creating a tokio runtime for `tokenizor init` and `tokenizor hook` subcommands, which are pure synchronous operations. Runtime is created explicitly only in `run_mcp_server()`.
- Extracted `merge_hooks_into_settings(settings_path, binary_path)` as a public function — the original `run_init()` used hardcoded `~/.claude/settings.json` with no way to inject a test path. The public function takes any path, enabling clean integration testing.
- Used `tokio::sync::Mutex` (not `std::sync::Mutex`) for the CWD_LOCK static in async tests. `std::sync::MutexGuard` is `!Send` and causes a compile error when held across `.await` points in `multi_thread` flavor tests.
- `once_cell::sync::Lazy` for static initialization of `tokio::sync::Mutex` — the simplest way to create an async mutex as a static in Rust stable without unsafe code.

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered
- `CwdGuard` struct approach failed: holding a `MutexGuard<'static, ()>` across `.await` points is rejected by the borrow checker in a `Send` async context. Resolved by switching to `tokio::sync::Mutex` which is `Send + Sync`.

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- Phase 5 complete: sidecar operational, CLI dispatch working, all HOOK-01/02/03/10 requirements proven by integration tests
- Phase 6 (Hook stdin JSON parsing) can build on the hook infrastructure — replace TOKENIZOR_HOOK_FILE_PATH env var shim with real stdin JSON parsing from Claude Code hook events

## Self-Check: PASSED
- FOUND: src/main.rs
- FOUND: tests/sidecar_integration.rs
- FOUND: tests/init_integration.rs
- FOUND: .planning/phases/05-http-sidecar-hook-infrastructure/05-03-SUMMARY.md
- FOUND: d9e4d83 (Task 1 commit)
- FOUND: 32c6d00 (Task 2 commit)
