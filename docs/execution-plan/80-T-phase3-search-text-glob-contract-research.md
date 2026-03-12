---
doc_type: task
task_id: 80
title: Phase 3 search_text glob contract research
status: done
sprint: tokenizor-upgrade-foundation
parent_plan: 04-P-phase-plan.md
prev_task: 79-T-phase3-search-text-context-window-shell.md
next_task: 81-T-phase3-search-text-glob-filter-shell.md
created: 2026-03-12
updated: 2026-03-12
---
# Task 80: Phase 3 Search Text Glob Contract Research

## Objective

- define the smallest public `glob` / `exclude_glob` contract that materially reduces shell fallback for scoped text search

## Why This Exists

- Phase 3 still calls for glob filters after the path-prefix and context slices
- task 79 made `search_text` much closer to `rg -n -C`, but path-pattern narrowing is still missing
- the next safe slice is to settle the contract before adding another filter family

## Read Before Work

- [04-P-phase-plan.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/04-P-phase-plan.md)
- [05-P-validation-and-backlog.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/05-P-validation-and-backlog.md)
- [76-R-phase3-scoped-search-contract-research.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/76-R-phase3-scoped-search-contract-research.md)
- [79-T-phase3-search-text-context-window-shell.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/79-T-phase3-search-text-context-window-shell.md)
- [80-R-phase3-search-text-glob-contract-research.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/80-R-phase3-search-text-glob-contract-research.md)

## Expected Touch Points

- `docs/execution-plan/80-R-phase3-search-text-glob-contract-research.md`
- `docs/execution-plan/80-T-phase3-search-text-glob-contract-research.md`
- `docs/execution-plan/81-T-phase3-search-text-glob-filter-shell.md`

## Deliverable

- a research note fixing the first glob-filter contract and a small implementation task for it

## Done When

- the first public glob contract is explicit
- matching order and error posture are explicit
- the next implementation slice is small and Phase 3 aligned

## Completion Notes

- added [80-R-phase3-search-text-glob-contract-research.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/80-R-phase3-search-text-glob-contract-research.md)
- recommendation:
  - add singular `glob` and `exclude_glob` string fields first
  - match against normalized repo-relative paths
  - require both `path_prefix` and `glob` when both are provided
  - apply `exclude_glob` last as a hard exclusion
  - use `globset` rather than ad-hoc wildcard matching
- authored the next execution slice as `81-T-phase3-search-text-glob-filter-shell.md`

## Carry Forward To Next Task

Next task:

- `81-T-phase3-search-text-glob-filter-shell.md`

Carry forward:

- keep the first glob slice additive and code-lane only
- keep singular `glob` and `exclude_glob` fields for now
- leave case-sensitivity and whole-word semantics for later work

Open points:

- OPEN: whether later search ergonomics should widen glob inputs to arrays or keep a single-pattern contract
