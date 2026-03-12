---
doc_type: task
task_id: 45
title: Phase 1 published handle state shell
status: done
sprint: tokenizor-upgrade-foundation
parent_plan: 04-P-phase-plan.md
prev_task: 44-T-phase1-published-handle-state-research.md
next_task: 
created: 2026-03-11
updated: 2026-03-11
---
# Task 45: Phase 1 Published Handle State Shell

## Objective

- make `SharedIndexHandle` publish lightweight authoritative state snapshots with generation bumps from real mutation paths, while keeping query readers on the live index for now

## Why This Exists

- task 44 chooses lightweight published handle state as the smallest safe publication step after the compatibility-handle shell
- this turns the shared handle into an active state container without paying the memory or churn cost of a full duplicated immutable query snapshot yet

## Read Before Work

- [26-R-phase1-live-state-snapshot-research.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/26-R-phase1-live-state-snapshot-research.md)
- [42-R-phase1-shared-index-handle-research.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/42-R-phase1-shared-index-handle-research.md)
- [44-R-phase1-published-handle-state-research.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/44-R-phase1-published-handle-state-research.md)

## Expected Touch Points

- `src/live_index/store.rs`
- `src/live_index/mod.rs`
- `src/live_index/persist.rs`
- `src/watcher/`
- `src/protocol/tools.rs`
- `src/daemon.rs`
- `src/sidecar/handlers.rs`

## Deliverable

- a lightweight published handle-state snapshot plus mutation-helper migration for production write paths, with focused regression coverage

## Done When

- `SharedIndexHandle` publishes generation/state snapshots on real mutation paths
- production writer paths use handle mutation helpers instead of raw `write()` mutation where publication would otherwise go stale
- focused tests cover generation/state publication behavior and a representative runtime path

## Completion Notes

- added lightweight `PublishedIndexState` publication to `SharedIndexHandle` in `src/live_index/store.rs`, including generation, file count, symbol count, provenance, verify state, and wall-clock load/mutation time
- added handle mutation helpers for reload, file update/add/remove, and snapshot-verify state transitions so publication can stay aligned with real production writes
- migrated production mutation paths in `src/protocol/tools.rs`, `src/daemon.rs`, `src/watcher/mod.rs`, `src/live_index/persist.rs`, and `src/sidecar/handlers.rs` away from raw live-lock mutation where published state would otherwise go stale
- added focused handle publication coverage in `src/live_index/store.rs` and extended `src/live_index/persist.rs` verification to assert published verify-state advancement through `background_verify`
- reran focused tests for shared-handle publication, background verify, watcher reindex, and reload behavior

## Carry Forward To Next Task

Next task:

- `TBD`

Carry forward:

- `SharedIndexHandle` now publishes authoritative lightweight state on real mutation paths, so later work can consume generation/state without depending on raw live-lock inspection
- query readers still intentionally read from the live index; this slice proves publication flow first without duplicating the full query surface
- the next architectural step should decide whether to publish a fuller read snapshot for one or more query paths, or first add internal consumers of the lightweight published state

Open points:

- OPEN: decide whether the next slice should introduce the first consumer of `PublishedIndexState` or move directly into research for a fuller published immutable query snapshot
