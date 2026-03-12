---
doc_type: task
task_id: 124
title: P1 get_file_content around_line contract research
status: done
sprint: tokenizor-upgrade-foundation
parent_plan: 05-P-validation-and-backlog.md
prev_task: 123-T-p1-prompt-context-slash-module-alias-line-hint-shell.md
next_task: 125-T-p1-get-file-content-around-line-shell.md
created: 2026-03-12
updated: 2026-03-12
---
# Task 124: P1 Get File Content Around Line Contract Research

## Objective

- define the smallest stable `get_file_content` contract for centered line-context reads around one anchor line

## Why This Exists

- the source backlog explicitly calls out `get_file_content` line-number and `around_line` ergonomics as the next high-value read-surface improvement
- current `get_file_content` only supports raw full-file reads and explicit `start_line` / `end_line` slicing
- agents still need a compact way to inspect one area of a file without manually calculating ranges

## Read Before Work

- [05-P-validation-and-backlog.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/05-P-validation-and-backlog.md)
- [36-T-phase1-file-content-read-view-capture.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/36-T-phase1-file-content-read-view-capture.md)
- [66-T-phase1-shared-query-option-struct-shell.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/66-T-phase1-shared-query-option-struct-shell.md)

## Expected Touch Points

- `docs/execution-plan/124-T-p1-get-file-content-around-line-contract-research.md`
- `docs/execution-plan/124-R-p1-get-file-content-around-line-contract-research.md`
- `docs/execution-plan/125-T-p1-get-file-content-around-line-shell.md`

## Deliverable

- a research task that chooses the first explicit `around_line` API shape and authors the next implementation shell

## Done When

- the accepted `around_line` input contract is explicit
- the interaction with existing `start_line` / `end_line` behavior is clear
- the next implementation slice is recoverable from disk

## Completion Notes

- added [124-R-p1-get-file-content-around-line-contract-research.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/124-R-p1-get-file-content-around-line-contract-research.md)
- chose `around_line` plus symmetric `context_lines` as the smallest stable centered-read contract
- authored the follow-on implementation slice as `125-T-p1-get-file-content-around-line-shell.md`

## Carry Forward To Next Task

Next task:

- `125-T-p1-get-file-content-around-line-shell.md`

Carry forward:

- keep `around_line` exact-path only
- preserve existing full-file and explicit line-range behavior
- avoid broadening this slice into `around_match`, chunking, or non-code text-lane reads

Open points:

- whether line numbers should appear only for `around_line` excerpts or also for later explicit range modes
