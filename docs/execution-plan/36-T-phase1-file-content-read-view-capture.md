---
doc_type: task
task_id: 36
title: Phase 1 file content read view capture
status: done
sprint: tokenizor-upgrade-foundation
parent_plan: 04-P-phase-plan.md
prev_task: 35-T-phase1-batch-symbol-read-view-capture.md
next_task: 
created: 2026-03-11
updated: 2026-03-11
---
# Task 36: Phase 1 File Content Read View Capture

## Objective

- move `get_file_content` to the capture-then-format pattern so line slicing happens after the live-index read guard is released

## Why This Exists

- tasks 34 and 35 already introduced content-oriented owned capture for symbol-detail and batch slice paths
- `get_file_content` is the next remaining single-file content formatter path and is smaller than the xref/context family

## Read Before Work

- [34-T-phase1-symbol-detail-read-view-capture.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/34-T-phase1-symbol-detail-read-view-capture.md)
- [35-T-phase1-batch-symbol-read-view-capture.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/35-T-phase1-batch-symbol-read-view-capture.md)

## Expected Touch Points

- `src/protocol/tools.rs`
- `src/protocol/format.rs`
- `src/live_index/`

## Deliverable

- an owned file-content view or equivalent capture path for `get_file_content`, with focused regression coverage

## Done When

- `get_file_content` captures the content it needs under a short read lock and formats after the guard is released
- full-file and line-range behavior remain unchanged
- focused tests cover parity or the migrated tool path

## Completion Notes

- migrated `get_file_content` off borrowed `&LiveIndex` formatting by adding `LiveIndex::capture_file_content_view()`
- added `FileContentView` in `src/live_index/query.rs` so the tool captures file-local bytes under the read lock
- updated `src/protocol/tools.rs` to capture the owned view under the guard and then call `format::file_content_view()` after the guard is released
- kept `format::file_content()` as a compatibility wrapper that delegates through the same owned-view path, preserving full-file and line-range behavior
- added focused helper and parity coverage, then reran `cargo test file_content`

## Carry Forward To Next Task

Next task:

- `TBD`

Carry forward:

- content-oriented read paths now use three small owned-view shapes: symbol detail, batch slices, and file content
- there is now a clearer boundary between easy single-file content migrations and the heavier xref/context family
- the next likely operational slices are `what_changed` timestamp mode or `health`, while the next structural slice is owned views for xref/context formatters

Open points:

- OPEN: decide whether the content-oriented owned views should be unified later or left separate until the larger query snapshot refactor
