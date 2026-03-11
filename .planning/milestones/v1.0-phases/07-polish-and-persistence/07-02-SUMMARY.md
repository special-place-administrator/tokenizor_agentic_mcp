---
phase: 07-polish-and-persistence
plan: 02
subsystem: search
tags: [trigram, text-search, symbol-search, file-tree, ranking]

# Dependency graph
requires:
  - phase: 06-hook-enrichment-integration
    provides: LiveIndex with full mutation API (update_file, remove_file, reload)
provides:
  - TrigramIndex module with posting list AND-intersection search
  - trigram_index field integrated into LiveIndex at all mutation paths
  - search_text_result accelerated via TrigramIndex candidate selection
  - search_symbols_result with 3-tier scored ranking (Exact > Prefix > Substring) and box-drawing tier headers
  - file_tree formatter with depth-limited source tree and per-file symbol counts
  - get_file_tree MCP tool handler (14th tool in server)
affects: [phase-08, any phase that adds text or symbol search features]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "TrigramIndex: HashMap<[u8;3], Vec<u32>> posting lists with sorted IDs for binary-search intersection"
    - "3-tier symbol ranking: Exact=0 / Prefix=1 / Substring=2 with MatchTier enum deriving Ord"
    - "file_tree recursive tree builder: BTreeMap for deterministic dir ordering, depth-collapse for dirs beyond max_depth"

key-files:
  created:
    - src/live_index/trigram.rs
  modified:
    - src/live_index/mod.rs
    - src/live_index/store.rs
    - src/live_index/query.rs
    - src/protocol/format.rs
    - src/protocol/tools.rs
    - src/sidecar/handlers.rs
    - src/sidecar/mod.rs
    - src/watcher/mod.rs

key-decisions:
  - "TrigramIndex stores sorted posting lists (Vec<u32>) so binary_search works for O(log n) ID lookup during intersection — no HashSet needed"
  - "Intersection starts with shortest posting list for early pruning of candidate sets"
  - "Trigram extraction is case-insensitive (to_ascii_lowercase before windowing) so search is case-insensitive without extra cost"
  - "search_text_result delegates short-query fallback entirely to TrigramIndex::search — no special-casing in the caller"
  - "MatchTier enum derives Ord with explicit discriminant values (0/1/2) so sort key is (tier, tiebreak, name)"
  - "file_tree uses nested helper fns (build_lines, count_files, count_dirs) co-located in the function scope — no module-level helpers needed for this single caller"
  - "get_file_tree is the 14th MCP tool — updated test_exactly_13_tools_registered to test_exactly_14_tools_registered"

patterns-established:
  - "Trigram false-positive elimination: after posting list intersection, verify each candidate with byte-level contains check"
  - "Tier headers use Unicode box-drawing chars U+2500 (─): '\u{2500}\u{2500} {label} \u{2500}\u{2500}'"
  - "Empty tier sections are omitted — don't push the header if that tier has no matches"

requirements-completed: [PLSH-01, PLSH-02, PLSH-03]

# Metrics
duration: 10min
completed: 2026-03-11
---

# Phase 7 Plan 02: Polish and Persistence — Trigram Search, Scored Rankings, File Tree Summary

**Trigram posting list index accelerates search_text, 3-tier Exact/Prefix/Substring ranking improves search_symbols UX, and a new get_file_tree MCP tool provides depth-limited source navigation with symbol counts**

## Performance

- **Duration:** 10 min
- **Started:** 2026-03-11T00:01:25Z
- **Completed:** 2026-03-11T00:11:52Z
- **Tasks:** 2
- **Files modified:** 9 (1 created: trigram.rs)

## Accomplishments

- Created `src/live_index/trigram.rs` with full TrigramIndex: posting list map, AND-intersection search, linear fallback for short queries, incremental update/remove
- Integrated TrigramIndex into LiveIndex at all mutation paths (load, empty, reload, update_file, add_file, remove_file)
- search_text_result now uses trigram candidate selection before line scanning — eliminates O(n) full scan for 3+ char queries
- search_symbols_result now returns 3-tier scored results (Exact/Prefix/Substring) with box-drawing tier headers per CONTEXT.md decision
- Added `file_tree(index, path, depth)` formatter and `get_file_tree` MCP tool handler — 14th tool in server

## Task Commits

Each task was committed atomically:

1. **Task 1: Create trigram index module and scored search logic** - `7ee7e94` (feat)
2. **Task 2: Wire trigram search, scored ranking, and file tree tool** - `3abdb3b` (feat)

**Plan metadata:** (in final commit below)

_Note: TDD tasks had tests-first approach; tests compiled and passed on first run after implementation_

## Files Created/Modified

- `src/live_index/trigram.rs` - TrigramIndex: posting list index with build/search/update/remove + 14 unit tests
- `src/live_index/mod.rs` - Added `pub mod trigram`
- `src/live_index/store.rs` - Added `trigram_index: TrigramIndex` field, integrated at all mutation paths
- `src/live_index/query.rs` - Updated test helpers to include trigram_index field
- `src/protocol/format.rs` - search_symbols_result replaced with scored 3-tier ranking; search_text_result uses TrigramIndex; added file_tree function; updated test helpers; added 14 new tests
- `src/protocol/tools.rs` - Added GetFileTreeInput, get_file_tree handler; updated tool count test to 14; added handler tests
- `src/sidecar/handlers.rs` - Updated LiveIndex struct literal in test helper
- `src/sidecar/mod.rs` - Updated LiveIndex struct literal in test helper
- `src/watcher/mod.rs` - Updated LiveIndex struct literals in test helpers (2 places)

## Decisions Made

- TrigramIndex stores sorted posting lists (Vec<u32>) so binary_search works for O(log n) ID lookup during intersection — no HashSet needed
- Intersection starts with shortest posting list for early pruning
- Trigram extraction is case-insensitive so search is case-insensitive without extra conversion at search time
- MatchTier enum derives Ord with explicit discriminant values (0/1/2)
- file_tree uses nested helper functions co-located in the function scope
- get_file_tree is now the 14th MCP tool

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 2 - Missing Critical] Added trigram_index field to all LiveIndex struct literal initializations across test helpers**
- **Found during:** Task 1 (TrigramIndex integration into LiveIndex)
- **Issue:** Adding a new non-optional field to LiveIndex caused compilation failures in 9 test helpers across 7 files (query.rs, format.rs, tools.rs, sidecar/mod.rs, sidecar/handlers.rs, watcher/mod.rs)
- **Fix:** Updated each LiveIndex struct literal to include `trigram_index: TrigramIndex::new()` or `TrigramIndex::build_from_files(&files_map)`
- **Files modified:** all files listed above
- **Verification:** All 354 lib tests pass
- **Committed in:** 7ee7e94 (Task 1 commit)

---

**Total deviations:** 1 auto-fixed (Rule 2 - required for compilation correctness)
**Impact on plan:** Standard consequence of adding a new struct field. No scope creep.

## Issues Encountered

- None beyond the struct field propagation above.
- The 3 pre-existing `test_init_*` integration test failures remain unchanged (documented in STATE.md as pre-existing since Plan 06-03).

## Next Phase Readiness

- Trigram search index is live and integrated; search_text performance should be measurably faster on large repos
- search_symbols results are now more useful with tiered ordering
- get_file_tree tool is available for source navigation
- Phase 7 Plan 03 (remaining polish: performance, error messages, final validation) is ready to execute

## Self-Check: PASSED

- FOUND: src/live_index/trigram.rs
- FOUND: .planning/phases/07-polish-and-persistence/07-02-SUMMARY.md
- FOUND: commit 7ee7e94 (Task 1)
- FOUND: commit 3abdb3b (Task 2)
- 354 lib tests pass, 0 failures

---
*Phase: 07-polish-and-persistence*
*Completed: 2026-03-11*
