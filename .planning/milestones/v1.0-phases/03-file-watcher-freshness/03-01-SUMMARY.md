---
phase: 03-file-watcher-freshness
plan: 01
subsystem: live_index
tags: [rust, notify, debouncer, file-watcher, health-stats, circuit-breaker]

# Dependency graph
requires:
  - phase: 02-mcp-tools-v1-parity
    provides: LiveIndex with health_stats, format::health_report, all 10 MCP tools

provides:
  - "update_file/add_file/remove_file on LiveIndex (single-file incremental mutation)"
  - "WatcherState enum (Active/Degraded/Off) in src/watcher/mod.rs"
  - "WatcherInfo struct with events_processed, last_event_at, debounce_window_ms"
  - "BurstTracker with adaptive debounce (200ms base, 500ms burst, 5s quiet reset)"
  - "HealthStats extended with 4 watcher fields"
  - "health_stats_with_watcher() for populating HealthStats from live WatcherInfo"
  - "health_report dynamically renders Active/Degraded/Off watcher state"
  - "notify = 8 and notify-debouncer-full = 0.7 declared in Cargo.toml"

affects:
  - 03-02-watcher-core
  - 03-03-wiring

# Tech tracking
tech-stack:
  added:
    - "notify = 8 (cross-platform file system event notification)"
    - "notify-debouncer-full = 0.7 (debounced FS events with timing control)"
    - "tokio time feature (for watcher backoff sleep in Plan 02)"
  patterns:
    - "Single-file mutation methods on LiveIndex (update_file/add_file/remove_file)"
    - "Timestamp update on mutation: loaded_at_system = SystemTime::now() on change"
    - "No-op remove: remove_file only updates timestamp if path was present"
    - "BurstTracker: sliding 200ms window, burst >3 events extends to 500ms, 5s quiet resets"
    - "HealthStats extended with watcher fields; health_stats() returns safe Off defaults"

key-files:
  created:
    - "src/watcher/mod.rs — WatcherState, WatcherInfo, BurstTracker types and tests"
  modified:
    - "src/live_index/store.rs — update_file, add_file, remove_file methods + 6 unit tests"
    - "src/live_index/query.rs — HealthStats watcher fields + health_stats_with_watcher()"
    - "src/protocol/format.rs — dynamic watcher display in health_report"
    - "src/lib.rs — pub mod watcher;"
    - "Cargo.toml — notify, notify-debouncer-full dependencies; tokio time feature"

key-decisions:
  - "[Phase 03-01] WatcherState is a separate enum in src/watcher/mod.rs, not nested in HealthStats — allows Plan 02 to import it independently of health concerns"
  - "[Phase 03-01] health_stats() always returns safe Off defaults — health_report is never broken without a watcher"
  - "[Phase 03-01] health_stats_with_watcher() is an additive method, not a replacement — callers choose which variant to use at the call site"
  - "[Phase 03-01] remove_file only updates loaded_at_system if the path was present — prevents spurious timestamp churn on phantom events"

patterns-established:
  - "Pattern: Single-file mutation does NOT reset the full index — it patches loaded_at_system only"
  - "Pattern: BurstTracker window starts fresh after >200ms quiet gap — prevents false burst detection"
  - "Pattern: effective_debounce_ms() checks last_event_at.elapsed() first — quiet reset trumps burst state"

requirements-completed: [FRSH-02, FRSH-03, FRSH-04, FRSH-05, RELY-03]

# Metrics
duration: 5min
completed: 2026-03-10
---

# Phase 3 Plan 01: Watcher Contracts and LiveIndex Mutation API Summary

**Single-file mutation API (update/add/remove) on LiveIndex, WatcherState/WatcherInfo/BurstTracker type stubs, extended HealthStats with dynamic watcher display, and notify crate declarations — all interface contracts Plan 02 (watcher core) needs**

## Performance

- **Duration:** 5 min
- **Started:** 2026-03-10T17:19:15Z
- **Completed:** 2026-03-10T17:24:30Z
- **Tasks:** 2
- **Files modified:** 6 (1 created)

## Accomplishments

- LiveIndex has update_file, add_file, remove_file for single-file incremental mutations without full reload
- WatcherState, WatcherInfo, BurstTracker types defined in src/watcher/mod.rs ready for Plan 02 import
- HealthStats extended with 4 watcher fields; health_stats() returns safe Off defaults; health_stats_with_watcher() provides caller-controlled watcher population
- health_report dynamically renders Active/Degraded/Off watcher state with event counts and timestamps
- All 135 lib tests pass, cargo check zero warnings

## Task Commits

Each task was committed atomically:

1. **Task 1: LiveIndex single-file mutation methods + watcher type stubs** - `52f7cf2` (feat)
2. **Task 2: Extended HealthStats with watcher fields** - `50e25a1` (feat)

## Files Created/Modified

- `src/watcher/mod.rs` — WatcherState enum, WatcherInfo struct, BurstTracker with adaptive debounce; 7 unit tests
- `src/live_index/store.rs` — update_file, add_file, remove_file methods on LiveIndex; 6 unit tests
- `src/live_index/query.rs` — HealthStats extended with 4 watcher fields; health_stats_with_watcher(); 2 unit tests
- `src/protocol/format.rs` — dynamic watcher display in health_report (Active/Degraded/Off); 2 unit tests
- `src/lib.rs` — pub mod watcher; declaration
- `Cargo.toml` — notify = "8", notify-debouncer-full = "0.7", tokio time feature

## Decisions Made

- WatcherState kept separate from HealthStats — allows Plan 02 to import it without the health module
- health_stats() always returns Off defaults — health_report remains correct with no active watcher
- remove_file no-ops silently without timestamp update when path not present — prevents churn on phantom events

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered

None.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- All interface contracts are established: Plan 02 (watcher core) can import WatcherState, WatcherInfo, BurstTracker and call update_file/add_file/remove_file without any additional type definitions
- Plan 03 (wiring) can display watcher state through health_report by calling health_stats_with_watcher()
- notify and notify-debouncer-full are declared and compiled — no blocking dependency issues

---
*Phase: 03-file-watcher-freshness*
*Completed: 2026-03-10*
