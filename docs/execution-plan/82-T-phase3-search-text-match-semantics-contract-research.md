---
doc_type: task
task_id: 82
title: Phase 3 search_text match semantics contract research
status: done
sprint: tokenizor-upgrade-foundation
parent_plan: 04-P-phase-plan.md
prev_task: 81-T-phase3-search-text-glob-filter-shell.md
next_task: 83-T-phase3-search-text-match-semantics-shell.md
created: 2026-03-12
updated: 2026-03-12
---
# Task 82: Phase 3 Search Text Match Semantics Contract Research

## Objective

- define the smallest stable `case_sensitive` / `whole_word` contract for `search_text`

## Why This Exists

- Phase 3 still requires the remaining match-semantics knobs after scope, context, and glob filtering
- these knobs affect matching behavior rather than just candidate selection, so a small contract note is warranted before implementation

## Read Before Work

- [04-P-phase-plan.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/04-P-phase-plan.md)
- [05-P-validation-and-backlog.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/05-P-validation-and-backlog.md)
- [79-T-phase3-search-text-context-window-shell.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/79-T-phase3-search-text-context-window-shell.md)
- [81-T-phase3-search-text-glob-filter-shell.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/81-T-phase3-search-text-glob-filter-shell.md)
- [82-R-phase3-search-text-match-semantics-contract-research.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/82-R-phase3-search-text-match-semantics-contract-research.md)

## Expected Touch Points

- `docs/execution-plan/82-R-phase3-search-text-match-semantics-contract-research.md`
- `docs/execution-plan/82-T-phase3-search-text-match-semantics-contract-research.md`
- `docs/execution-plan/83-T-phase3-search-text-match-semantics-shell.md`

## Deliverable

- a research note fixing the next Phase 3 match-semantics contract and a small implementation task for it

## Done When

- the `case_sensitive` / `whole_word` contract is explicit
- default behavior and regex interactions are explicit
- the next implementation slice is small, additive, and recoverable from disk

## Completion Notes

- added [82-R-phase3-search-text-match-semantics-contract-research.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/82-R-phase3-search-text-match-semantics-contract-research.md)
- recommendation:
  - add `case_sensitive` and `whole_word` together in one small shell
  - preserve current defaults by making literal search case-insensitive by default and regex search case-sensitive by default
  - support whole-word only for literal mode in the first shell
  - reject `regex=true` plus `whole_word=true` with a stable error
  - keep the trigram prefilter unchanged and enforce stricter semantics in final line matching
- authored the next execution slice as `83-T-phase3-search-text-match-semantics-shell.md`

## Carry Forward To Next Task

Next task:

- `83-T-phase3-search-text-match-semantics-shell.md`

Carry forward:

- keep the slice confined to `search_text` semantics and avoid a broader query-substrate rewrite
- preserve existing formatter and cap behavior
- defer regex whole-word semantics until a later task explicitly justifies them

Open points:

- OPEN: whether a later regex-focused slice should support a broader word-boundary contract once there is evidence it is needed
