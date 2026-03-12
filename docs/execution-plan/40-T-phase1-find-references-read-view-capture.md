---
doc_type: task
task_id: 40
title: Phase 1 find references read view capture
status: done
sprint: tokenizor-upgrade-foundation
parent_plan: 04-P-phase-plan.md
prev_task: 39-T-phase1-find-dependents-read-view-capture.md
next_task: 41-T-phase1-context-bundle-read-view-capture.md
created: 2026-03-11
updated: 2026-03-11
---
# Task 40: Phase 1 Find References Read View Capture

## Objective

- move `find_references` to the capture-then-format pattern so grouped hits, per-hit context lines, and enclosing-symbol annotations are rendered after the live-index read guard is released

## Why This Exists

- after task 39, `find_references` is the next xref path still doing richer output assembly against a borrowed `&LiveIndex`
- it is more complex than `find_dependents` because each hit includes a multi-line context window and optional enclosing-symbol decoration
- landing this separately de-risks the remaining `get_context_bundle` migration by establishing the owned view shape for grouped reference hits

## Read Before Work

- [24-T-phase1-shared-search-module-text-symbol.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/24-T-phase1-shared-search-module-text-symbol.md)
- [39-T-phase1-find-dependents-read-view-capture.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/39-T-phase1-find-dependents-read-view-capture.md)

## Expected Touch Points

- `src/protocol/tools.rs`
- `src/protocol/format.rs`
- `src/live_index/`

## Deliverable

- an owned grouped references view or equivalent capture path for `find_references`, with focused regression coverage

## Done When

- `find_references` captures grouped hit data, context lines, and enclosing-symbol labels under a short read lock and formats after the guard is released
- current public output remains unchanged
- focused tests cover parity or the migrated tool path

## Completion Notes

- migrated `find_references` off borrowed `&LiveIndex` formatting by adding `LiveIndex::capture_find_references_view()`
- added grouped display-oriented reference-hit views in `src/live_index/query.rs` that capture file grouping, 3-line context windows, and optional enclosing-symbol annotations under the read lock
- updated `src/protocol/tools.rs` to capture the owned grouped references view under the guard and then call `format::find_references_result_view()` after the guard is released
- kept `format::find_references_result()` as a compatibility wrapper that delegates through the same owned-view path, preserving current public output
- added focused helper and parity coverage, removed a now-dead formatter kind-filter helper, and reran `cargo test find_references`

## Carry Forward To Next Task

Next task:

- `41-T-phase1-context-bundle-read-view-capture.md`

Carry forward:

- the grouped xref capture pattern from task 39 scaled cleanly to richer per-hit reference context
- `get_context_bundle` is still the heaviest remaining borrowed formatter-backed read path and should be the next migration
- `find_references` needed its own hit/context view shape instead of directly reusing the simpler dependents view

Open points:

- OPEN: decide whether `get_context_bundle` should compose the new grouped reference views internally or define a separate section-oriented owned bundle view
