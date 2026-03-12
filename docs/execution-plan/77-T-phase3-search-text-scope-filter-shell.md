---
doc_type: task
task_id: 77
title: Phase 3 search_text scope filter shell
status: done
sprint: tokenizor-upgrade-foundation
parent_plan: 04-P-phase-plan.md
prev_task: 76-T-phase3-scoped-search-contract-research.md
next_task: 
created: 2026-03-12
updated: 2026-03-12
---
# Task 77: Phase 3 Search Text Scope Filter Shell

## Objective

- implement the first scoped `search_text` shell by adding public path/language filters, deterministic caps, and generated/test suppression over the current code lane

## Why This Exists

- task 76 concluded that the first Phase 3 shell should extend `search_text` with scope and noise controls before changing context rendering
- the internal `TextSearchOptions` seam and file classification metadata already support this direction
- this is the next highest-value shell replacement after Phase 2 path discovery

## Read Before Work

- [76-R-phase3-scoped-search-contract-research.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/76-R-phase3-scoped-search-contract-research.md)
- [23-R-phase1-text-lane-boundary-research.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/23-R-phase1-text-lane-boundary-research.md)
- [64-R-phase1-file-classification-heuristics-research.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/64-R-phase1-file-classification-heuristics-research.md)

## Expected Touch Points

- `src/protocol/tools.rs`
- `src/live_index/search.rs`
- `src/protocol/format.rs`

## Deliverable

- a first public scoped `search_text` contract for the current code lane without changing the basic line-grouped output shape

## Done When

- `search_text` accepts public scope/filter fields for the first shell
- generated and test noise can be excluded by default and opted back in
- total and per-file match caps are deterministic
- focused tests cover the new filters and caps

## Completion Notes

- added the first public scoped `search_text` shell over the code lane by extending the tool input with `path_prefix`, `language`, `limit`, `max_per_file`, `include_generated`, and `include_tests`
- wired the tool layer into `TextSearchOptions` so scoped searches stay deterministic with default caps of `limit=50` and `max_per_file=5`
- made current-code searches exclude generated and test noise by default while still allowing explicit opt-in
- kept the output shape unchanged and left alias keywords such as `ts` and `js` deferred
- verification run for this task:
  - `cargo test search_text -- --nocapture`
  - `cargo test test_current_code_text_search_options_are_explicit -- --nocapture`
  - `cargo test`

## Carry Forward To Next Task

Next task:

- `TBD`

Carry forward:

- keep the first Phase 3 implementation code-lane only and avoid expanding into context-window formatting yet
- preserve canonical language-name filtering only until a later task explicitly adds alias support

Open points:

- OPEN: whether language filtering should accept only concrete language ids first or also alias keywords such as `ts` and `js`
