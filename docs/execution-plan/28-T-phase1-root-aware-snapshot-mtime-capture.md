---
doc_type: task
task_id: 28
title: Phase 1 root-aware snapshot mtime capture
status: done
sprint: tokenizor-upgrade-foundation
parent_plan: 04-P-phase-plan.md
prev_task: 27-T-phase1-snapshot-provenance-and-verify-state.md
next_task: 29-T-phase1-persistence-lock-narrowing.md
created: 2026-03-11
updated: 2026-03-11
---
# Task 28: Phase 1 Root-Aware Snapshot Mtime Capture

## Objective

- make persisted snapshot metadata resolve file mtimes against the project root instead of the current working directory

## Why This Exists

- task 26 flagged a persistence correctness risk in `build_snapshot`
- snapshot verification depends on persisted mtimes being rooted correctly, otherwise restore can spuriously treat unchanged files as stale

## Read Before Work

- [26-R-phase1-live-state-snapshot-research.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/26-R-phase1-live-state-snapshot-research.md)
- [27-T-phase1-snapshot-provenance-and-verify-state.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/27-T-phase1-snapshot-provenance-and-verify-state.md)

## Expected Touch Points

- `src/live_index/persist.rs`
- `tests/live_index_integration.rs`

## Deliverable

- root-aware snapshot mtime capture with focused regression coverage

## Done When

- snapshot build resolves file metadata using the provided project root
- a focused test proves serialization captures usable mtimes even when the current process directory is not the project root

## Completion Notes

- updated snapshot build so file metadata resolves against the provided project root instead of the current process directory
- added focused persistence coverage proving root-aware mtime capture for a relative-path index entry
- reran the full `live_index::persist::tests::` suite after the fix

## Carry Forward To Next Task

Next task:

- `29-T-phase1-persistence-lock-narrowing.md`

Carry forward:

- review whether any other persistence fields still assume cwd-relative resolution

Open points:

- OPEN: a later pass may still want to persist richer snapshot provenance beyond mtimes
