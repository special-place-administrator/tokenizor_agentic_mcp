---
doc_type: task
task_id: 55
title: Phase 1 first file-local shared consumer research
status: done
sprint: tokenizor-upgrade-foundation
parent_plan: 04-P-phase-plan.md
prev_task: 54-T-phase1-arc-indexed-file-substrate-shell.md
next_task: 56-T-phase1-file-content-shared-file-capture-shell.md
created: 2026-03-11
updated: 2026-03-11
---
# Task 55: Phase 1 First File-Local Shared Consumer Research

## Objective

- choose the first file-local read family and read shape that should consume the new `Arc<IndexedFile>` substrate

## Why This Exists

- task 54 landed the shared immutable file unit, but that alone does not decide the right first consumer
- choosing the wrong file-local family or forcing the wrong publication shape would either waste the substrate or reintroduce hidden duplication

## Read Before Work

- [52-R-phase1-next-published-query-family-research.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/52-R-phase1-next-published-query-family-research.md)
- [53-R-phase1-shared-file-read-substrate-research.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/53-R-phase1-shared-file-read-substrate-research.md)
- [54-T-phase1-arc-indexed-file-substrate-shell.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/54-T-phase1-arc-indexed-file-substrate-shell.md)

## Expected Touch Points

- `src/live_index/query.rs`
- `src/protocol/tools.rs`
- `src/protocol/format.rs`
- `src/live_index/store.rs`

## Deliverable

- a research note that recommends the first consumer family and the smallest safe shape for it

## Done When

- the first consumer family is chosen with concrete rationale
- the recommendation distinguishes between repo-wide publication and narrow Arc-backed capture
- the next implementation slice is clear enough to execute without reopening the architecture question

## Completion Notes

- the first shared-file consumer should be the single-file direct-reader family, starting with `get_file_content`
- the first implementation should use narrow `Arc<IndexedFile>` capture under the live read lock, not a repo-wide published file map
- `get_file_content` is the best first proof because it removes the heaviest current deep clone (`Vec<u8>`), keeps semantics to one path plus optional line slicing, and avoids the branching complexity of symbol/detail lookups
- repo-wide publication of a path -> shared-file map stays deferred because it would republish an O(repo-file-count) map on every mutation when the immediate consumer path only needs a single keyed lookup

## Carry Forward To Next Task

Next task:

- `56-T-phase1-file-content-shared-file-capture-shell.md`

Carry forward:

- the new `Arc<IndexedFile>` substrate is most valuable when consumed through short-lock path lookup plus post-lock formatting, not through another broad published snapshot by default
- after `get_file_content` proves the pattern, `get_file_outline` is the next low-risk extension and symbol/detail readers can follow once the direct shared-file helper shape is stable

Open points:

- OPEN: decide whether the next follow-on after `get_file_content` should be `get_file_outline` or the symbol/detail family
