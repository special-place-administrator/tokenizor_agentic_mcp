---
doc_type: task
task_id: 79
title: Phase 3 search_text context window shell
status: done
sprint: tokenizor-upgrade-foundation
parent_plan: 04-P-phase-plan.md
prev_task: 78-T-phase3-search-text-context-contract-research.md
next_task: 
created: 2026-03-12
updated: 2026-03-12
---
# Task 79: Phase 3 Search Text Context Window Shell

## Objective

- add the first context-window `search_text` shell using a symmetric `context` field, merged windows, and deterministic in-file rendering

## Why This Exists

- Phase 3 needs context lines before `search_text` can credibly replace common `rg -n -C` workflows
- task 78 fixed the smallest stable context contract, so the next step is now implementation rather than more API debate
- the current task 77 shell already established scope filters and deterministic match caps, which makes a bounded context slice feasible

## Read Before Work

- [78-R-phase3-search-text-context-contract-research.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/78-R-phase3-search-text-context-contract-research.md)
- [78-T-phase3-search-text-context-contract-research.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/78-T-phase3-search-text-context-contract-research.md)
- [77-T-phase3-search-text-scope-filter-shell.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/77-T-phase3-search-text-scope-filter-shell.md)

## Expected Touch Points

- `src/protocol/tools.rs`
- `src/live_index/search.rs`
- `src/protocol/format.rs`

## Deliverable

- the first public `search_text` context-window shell that preserves old output when unused and renders merged context windows when requested

## Done When

- `search_text` accepts `context`
- no-context output remains unchanged
- merged context windows render deterministically inside each file
- focused tests cover merged windows, separators, and cap semantics

## Completion Notes

- added `context` to the public `search_text` input and threaded it through `TextSearchOptions`
- kept the old no-context output path unchanged while adding context-window rendering only when `context` is present
- expanded the owned search result shape so the query layer now materializes merged window lines and explicit separators instead of making the formatter re-read file content
- context rendering behavior:
  - context lines render as `  {line_number}: {line}`
  - matched lines render as `> {line_number}: {line}`
  - disjoint windows inside one file render `  ...` between them
- preserved task 77 cap semantics: `limit` and `max_per_file` still count matches, not rendered lines
- verification run for this task:
  - `cargo test test_search_module_text_search_with_context_merges_windows_and_marks_matches -- --nocapture`
  - `cargo test test_search_text_result_view_renders_context_windows_with_separators -- --nocapture`
  - `cargo test test_search_text_tool_context_renders_windows -- --nocapture`
  - `cargo test search_text -- --nocapture`
  - `cargo test`

## Carry Forward To Next Task

Next task:

- `TBD`

Carry forward:

- keep the first context slice symmetric with `context` only
- keep match caps based on match count rather than rendered line count
- do not introduce mixed-lane text search in this shell

Open points:

- OPEN: whether later work should add `before` and `after` separately or jump straight to richer read-surface follow-up tools
