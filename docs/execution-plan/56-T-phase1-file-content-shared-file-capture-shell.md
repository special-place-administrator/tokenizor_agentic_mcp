---
doc_type: task
task_id: 56
title: Phase 1 file content shared file capture shell
status: done
sprint: tokenizor-upgrade-foundation
parent_plan: 04-P-phase-plan.md
prev_task: 55-T-phase1-first-file-local-shared-consumer-research.md
next_task: 57-T-phase1-file-outline-shared-file-capture-shell.md
created: 2026-03-11
updated: 2026-03-11
---
# Task 56: Phase 1 File Content Shared File Capture Shell

## Objective

- migrate `get_file_content` to consume the new shared immutable file substrate through short-lock `Arc<IndexedFile>` capture

## Why This Exists

- task 55 chooses `get_file_content` as the safest first consumer of the Arc-backed file substrate
- this proves the shared-file read pattern without committing to a repo-wide published file map or the broader symbol/detail family yet

## Read Before Work

- [55-R-phase1-first-file-local-shared-consumer-research.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/55-R-phase1-first-file-local-shared-consumer-research.md)
- [54-T-phase1-arc-indexed-file-substrate-shell.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/54-T-phase1-arc-indexed-file-substrate-shell.md)
- [36-T-phase1-file-content-read-view-capture.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/36-T-phase1-file-content-read-view-capture.md)

## Expected Touch Points

- `src/live_index/query.rs`
- `src/protocol/format.rs`
- `src/protocol/tools.rs`

## Deliverable

- a file-content reader path that captures one shared file handle under lock and formats after the lock is released, with focused regression coverage

## Done When

- `get_file_content` no longer deep-clones full file bytes into `FileContentView` on the main tool path
- file-content formatting still matches existing behavior, including line slicing
- focused tests cover the shared capture path and behavior parity

## Completion Notes

- added `LiveIndex::capture_shared_file()` in `src/live_index/query.rs` so single-file readers can clone one `Arc<IndexedFile>` under the read lock instead of deep-cloning content into a view
- migrated `get_file_content` in `src/protocol/tools.rs` to capture one shared file handle and then render after the lock is released
- added `format::file_content_from_indexed_file()` in `src/protocol/format.rs` and routed the compatibility `file_content()` wrapper through the same shared-file path, preserving full-file and line-range behavior
- added focused coverage for shared-file capture identity and shared-file formatting parity
- focused verification passed:
- `cargo test --no-run`
- `cargo test file_content -- --nocapture`
- `cargo test capture_shared_file -- --nocapture`

## Carry Forward To Next Task

Next task:

- `57-T-phase1-file-outline-shared-file-capture-shell.md`

Carry forward:

- the first file-local reader now consumes the `Arc<IndexedFile>` substrate directly without introducing a repo-wide published file snapshot
- the shared-file pattern is now explicit:
- capture one `Arc<IndexedFile>` under lock
- release the guard
- format from `&IndexedFile`
- `get_file_outline` is the next low-risk extension because it can reuse the same helper shape with symbol-only formatting before the broader symbol/detail family

Open points:

- OPEN: decide whether `get_file_outline` should be followed by the symbol/detail family or whether one more shared-file utility helper should land first
