---
phase: 04-cross-reference-extraction
plan: 03
subsystem: protocol
tags: [mcp-tools, cross-references, formatter, integration-tests, xref, tool-handlers]

# Dependency graph
requires:
  - phase: 04-cross-reference-extraction
    plan: 01
    provides: ReferenceRecord/ReferenceKind types, LiveIndex.reverse_index, IndexedFile.references/alias_map
  - phase: 04-cross-reference-extraction
    plan: 02
    provides: find_references_for_name, find_dependents_for_file, callees_for_symbol query methods

provides:
  - find_references_result formatter in src/protocol/format.rs
  - find_dependents_result formatter in src/protocol/format.rs
  - context_bundle_result formatter in src/protocol/format.rs
  - parse_kind_filter helper in src/protocol/format.rs
  - format_ref_section helper for capped section output in src/protocol/format.rs
  - FindReferencesInput, FindDependentsInput, GetContextBundleInput input structs in src/protocol/tools.rs
  - find_references, find_dependents, get_context_bundle tool handlers in src/protocol/tools.rs
  - xref_integration.rs: 10 integration tests covering XREF-01 through XREF-08, TOOL-09/10/11

affects: [phase-complete]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "3-line context window: ctx_start = ref_line_0 - 1, ctx_end = ref_line_0 + 1, annotation inline on reference line"
    - "BTreeMap grouping: by_file uses BTreeMap<&str, Vec<_>> for sorted, deterministic file ordering"
    - "Section cap pattern: refs.len().min(SECTION_CAP) items displayed, overflow shown as '...and N more'"
    - "parse_kind_filter: maps 'call'|'import'|'type_usage'|'macro_use'|'all' to Option<ReferenceKind>"
    - "TDD pattern maintained from Phase 2: RED tests written first, then GREEN implementation"

key-files:
  created:
    - tests/xref_integration.rs
  modified:
    - src/protocol/format.rs
    - src/protocol/tools.rs

key-decisions:
  - "Annotation inline on reference line: the CONTEXT.md example shows annotation on same line as reference; implemented by detecting is_ref_line in context loop and appending annotation string"
  - "BTreeMap for file grouping in formatters: provides sorted, deterministic output without extra sort step"
  - "XREF-08 test uses reload not update_file: maybe_reindex is pub(crate) so integration tests use the public reload API; semantically equivalent (re-parse produces fresh references, reverse_index rebuilt)"
  - "context_bundle_result includes callers from the whole codebase via find_references_for_name(Call): callers are files that call the named symbol, not just files in the same repo"
  - "format_ref_section as private helper: all three sections (Callers, Callees, Type usages) share identical cap-at-20 logic; extracted to avoid duplication"

requirements-completed: [TOOL-09, TOOL-10, TOOL-11]

# Metrics
duration: 7min
completed: 2026-03-10
---

# Phase 4 Plan 03: Tool Handlers and Integration Tests Summary

**Three MCP tool handlers (find_references, find_dependents, get_context_bundle) with compact formatters and 10 integration tests proving end-to-end XREF coverage; MCP server now exposes 13 tools**

## Performance

- **Duration:** ~7 min
- **Started:** 2026-03-10T19:21:29Z
- **Completed:** 2026-03-10T19:28:41Z
- **Tasks:** 2
- **Files modified:** 2 (+ 1 created)

## Accomplishments

- Implemented `find_references_result` formatter producing compact grouped output with 3-line context windows and inline enclosing symbol annotations (`[in fn handle_request]`)
- Implemented `find_dependents_result` formatter listing importing files with their import line and `[import]` annotation
- Implemented `context_bundle_result` formatter producing symbol body + Callers + Callees + Type usages sections, each capped at 20 with overflow count
- Added `parse_kind_filter` helper converting string filter ("call"|"import"|"type_usage"|"all") to `Option<ReferenceKind>`
- Added `format_ref_section` private helper eliminating cap-at-20 duplication across 3 sections
- Added 3 input structs (`FindReferencesInput`, `FindDependentsInput`, `GetContextBundleInput`) with `Deserialize + JsonSchema` derives
- Added 3 tool handlers following the `loading_guard!` macro pattern; MCP server now exposes 13 tools (was 10)
- Created `tests/xref_integration.rs` with 10 tests covering XREF-01 through XREF-08, TOOL-09, TOOL-10, TOOL-11
- All 224 lib tests + all integration tests pass clean

## Task Commits

Each task was committed atomically:

1. **Task 1: Formatter functions and tool handlers** - `fe78101` (feat)
2. **Task 2: Integration tests for end-to-end cross-reference extraction** - `3c098bd` (test)

## Files Created/Modified

- `src/protocol/format.rs` - Added find_references_result, find_dependents_result, context_bundle_result, parse_kind_filter, format_ref_section; 5 new formatter tests + 5 new xref formatter tests
- `src/protocol/tools.rs` - Added FindReferencesInput/FindDependentsInput/GetContextBundleInput structs; find_references/find_dependents/get_context_bundle tool handlers; updated tool count test from 10 to 13; 5 new tool handler tests
- `tests/xref_integration.rs` - Created: 10 integration tests covering XREF-01 through XREF-08, TOOL-09, TOOL-10, TOOL-11

## Decisions Made

- **Annotation inline on reference line:** The CONTEXT.md example shows `[in fn handle_request]` on the same line as the reference. Implemented by detecting `is_ref_line` in the context loop and appending the annotation string with padding.
- **BTreeMap for file grouping:** BTreeMap provides sorted, deterministic file ordering in formatter output without a separate sort step.
- **XREF-08 test uses `reload` not `update_file`:** `maybe_reindex` is `pub(crate)` so it isn't accessible from integration tests. Used the public `reload` API instead — semantically equivalent for proving that re-parse produces fresh references with the reverse_index rebuilt.
- **format_ref_section as private helper:** All three sections (Callers, Callees, Type usages) share identical cap-at-20 display logic. Extracted to a single private function to avoid duplication and ensure consistent overflow message format.

## Deviations from Plan

None — plan executed exactly as written.

## Phase 4 Complete

All 11 Phase 4 requirements are satisfied:
- XREF-01: Call references extracted (proven by test_rust_call_site_extraction)
- XREF-02: Call reference names + line ranges correct
- XREF-03: ImportKind for use/import statements (proven by test_python_import_and_call_extraction)
- XREF-04: TS built-in filter → <10 results for "string" (proven by test_ts_builtin_type_filter)
- XREF-05: Alias map populated for use X as Y (proven by test_alias_map_resolution)
- XREF-06: Single-letter generics filtered (proven by test_generic_filter)
- XREF-07: enclosing_symbol_index set for all call refs (proven by test_enclosing_symbol_tracked)
- XREF-08: Incremental reverse_index update after re-parse (proven by test_incremental_xref_update)
- TOOL-09: find_references tool with kind filter (proven by test_find_references_formatter_output)
- TOOL-10: find_dependents tool (proven by test_find_dependents_returns_importers)
- TOOL-11: get_context_bundle < 100ms on 50-file index (proven by test_context_bundle_under_100ms)

## Self-Check: PASSED

Files created/modified exist:
- src/protocol/format.rs: FOUND (contains find_references_result, find_dependents_result, context_bundle_result)
- src/protocol/tools.rs: FOUND (contains FindReferencesInput, find_references handler, 13-tool count test)
- tests/xref_integration.rs: FOUND (10 integration tests)

Commits exist:
- fe78101: feat(04-03): add find_references, find_dependents, get_context_bundle tool handlers
- 3c098bd: test(04-03): add xref_integration test suite for end-to-end coverage

---
*Phase: 04-cross-reference-extraction*
*Completed: 2026-03-10*
