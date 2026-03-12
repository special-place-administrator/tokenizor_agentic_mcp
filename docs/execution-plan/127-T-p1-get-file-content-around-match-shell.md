---
doc_type: task
task_id: 127
title: P1 get_file_content around_match shell
status: done
sprint: tokenizor-upgrade-foundation
parent_plan: 05-P-validation-and-backlog.md
prev_task: 126-T-p1-get-file-content-around-match-contract-research.md
next_task: 128-T-p1-get-file-content-chunking-contract-research.md
created: 2026-03-12
updated: 2026-03-12
---
# Task 127: P1 Get File Content Around Match Shell

## Objective

- let `get_file_content` return a numbered local excerpt around the first literal text match inside one exact file

## Why This Exists

- task 126 chooses a first exact-path `around_match` contract as the smallest next read-surface upgrade after `around_line`
- agents need a direct path from a file hint plus match text to a local excerpt without manually computing anchor lines

## Read Before Work

- [126-R-p1-get-file-content-around-match-contract-research.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/126-R-p1-get-file-content-around-match-contract-research.md)
- [126-T-p1-get-file-content-around-match-contract-research.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/126-T-p1-get-file-content-around-match-contract-research.md)
- [125-T-p1-get-file-content-around-line-shell.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/125-T-p1-get-file-content-around-line-shell.md)

## Expected Touch Points

- `src/protocol/tools.rs`
- `src/protocol/format.rs`
- `src/live_index/search.rs`
- `tests/live_index_integration.rs`

## Deliverable

- a `get_file_content` shell that supports first-match exact-path reads around `around_match` while preserving full-file, explicit range, and `around_line` behavior

## Done When

- `around_match` plus `context_lines` returns the expected numbered excerpt
- mixed `around_match` with range or `around_line` inputs is rejected deterministically
- no-match requests return a stable message
- current full-file, explicit line-range, and `around_line` calls keep their existing behavior
- focused tests cover the new match-anchored mode and validation

## Completion Notes

- extended `GetFileContentInput` with exact-path `around_match`
- added deterministic validation that rejects mixing `around_match` with `start_line`, `end_line`, or `around_line`
- reused numbered excerpt rendering to anchor on the first case-insensitive literal match line
- added a stable no-match message for exact-path reads that do not contain the requested text
- preserved existing full-file, explicit range, and `around_line` behavior

## Carry Forward To Next Task

Next task:

- `128-T-p1-get-file-content-chunking-contract-research.md`

Carry forward:

- keep `around_match` exact-path only
- keep the first slice literal-only and first-match deterministic
- avoid broadening the next slice into regex, multi-match selection, or non-code file reads while chunking is designed

Open points:

- whether later slices should add `match_index` before `regex`
