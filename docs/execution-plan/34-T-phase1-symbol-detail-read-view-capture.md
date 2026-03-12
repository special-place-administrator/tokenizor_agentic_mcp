---
doc_type: task
task_id: 34
title: Phase 1 symbol detail read view capture
status: done
sprint: tokenizor-upgrade-foundation
parent_plan: 04-P-phase-plan.md
prev_task: 33-T-phase1-file-tree-read-view-capture.md
next_task: 
created: 2026-03-11
updated: 2026-03-11
---
# Task 34: Phase 1 Symbol Detail Read View Capture

## Objective

- move `get_symbol` to the capture-then-format pattern with a dedicated owned view for symbol-body rendering and symbol-not-found fallback

## Why This Exists

- tasks 30 through 33 narrowed the formatter-backed paths that only needed metadata or existing owned search results
- `get_symbol` is the next remaining single-file formatter path, but it depends on raw content bytes plus file-local symbol metadata
- this makes it a better next step than xref/context work, while still staying smaller than the batched `get_symbols` path

## Read Before Work

- [26-R-phase1-live-state-snapshot-research.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/26-R-phase1-live-state-snapshot-research.md)
- [31-T-phase1-file-outline-read-view-capture.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/31-T-phase1-file-outline-read-view-capture.md)
- [33-T-phase1-file-tree-read-view-capture.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/33-T-phase1-file-tree-read-view-capture.md)

## Expected Touch Points

- `src/protocol/tools.rs`
- `src/protocol/format.rs`
- `src/live_index/`

## Deliverable

- an owned symbol-detail view or equivalent capture path for `get_symbol`, with focused regression coverage

## Done When

- `get_symbol` captures the file-local data it needs under a short read lock and formats after the guard is dropped
- public output remains unchanged
- focused tests cover the new helper or parity with the previous output

## Completion Notes

- migrated `get_symbol` off borrowed `&LiveIndex` formatting by adding `LiveIndex::capture_symbol_detail_view()`
- added `SymbolDetailView` in `src/live_index/query.rs` so the tool captures file-local content bytes and cloned symbols under the read lock
- updated `src/protocol/tools.rs` to capture the owned view under the guard and then call `format::symbol_detail_view()` after the guard is released
- refactored `src/protocol/format.rs` so `symbol_detail()` and `not_found_symbol()` delegate through the same owned-view rendering path, preserving the existing output contract
- added focused helper and parity coverage, then reran `cargo test symbol_detail` and `cargo test get_symbol`

## Carry Forward To Next Task

Next task:

- `TBD`

Carry forward:

- the symbol-detail view was sufficient for body rendering and missing-symbol fallback without needing broader index access during formatting
- the same file-local view is a strong candidate for the symbol-lookup branch of `get_symbols`
- `get_symbols` still needs its own batch-capture shape because it mixes symbol lookups and byte-range slices in one response

Open points:

- OPEN: keep the next slice focused on batch capture and post-capture formatting for `get_symbols`, without broadening into xref/context work
