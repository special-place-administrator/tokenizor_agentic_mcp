---
doc_type: task
task_id: 26
title: Phase 1 live state and snapshot research
status: done
sprint: tokenizor-upgrade-foundation
parent_plan: 04-P-phase-plan.md
prev_task: 25-T-phase1-path-metadata-indices.md
next_task: 27-T-phase1-snapshot-provenance-and-verify-state.md
created: 2026-03-11
updated: 2026-03-11
---
# Task 26: Phase 1 Live State And Snapshot Research

## Objective

- define the smallest safe model for always-hot in-memory state, real-time disk-change ingestion, and durable snapshot recovery

## Why This Exists

- the current Phase 1 work added query substrate, but the project still needs an explicit answer for how live state, watcher updates, and persisted snapshots relate
- watcher behavior, memory profile, and index structure are all research-gated in the phase plan before a stronger state model is coded

## Read Before Work

- [03-P-architecture-and-guardrails.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/03-P-architecture-and-guardrails.md)
- [04-P-phase-plan.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/04-P-phase-plan.md)
- [23-R-phase1-text-lane-boundary-research.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/23-R-phase1-text-lane-boundary-research.md)
- [25-T-phase1-path-metadata-indices.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/25-T-phase1-path-metadata-indices.md)

## Expected Touch Points

- `src/live_index/store.rs`
- `src/live_index/persist.rs`
- `src/watcher/mod.rs`
- `src/main.rs`

## Deliverable

- one research note choosing the preferred live-state coordination model and the smallest next implementation slice

## Done When

- the note explains how in-memory content, watcher updates, and snapshot persistence should interact
- the note compares at least the current mutable-lock model against an immutable-snapshot publish model
- the note names the smallest next implementation slice and the risks it intentionally defers

## Completion Notes

- wrote [26-R-phase1-live-state-snapshot-research.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/26-R-phase1-live-state-snapshot-research.md)
- mapped the current coordination model across `store.rs`, `watcher/mod.rs`, `persist.rs`, and `main.rs`
- compared the current mutable-lock model, an immediate immutable published-snapshot model, and a staged migration
- selected staged migration as the smallest safe next move:
- keep the current container temporarily
- add explicit snapshot provenance and verify-state metadata next
- move to immutable published read snapshots after that metadata exists
- recorded one persistence-risk follow-up: snapshot metadata capture should be reviewed for root-aware path resolution

## Carry Forward To Next Task

Next task:

- `27-T-phase1-snapshot-provenance-and-verify-state.md`

Carry forward:

- exact chosen live-state model is in [26-R-phase1-live-state-snapshot-research.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/26-R-phase1-live-state-snapshot-research.md)
- read-snapshot publication should be staged behind smaller provenance and verify-state work, not done immediately

Open points:

- OPEN: whether the first publishable-snapshot implementation should use the standard library only or justify a dedicated atomic-swap dependency
