---
doc_type: task
task_id: 75
title: Phase 2 text-lane bridge timing research
status: done
sprint: tokenizor-upgrade-foundation
parent_plan: 04-P-phase-plan.md
prev_task: 74-T-phase2-repo-outline-unique-suffix-label-shell.md
next_task: 76-T-phase3-scoped-search-contract-research.md
created: 2026-03-12
updated: 2026-03-12
---
# Task 75: Phase 2 Text-Lane Bridge Timing Research

## Objective

- decide whether the next path-discovery slice should widen toward future text-lane resolution now, or explicitly wait for the text-registry prerequisites in later phases

## Why This Exists

- the code-lane `resolve_path`, `search_files`, and upgraded `repo_outline` surfaces are now in place
- Phase 2 still carries an intent to support code-first ranking while eventually allowing non-binary text resolution for read workflows
- forcing the next implementation without clarifying that dependency risks smearing Phase 2 into the unfinished text-lane design

## Read Before Work

- [04-P-phase-plan.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/04-P-phase-plan.md)
- [23-R-phase1-text-lane-boundary-research.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/23-R-phase1-text-lane-boundary-research.md)
- [64-R-phase1-file-classification-heuristics-research.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/64-R-phase1-file-classification-heuristics-research.md)
- [69-R-phase2-path-discovery-lane-defaults-research.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/69-R-phase2-path-discovery-lane-defaults-research.md)
- [73-R-phase2-repo-outline-path-rich-label-research.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/73-R-phase2-repo-outline-path-rich-label-research.md)

## Expected Touch Points

- `docs/execution-plan/75-R-phase2-text-lane-bridge-timing-research.md`
- `docs/execution-plan/75-T-phase2-text-lane-bridge-timing-research.md`

## Deliverable

- a research note that recommends whether Phase 2 should continue with any path-discovery implementation before a text registry exists, plus the next smallest justified task

## Done When

- the dependency between path discovery and the future text lane is explicit
- the recommended next task is small and phase-correct
- Phase 2 is either cleanly closed on current substrate or given one clearly justified remaining slice

## Completion Notes

- added [75-R-phase2-text-lane-bridge-timing-research.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/75-R-phase2-text-lane-bridge-timing-research.md)
- recommendation:
  - do not add more Phase 2 implementation before a text registry exists
  - treat current Phase 2 path discovery as complete on the present code-lane substrate
  - keep future mixed-lane `resolve_path` as a later text-registry consumer, not an interim disk-scan hack
- next safest task is Phase 3 scoped-search contract research

## Carry Forward To Next Task

Next task:

- `76-T-phase3-scoped-search-contract-research.md`

Carry forward:

- preserve the current code-lane path behavior until there is an authoritative text-lane source of truth

Open points:

- OPEN: whether Phase 3 should start with the scope/filter contract first or with context-rendering format research first
