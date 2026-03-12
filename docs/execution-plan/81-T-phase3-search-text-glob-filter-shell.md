---
doc_type: task
task_id: 81
title: Phase 3 search_text glob filter shell
status: done
sprint: tokenizor-upgrade-foundation
parent_plan: 04-P-phase-plan.md
prev_task: 80-T-phase3-search-text-glob-contract-research.md
next_task: 
created: 2026-03-12
updated: 2026-03-12
---
# Task 81: Phase 3 Search Text Glob Filter Shell

## Objective

- add the first public `glob` and `exclude_glob` path-pattern filters to `search_text`

## Why This Exists

- path-prefix narrowing and context windows are in place, but real grep replacement still needs path-pattern filters
- task 80 fixed the smallest stable glob contract, so implementation can stay small and deterministic

## Read Before Work

- [80-R-phase3-search-text-glob-contract-research.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/80-R-phase3-search-text-glob-contract-research.md)
- [80-T-phase3-search-text-glob-contract-research.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/80-T-phase3-search-text-glob-contract-research.md)
- [79-T-phase3-search-text-context-window-shell.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/79-T-phase3-search-text-context-window-shell.md)

## Expected Touch Points

- `Cargo.toml`
- `src/protocol/tools.rs`
- `src/live_index/search.rs`
- `src/protocol/format.rs`

## Deliverable

- a `search_text` shell that supports a first include/exclude glob contract without changing the existing output shape

## Done When

- `search_text` accepts `glob` and `exclude_glob`
- include and exclude path filters work with normalized repo-relative paths
- invalid glob input returns a stable user-facing error
- focused tests cover include, exclude, combined path scoping, and invalid patterns

## Completion Notes

- added singular `glob` and `exclude_glob` fields to the public `search_text` input
- extended `TextSearchOptions` with normalized include/exclude glob strings while preserving the existing task 77 and task 79 behavior
- implemented glob matching against normalized repo-relative paths using `globset`
- filter order is now deterministic:
  - `path_prefix`
  - include `glob` if present
  - `exclude_glob` if present
  - existing language/scope/noise checks
- invalid glob patterns now return a stable user-facing error instead of falling through to empty results
- verification run for this task:
  - `cargo test test_search_module_text_search_with_options_respects_glob_filters -- --nocapture`
  - `cargo test test_search_module_text_search_invalid_glob_returns_error -- --nocapture`
  - `cargo test test_search_text_tool_respects_glob_and_exclude_glob -- --nocapture`
  - `cargo test test_search_text_tool_reports_invalid_glob -- --nocapture`
  - `cargo test search_text -- --nocapture`
  - `cargo test`

## Carry Forward To Next Task

Next task:

- `TBD`

Carry forward:

- keep singular `glob` and `exclude_glob` fields until real usage justifies widening them
- do not mix this slice with whole-word or case-sensitivity work
- preserve the current `search_text` rendering contract

Open points:

- OPEN: whether a later slice should widen glob filters to arrays before or after `case_sensitive` / `whole_word`
