---
doc_type: task
task_id: 25
title: Phase 1 path metadata indices
status: done
sprint: tokenizor-upgrade-foundation
parent_plan: 04-P-phase-plan.md
prev_task: 24-T-phase1-shared-search-module-text-symbol.md
next_task: 26-T-phase1-live-state-snapshot-research.md
created: 2026-03-11
updated: 2026-03-11
---
# Task 25: Phase 1 Path Metadata Indices

## Objective

- add the first cheap path discovery substrate to `LiveIndex`: basename and directory-component indices

## Why This Exists

- task 22 selected basename plus directory-component maps as the lightest first path substrate
- Phase 2 path tools need these indices before public `search_files` or `resolve_path` work can stay fast and deterministic

## Read Before Work

- [03-P-architecture-and-guardrails.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/03-P-architecture-and-guardrails.md)
- [22-R-phase1-path-index-options-research.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/22-R-phase1-path-index-options-research.md)
- [24-T-phase1-shared-search-module-text-symbol.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/24-T-phase1-shared-search-module-text-symbol.md)

## Expected Touch Points

- `src/live_index/store.rs`
- `src/live_index/query.rs`

## Deliverable

- maintained basename and directory-component indices with focused tests and cheap mutation behavior

## Done When

- `LiveIndex::load`, `reload`, `update_file`, and `remove_file` maintain both path indices
- query-facing helpers expose deterministic lookups for basename and directory component matches
- focused tests cover load and mutation correctness

## Completion Notes

- added `files_by_basename` and `files_by_dir_component` to `LiveIndex`
- `LiveIndex::load`, `empty`, `reload`, `update_file`, and `remove_file` now keep both path indices in sync
- added query-facing helpers for deterministic, case-insensitive basename and directory-component lookup
- rebuilt path indices in persistence and test/helper constructors that materialize populated `LiveIndex` snapshots
- hardened directory-component extraction to deduplicate repeated path components and accept both `/` and `\\` separators
- added focused tests for load/reload parity, mutation correctness, query lookup behavior, and separator/dedup edge cases

## Carry Forward To Next Task

Next task:

- `26-T-phase1-live-state-snapshot-research.md`

Carry forward:

- exact index shapes and helper method names
- whether a later path trigram is still justified after the cheap substrate lands

Open points:

- OPEN: basename stem indexing remains deferred unless Phase 2 path-tool implementation proves it is necessary
