---
doc_type: task
task_id: 39
title: Phase 1 find dependents read view capture
status: done
sprint: tokenizor-upgrade-foundation
parent_plan: 04-P-phase-plan.md
prev_task: 38-T-phase1-health-stats-format-after-capture.md
next_task: 
created: 2026-03-11
updated: 2026-03-11
---
# Task 39: Phase 1 Find Dependents Read View Capture

## Objective

- move `find_dependents` to the capture-then-format pattern so dependent-file grouping and line rendering happen after the live-index read guard is released

## Why This Exists

- after task 38, the remaining borrowed formatter-held read paths are the xref/context family
- `find_dependents` is the smallest of those paths because it only needs grouped importer files, import-line text, and reference-kind labels
- this makes it a safer first xref migration than `find_references` or `get_context_bundle`

## Read Before Work

- [24-T-phase1-shared-search-module-text-symbol.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/24-T-phase1-shared-search-module-text-symbol.md)
- [38-T-phase1-health-stats-format-after-capture.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/38-T-phase1-health-stats-format-after-capture.md)

## Expected Touch Points

- `src/protocol/tools.rs`
- `src/protocol/format.rs`
- `src/live_index/`

## Deliverable

- an owned grouped dependents view or equivalent capture path for `find_dependents`, with focused regression coverage

## Done When

- `find_dependents` captures the grouped importer data it needs under a short read lock and formats after the guard is released
- current public output remains unchanged
- focused tests cover parity or the migrated tool path

## Completion Notes

- migrated `find_dependents` off borrowed `&LiveIndex` formatting by adding `LiveIndex::capture_find_dependents_view()`
- added a grouped display-oriented dependents view in `src/live_index/query.rs` that captures importer file paths, line numbers, line text, and kind labels under the read lock
- updated `src/protocol/tools.rs` to capture the owned grouped view under the guard and then call `format::find_dependents_result_view()` after the guard is released
- kept `format::find_dependents_result()` as a compatibility wrapper that delegates through the same owned-view path, preserving current public output
- added focused helper and parity coverage, then reran `cargo test find_dependents`

## Carry Forward To Next Task

Next task:

- `TBD`

Carry forward:

- the grouped dependents view worked cleanly as the first xref-family capture shape
- `find_references` is the next likely migration, but it will need richer per-hit context than the dependents view
- `get_context_bundle` remains the heaviest remaining borrowed formatter-held read path

Open points:

- OPEN: decide whether to migrate `find_references` next or stop at this checkpoint before the richer multi-section context work
