---
doc_type: task
task_id: 61
title: Phase 1 file local view compatibility research
status: done
sprint: tokenizor-upgrade-foundation
parent_plan: 04-P-phase-plan.md
prev_task: 60-T-phase1-get-symbols-code-slice-shared-file-capture-shell.md
next_task: 62-T-phase1-file-local-view-compatibility-labeling-shell.md
created: 2026-03-11
updated: 2026-03-11
---
# Task 61: Phase 1 File-Local View Compatibility Research

## Objective

- decide whether the clone-based file-local view types should now be removed, retained, or explicitly marked as compatibility-only after the shared-file migration work

## Why This Exists

- tasks 56 through 60 moved the main tool paths away from `FileContentView`, `FileOutlineView`, and `SymbolDetailView`
- removing them too early could break parity tests or future narrow snapshot work, while keeping them silently could leave confusing duplicate patterns in the codebase

## Read Before Work

- [55-R-phase1-first-file-local-shared-consumer-research.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/55-R-phase1-first-file-local-shared-consumer-research.md)
- [56-T-phase1-file-content-shared-file-capture-shell.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/56-T-phase1-file-content-shared-file-capture-shell.md)
- [57-T-phase1-file-outline-shared-file-capture-shell.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/57-T-phase1-file-outline-shared-file-capture-shell.md)
- [58-T-phase1-symbol-detail-shared-file-capture-shell.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/58-T-phase1-symbol-detail-shared-file-capture-shell.md)
- [60-T-phase1-get-symbols-code-slice-shared-file-capture-shell.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/60-T-phase1-get-symbols-code-slice-shared-file-capture-shell.md)

## Expected Touch Points

- `src/live_index/query.rs`
- `src/protocol/format.rs`
- `src/live_index/mod.rs`

## Deliverable

- a research note that recommends whether the old owned file-local view types stay, go, or get relabeled

## Done When

- current usage of the view types is explicit
- the recommendation explains whether they still serve tests or future architecture
- the next cleanup or retention step is clear

## Completion Notes

- the clone-based file-local view types should not be removed yet
- `FileOutlineView`, `FileContentView`, and `SymbolDetailView` are no longer the preferred hot-path substrate, but they still serve compatibility wrappers and parity-focused tests
- the right short-term move is to keep them explicitly as compatibility/test shapes and avoid using them for new main-path reader work

## Carry Forward To Next Task

Next task:

- `62-T-phase1-file-local-view-compatibility-labeling-shell.md`

Carry forward:

- shared `Arc<IndexedFile>` capture is now the preferred path for hot file-local readers
- the old owned view types still have limited value for test parity and compatibility helpers, so they should be labeled rather than deleted immediately

Open points:

- OPEN: decide whether later cleanup should delete the owned view types once parity helpers stop depending on them
