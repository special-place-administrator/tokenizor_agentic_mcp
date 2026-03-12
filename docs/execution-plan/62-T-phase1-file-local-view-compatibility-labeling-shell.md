---
doc_type: task
task_id: 62
title: Phase 1 file local view compatibility labeling shell
status: done
sprint: tokenizor-upgrade-foundation
parent_plan: 04-P-phase-plan.md
prev_task: 61-T-phase1-file-local-view-compatibility-research.md
next_task: 
created: 2026-03-11
updated: 2026-03-11
---
# Task 62: Phase 1 File-Local View Compatibility Labeling Shell

## Objective

- make the retained clone-based file-local view types explicitly compatibility-only so new hot-path work follows the shared-file pattern by default

## Why This Exists

- task 61 concludes the old owned view types should stay for now, but keeping them unlabeled would leave the architectural direction ambiguous

## Read Before Work

- [61-R-phase1-file-local-view-compatibility-research.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/61-R-phase1-file-local-view-compatibility-research.md)
- [56-T-phase1-file-content-shared-file-capture-shell.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/56-T-phase1-file-content-shared-file-capture-shell.md)
- [57-T-phase1-file-outline-shared-file-capture-shell.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/57-T-phase1-file-outline-shared-file-capture-shell.md)
- [58-T-phase1-symbol-detail-shared-file-capture-shell.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/58-T-phase1-symbol-detail-shared-file-capture-shell.md)

## Expected Touch Points

- `src/live_index/query.rs`
- `src/protocol/format.rs`

## Deliverable

- small comments or labels that make the compatibility status of the clone-based file-local view path explicit

## Done When

- retained owned view types are clearly marked as compatibility/test scaffolding
- shared-file capture remains the obvious preferred path for new hot reader work
- no behavior changes are introduced

## Completion Notes

- added small comments in `src/live_index/query.rs` marking `FileOutlineView`, `FileContentView`, `SymbolDetailView`, and their capture helpers as compatibility/test scaffolding
- added matching comments in `src/protocol/format.rs` marking the `*_view` renderers as compatibility renderers and pointing hot-path readers at the `*_from_indexed_file()` shared-file path
- kept the slice comment-only with no behavior changes
- verification passed:
- `cargo test --no-run`

## Carry Forward To Next Task

Next task:

- `TBD`

Carry forward:

- shared-file capture is now both implemented and explicitly documented as the preferred hot-path substrate for file-local readers
- the clone-based file-local views remain available for compatibility wrappers and parity tests, but their status is no longer ambiguous

Open points:

- OPEN: decide whether a later cleanup phase should remove the compatibility views entirely
