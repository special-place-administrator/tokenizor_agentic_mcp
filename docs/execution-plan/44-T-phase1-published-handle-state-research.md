---
doc_type: task
task_id: 44
title: Phase 1 published handle state research
status: done
sprint: tokenizor-upgrade-foundation
parent_plan: 04-P-phase-plan.md
prev_task: 43-T-phase1-shared-index-handle-shell.md
next_task: 45-T-phase1-published-handle-state-shell.md
created: 2026-03-11
updated: 2026-03-11
---
# Task 44: Phase 1 Published Handle State Research

## Objective

- choose the smallest publication step after the shared handle shell so the in-memory state container begins publishing authoritative generation/state snapshots from real mutation paths

## Why This Exists

- task 43 created a central shared-index handle, but it is still only a compatibility shell over the live lock
- the next architectural move should make the handle publish state from load/reload/watcher/verify mutations without attempting a full duplicated immutable query snapshot in the same slice
- this phase changes memory/state structure and watcher behavior, so it needs the explicit research pass required by the phase plan

## Read Before Work

- [04-P-phase-plan.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/04-P-phase-plan.md)
- [26-R-phase1-live-state-snapshot-research.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/26-R-phase1-live-state-snapshot-research.md)
- [42-R-phase1-shared-index-handle-research.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/42-R-phase1-shared-index-handle-research.md)
- [43-T-phase1-shared-index-handle-shell.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/43-T-phase1-shared-index-handle-shell.md)

## Expected Touch Points

- `docs/execution-plan/`
- `src/live_index/store.rs`
- `src/live_index/persist.rs`
- `src/watcher/`
- `src/protocol/tools.rs`

## Deliverable

- a short research note comparing full published query snapshots vs a lighter published handle-state shell and naming the next smallest safe implementation slice

## Done When

- the note identifies the real production mutation paths that should trigger publication
- candidate publication shapes are compared against churn, memory cost, and future read-snapshot direction
- one immediate implementation slice is chosen and recorded

## Completion Notes

- documented the real production mutation paths that still bypass handle-level publication in [44-R-phase1-published-handle-state-research.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/44-R-phase1-published-handle-state-research.md)
- compared three options: full duplicated published query snapshots now, lightweight published handle-state only, or generation-only bookkeeping
- chose lightweight published handle-state plus mutation-helper migration as the next slice because it proves real publication flow with low churn and without the memory risk of duplicating the whole query surface yet

## Carry Forward To Next Task

Next task:

- `45-T-phase1-published-handle-state-shell.md`

Carry forward:

- the next implementation should add generation and lightweight published state to `SharedIndexHandle`
- production write paths need to migrate from raw `write()` mutation to handle helpers where publication would otherwise go stale
- query readers should stay on the live index for now; full immutable reader migration remains a later slice

Open points:

- OPEN: keep the first publication slice lightweight unless benchmarks or a concrete reader migration requirement justify duplicating full query state
