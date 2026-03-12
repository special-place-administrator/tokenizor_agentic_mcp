---
doc_type: task
task_id: 69
title: Phase 2 path discovery lane defaults research
status: done
sprint: tokenizor-upgrade-foundation
parent_plan: 04-P-phase-plan.md
prev_task: 68-T-phase1-explicit-current-tool-option-defaults-shell.md
next_task: 70-T-phase2-resolve-path-code-lane-shell.md
created: 2026-03-12
updated: 2026-03-12
---
# Task 69: Phase 2 Path Discovery Lane Defaults Research

## Objective

- decide how upcoming path discovery surfaces should default across the current semantic code lane and the future lightweight text lane

## Why This Exists

- Phase 2 explicitly requires code-first ranking while still allowing non-binary text resolution for read workflows
- tasks 67 and 68 made current search/read defaults explicit, but path discovery is the next family that can accidentally encode the wrong lane semantics
- current discovery-oriented surfaces such as `repo_outline` and `file_tree` are still semantic-lane only because no text registry exists yet

## Read Before Work

- [04-P-phase-plan.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/04-P-phase-plan.md)
- [22-R-phase1-path-index-options-research.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/22-R-phase1-path-index-options-research.md)
- [23-R-phase1-text-lane-boundary-research.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/23-R-phase1-text-lane-boundary-research.md)
- [67-R-phase1-dual-lane-option-defaults-research.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/67-R-phase1-dual-lane-option-defaults-research.md)
- [68-T-phase1-explicit-current-tool-option-defaults-shell.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/68-T-phase1-explicit-current-tool-option-defaults-shell.md)

## Expected Touch Points

- `docs/execution-plan/69-R-phase2-path-discovery-lane-defaults-research.md`
- `docs/execution-plan/69-T-phase2-path-discovery-lane-defaults-research.md`

## Deliverable

- a research note that recommends default lane behavior and ranking posture for `search_files`, `resolve_path`, and upgraded `repo_outline`, plus the next smallest implementation slice

## Done When

- default lane behavior for path discovery surfaces is explicit
- the recommendation preserves code-first ranking without blocking eventual text-lane path resolution
- the next implementation slice is small and consistent with Phase 2

## Completion Notes

- added [69-R-phase2-path-discovery-lane-defaults-research.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/69-R-phase2-path-discovery-lane-defaults-research.md)
- recommendation:
  - keep `repo_outline` code-lane only until the text registry exists
  - make first `search_files` implementation code-lane only, but preserve a later `All`-lane expansion seam
  - let `resolve_path` be the first future path discovery surface that can widen to text-lane candidates for read workflows
- next safest implementation is a code-lane `resolve_path` shell over basename and directory-component indices

## Carry Forward To Next Task

Next task:

- `70-T-phase2-resolve-path-code-lane-shell.md`

Carry forward:

- path discovery should solve shell escapes without collapsing semantic and text lanes into one undifferentiated path list

Open points:

- OPEN: what the best bounded ambiguous-output shape is for `resolve_path`
- OPEN: whether `repo_outline` should later gain a separate mixed-lane mode or stay semantic-only permanently
