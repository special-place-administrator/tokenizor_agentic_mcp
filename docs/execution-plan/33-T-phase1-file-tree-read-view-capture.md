---
doc_type: task
task_id: 33
title: Phase 1 file tree read view capture
status: done
sprint: tokenizor-upgrade-foundation
parent_plan: 04-P-phase-plan.md
prev_task: 32-T-phase1-search-result-format-after-capture.md
next_task: 
created: 2026-03-11
updated: 2026-03-11
---
# Task 33: Phase 1 File Tree Read View Capture

## Objective

- move `get_file_tree` to the capture-then-format pattern so whole-index tree rendering happens after the live-index read guard is released

## Why This Exists

- tasks 30 through 32 already narrowed repo-outline, file-outline, and search-result formatting
- `get_file_tree` is the next broad formatter-backed whole-index path, but it only needs path, language, and symbol-count metadata, not file bytes or deeper symbol bodies
- this makes it a safer next migration than `get_symbol` or xref/context formatters

## Read Before Work

- [26-R-phase1-live-state-snapshot-research.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/26-R-phase1-live-state-snapshot-research.md)
- [30-T-phase1-query-read-view-capture.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/30-T-phase1-query-read-view-capture.md)
- [32-T-phase1-search-result-format-after-capture.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/32-T-phase1-search-result-format-after-capture.md)

## Expected Touch Points

- `src/protocol/tools.rs`
- `src/protocol/format.rs`
- `src/live_index/`

## Deliverable

- an owned metadata view or equivalent capture path for `get_file_tree`, with focused regression coverage

## Done When

- `get_file_tree` captures the metadata it needs under a short read lock and formats after the guard is dropped
- public output remains unchanged
- focused tests cover parity or the migrated helper path

## Completion Notes

- migrated `get_file_tree` off borrowed `&LiveIndex` formatting by reusing `LiveIndex::capture_repo_outline_view()` as the captured whole-index metadata source
- added `format::file_tree_view()` so tree rendering can operate on owned path/language/symbol-count metadata after the guard is released
- updated `src/protocol/tools.rs` to capture the metadata view under the guard and then call `format::file_tree_view()` after the guard is released
- kept `format::file_tree()` as a compatibility wrapper that delegates through the same captured metadata path, preserving output shape
- added focused parity coverage and reran `cargo test file_tree`

## Carry Forward To Next Task

Next task:

- `TBD`

Carry forward:

- sharing the captured repo metadata view between repo-outline and file-tree worked cleanly for the current behavior-preserving slice
- the next remaining formatter-held read paths are more likely to need dedicated views, especially `get_symbol` and `get_symbols`
- xref/context formatters still remain a later step because they require owned hit/context structures, not just file metadata

Open points:

- OPEN: decide whether the next slice should start the symbol-detail family with a dedicated owned symbol/body view or continue elsewhere on the remaining borrowed protocol surface
