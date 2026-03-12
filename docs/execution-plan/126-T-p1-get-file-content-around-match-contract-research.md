---
doc_type: task
task_id: 126
title: P1 get_file_content around_match contract research
status: done
sprint: tokenizor-upgrade-foundation
parent_plan: 05-P-validation-and-backlog.md
prev_task: 125-T-p1-get-file-content-around-line-shell.md
next_task: 127-T-p1-get-file-content-around-match-shell.md
created: 2026-03-12
updated: 2026-03-12
---
# Task 126: P1 Get File Content Around Match Contract Research

## Objective

- choose the first explicit `get_file_content` contract for rendering a local excerpt around a text match inside one exact file

## Why This Exists

- the source backlog calls out `around_match` as the next read-surface upgrade after line-based context
- task 125 now covers `around_line`, so the next small slice is a deterministic match-anchored excerpt without opening chunking or full `search_text` parity

## Read Before Work

- [125-T-p1-get-file-content-around-line-shell.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/125-T-p1-get-file-content-around-line-shell.md)
- [124-R-p1-get-file-content-around-line-contract-research.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/124-R-p1-get-file-content-around-line-contract-research.md)
- [05-P-validation-and-backlog.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/05-P-validation-and-backlog.md)

## Deliverable

- a research note that chooses the smallest stable `around_match` input and output contract and authors the next implementation shell

## Done When

- the accepted `around_match` contract is explicit
- ambiguity around repeated matches is handled deterministically
- the next shell task is authored and recoverable from disk

## Completion Notes

- chose exact-path `around_match` with first-match selection as the smallest stable contract
- kept the first slice literal-only and case-insensitive to match current simple read ergonomics
- reused symmetric `context_lines` and numbered excerpt rendering from `around_line`
- deferred match indexing, highlighting, regex, and chunking to later slices

## Carry Forward To Next Task

Next task:

- `127-T-p1-get-file-content-around-match-shell.md`

Carry forward:

- keep `around_match` exact-path only
- make `around_match` mutually exclusive with `start_line`, `end_line`, and `around_line`
- return a deterministic not-found message when the file does not contain the requested text

Open points:

- whether a later slice should add `match_index` for repeated matches before regex support
