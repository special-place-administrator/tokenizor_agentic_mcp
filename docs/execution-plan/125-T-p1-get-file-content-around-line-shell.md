---
doc_type: task
task_id: 125
title: P1 get_file_content around_line shell
status: done
sprint: tokenizor-upgrade-foundation
parent_plan: 05-P-validation-and-backlog.md
prev_task: 124-T-p1-get-file-content-around-line-contract-research.md
next_task: 126-T-p1-get-file-content-around-match-contract-research.md
created: 2026-03-12
updated: 2026-03-12
---
# Task 125: P1 Get File Content Around Line Shell

## Objective

- let `get_file_content` return a centered excerpt around one anchor line using `around_line` and symmetric `context_lines`

## Why This Exists

- task 124 chooses `around_line` plus `context_lines` as the first compact read-surface upgrade for `get_file_content`
- agents need a path-exact way to inspect one region of a file without calculating explicit line ranges manually

## Read Before Work

- [124-R-p1-get-file-content-around-line-contract-research.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/124-R-p1-get-file-content-around-line-contract-research.md)
- [124-T-p1-get-file-content-around-line-contract-research.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/124-T-p1-get-file-content-around-line-contract-research.md)
- [66-T-phase1-shared-query-option-struct-shell.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/66-T-phase1-shared-query-option-struct-shell.md)

## Expected Touch Points

- `src/protocol/tools.rs`
- `src/protocol/format.rs`
- `src/live_index/search.rs`
- `tests/live_index_integration.rs`

## Deliverable

- a `get_file_content` shell that supports centered exact-path reads around one line while preserving current full-file and explicit range behavior

## Done When

- `around_line` plus `context_lines` returns the expected centered excerpt
- mixed `around_line` with `start_line` / `end_line` is rejected deterministically
- current full-file and explicit line-range calls keep their existing behavior
- focused tests cover the new around-line mode and validation

## Completion Notes

- extended `GetFileContentInput` with `around_line` and `context_lines`
- added deterministic validation that rejects mixing `around_line` with explicit `start_line` / `end_line`
- added exact-path around-line content context and numbered excerpt rendering
- preserved existing raw full-file and explicit line-range output contracts
- covered the new mode in tool, formatter, search-option, and live-index integration tests

## Carry Forward To Next Task

Next task:

- `126-T-p1-get-file-content-around-match-contract-research.md`

Carry forward:

- keep `around_line` exact-path only
- preserve existing full-file and explicit line-range compatibility
- avoid broadening this slice into chunking or non-code file reads while the next `around_match` contract is chosen

Open points:

- whether later slices should make explicit line-range reads line-numbered too
