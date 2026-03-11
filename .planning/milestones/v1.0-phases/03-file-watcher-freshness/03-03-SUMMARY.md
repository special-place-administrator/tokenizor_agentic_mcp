---
phase: 03-file-watcher-freshness
plan: 03
subsystem: live_index
tags: [rust, notify, debouncer, file-watcher, integration-tests, tokio, watcher-wiring]

# Dependency graph
requires:
  - phase: 03-file-watcher-freshness
    plan: 01
    provides: "WatcherState, WatcherInfo, BurstTracker types; LiveIndex mutation API; HealthStats with watcher fields"
  - phase: 03-file-watcher-freshness
    plan: 02
    provides: "run_watcher, restart_watcher, process_events, normalize_event_path, start_watcher"

provides:
  - "main.rs spawns run_watcher after initial load when TOKENIZOR_AUTO_INDEX=true"
  - "TokenizorServer has watcher_info (Arc<Mutex<WatcherInfo>>) and repo_root fields"
  - "health tool uses health_report_with_watcher to reflect live watcher state"
  - "index_folder restarts watcher via restart_watcher after successful reload"
  - "format.rs has health_report_with_watcher for production health reporting"
  - "8 integration tests in tests/watcher_integration.rs proving all FRSH + RELY-03 reqs"
  - "run_watcher uses recv_timeout + yield_now to avoid blocking tokio executor"

affects:
  - 04-cross-references
  - all-future-phases

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Watcher spawning pattern: tokio::spawn(async move { run_watcher(...).await }) after LiveIndex::load"
    - "recv_timeout(50ms) + yield_now() pattern for mixing std::sync::mpsc with tokio async"
    - "health_report_with_watcher: production variant that passes live WatcherInfo through health_stats_with_watcher"
    - "Integration test pattern: tempdir + LiveIndex::load + tokio::spawn(run_watcher) + 500ms wait + assert"
    - "Multi-thread tokio test flavor required when run_watcher is spawned (recv_timeout still needs multiple workers)"

key-files:
  created:
    - "tests/watcher_integration.rs — 8 integration tests for FRSH-01 through FRSH-06 and RELY-03"
  modified:
    - "src/main.rs — spawns run_watcher after initial load; WatcherInfo shared state; watcher_root tracking"
    - "src/protocol/mod.rs — TokenizorServer gains watcher_info and repo_root fields; new() updated"
    - "src/protocol/tools.rs — health calls health_report_with_watcher; index_folder calls restart_watcher"
    - "src/protocol/format.rs — health_report_with_watcher added (production health reporting with live watcher)"
    - "src/watcher/mod.rs — recv() replaced with recv_timeout(50ms) + yield_now() to avoid executor starvation"

key-decisions:
  - "[Phase 03-03] recv_timeout(50ms) + yield_now() replaces blocking recv() in run_watcher — std::sync::mpsc::Receiver::recv() blocks the tokio worker thread; recv_timeout releases the thread every 50ms to allow the executor to process other tasks"
  - "[Phase 03-03] health_report_with_watcher is a new function alongside health_report — health_report (using Off defaults) is preserved for unit tests and contexts without a watcher; production health tool always uses the watcher variant"
  - "[Phase 03-03] #[allow(dead_code)] on repo_root field — stored for future diagnostics/restart-from-last-root, not yet read by any production code path"
  - "[Phase 03-03] Integration tests use #[tokio::test(flavor = multi_thread, worker_threads = 2)] — run_watcher must run on a separate worker thread; single-thread tokio would deadlock"

patterns-established:
  - "Pattern: Watcher info flows from main.rs → TokenizorServer → health tool via Arc<Mutex<WatcherInfo>>"
  - "Pattern: restart_watcher called from index_folder immediately after reload() succeeds — watcher always tracks current root"
  - "Pattern: Integration tests initialize with 100ms sleep (watcher startup), assert with 500ms total (200ms debounce + 300ms margin)"

requirements-completed: [FRSH-01, FRSH-02, FRSH-03, FRSH-04, FRSH-05, FRSH-06, RELY-03]

# Metrics
duration: 18min
completed: 2026-03-10
---

# Phase 3 Plan 03: Watcher Wiring and Integration Tests Summary

**MCP server fully wired with active file watcher: main.rs spawns run_watcher after load, health tool shows live watcher state, index_folder restarts watcher, and 8 integration tests prove all FRSH-01 through FRSH-06 and RELY-03 end-to-end**

## Performance

- **Duration:** 18 min
- **Started:** 2026-03-10T17:40:05Z
- **Completed:** 2026-03-10T17:58:00Z
- **Tasks:** 2
- **Files modified:** 5 (1 created)

## Accomplishments

- MCP server is a fully wired system: initial load triggers automatic file watching, health tool reports live watcher state, index_folder restarts watcher at new root
- All 7 phase requirements (FRSH-01 through FRSH-06 + RELY-03) are proven by automated integration tests using real filesystem operations
- Fixed a blocking-recv bug in run_watcher that would starve the tokio executor when used in async contexts (both tests and production with limited worker threads)
- Full test suite: 180 tests pass (148 unit + 18 live_index integration + 6 grammar + 8 watcher integration), zero warnings

## Task Commits

Each task was committed atomically:

1. **Task 1: Wire watcher into main.rs, TokenizorServer, health/index_folder tools** - `ddce97d` (feat)
2. **Task 2: Integration tests + fix blocking recv in run_watcher** - `88229cd` (feat)

**Plan metadata:** (pending docs commit)

## Files Created/Modified

- `src/main.rs` — spawns run_watcher after initial load; creates WatcherInfo shared state; passes watcher_root to TokenizorServer
- `src/protocol/mod.rs` — TokenizorServer gains watcher_info and repo_root fields; new() signature updated to accept both
- `src/protocol/tools.rs` — health handler calls health_report_with_watcher (live state); index_folder calls restart_watcher after reload; Arc import added
- `src/protocol/format.rs` — health_report_with_watcher function added; preserves original health_report for unit tests
- `src/watcher/mod.rs` — recv() replaced with recv_timeout(50ms) + yield_now() in run_watcher inner event loop
- `tests/watcher_integration.rs` — 8 integration tests: modify/create/delete/hash-skip/ENOENT/perf/state/filter

## Decisions Made

- `recv_timeout(50ms) + yield_now()` replaces blocking `recv()` — blocking `std::sync::mpsc::Receiver::recv()` on a tokio worker thread starves the executor; the timeout variant releases the thread every 50ms; this is the correct pattern for mixing sync mpsc channels with async tokio code
- `health_report_with_watcher` is a new function, not a replacement — unit tests and contexts without a watcher continue to use `health_report` with Off defaults; production always uses the watcher variant; both remain valid
- `repo_root` field on TokenizorServer gets `#[allow(dead_code)]` — stored for future use (diagnostics, restart-from-last-root), not yet read in any production code path

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Fixed blocking recv() starving tokio executor in run_watcher**
- **Found during:** Task 2 (integration tests for test_watcher_state_reports_active)
- **Issue:** `std::sync::mpsc::Receiver::recv()` in the run_watcher event loop is a blocking call that occupies the tokio worker thread indefinitely. In async integration tests (and any production scenario with limited worker threads), this prevents the tokio executor from processing timer futures (`tokio::time::sleep`), causing async tests to hang and never complete.
- **Fix:** Changed `recv()` to `recv_timeout(Duration::from_millis(50))`. On `Timeout`, call `tokio::task::yield_now().await` to release the thread. On `Disconnected`, break the loop (same as before). Created/Modify/Error events handled identically to before.
- **Files modified:** src/watcher/mod.rs
- **Verification:** `cargo test --test watcher_integration` — all 8 tests pass in <1s; `cargo check` zero warnings; `cargo test --lib` 148/148 pass
- **Committed in:** 88229cd (Task 2 commit)

---

**Total deviations:** 1 auto-fixed (Rule 1 — blocking call bug that would starve tokio in async contexts)
**Impact on plan:** Critical correctness fix. Without it, integration tests hang indefinitely and production with constrained worker threads may exhibit latency spikes.

## Issues Encountered

- Windows linker LNK1104 errors when trying to recompile while a previous test binary was still running (stuck in blocking recv). Resolved by killing the hung test process before rebuilding.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- Phase 3 is complete: all 7 FRSH requirements and RELY-03 are implemented and proven by integration tests
- Phase 4 (cross-references) can proceed: LiveIndex now has update_file/add_file/remove_file for incremental mutation, which cross-reference updates will need
- Watcher is now a first-class server component: health tool shows Active/Degraded/Off, index_folder restarts it cleanly

## Self-Check: PASSED

- `src/main.rs` modified: YES
- `src/protocol/mod.rs` modified: YES
- `src/protocol/tools.rs` modified: YES
- `src/protocol/format.rs` modified: YES
- `src/watcher/mod.rs` modified: YES
- `tests/watcher_integration.rs` created: YES
- Commit `ddce97d` exists: YES
- Commit `88229cd` exists: YES
- `cargo check` zero warnings: YES
- `cargo test` 180/180 pass: YES

---
*Phase: 03-file-watcher-freshness*
*Completed: 2026-03-10*
