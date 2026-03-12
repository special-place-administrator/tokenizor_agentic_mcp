---
doc_type: task
task_id: 68
title: Phase 1 explicit current-tool option defaults shell
status: done
sprint: tokenizor-upgrade-foundation
parent_plan: 04-P-phase-plan.md
prev_task: 67-T-phase1-dual-lane-option-defaults-research.md
next_task: 69-T-phase2-path-discovery-lane-defaults-research.md
created: 2026-03-12
updated: 2026-03-12
---
# Task 68: Phase 1 Explicit Current-Tool Option Defaults Shell

## Objective

- encode the current semantic-tool default option mapping explicitly in code instead of relying on raw `Default` construction

## Why This Exists

- task 66 introduced internal option structs
- task 67 concluded that current public search defaults should stay code-lane and noise-permissive, while exact file reads stay unsuppressed
- those semantics are still implicit in scattered `Default::default()` calls and ad hoc field mutation

## Read Before Work

- [04-P-phase-plan.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/04-P-phase-plan.md)
- [66-T-phase1-shared-query-option-struct-shell.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/66-T-phase1-shared-query-option-struct-shell.md)
- [67-R-phase1-dual-lane-option-defaults-research.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/67-R-phase1-dual-lane-option-defaults-research.md)
- [67-T-phase1-dual-lane-option-defaults-research.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/67-T-phase1-dual-lane-option-defaults-research.md)

## Expected Touch Points

- `src/live_index/search.rs`
- `src/protocol/format.rs`
- `src/protocol/tools.rs`

## Deliverable

- named internal constructors or adapter helpers for current semantic search and exact file-content defaults, wired into the existing public tool paths without changing behavior

## Done When

- current public search adapters do not rely on raw generic defaults for semantic-lane behavior
- exact file-content path handling has an explicit helper boundary
- behavior remains unchanged and is covered by focused tests

## Completion Notes

- added named current-tool option adapters in `src/live_index/search.rs`:
  - `SymbolSearchOptions::for_current_code_search()`
  - `TextSearchOptions::for_current_code_search()`
  - `FileContentOptions::for_explicit_path_read()`
- rewired current public adapter paths to use explicit semantics instead of raw generic defaults:
  - `search::search_symbols()`
  - `search::search_text()`
  - `format::search_symbols_result_with_kind()`
  - `format::search_text_result_with_options()`
  - `format::file_content()`
  - `TokenizorServer::get_file_content()`
- added focused tests that pin the named adapters and reran the existing search/file-content suites to confirm unchanged behavior

## Carry Forward To Next Task

Next task:

- `69-T-phase2-path-discovery-lane-defaults-research.md`

Carry forward:

- keep the current semantic defaults explicit so future text-lane adapters can diverge cleanly

Open points:

- OPEN: whether Phase 2 path discovery should default to code-only until the text registry exists, or adopt an eventual `All`-lane contract up front
