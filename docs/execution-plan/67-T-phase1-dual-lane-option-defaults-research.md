---
doc_type: task
task_id: 67
title: Phase 1 dual-lane option defaults research
status: done
sprint: tokenizor-upgrade-foundation
parent_plan: 04-P-phase-plan.md
prev_task: 66-T-phase1-shared-query-option-struct-shell.md
next_task: 68-T-phase1-explicit-current-tool-option-defaults-shell.md
created: 2026-03-12
updated: 2026-03-12
---
# Task 67: Phase 1 Dual-Lane Option Defaults Research

## Objective

- define how the new internal query option types should default across the current code lane and the future lightweight text lane before wider adoption

## Why This Exists

- task 66 introduced `PathScope`, `SearchScope`, `ResultLimit`, `ContentContext`, and `NoisePolicy`, but their defaults are still only implicit
- file classification metadata now exists, but the project still needs an explicit dual-lane retrieval boundary:
  semantic code lane vs lightweight plain-text lane
- widening option adoption before deciding default scope/noise behavior would hard-code accidental semantics

## Read Before Work

- [04-P-phase-plan.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/04-P-phase-plan.md)
- [23-R-phase1-text-lane-boundary-research.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/23-R-phase1-text-lane-boundary-research.md)
- [63-R-phase1-remaining-substrate-priority-research.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/63-R-phase1-remaining-substrate-priority-research.md)
- [64-R-phase1-file-classification-heuristics-research.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/64-R-phase1-file-classification-heuristics-research.md)
- [66-T-phase1-shared-query-option-struct-shell.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/66-T-phase1-shared-query-option-struct-shell.md)

## Expected Touch Points

- `docs/execution-plan/67-R-phase1-dual-lane-option-defaults-research.md`
- `docs/execution-plan/67-T-phase1-dual-lane-option-defaults-research.md`

## Deliverable

- a small research note that assigns recommended default `SearchScope` and `NoisePolicy` behavior to the current tool families, and identifies the next safest implementation slice

## Done When

- current tool families are classified by intended lane and default option behavior
- the recommendation is explicit about what stays code-only for now
- the next implementation slice is small and follows the source-plan intent

## Completion Notes

- added [67-R-phase1-dual-lane-option-defaults-research.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/67-R-phase1-dual-lane-option-defaults-research.md)
- recommendation:
  - current public search adapters stay `SearchScope::Code` and `NoisePolicy::permissive()`
  - explicit file-content reads stay unsuppressed and lane-aware by exact membership
  - structural symbol/xref tools remain semantic-lane only instead of being forced onto generic option defaults
- next safest implementation is to replace implicit `Default`-based option construction with named current-tool adapters

## Carry Forward To Next Task

Next task:

- `68-T-phase1-explicit-current-tool-option-defaults-shell.md`

Carry forward:

- keep the product coding-first while leaving room for a lightweight text lane

Open points:

- OPEN: whether path discovery should get its own `All`-lane default adapter or wait for real text-lane data structures
- OPEN: when Phase 6 suppression tuning begins, should vendor noise diverge from generated/test defaults
