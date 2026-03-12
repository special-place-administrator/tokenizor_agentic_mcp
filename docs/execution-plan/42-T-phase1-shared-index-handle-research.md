---
doc_type: task
task_id: 42
title: Phase 1 shared index handle research
status: done
sprint: tokenizor-upgrade-foundation
parent_plan: 04-P-phase-plan.md
prev_task: 41-T-phase1-context-bundle-read-view-capture.md
next_task: 43-T-phase1-shared-index-handle-shell.md
created: 2026-03-11
updated: 2026-03-11
---
# Task 42: Phase 1 Shared Index Handle Research

## Objective

- choose the smallest implementation step from the current raw `Arc<RwLock<LiveIndex>>` model toward a central in-memory state handle that can later publish immutable read snapshots

## Why This Exists

- the queue just finished narrowing query-side read-lock breadth, which removes one blocker to thinking clearly about the longer-lived in-memory state container
- task 26 chose staged migration toward published read snapshots, but the code still bakes `Arc<RwLock<LiveIndex>>` directly into shared aliases and consumers
- before changing the container, this phase needs a short explicit research pass because the next step affects memory/state structure and watcher behavior

## Read Before Work

- [04-P-phase-plan.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/04-P-phase-plan.md)
- [26-R-phase1-live-state-snapshot-research.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/26-R-phase1-live-state-snapshot-research.md)
- [41-T-phase1-context-bundle-read-view-capture.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/41-T-phase1-context-bundle-read-view-capture.md)

## Expected Touch Points

- `docs/execution-plan/`
- `src/live_index/store.rs`
- `src/protocol/mod.rs`
- `src/watcher/`

## Deliverable

- a short research note naming the current shared-index seam, candidate wrapper/snapshot approaches, and the next smallest safe implementation slice

## Done When

- the note identifies the real shared-index seam in current code
- candidate approaches are compared against churn, correctness, and future published-snapshot direction
- one immediate follow-up implementation slice is chosen and recorded

## Completion Notes

- documented the current shared-index seam in `src/live_index/store.rs`, `src/protocol/mod.rs`, and watcher/tool consumers in [42-R-phase1-shared-index-handle-research.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/42-R-phase1-shared-index-handle-research.md)
- compared three approaches: keep the raw alias until full snapshot publication, add a compatibility handle shell first, or add both a handle and published snapshot immediately
- chose the compatibility handle shell as the next slice because it centralizes the in-memory state identity with low churn while preserving `.read()` / `.write()` call-site stability

## Carry Forward To Next Task

Next task:

- `43-T-phase1-shared-index-handle-shell.md`

Carry forward:

- the next implementation should centralize the shared-index type before adding published read snapshots
- watcher and protocol code currently only need compatibility methods, not new snapshot semantics yet
- the eventual published-snapshot field should attach to the new handle rather than to another duplicated alias layer

Open points:

- OPEN: keep the first handle-shell slice behavior-preserving unless benchmarks or integration risks justify bundling snapshot publication into the same change
