---
doc_type: task
task_id: 57
title: Phase 1 file outline shared file capture shell
status: done
sprint: tokenizor-upgrade-foundation
parent_plan: 04-P-phase-plan.md
prev_task: 56-T-phase1-file-content-shared-file-capture-shell.md
next_task: 58-T-phase1-symbol-detail-shared-file-capture-shell.md
created: 2026-03-11
updated: 2026-03-11
---
# Task 57: Phase 1 File Outline Shared File Capture Shell

## Objective

- migrate `get_file_outline` to consume the shared immutable file substrate through short-lock `Arc<IndexedFile>` capture

## Why This Exists

- task 56 proved the shared-file capture pattern on raw file content
- `get_file_outline` is the next lowest-risk single-file reader because it only needs path plus symbols and does not add symbol-name lookup branching

## Read Before Work

- [55-R-phase1-first-file-local-shared-consumer-research.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/55-R-phase1-first-file-local-shared-consumer-research.md)
- [56-T-phase1-file-content-shared-file-capture-shell.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/56-T-phase1-file-content-shared-file-capture-shell.md)
- [31-T-phase1-file-outline-read-view-capture.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/31-T-phase1-file-outline-read-view-capture.md)

## Expected Touch Points

- `src/live_index/query.rs`
- `src/protocol/format.rs`
- `src/protocol/tools.rs`

## Deliverable

- a file-outline reader path that captures one shared file handle under lock and formats after the lock is released, with focused regression coverage

## Done When

- `get_file_outline` no longer deep-clones the symbol list into `FileOutlineView` on the main tool path
- file-outline formatting still matches existing behavior
- focused tests cover the shared capture path and behavior parity

## Completion Notes

- migrated `get_file_outline` in `src/protocol/tools.rs` from clone-heavy `FileOutlineView` capture to the shared-file path by cloning one `Arc<IndexedFile>` under the read lock
- added `format::file_outline_from_indexed_file()` in `src/protocol/format.rs` and routed the compatibility `file_outline()` wrapper through the same shared-file path, preserving output shape
- kept the earlier owned `FileOutlineView` compatibility surface intact for parity tests while removing symbol-vector cloning from the main tool path
- added focused formatting parity coverage for the shared-file outline helper
- focused verification passed:
- `cargo test --no-run`
- `cargo test file_outline -- --nocapture`
- `cargo test get_file_outline -- --nocapture`

## Carry Forward To Next Task

Next task:

- `58-T-phase1-symbol-detail-shared-file-capture-shell.md`

Carry forward:

- the shared-file pattern now covers both bytes-first and symbols-first single-file readers without introducing a repo-wide published file snapshot
- `get_symbol` is the next direct consumer because it can reuse the same captured `Arc<IndexedFile>` but adds symbol-name and kind-filter behavior that should stay isolated from the simpler outline/content tasks

Open points:

- OPEN: decide whether `get_symbols` symbol-lookup mode should follow `get_symbol` immediately or remain a separate batch-oriented slice
