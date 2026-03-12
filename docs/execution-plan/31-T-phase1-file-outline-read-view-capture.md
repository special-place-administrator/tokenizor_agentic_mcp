---
doc_type: task
task_id: 31
title: Phase 1 file outline read view capture
status: done
sprint: tokenizor-upgrade-foundation
parent_plan: 04-P-phase-plan.md
prev_task: 30-T-phase1-query-read-view-capture.md
next_task: 
created: 2026-03-11
updated: 2026-03-11
---
# Task 31: Phase 1 File Outline Read View Capture

## Objective

- apply the owned-view capture pattern to `get_file_outline` so file-local outline formatting no longer depends on a borrowed `&LiveIndex`

## Why This Exists

- task 30 proved the pattern on one whole-index formatter path with `get_repo_outline`
- `get_file_outline` is the next low-risk protocol formatter path because it only needs one file's path and symbol list, not broader query state

## Read Before Work

- [26-R-phase1-live-state-snapshot-research.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/26-R-phase1-live-state-snapshot-research.md)
- [30-T-phase1-query-read-view-capture.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/30-T-phase1-query-read-view-capture.md)

## Expected Touch Points

- `src/protocol/tools.rs`
- `src/protocol/format.rs`
- `src/live_index/`

## Deliverable

- an owned file-outline view or equivalent capture path for `get_file_outline`, with focused regression coverage

## Done When

- `get_file_outline` captures the data it needs under a short read lock and formats after the guard is dropped
- public output for `get_file_outline` remains unchanged
- focused tests cover the new helper or migrated path

## Completion Notes

- migrated `get_file_outline` off borrowed `&LiveIndex` formatting by adding `LiveIndex::capture_file_outline_view()`
- added `FileOutlineView` in `src/live_index/query.rs` so the tool captures only the file path and cloned symbol list under the read lock
- updated `src/protocol/tools.rs` to capture the owned view under the guard and then call `format::file_outline_view()` after the guard is released
- kept `format::file_outline()` as a compatibility wrapper that delegates through the same owned view, preserving output shape
- added focused helper and parity coverage, then reran `cargo test file_outline`

## Carry Forward To Next Task

Next task:

- `TBD`

Carry forward:

- the file-outline migration was straightforward because it only needed path and symbol metadata
- `get_symbol` may still want a narrower capture contract than cloning a whole indexed file, because symbol-body output depends on content bytes and not-found fallback details
- existing owned search result types in `src/live_index/search.rs` make `search_symbols` and `search_text` a strong next read-lock narrowing candidate

Open points:

- OPEN: decide whether the next slice should follow the explorer recommendation and migrate the search family before returning to single-file symbol detail
