---
phase: 03-file-watcher-freshness
plan: 02
subsystem: live_index
tags: [rust, notify, debouncer, file-watcher, content-hash, path-normalization, windows-unc]

# Dependency graph
requires:
  - phase: 03-file-watcher-freshness
    plan: 01
    provides: "WatcherState, WatcherInfo, BurstTracker types; SharedIndex mutation API (update_file/add_file/remove_file)"

provides:
  - "normalize_event_path: strips \\\\?\\ prefix, strip_prefix fallback, backslash→slash normalization"
  - "supported_language: LanguageId::from_extension delegation"
  - "is_relevant_event: Create/Modify/Remove pass, Access filtered"
  - "maybe_reindex: content-hash skip, ENOENT→remove_file, parse-before-lock (ReindexResult enum)"
  - "WatcherHandle: Debouncer<RecommendedWatcher, RecommendedCache> + std::sync::mpsc channel"
  - "start_watcher: new_debouncer(200ms) + watch(repo_root, Recursive)"
  - "process_events: batch event processing with burst tracking and watcher_info updates"
  - "run_watcher: async supervision loop, 1s backoff, Degraded after 3 consecutive failures"
  - "restart_watcher: sets Off state then tokio::spawn(run_watcher)"

affects:
  - 03-03-wiring
  - 04-cross-references

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "ReindexResult enum: HashSkip/Reindexed/Removed/ReadError — typed outcome for single-file re-index"
    - "Parse-before-lock: read lock (hash check) → drop → parse → write lock (update_file)"
    - "ENOENT-as-remove: NotFound IO error maps to remove_file, not panic or skip"
    - "std::sync::mpsc for notify callback (not tokio channel — notify runs on its own OS thread)"
    - "Supervision loop: start_watcher → event loop → restart with 1s backoff → Degraded after 3 failures"
    - "WatcherHandle owns Debouncer — dropping it stops the OS watcher automatically"
    - "normalize_event_path tries original repo_root first, falls back to \\\\?\\ stripped root"

key-files:
  created: []
  modified:
    - "src/watcher/mod.rs — full watcher implementation: 640 lines, all 9 required functions, 24 unit tests"

key-decisions:
  - "[Phase 03-02] ReindexResult is a local enum in watcher/mod.rs — caller can pattern match without unwrapping bool; enables future telemetry per outcome type"
  - "[Phase 03-02] std::sync::mpsc (not tokio::sync::mpsc) for notify callback — notify's debouncer runs on its own OS thread, tokio channels require async context"
  - "[Phase 03-02] ENOENT handled at maybe_reindex boundary — watcher never panics on file deletion, consistent with the file system's actual state"
  - "[Phase 03-02] normalize_event_path tries both original and stripped root on strip_prefix failure — handles mixed \\\\?\\ scenarios where watcher and index may disagree on path format"

patterns-established:
  - "Pattern: Lock discipline — READ lock for hash comparison (dropped), WRITE lock for mutation (minimal scope)"
  - "Pattern: process_events is pure (no async) — can be unit tested without a tokio runtime"
  - "Pattern: WatcherHandle lifetime manages OS watcher — no explicit stop needed, drop is cleanup"

requirements-completed: [FRSH-01, FRSH-06]

# Metrics
duration: 4min
completed: 2026-03-10
---

# Phase 3 Plan 02: Watcher Core — Event Processing and Lifecycle Summary

**Content-hash-gated file watcher using notify-debouncer-full: path normalization (Windows UNC), hash skip, ENOENT-as-remove, parse-before-lock, and supervision loop with 1s backoff and Degraded-after-3-failures**

## Performance

- **Duration:** 4 min
- **Started:** 2026-03-10T17:31:13Z
- **Completed:** 2026-03-10T17:34:58Z
- **Tasks:** 2
- **Files modified:** 1

## Accomplishments

- `maybe_reindex` implements hash-skip (skips tree-sitter entirely on unchanged content), ENOENT→remove_file, and correct lock discipline (never holds write lock during parse)
- `normalize_event_path` handles Windows `\\?\` extended-length paths, strip_prefix with fallback, and backslash→slash normalization for MSYS/Unix-style paths
- `run_watcher` is a complete supervision loop: starts watcher, processes events, restarts with 1s backoff on failure, enters Degraded state after 3 consecutive failures
- All 24 watcher unit tests pass; 148 total lib tests pass with zero regressions

## Task Commits

Each task was committed atomically:

1. **Task 1 + Task 2: Watcher core implementation (both tasks in single commit)** - `7028a3b` (feat)

**Plan metadata:** (pending)

## Files Created/Modified

- `src/watcher/mod.rs` — Full watcher: normalize_event_path, supported_language, is_relevant_event, maybe_reindex (ReindexResult), WatcherHandle, start_watcher, process_events, run_watcher, restart_watcher; 640 lines, 24 unit tests

## Decisions Made

- `ReindexResult` enum used instead of bool return from maybe_reindex — typed outcomes enable future per-outcome telemetry without API changes
- `std::sync::mpsc` channel chosen over `tokio::sync::mpsc` for notify callback — notify's debouncer thread is a native OS thread, not a tokio task; tokio channels would require `block_on` which is not safe in async context
- ENOENT handled at `maybe_reindex` boundary rather than `process_events` — keeps the event processing function simple and makes the ENOENT→remove_file invariant local to the hash-check function

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Fixed deprecated watcher() API in notify-debouncer-full 0.7**
- **Found during:** Task 2 (start_watcher implementation)
- **Issue:** Plan specified `debouncer.watcher().watch(repo_root, ...)` but `watcher()` is deprecated in 0.7.0 — `Debouncer` now implements `watch()` directly
- **Fix:** Called `debouncer.watch(repo_root, RecursiveMode::Recursive)` directly without `.watcher()` intermediary
- **Files modified:** src/watcher/mod.rs
- **Verification:** cargo check zero errors, 148 tests pass
- **Committed in:** 7028a3b (task commit)

**2. [Rule 3 - Blocking] Fixed RecommendedWatcher import — re-exported from notify, not notify-debouncer-full**
- **Found during:** Task 2 (WatcherHandle type signature)
- **Issue:** `notify_debouncer_full::RecommendedWatcher` is private — only re-exported from `notify`
- **Fix:** Used `notify::RecommendedWatcher as NotifyRecommendedWatcher` in imports
- **Files modified:** src/watcher/mod.rs
- **Verification:** cargo check clean
- **Committed in:** 7028a3b (task commit)

---

**Total deviations:** 2 auto-fixed (both Rule 3 — blocking)
**Impact on plan:** Both auto-fixes were compile errors discovered immediately; no scope changes.

## Issues Encountered

None beyond the two blocking import issues documented above.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- Plan 03 (wiring) can call `run_watcher(repo_root, shared, watcher_info)` via `tokio::spawn` from main.rs
- `restart_watcher(repo_root, shared, watcher_info)` is ready for index_folder tool integration
- All lock discipline invariants are in place — no read/write lock inversion risk
- Windows UNC path normalization covers the `ReadDirectoryChangesW` path format issue flagged in STATE.md blockers

## Self-Check: PASSED

- `src/watcher/mod.rs` exists: YES
- `03-02-SUMMARY.md` exists: YES
- Commit `7028a3b` exists: YES

---
*Phase: 03-file-watcher-freshness*
*Completed: 2026-03-10*
