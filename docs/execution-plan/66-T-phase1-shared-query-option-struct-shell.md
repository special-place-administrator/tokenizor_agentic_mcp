---
doc_type: task
task_id: 66
title: Phase 1 shared query option struct shell
status: done
sprint: tokenizor-upgrade-foundation
parent_plan: 04-P-phase-plan.md
prev_task: 65-T-phase1-file-classification-metadata-shell.md
next_task: 67-T-phase1-dual-lane-option-defaults-research.md
created: 2026-03-11
updated: 2026-03-12
---
# Task 66: Phase 1 Shared Query Option Struct Shell

## Objective

- introduce the first reusable internal query option types on top of the new file classification substrate without changing public tool contracts yet

## Why This Exists

- the Phase 1 plan still calls for shared internal option structs such as `PathScope`, `SearchScope`, `ResultLimit`, `ContentContext`, and `NoisePolicy`
- task 65 added the missing file classification substrate those options need
- the current public tool inputs are still narrow and ad hoc, so there is still duplicated validation and no shared internal request vocabulary

## Read Before Work

- [04-P-phase-plan.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/04-P-phase-plan.md)
- [20-D-phase1-query-duplication-discovery.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/20-D-phase1-query-duplication-discovery.md)
- [21-R-phase1-query-layer-shape-research.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/21-R-phase1-query-layer-shape-research.md)
- [63-R-phase1-remaining-substrate-priority-research.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/63-R-phase1-remaining-substrate-priority-research.md)
- [64-R-phase1-file-classification-heuristics-research.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/64-R-phase1-file-classification-heuristics-research.md)
- [65-T-phase1-file-classification-metadata-shell.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/65-T-phase1-file-classification-metadata-shell.md)

## Expected Touch Points

- `src/live_index/search.rs`
- `src/live_index/query.rs`
- `src/protocol/tools.rs`
- `src/protocol/format.rs`

## Deliverable

- small internal option types and adapters reused by at least the current search/read hot paths, while preserving existing public MCP inputs and outputs

## Done When

- the first shared internal option structs exist in code
- at least one current search path and one current read path consume them internally
- public tool contracts remain unchanged

## Completion Notes

- added first shared internal query option types in `src/live_index/search.rs`: `PathScope`, `SearchScope`, `ResultLimit`, `ContentContext`, `NoisePolicy`, plus `SymbolSearchOptions`, `TextSearchOptions`, and `FileContentOptions`
- moved current search hot paths onto the new option layer without changing MCP inputs:
  - `search_symbols()` now delegates through `search_symbols_with_options()`
  - `search_text()` now delegates through `search_text_with_options()`
- added the first read-path adapter on top of the same vocabulary:
  - `LiveIndex::capture_shared_file_for_scope()` bridges `PathScope` to shared file capture
  - `format::file_content()` and `TokenizorServer::get_file_content()` now route through `FileContentOptions` and `ContentContext`
- preserved existing public tool contracts and output shapes
- added focused tests for:
  - path/noise filtering in symbol search
  - search-scope/path filtering in text search
  - scoped shared-file capture behavior
  - `get_file_content` line-range contract preservation

## Carry Forward To Next Task

Next task:

- `67-T-phase1-dual-lane-option-defaults-research.md`

Carry forward:

- build on the new file classification substrate rather than treating scope/noise as formatter-only decisions
- keep public MCP inputs stable until the default option mapping for current tool families is explicit

Open points:

- OPEN: decide which current tool families should default to `Code`, `Text`, or `All` once the lightweight text lane is introduced
- OPEN: decide whether `NoisePolicy` should stay permissive by default for current coding-first tools or become stricter for future text-lane paths
