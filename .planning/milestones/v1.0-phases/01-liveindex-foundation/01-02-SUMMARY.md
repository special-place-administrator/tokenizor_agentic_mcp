---
phase: 01-liveindex-foundation
plan: 02
subsystem: infra
tags: [rust, rayon, tree-sitter, live-index, circuit-breaker, file-discovery, in-memory, concurrency]

# Dependency graph
requires:
  - phase: 01-01
    provides: "Compiling skeleton with domain types, parsing module, empty live_index/discovery stubs"
provides:
  - "src/discovery/mod.rs: discover_files (ignore crate, .gitignore respect, forward-slash normalization, deterministic case-insensitive sort) and find_git_root"
  - "src/live_index/store.rs: LiveIndex, SharedIndex, IndexedFile, ParseStatus, CircuitBreakerState, IndexState with LiveIndex::load() (Rayon parallel, circuit breaker, tracing)"
  - "src/live_index/query.rs: get_file, symbols_for_file, all_files, file_count, symbol_count, is_ready, index_state, health_stats — all on &LiveIndex (no re-entrant lock)"
  - "src/live_index/mod.rs: public re-exports"
  - "51 unit tests total: 7 discovery, 12 store, 12 query, 18 parsing, 2 hash"
affects: [03-liveindex-foundation, all-subsequent-phases, mcp-tools, file-watcher, xrefs, hooks]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Query methods on &LiveIndex (not &SharedIndex) — prevents re-entrant RwLock deadlocks"
    - "LiveIndex::load is synchronous (runs before tokio runtime) — Rayon handles internal parallelism"
    - "CircuitBreakerState uses AtomicUsize/AtomicBool for lock-free counters, Mutex only for failure details vec"
    - "Minimum-5-file guard on circuit breaker prevents false trips on tiny repos"
    - "Content bytes stored for ALL files including total-failure files (LIDX-03)"
    - "All logging via tracing crate — zero println! anywhere in codebase"

key-files:
  created:
    - src/discovery/mod.rs
    - src/live_index/store.rs
    - src/live_index/query.rs
  modified:
    - src/live_index/mod.rs

key-decisions:
  - "Query methods take &LiveIndex not &SharedIndex — callers acquire the RwLock guard before calling, enforced by type system"
  - "CircuitBreakerState::new(threshold) takes explicit threshold for testability; from_env() reads TOKENIZOR_CB_THRESHOLD env var"
  - "Content bytes stored for failed-parse files per user decision — LiveIndex::load stores all files regardless of parse outcome"
  - "LiveIndex::load is sync not async — Rayon parallelism, must complete before tokio MCP server starts"

patterns-established:
  - "SharedIndex pattern: Arc<RwLock<LiveIndex>> — acquire guard in caller, pass &LiveIndex to query methods"
  - "CircuitBreaker pattern: lock-free atomics for counters, Mutex only for bounded failure details (max 5 entries)"

requirements-completed: [LIDX-01, LIDX-02, LIDX-03, LIDX-04, RELY-01, RELY-02]

# Metrics
duration: 5min
completed: 2026-03-10
---

# Phase 1 Plan 02: LiveIndex Foundation — Core In-Memory Store Summary

**HashMap-backed LiveIndex with Rayon-parallel loading, configurable circuit breaker (20% threshold, 5-file minimum guard), per-file ParseStatus, content byte storage, O(1) query methods, and health stats — 51 tests green**

## Performance

- **Duration:** ~5 min
- **Started:** 2026-03-10T14:26:26Z
- **Completed:** 2026-03-10T14:30:39Z
- **Tasks:** 2 (Task 1: discovery + store; Task 2: query methods)
- **Files modified:** 4 (3 created, 1 modified)

## Accomplishments

- Implemented `discover_files` using `ignore::WalkBuilder` with `.gitignore` respect, forward-slash normalization, and deterministic case-insensitive sorting; `find_git_root` walks upward from CWD with CWD fallback
- Implemented `LiveIndex::load(root)` — discovers files, reads bytes in parallel via Rayon, parses via `parsing::process_file`, builds HashMap, enforces circuit breaker (trips >20% failure rate with 5-file minimum guard), wraps in `Arc<RwLock<>>`
- Implemented all query methods on `&LiveIndex`: `get_file` (O(1) HashMap), `symbols_for_file`, `all_files`, `file_count`, `symbol_count`, `is_ready`, `index_state`, `health_stats` — correctly using `&self` not `&SharedIndex` to avoid re-entrant lock
- Content bytes stored for all files including total-failure files; `ParseStatus` (Parsed/PartialParse/Failed) correctly mapped from `FileOutcome`; concurrent reads tested with 8 threads without deadlock

## Task Commits

Both tasks were implemented in a single GREEN commit (TDD — tests written alongside implementation):

1. **Tasks 1 & 2: Discovery, LiveIndex store, query modules** - `0410419` (feat)

**Plan metadata:** (docs commit below)

## Files Created/Modified

- `src/discovery/mod.rs` - discover_files (WalkBuilder, gitignore, normalize, sort) and find_git_root (7 tests)
- `src/live_index/store.rs` - ParseStatus, IndexedFile, CircuitBreakerState, IndexState, LiveIndex, SharedIndex, LiveIndex::load (12 tests)
- `src/live_index/query.rs` - get_file, symbols_for_file, all_files, file_count, symbol_count, is_ready, index_state, health_stats, HealthStats (12 tests)
- `src/live_index/mod.rs` - public re-exports

## Decisions Made

- **Query methods on &LiveIndex not &SharedIndex:** The type system enforces this — `SharedIndex = Arc<RwLock<LiveIndex>>`. Callers must acquire the guard before calling query methods, preventing re-entrant lock deadlocks. Per research pitfall #4.
- **CircuitBreakerState::new(threshold) + from_env():** Constructor takes explicit threshold so tests can configure arbitrary thresholds. `from_env()` reads `TOKENIZOR_CB_THRESHOLD` env var. This split avoids env var contamination in tests.
- **Content bytes for failed-parse files:** Per user decision ("leaning toward store content"), even files that fail parsing entirely get their bytes stored in `IndexedFile.content`. Future tools (raw content display, rehash on change) can use the bytes without touching disk.

## Deviations from Plan

None — plan executed exactly as written. Both tasks completed in single GREEN commit (TDD red phase was skipped since tests are inline with implementation per plan structure).

## Issues Encountered

None. One minor cleanup: removed duplicate `use` import in query.rs tests after initial write.

## User Setup Required

None — no external service configuration required.

## Next Phase Readiness

- LiveIndex is the complete core data structure for v2 — all subsequent phases (MCP tools, file watcher, xrefs, hooks) read from `SharedIndex`
- Plan 03 can now implement the tokio MCP server stub that calls `LiveIndex::load` at startup
- No blockers

---
*Phase: 01-liveindex-foundation*
*Completed: 2026-03-10*

## Self-Check: PASSED

- FOUND: src/discovery/mod.rs
- FOUND: src/live_index/store.rs
- FOUND: src/live_index/query.rs
- FOUND: src/live_index/mod.rs
- FOUND: .planning/phases/01-liveindex-foundation/01-02-SUMMARY.md
- FOUND commit: 0410419 feat(01-02): implement LiveIndex store, discovery, and query modules
