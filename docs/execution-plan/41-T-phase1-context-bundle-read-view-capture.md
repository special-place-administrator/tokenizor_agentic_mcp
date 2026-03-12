---
doc_type: task
task_id: 41
title: Phase 1 context bundle read view capture
status: done
sprint: tokenizor-upgrade-foundation
parent_plan: 04-P-phase-plan.md
prev_task: 40-T-phase1-find-references-read-view-capture.md
next_task: 
created: 2026-03-11
updated: 2026-03-11
---
# Task 41: Phase 1 Context Bundle Read View Capture

## Objective

- move `get_context_bundle` to the capture-then-format pattern so symbol definition details, callers, callees, and type-usage sections are assembled into an owned view under a short read lock and rendered after release

## Why This Exists

- after task 40, `get_context_bundle` is the heaviest remaining formatter-backed read path in the query surface
- it combines single-symbol detail with multiple cross-reference sections, so it is the last major place where borrowed formatting can still expand lock hold time
- finishing it should leave the query surface much closer to a consistent owned-view model before any larger immutable-snapshot work

## Read Before Work

- [24-T-phase1-shared-search-module-text-symbol.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/24-T-phase1-shared-search-module-text-symbol.md)
- [40-T-phase1-find-references-read-view-capture.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/40-T-phase1-find-references-read-view-capture.md)

## Expected Touch Points

- `src/protocol/tools.rs`
- `src/protocol/format.rs`
- `src/live_index/`

## Deliverable

- an owned context-bundle view or equivalent capture path for `get_context_bundle`, with focused regression coverage

## Done When

- `get_context_bundle` captures all section data it needs under a short read lock and formats after the guard is released
- current public output remains unchanged
- focused tests cover parity or the migrated tool path

## Completion Notes

- migrated `get_context_bundle` off borrowed `&LiveIndex` formatting by adding `LiveIndex::capture_context_bundle_view()`
- added a dedicated owned context-bundle result shape in `src/live_index/query.rs` with explicit `file missing`, `symbol missing`, and `found` variants plus capped section-entry views for callers, callees, and type usages
- updated `src/protocol/tools.rs` to capture the owned context bundle under the guard and then call `format::context_bundle_result_view()` after the guard is released
- kept `format::context_bundle_result()` as a compatibility wrapper that delegates through the same owned-view path, preserving current public output
- added focused capture, formatter parity, and tool-path coverage, then reran `cargo test context_bundle` including the existing `test_context_bundle_under_100ms` guardrail

## Carry Forward To Next Task

Next task:

- `TBD`

Carry forward:

- the local query/tool paths targeted in this phase now capture owned views under the read lock or use shared owned search results before formatting
- `get_context_bundle` needed its own lighter section-oriented owned view instead of reusing the richer grouped hit-context shape from `find_references`
- remaining context-heavy helpers in `src/protocol/tools.rs`, such as `get_symbol_context` and `get_file_context`, are sidecar-backed paths rather than local borrowed-formatter `LiveIndex` reads

Open points:

- OPEN: decide whether the next slice should be a closeout audit for wrapper-only formatter entry points or a return to the larger live-state snapshot/publication work
