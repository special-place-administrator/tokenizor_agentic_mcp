---
doc_type: task
task_id: 32
title: Phase 1 search result format after capture
status: done
sprint: tokenizor-upgrade-foundation
parent_plan: 04-P-phase-plan.md
prev_task: 31-T-phase1-file-outline-read-view-capture.md
next_task: 
created: 2026-03-11
updated: 2026-03-11
---
# Task 32: Phase 1 Search Result Format After Capture

## Objective

- move `search_symbols` and `search_text` to the capture-then-format pattern so formatter assembly happens after the live-index read guard is released

## Why This Exists

- task 24 already moved text and symbol search semantics into `src/live_index/search.rs`
- those search helpers already return owned result structs, so the tool layer can capture results under the guard and let `src/protocol/format.rs` render them afterward
- this is the smallest higher-value query-path migration after the repo-outline and file-outline slices

## Read Before Work

- [24-T-phase1-shared-search-module-text-symbol.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/24-T-phase1-shared-search-module-text-symbol.md)
- [26-R-phase1-live-state-snapshot-research.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/26-R-phase1-live-state-snapshot-research.md)
- [31-T-phase1-file-outline-read-view-capture.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/31-T-phase1-file-outline-read-view-capture.md)

## Expected Touch Points

- `src/protocol/tools.rs`
- `src/protocol/format.rs`
- `src/live_index/search.rs`

## Deliverable

- search result formatting helpers that consume owned `search.rs` outputs, plus tool-path changes so formatting runs after the guard is dropped

## Done When

- `search_symbols` and `search_text` compute owned search results under the read lock and format after the guard is released
- public output remains unchanged for both tools
- focused tests cover owned-result formatting parity or the migrated tool paths

## Completion Notes

- migrated `search_symbols` and `search_text` tool handlers to compute owned search results under the read lock and format after the guard is released
- reused the existing owned result structs in `src/live_index/search.rs` rather than introducing another query-view layer for this slice
- added `format::search_symbols_result_view()` and `format::search_text_result_view()` as pure rendering helpers over owned search outputs
- kept the existing `format::search_symbols_result*` and `format::search_text_result*` entrypoints as compatibility wrappers, preserving current output contracts
- added parity coverage for both new rendering helpers and reran `cargo test search_`

## Carry Forward To Next Task

Next task:

- `TBD`

Carry forward:

- search result formatting now follows the same capture-then-format pattern as repo outline and file outline
- `get_symbol` and `get_symbols` remain good next single-file candidates, but will likely want narrower owned capture than a full `IndexedFile` clone
- `get_file_tree` is the next whole-index formatter candidate if the goal is to keep reducing broad read-lock formatting paths
- xref/context formatters still need dedicated owned hit/view types before they can migrate cleanly

Open points:

- OPEN: decide whether the next slice should prioritize `get_file_tree` for another whole-index path or `get_symbol` / `get_symbols` for the single-file family
