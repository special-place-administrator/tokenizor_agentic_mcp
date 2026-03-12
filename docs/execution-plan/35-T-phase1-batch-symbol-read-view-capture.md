---
doc_type: task
task_id: 35
title: Phase 1 batch symbol read view capture
status: done
sprint: tokenizor-upgrade-foundation
parent_plan: 04-P-phase-plan.md
prev_task: 34-T-phase1-symbol-detail-read-view-capture.md
next_task: 
created: 2026-03-11
updated: 2026-03-11
---
# Task 35: Phase 1 Batch Symbol Read View Capture

## Objective

- move `get_symbols` to the capture-then-format pattern so the batch response is assembled after the live-index read guard is released

## Why This Exists

- task 34 introduced the owned symbol-detail view needed for single-symbol body rendering
- `get_symbols` still holds the live-index read guard while it formats a mixed batch of symbol lookups and byte-range slices
- migrating this path now reuses the fresh symbol-detail view while keeping the slice smaller than the xref/context family

## Read Before Work

- [24-T-phase1-shared-search-module-text-symbol.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/24-T-phase1-shared-search-module-text-symbol.md)
- [34-T-phase1-symbol-detail-read-view-capture.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/34-T-phase1-symbol-detail-read-view-capture.md)

## Expected Touch Points

- `src/protocol/tools.rs`
- `src/protocol/format.rs`
- `src/live_index/`

## Deliverable

- a captured batch representation for `get_symbols`, with focused regression coverage and unchanged public output

## Done When

- `get_symbols` captures all needed per-target data under one short read lock and formats after the guard is released
- symbol lookup and byte-range slice branches preserve current output behavior
- focused tests cover parity or the migrated tool path

## Completion Notes

- migrated `get_symbols` to capture the whole batch under one read lock and assemble the response after the guard is released
- reused `SymbolDetailView` for the symbol-lookup branch and introduced a small tool-local captured enum for symbol lookups, code slices, and file-not-found cases
- added `format::code_slice_view()` so the byte-range branch also formats after the guard is dropped
- kept the batch response shape unchanged, including `---` separators and current slice clamping behavior
- reran `cargo test get_symbols` and `cargo test code_slice_view`

## Carry Forward To Next Task

Next task:

- `TBD`

Carry forward:

- the tool-local captured batch enum was sufficient for the mixed symbol-lookup and code-slice branches
- the remaining easy single-file read path is `get_file_content`, which can likely reuse the same capture-then-format pattern without needing broader query changes
- xref/context formatters still remain a separate, heavier track because they need owned reference/context structures

Open points:

- OPEN: decide whether the captured batch helper should stay local once more content-oriented read paths have been migrated
