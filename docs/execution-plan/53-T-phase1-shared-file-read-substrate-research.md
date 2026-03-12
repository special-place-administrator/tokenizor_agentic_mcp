---
doc_type: task
task_id: 53
title: Phase 1 shared file read substrate research
status: done
sprint: tokenizor-upgrade-foundation
parent_plan: 04-P-phase-plan.md
prev_task: 52-T-phase1-next-published-query-family-research.md
next_task: 54-T-phase1-arc-indexed-file-substrate-shell.md
created: 2026-03-11
updated: 2026-03-11
---
# Task 53: Phase 1 Shared File Read Substrate Research

## Objective

- define the smallest storage and publication substrate that can support future published file-local read families without duplicating repository bytes

## Why This Exists

- the likely next high-value published readers (`get_file_content`, `get_symbol`, `get_symbols`, `get_file_outline`) all depend on file-local bytes and symbol lists
- the current owned views clone `Vec<u8>` and `Vec<SymbolRecord>`, which is acceptable under short-lived capture but risky for repo-wide published snapshots

## Read Before Work

- [52-R-phase1-next-published-query-family-research.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/52-R-phase1-next-published-query-family-research.md)
- [31-T-phase1-file-outline-read-view-capture.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/31-T-phase1-file-outline-read-view-capture.md)
- [34-T-phase1-symbol-detail-read-view-capture.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/34-T-phase1-symbol-detail-read-view-capture.md)
- [35-T-phase1-batch-symbol-read-view-capture.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/35-T-phase1-batch-symbol-read-view-capture.md)
- [36-T-phase1-file-content-read-view-capture.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/36-T-phase1-file-content-read-view-capture.md)

## Expected Touch Points

- `src/live_index/store.rs`
- `src/live_index/query.rs`
- `src/live_index/trigram.rs`
- `src/live_index/persist.rs`
- `docs/execution-plan/`

## Deliverable

- a research note choosing the shared file unit for future published read families

## Done When

- the note compares at least two concrete substrate options
- it identifies the real edit points and migration risk
- it recommends the first implementation substrate before any richer published read-family shell

## Completion Notes

- the preferred shared file unit is `Arc<IndexedFile>`, not direct publication of the current clone-based file views and not an immediate field-level `Arc<[T]>` refactor inside `IndexedFile`
- this matches the current mutation model, which already replaces files atomically instead of mutating file bytes or symbol vectors in place
- the main edit points are `LiveIndex.files`, the `get_file` / `all_files` accessors, trigram helpers that currently take `HashMap<String, IndexedFile>`, persistence conversion points, and test builders
- explicitly deferred: publishing any richer file-local reader family in the same slice; the storage substrate should land first

## Carry Forward To Next Task

Next task:

- `54-T-phase1-arc-indexed-file-substrate-shell.md`

Carry forward:

- adapt readers through accessors and iterator helpers so public query behavior can stay stable while the shared file unit changes underneath
- keep the implementation slice limited to the substrate; do not mix it with a new published file-reader family yet

Open points:

- OPEN: once `Arc<IndexedFile>` lands, choose which first published file-local reader family should consume it
