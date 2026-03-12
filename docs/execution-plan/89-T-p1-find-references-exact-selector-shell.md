---
doc_type: task
task_id: 89
title: P1 find_references exact selector shell
status: done
sprint: tokenizor-upgrade-foundation
parent_plan: 05-P-validation-and-backlog.md
prev_task: 88-T-p1-find-references-exact-selector-contract-research.md
next_task: 90-T-p1-get-context-bundle-exact-selector-contract-research.md
created: 2026-03-12
updated: 2026-03-12
---
# Task 89: P1 Find References Exact Selector Shell

## Objective

- add the first exact-selector `find_references` shell that chains from current `search_symbols` output without changing name-only behavior

## Why This Exists

- task 88 fixed the smallest stable exact-selector contract
- current `find_references` is still fundamentally global-name-driven for common symbols such as `new`
- the next bounded improvement is to make symbol selection exact on input and dependency-scoped on cross-file matching

## Read Before Work

- [88-R-p1-find-references-exact-selector-contract-research.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/88-R-p1-find-references-exact-selector-contract-research.md)
- [88-T-p1-find-references-exact-selector-contract-research.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/88-T-p1-find-references-exact-selector-contract-research.md)
- [04-P-phase-plan.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/04-P-phase-plan.md)
- [05-P-validation-and-backlog.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/05-P-validation-and-backlog.md)
- [40-T-phase1-find-references-read-view-capture.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/40-T-phase1-find-references-read-view-capture.md)

## Expected Touch Points

- `src/protocol/tools.rs`
- `src/live_index/query.rs`
- likely `src/protocol/format.rs`

## Deliverable

- a first exact-selector `find_references` shell that accepts `path`, `symbol_kind`, and `symbol_line`, keeps the current formatter output for successful lookups, and materially reduces same-name noise for exact follow-up flows

## Done When

- `find_references` preserves current name-only behavior when no exact selector is supplied
- exact-selector mode accepts `path`, `symbol_kind`, and `symbol_line`
- ambiguous same-file selectors fail deterministically with a stable message when `symbol_line` is required
- exact-selector mode excludes unrelated same-name references outside the selected file dependency scope
- focused tests cover exact-selection success, ambiguity, and backward compatibility

## Completion Notes

- extended `find_references` with optional exact-selector inputs: `path`, `symbol_kind`, and `symbol_line`
- preserved current name-only behavior when `path` is omitted
- added `LiveIndex::capture_find_references_view_for_symbol()` to resolve an exact same-file symbol and then limit cross-file matching to the selected file dependency scope
- kept successful grouped formatter output unchanged while returning stable user-facing errors for missing files, missing symbols, and ambiguous selectors
- added focused unit and tool tests for exact-selector success, ambiguity without `symbol_line`, and backward-compatible name-only behavior
- verification run for this task:
  - `cargo test find_references -- --nocapture`
  - `cargo test`

## Carry Forward To Next Task

Next task:

- `90-T-p1-get-context-bundle-exact-selector-contract-research.md`

Carry forward:

- keep this slice separate from stable symbol-id work
- keep successful formatter output unchanged unless exact-selector errors require a tiny helper
- carry the exact-selector follow-up into `get_context_bundle` without reopening stable symbol-id work

Resolved point:

- the next follow-on should extend exact selection into `get_context_bundle` before broader read-surface parity work
