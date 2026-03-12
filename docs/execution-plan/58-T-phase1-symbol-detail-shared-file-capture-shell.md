---
doc_type: task
task_id: 58
title: Phase 1 symbol detail shared file capture shell
status: done
sprint: tokenizor-upgrade-foundation
parent_plan: 04-P-phase-plan.md
prev_task: 57-T-phase1-file-outline-shared-file-capture-shell.md
next_task: 59-T-phase1-get-symbols-shared-file-capture-shell.md
created: 2026-03-11
updated: 2026-03-11
---
# Task 58: Phase 1 Symbol Detail Shared File Capture Shell

## Objective

- migrate `get_symbol` to consume the shared immutable file substrate through short-lock `Arc<IndexedFile>` capture

## Why This Exists

- tasks 56 and 57 proved the shared-file capture pattern for simpler single-file readers
- `get_symbol` is the next direct consumer, but it adds symbol-name lookup and optional kind filtering, so it should land as its own slice

## Read Before Work

- [55-R-phase1-first-file-local-shared-consumer-research.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/55-R-phase1-first-file-local-shared-consumer-research.md)
- [57-T-phase1-file-outline-shared-file-capture-shell.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/57-T-phase1-file-outline-shared-file-capture-shell.md)
- [34-T-phase1-symbol-detail-read-view-capture.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/34-T-phase1-symbol-detail-read-view-capture.md)

## Expected Touch Points

- `src/live_index/query.rs`
- `src/protocol/format.rs`
- `src/protocol/tools.rs`

## Deliverable

- a symbol-detail reader path that captures one shared file handle under lock and formats after the lock is released, with focused regression coverage

## Done When

- `get_symbol` no longer deep-clones file bytes and symbols into `SymbolDetailView` on the main tool path
- symbol-body extraction and not-found behavior still match existing output
- focused tests cover the shared capture path and behavior parity

## Completion Notes

- migrated `get_symbol` in `src/protocol/tools.rs` from clone-heavy `SymbolDetailView` capture to the shared-file path by cloning one `Arc<IndexedFile>` under the read lock
- added `format::symbol_detail_from_indexed_file()` in `src/protocol/format.rs` plus shared not-found rendering helpers so the compatibility `symbol_detail()` and `not_found_symbol()` wrappers now use the same shared-file path
- kept the earlier owned `SymbolDetailView` compatibility surface intact for parity tests while removing full content-and-symbol cloning from the main `get_symbol` tool path
- added focused parity coverage for the shared-file symbol-detail helper
- focused verification passed:
- `cargo test --no-run`
- `cargo test symbol_detail -- --nocapture`
- `cargo test get_symbol -- --nocapture`

## Carry Forward To Next Task

Next task:

- `59-T-phase1-get-symbols-shared-file-capture-shell.md`

Carry forward:

- the shared-file pattern now covers the direct single-file readers `get_file_content`, `get_file_outline`, and `get_symbol`
- the remaining clone-heavy single-file-adjacent path is the symbol-lookup branch inside batch `get_symbols`, which should be migrated separately from the code-slice branch to keep behavior clear

Open points:

- OPEN: decide whether `get_symbols` should migrate only the symbol-lookup branch first or whether both batch branches should be refactored behind a shared helper together
