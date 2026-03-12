---
doc_type: task
task_id: 43
title: Phase 1 shared index handle shell
status: done
sprint: tokenizor-upgrade-foundation
parent_plan: 04-P-phase-plan.md
prev_task: 42-T-phase1-shared-index-handle-research.md
next_task: 
created: 2026-03-11
updated: 2026-03-11
---
# Task 43: Phase 1 Shared Index Handle Shell

## Objective

- replace the duplicated raw shared-index alias with a central handle type that still exposes `.read()` and `.write()` over the current live `RwLock<LiveIndex>`

## Why This Exists

- task 42 identifies the shared-container seam as the next smallest step toward the longer-lived in-memory state model
- this creates a concrete home for future published read snapshots without forcing that wider change into the same slice

## Read Before Work

- [26-R-phase1-live-state-snapshot-research.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/26-R-phase1-live-state-snapshot-research.md)
- [42-R-phase1-shared-index-handle-research.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/42-R-phase1-shared-index-handle-research.md)

## Expected Touch Points

- `src/live_index/store.rs`
- `src/live_index/mod.rs`
- `src/protocol/mod.rs`
- `src/watcher/`

## Deliverable

- a central shared-index handle type and alias consolidation, with focused compatibility coverage

## Done When

- the project no longer duplicates `Arc<RwLock<LiveIndex>>` as the authoritative shared-index alias in multiple modules
- existing watcher/tool call sites still compile through the new handle’s compatibility methods
- focused tests cover basic read/write compatibility or constructor behavior

## Completion Notes

- introduced `SharedIndexHandle` in `src/live_index/store.rs` as the central shared-container shell around the existing live `RwLock<LiveIndex>`
- changed the authoritative `SharedIndex` alias to `Arc<SharedIndexHandle>` and kept compatibility `.read()` / `.write()` methods so watcher and protocol call sites did not need semantic rewrites
- updated `LiveIndex::load()` and `LiveIndex::empty()` plus direct raw-construction seams in snapshot-restore and test helpers to use the new shared handle constructor
- removed the duplicated raw shared-index alias from `src/protocol/mod.rs` and re-exported the central handle from `src/live_index/mod.rs`
- added focused compatibility coverage in `src/live_index/store.rs` and reran shared-handle, watcher reindex, and snapshot-restore-focused tests

## Carry Forward To Next Task

Next task:

- `TBD`

Carry forward:

- the project now has one named home for future published read snapshots instead of repeating `Arc<RwLock<LiveIndex>>` across modules
- current behavior is intentionally unchanged: the new handle is still only a compatibility shell over the live lock
- the next state-machine slice can attach published read snapshots or generation metadata to `SharedIndexHandle` without another alias migration

Open points:

- OPEN: decide whether the next slice should add a published read-snapshot field directly to `SharedIndexHandle` or first add generation/provenance helpers on the handle before publishing reads from it
