---
phase: 04-cross-reference-extraction
plan: 02
subsystem: live_index
tags: [xref, cross-references, reverse-index, query, filter, alias-resolution, watcher, rust]

# Dependency graph
requires:
  - phase: 04-cross-reference-extraction
    plan: 01
    provides: ReferenceRecord/ReferenceKind types, LiveIndex.reverse_index, IndexedFile.references/alias_map, rebuild_reverse_index
  - phase: 03-file-watcher-freshness
    provides: maybe_reindex watcher pipeline that calls update_file

provides:
  - find_references_for_name query method on LiveIndex with kind filter, built-in/generic filter, alias expansion, and qualified name matching
  - find_dependents_for_file query method for heuristic import-path dependency resolution
  - callees_for_symbol query method returning Call refs enclosed in a given symbol
  - is_filtered_name helper with per-language built-in lists and single-letter generic list
  - collect_refs_for_key private helper for reverse_index resolution
  - Proof that watcher maybe_reindex pipeline carries references end-to-end (XREF-08)
  - Two new watcher unit tests proving incremental xref update

affects: [04-03, 04-cross-reference-extraction]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Qualified-vs-simple dispatch: qualified queries (contains :: or .) do a full-scan against qualified_name field; simple queries use the O(1) reverse_index"
    - "Alias expansion via collect+iterate: aliases collected as Vec<String> first to avoid re-borrowing self during mutation of results"
    - "Heuristic stem matching for dependents: import name contains file stem as a whole segment (crate::db matches src/db.rs)"

key-files:
  created: []
  modified:
    - src/live_index/query.rs
    - src/watcher/mod.rs

key-decisions:
  - "Qualified queries do a full file scan (not reverse_index): reverse_index is keyed by simple name; qualified names like Vec::new are stored under 'new', so qualified lookup must scan files and match qualified_name field"
  - "collect_refs_for_key extracted as private method: closures capturing &self and &mut results hit E0521 lifetime escape; method with explicit lifetime annotation solves this cleanly"
  - "alias expansion collects aliases before iterating results: avoids simultaneous &self borrow for files lookup and &mut results"
  - "is_filtered_name is cross-language: checks all 6 language lists so mixed-language repos are handled without per-file language detection at query time"

patterns-established:
  - "Query dispatch pattern: is_qualified check at method entry splits into scan path vs reverse_index lookup path"
  - "Incremental xref test pattern: write file -> parse into SharedIndex -> overwrite file -> maybe_reindex -> assert reverse_index consistency"

requirements-completed: [XREF-04, XREF-05, XREF-06, XREF-08]

# Metrics
duration: 6min
completed: 2026-03-10
---

# Phase 4 Plan 02: Cross-Reference Query Methods Summary

**Cross-reference query API (find_references_for_name/find_dependents_for_file/callees_for_symbol) with per-language built-in filtering, single-letter generic filtering, and alias expansion; watcher XREF-08 proven by incremental reverse_index update test**

## Performance

- **Duration:** ~6 min
- **Started:** 2026-03-10T19:10:35Z
- **Completed:** 2026-03-10T19:16:52Z
- **Tasks:** 2
- **Files modified:** 2

## Accomplishments
- Implemented `find_references_for_name` on `LiveIndex` with kind filtering, built-in type filter (6 languages), single-letter generic filter, alias expansion, and qualified name matching via `::` / `.` detection
- Implemented `find_dependents_for_file` using heuristic file-stem segment matching for import references
- Implemented `callees_for_symbol` returning Call references enclosed in a given symbol index
- Added 23 new tests in `src/live_index/query.rs` covering all specified behaviors (XREF-04, XREF-05, XREF-06)
- Verified watcher pipeline is already end-to-end connected; added 2 unit tests in `src/watcher/mod.rs` proving XREF-08 (incremental reverse_index update through maybe_reindex)
- All 210 lib tests pass (185 prior + 23 new query + 2 new watcher)

## Task Commits

Each task was committed atomically:

1. **Task 1: Cross-reference query methods with filtering and alias resolution** - `5bea7f9` (feat)
2. **Task 2: Verify watcher xref pipeline and add XREF-08 incremental update test** - `21ae69a` (feat)

## Files Created/Modified
- `src/live_index/query.rs` - Added RUST_BUILTINS/PYTHON_BUILTINS/JS_BUILTINS/TS_BUILTINS/GO_BUILTINS/JAVA_BUILTINS/SINGLE_LETTER_GENERICS constants, is_filtered_name helper, find_references_for_name, collect_refs_for_key, find_dependents_for_file, callees_for_symbol; updated test helpers to rebuild_reverse_index; added 23 tests
- `src/watcher/mod.rs` - Added test_maybe_reindex_updates_reverse_index_on_change and test_maybe_reindex_hash_skip_on_unchanged_content proving XREF-08

## Decisions Made
- **Qualified queries use full-scan not reverse_index:** The reverse index is keyed by `reference.name` (e.g. "new"), not by the qualified form "Vec::new". A qualified query must iterate all files and match against the `qualified_name` field. The reverse_index is only used for simple name lookups.
- **collect_refs_for_key as private method:** The initial implementation used a closure `collect_for_key` capturing `self` and `results` which hit E0521 (borrowed data escapes method body). Extracted to a private method with explicit lifetime `'a` which the borrow checker accepts.
- **Alias expansion collects aliases to Vec<String> first:** Iterating `self.files` to find aliases while also holding `results: &mut Vec<(&'a str, &'a ReferenceRecord)>` causes simultaneous borrow conflicts. Collecting aliases to a `Vec<String>` before the second loop resolves this.
- **is_filtered_name checks all language lists unconditionally:** This avoids needing to know which language a query is for at call time, keeps the filter fast (simple array scan), and correctly handles cross-language repos where the same name might be a built-in in one language but user-defined in another.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Unused import warnings in watcher tests**
- **Found during:** Task 2 (watcher incremental update test)
- **Issue:** Initial test helpers imported `ReferenceKind`, `LiveIndex`, and `ParseStatus` which were unused
- **Fix:** Removed unused imports from test functions
- **Files modified:** src/watcher/mod.rs
- **Verification:** `cargo test --lib watcher::` passes with no warnings
- **Committed in:** `21ae69a` (Task 2 commit)

---

**Total deviations:** 1 auto-fixed (1 bug fix — unused imports)
**Impact on plan:** Trivial cleanup. No scope creep.

## Issues Encountered
- **E0521 lifetime escape on closure:** The `collect_for_key` closure capturing both `&self` and `&mut results` caused a borrow checker error (`borrowed data escapes outside of method`). Root cause: closures don't support expressing lifetime relationships between `self` and the output references. Solved by promoting the closure to a private method `collect_refs_for_key<'a>(&'a self, ..., results: &mut Vec<(&'a str, &'a ReferenceRecord)>)`.

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- Query API complete: `find_references_for_name`, `find_dependents_for_file`, and `callees_for_symbol` are ready for MCP tool wiring in Plan 04-03
- XREF-04, XREF-05, XREF-06, XREF-08 all satisfied
- Watcher pipeline carries references end-to-end with fresh reverse_index after every `update_file`

---
*Phase: 04-cross-reference-extraction*
*Completed: 2026-03-10*
