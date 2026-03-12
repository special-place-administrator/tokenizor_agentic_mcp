---
doc_type: task
task_id: 128
title: P1 get_file_content chunking contract research
status: done
sprint: tokenizor-upgrade-foundation
parent_plan: 05-P-validation-and-backlog.md
prev_task: 127-T-p1-get-file-content-around-match-shell.md
next_task: 129-T-p1-get-file-content-chunking-shell.md
created: 2026-03-12
updated: 2026-03-12
---
# Task 128: P1 Get File Content Chunking Contract Research

## Objective

- choose the first explicit `get_file_content` contract for progressive chunked reads of large exact-path files

## Why This Exists

- the source backlog pairs `around_match` with chunking as the next major read-surface upgrade
- task 127 now covers exact-path local excerpts, so the next small slice is a deterministic way to page through a large file without shell fallback

## Read Before Work

- [127-T-p1-get-file-content-around-match-shell.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/127-T-p1-get-file-content-around-match-shell.md)
- [02-P-workstreams-and-tool-surface.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/02-P-workstreams-and-tool-surface.md)
- [05-P-validation-and-backlog.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/05-P-validation-and-backlog.md)

## Deliverable

- a research note that chooses the smallest stable chunked-read contract and authors the next implementation shell

## Done When

- the accepted chunking input and output contract is explicit
- compatibility boundaries against existing range and around-* modes are clear
- the next shell task is authored and recoverable from disk

## Completion Notes

- chose exact-path line-oriented chunking with `chunk_index` plus `max_lines` as the first stable progressive-read contract
- kept the first chunking slice mutually exclusive with `start_line`, `end_line`, `around_line`, and `around_match`
- recommended numbered output with a small header that identifies the selected chunk and total chunks
- deferred byte-based chunking, symbol-anchored chunk selection, and chunk-count hints from callers to later slices

## Carry Forward To Next Task

Next task:

- `129-T-p1-get-file-content-chunking-shell.md`

Carry forward:

- keep chunking exact-path only
- keep the first slice line-oriented and deterministic
- preserve current full-file, explicit range, `around_line`, and `around_match` behavior

Open points:

- whether later slices should let callers request the next chunk without repeating `max_lines`
