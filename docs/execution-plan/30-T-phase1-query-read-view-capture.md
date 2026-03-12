---
doc_type: task
task_id: 30
title: Phase 1 query read view capture
status: done
sprint: tokenizor-upgrade-foundation
parent_plan: 04-P-phase-plan.md
prev_task: 29-T-phase1-persistence-lock-narrowing.md
next_task: 
created: 2026-03-11
updated: 2026-03-11
---
# Task 30: Phase 1 Query Read View Capture

## Objective

- apply the owned-view capture pattern to one small set of formatter-backed query paths so heavy formatting no longer depends on holding the live index read lock

## Why This Exists

- task 29 proved the pattern on persistence and background verification
- the same lock-breadth issue still exists in protocol query paths, especially where `tools.rs` keeps a read guard while `format.rs` performs larger output assembly

## Read Before Work

- [26-R-phase1-live-state-snapshot-research.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/26-R-phase1-live-state-snapshot-research.md)
- [29-T-phase1-persistence-lock-narrowing.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/29-T-phase1-persistence-lock-narrowing.md)

## Expected Touch Points

- `src/protocol/tools.rs`
- `src/protocol/format.rs`
- `src/live_index/`

## Deliverable

- one small owned read-view or equivalent capture path for selected formatter-backed queries, with focused regression coverage

## Done When

- at least one selected query path captures the data it needs under a short read lock and formats after the guard is dropped
- public output for the selected path remains unchanged
- focused tests cover the new view-capture helper or migrated path

## Completion Notes

- migrated `get_repo_outline` off the borrowed `&LiveIndex` formatting path by adding `LiveIndex::capture_repo_outline_view()`
- added `RepoOutlineView` and `RepoOutlineFileView` in `src/live_index/query.rs` so the tool captures only path, language, and symbol-count data under the read lock
- updated `src/protocol/tools.rs` to capture the view under the guard and then call `format::repo_outline_view()` after the guard is released
- kept `format::repo_outline()` as a compatibility wrapper that delegates through the same owned view, preserving output shape
- added focused coverage for view capture ordering/counts and output parity, then reran `cargo test repo_outline`

## Carry Forward To Next Task

Next task:

- `TBD`

Carry forward:

- whole-index `get_repo_outline` migrated cleanly with a compact owned view and no output change
- file-local formatter paths such as `get_file_outline` and `get_symbol` are likely the next low-risk migrations
- heavier symbol/text/xref formatters still need deeper view design before they can leave borrowed `&LiveIndex` formatting behind

Open points:

- OPEN: decide whether the next slice should keep expanding small file-scoped views or jump to one of the larger search/xref formatters
