---
doc_type: task
task_id: 129
title: P1 get_file_content chunking shell
status: in_progress
sprint: tokenizor-upgrade-foundation
parent_plan: 05-P-validation-and-backlog.md
prev_task: 128-T-p1-get-file-content-chunking-contract-research.md
next_task:
created: 2026-03-12
updated: 2026-03-12
---
# Task 129: P1 Get File Content Chunking Shell

## Objective

- let `get_file_content` return one deterministic numbered line chunk from an exact file path

## Why This Exists

- task 128 chooses exact-path line-oriented chunking as the first progressive-read contract for large files
- agents need a bounded way to page through large files without shell fallback once local excerpts are not enough

## Read Before Work

- [128-R-p1-get-file-content-chunking-contract-research.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/128-R-p1-get-file-content-chunking-contract-research.md)
- [128-T-p1-get-file-content-chunking-contract-research.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/128-T-p1-get-file-content-chunking-contract-research.md)
- [127-T-p1-get-file-content-around-match-shell.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/127-T-p1-get-file-content-around-match-shell.md)

## Expected Touch Points

- `src/protocol/tools.rs`
- `src/protocol/format.rs`
- `src/live_index/search.rs`
- `tests/live_index_integration.rs`

## Deliverable

- a `get_file_content` shell that supports exact-path chunked reads while preserving full-file, explicit range, `around_line`, and `around_match` behavior

## Done When

- `chunk_index` plus `max_lines` returns the expected numbered chunk and header
- mixed chunking with range or around-* selectors is rejected deterministically
- out-of-range chunk requests return a stable message
- current full-file, explicit range, `around_line`, and `around_match` calls keep their existing behavior
- focused tests cover the new chunked-read mode and validation

## Completion Notes

- pending

## Carry Forward To Next Task

Next task:

- `TBD`

Carry forward:

- keep chunking exact-path only
- keep the first slice line-oriented and deterministic
- avoid broadening this slice into byte-range paging or continuation-state helpers

Open points:

- whether later slices should add caller-provided chunk-count hints or explicit next/prev helpers
