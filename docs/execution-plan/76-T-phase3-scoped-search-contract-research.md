---
doc_type: task
task_id: 76
title: Phase 3 scoped search contract research
status: done
sprint: tokenizor-upgrade-foundation
parent_plan: 04-P-phase-plan.md
prev_task: 75-T-phase2-text-lane-bridge-timing-research.md
next_task: 77-T-phase3-search-text-scope-filter-shell.md
created: 2026-03-12
updated: 2026-03-12
---
# Task 76: Phase 3 Scoped Search Contract Research

## Objective

- define the minimum public `search_text` scope and filtering contract that can replace common `rg` workflows without overcomplicating the first Phase 3 shell

## Why This Exists

- task 75 concluded that further mixed-lane path work should wait for later text-lane substrate work
- Phase 3 is the next roadmap step and explicitly requires research on scope/filter contract and non-code text participation
- `search_text` is the next highest-value shell replacement surface after path discovery

## Read Before Work

- [04-P-phase-plan.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/04-P-phase-plan.md)
- [02-P-workstreams-and-tool-surface.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/02-P-workstreams-and-tool-surface.md)
- [23-R-phase1-text-lane-boundary-research.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/23-R-phase1-text-lane-boundary-research.md)
- [64-R-phase1-file-classification-heuristics-research.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/64-R-phase1-file-classification-heuristics-research.md)
- [68-T-phase1-explicit-current-tool-option-defaults-shell.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/68-T-phase1-explicit-current-tool-option-defaults-shell.md)

## Expected Touch Points

- `docs/execution-plan/76-R-phase3-scoped-search-contract-research.md`
- `docs/execution-plan/76-T-phase3-scoped-search-contract-research.md`

## Deliverable

- a research note that recommends the smallest stable Phase 3 `search_text` filter/scope contract and the next implementation slice

## Done When

- the first public scope/filter contract is explicit
- code-lane defaults versus future text-lane participation are addressed
- the next implementation slice is small and Phase 3 aligned

## Completion Notes

- added [76-R-phase3-scoped-search-contract-research.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/76-R-phase3-scoped-search-contract-research.md)
- recommendation:
  - extend `search_text` additively instead of creating a new tool
  - make the first Phase 3 shell about scope, caps, and noise suppression
  - defer globs, whole-word matching, and context-window output to later slices
  - keep the public contract code-lane only until a real text registry exists
- next safest implementation is a scoped-filter `search_text` shell

## Carry Forward To Next Task

Next task:

- `77-T-phase3-search-text-scope-filter-shell.md`

Carry forward:

- keep the first Phase 3 slice small enough that it extends real `search_text` workflows without entangling the entire future text lane at once

Open points:

- OPEN: whether later Phase 3 context rendering should use one `context` field or separate `before` and `after` fields first
