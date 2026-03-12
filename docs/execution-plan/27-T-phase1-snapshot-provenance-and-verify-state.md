---
doc_type: task
task_id: 27
title: Phase 1 snapshot provenance and verify state
status: done
sprint: tokenizor-upgrade-foundation
parent_plan: 04-P-phase-plan.md
prev_task: 26-T-phase1-live-state-snapshot-research.md
next_task: 28-T-phase1-root-aware-snapshot-mtime-capture.md
created: 2026-03-11
updated: 2026-03-11
---
# Task 27: Phase 1 Snapshot Provenance And Verify State

## Objective

- make it explicit when the live index was restored from snapshot, whether reconciliation is still pending, and whether the current state is ready or degraded

## Why This Exists

- the research task before this one should choose the live-state model, but the first implementation slice should stay additive and avoid a full container rewrite
- explicit provenance and verify state are the smallest substrate for later immutable-snapshot publication and repair flows

## Read Before Work

- [03-P-architecture-and-guardrails.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/03-P-architecture-and-guardrails.md)
- [26-T-phase1-live-state-snapshot-research.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/26-T-phase1-live-state-snapshot-research.md)
- [26-R-phase1-live-state-snapshot-research.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/26-R-phase1-live-state-snapshot-research.md)

## Expected Touch Points

- `src/live_index/store.rs`
- `src/live_index/persist.rs`
- `src/main.rs`

## Deliverable

- explicit internal metadata for snapshot provenance and verification progress, with focused tests

## Done When

- the index can distinguish fresh load vs snapshot restore vs ongoing verify
- background verification clears its pending state deterministically on completion
- focused tests cover the new metadata transitions

## Completion Notes

- added explicit internal `IndexLoadSource` and `SnapshotVerifyState` metadata to `LiveIndex`
- wired fresh load, empty bootstrap, reload, snapshot restore, and background verification through those state transitions
- kept the public `IndexState` contract unchanged while making snapshot provenance and verify progress queryable internally
- added focused persistence tests for snapshot restore pending state and verify completion, plus store-side assertions for fresh-load and empty-bootstrap defaults

## Carry Forward To Next Task

Next task:

- `28-T-phase1-root-aware-snapshot-mtime-capture.md`

Carry forward:

- exact provenance fields and state names now live in `src/live_index/store.rs`
- later immutable read-snapshot publication should wrap this metadata rather than discard it

Open points:

- OPEN: public `IndexState` expansion may remain deferred if internal metadata is enough for the first slice
