---
doc_type: task
task_id: 59
title: Phase 1 get symbols shared file capture shell
status: done
sprint: tokenizor-upgrade-foundation
parent_plan: 04-P-phase-plan.md
prev_task: 58-T-phase1-symbol-detail-shared-file-capture-shell.md
next_task: 60-T-phase1-get-symbols-code-slice-shared-file-capture-shell.md
created: 2026-03-11
updated: 2026-03-11
---
# Task 59: Phase 1 Get Symbols Shared File Capture Shell

## Objective

- migrate the symbol-lookup branch of `get_symbols` to consume the shared immutable file substrate through short-lock `Arc<IndexedFile>` capture

## Why This Exists

- task 58 moved direct `get_symbol` lookups onto the shared-file path
- the batch `get_symbols` tool still deep-clones file bytes and symbols for its symbol-lookup branch, but it also mixes code-slice behavior that should stay scoped and explicit

## Read Before Work

- [58-T-phase1-symbol-detail-shared-file-capture-shell.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/58-T-phase1-symbol-detail-shared-file-capture-shell.md)
- [55-R-phase1-first-file-local-shared-consumer-research.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/55-R-phase1-first-file-local-shared-consumer-research.md)
- [35-T-phase1-batch-symbol-read-view-capture.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/35-T-phase1-batch-symbol-read-view-capture.md)

## Expected Touch Points

- `src/protocol/tools.rs`
- `src/protocol/format.rs`
- `src/live_index/query.rs`

## Deliverable

- a batch symbol-lookup path that captures shared file handles under lock and formats after the lock is released, with focused regression coverage

## Done When

- the symbol-lookup branch of `get_symbols` no longer deep-clones file bytes and symbols into `SymbolDetailView` on the main tool path
- batch symbol lookup output remains unchanged
- focused tests cover the shared capture path while preserving the separate code-slice branch

## Completion Notes

- migrated the symbol-lookup branch of `get_symbols` in `src/protocol/tools.rs` from clone-heavy `SymbolDetailView` capture to the shared-file path by cloning one `Arc<IndexedFile>` under the read lock
- reused the shared-file symbol renderer from `src/protocol/format.rs` so batch symbol lookups now format through `symbol_detail_from_indexed_file()` after the lock is released
- kept the batch code-slice branch unchanged in this slice so symbol-lookup migration stayed isolated and low-risk
- strengthened the batch symbol-lookup regression test to assert body output instead of only checking for guard-message absence
- focused verification passed:
- `cargo test --no-run`
- `cargo test get_symbols -- --nocapture`
- `cargo test code_slice_view -- --nocapture`
- `cargo test get_symbol -- --nocapture`

## Carry Forward To Next Task

Next task:

- `60-T-phase1-get-symbols-code-slice-shared-file-capture-shell.md`

Carry forward:

- the direct single-file readers plus the `get_symbols` symbol-lookup branch now all use shared `Arc<IndexedFile>` capture instead of deep-cloning whole file content and symbol vectors on the main tool path
- the remaining inconsistency inside `get_symbols` is the code-slice branch, which still copies the requested slice under the read lock even though it no longer formats there

Open points:

- OPEN: decide whether the batch code-slice branch should move to shared-file capture plus post-lock slice extraction, or remain a bounded under-lock copy by design
