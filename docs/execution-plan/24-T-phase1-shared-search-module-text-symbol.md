---
doc_type: task
task_id: 24
title: Phase 1 shared search module for text and symbol search
status: done
sprint: tokenizor-upgrade-foundation
parent_plan: 04-P-phase-plan.md
prev_task: 23-T-phase1-text-lane-boundary-research.md
next_task: 25-T-phase1-path-metadata-indices.md
created: 2026-03-11
updated: 2026-03-11
---
# Task 24: Phase 1 Shared Search Module For Text And Symbol Search

## Objective

- add the first shared query-semantic module and move text-search and symbol-search selection logic under it without changing current output contracts

## Why This Exists

- task 21 chose `src/live_index/search.rs` as the smallest safe query-layer starting point
- text search and symbol search are the cleanest first extraction points because they do not require broader reference or sidecar refactors yet

## Read Before Work

- [03-P-architecture-and-guardrails.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/03-P-architecture-and-guardrails.md)
- [20-D-phase1-query-duplication-discovery.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/20-D-phase1-query-duplication-discovery.md)
- [21-R-phase1-query-layer-shape-research.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/21-R-phase1-query-layer-shape-research.md)

## Expected Touch Points

- `src/live_index/mod.rs`
- `src/live_index/search.rs`
- `src/protocol/format.rs`

## Deliverable

- shared internal search helpers and result structs for text and symbol search, with current public output preserved

## Done When

- `search_text_result_with_options` no longer owns query normalization or candidate selection
- `search_symbols_result_with_kind` no longer owns ranking logic
- focused tests cover the shared search module and public output remains stable

## Completion Notes

- added `src/live_index/search.rs` as the first shared query-semantic module
- moved text-search normalization and candidate selection out of `src/protocol/format.rs`
- moved symbol-search ranking and result limiting out of `src/protocol/format.rs`
- preserved current public output contracts for `search_text` and `search_symbols`
- added focused shared-module tests and reran the existing formatter and tool search tests

## Carry Forward To Next Task

Next task:

- `25-T-phase1-path-metadata-indices.md`

Carry forward:

- `src/protocol/format.rs` still owns reference grouping, symbol-context assembly, and string rendering
- `src/sidecar/handlers.rs` still owns prompt-path and prompt-symbol matching plus symbol-context grouping
- path index substrate work can proceed independently from the next shared-query extraction

Open points:

- OPEN: benchmark capture for the new shared search module may be deferred if this slice stays behavior-preserving and test-focused
