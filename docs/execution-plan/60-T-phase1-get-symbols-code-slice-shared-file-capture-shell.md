---
doc_type: task
task_id: 60
title: Phase 1 get symbols code slice shared file capture shell
status: done
sprint: tokenizor-upgrade-foundation
parent_plan: 04-P-phase-plan.md
prev_task: 59-T-phase1-get-symbols-shared-file-capture-shell.md
next_task: 61-T-phase1-file-local-view-compatibility-research.md
created: 2026-03-11
updated: 2026-03-11
---
# Task 60: Phase 1 Get Symbols Code Slice Shared File Capture Shell

## Objective

- migrate the code-slice branch of `get_symbols` to consume the shared immutable file substrate through short-lock `Arc<IndexedFile>` capture

## Why This Exists

- task 59 moved the batch symbol-lookup branch onto the shared-file path
- the remaining code-slice branch still copies bytes under the live read lock, so `get_symbols` is not yet internally consistent on the shared-file capture model

## Read Before Work

- [59-T-phase1-get-symbols-shared-file-capture-shell.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/59-T-phase1-get-symbols-shared-file-capture-shell.md)
- [58-T-phase1-symbol-detail-shared-file-capture-shell.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/58-T-phase1-symbol-detail-shared-file-capture-shell.md)
- [35-T-phase1-batch-symbol-read-view-capture.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/35-T-phase1-batch-symbol-read-view-capture.md)

## Expected Touch Points

- `src/protocol/tools.rs`
- `src/protocol/format.rs`

## Deliverable

- a code-slice batch path that captures shared file handles and slice bounds under lock, then performs slice extraction and formatting after the lock is released

## Done When

- the code-slice branch of `get_symbols` no longer copies slice bytes under the main live read lock
- code-slice output remains unchanged, including path header and clamped slice behavior
- focused tests cover the migrated batch path

## Completion Notes

- migrated the `get_symbols` code-slice branch in `src/protocol/tools.rs` from under-lock byte copying to shared-file capture by storing one `Arc<IndexedFile>` plus requested slice bounds under the read lock
- added `format::code_slice_from_indexed_file()` in `src/protocol/format.rs` so clamping, slice extraction, and formatting now happen after the guard is released
- kept output behavior unchanged, including path header and slice clamping
- added focused direct coverage for the new shared-file code-slice helper
- focused verification passed:
- `cargo test --no-run`
- `cargo test get_symbols -- --nocapture`
- `cargo test code_slice_view -- --nocapture`
- `cargo test code_slice_from_indexed_file -- --nocapture`

## Carry Forward To Next Task

Next task:

- `61-T-phase1-file-local-view-compatibility-research.md`

Carry forward:

- `get_symbols` is now internally consistent with the shared-file capture model across both the symbol-lookup and code-slice branches
- the remaining clone-based file-local reader shapes are now compatibility-only surfaces rather than active hot-path dependencies

Open points:

- OPEN: decide whether the remaining owned file-local view types should be explicitly retained as compatibility scaffolding or cleaned up further
