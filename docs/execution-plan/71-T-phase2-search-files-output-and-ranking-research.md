---
doc_type: task
task_id: 71
title: Phase 2 search_files output and ranking research
status: done
sprint: tokenizor-upgrade-foundation
parent_plan: 04-P-phase-plan.md
prev_task: 70-T-phase2-resolve-path-code-lane-shell.md
next_task: 72-T-phase2-search-files-code-lane-shell.md
created: 2026-03-12
updated: 2026-03-12
---
# Task 71: Phase 2 Search Files Output And Ranking Research

## Objective

- define the first bounded output shape and deterministic ranking policy for a code-lane `search_files` shell

## Why This Exists

- Phase 2 still needs the broader discovery surface after `resolve_path`
- the plan explicitly calls for research on ranking heuristics and token-efficient disambiguation output
- implementing `search_files` without settling output and ranking would risk another round of accidental semantics

## Read Before Work

- [04-P-phase-plan.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/04-P-phase-plan.md)
- [22-R-phase1-path-index-options-research.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/22-R-phase1-path-index-options-research.md)
- [69-R-phase2-path-discovery-lane-defaults-research.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/69-R-phase2-path-discovery-lane-defaults-research.md)
- [70-T-phase2-resolve-path-code-lane-shell.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/70-T-phase2-resolve-path-code-lane-shell.md)
- [02-P-workstreams-and-tool-surface.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/02-P-workstreams-and-tool-surface.md)

## Expected Touch Points

- `docs/execution-plan/71-R-phase2-search-files-output-and-ranking-research.md`
- `docs/execution-plan/71-T-phase2-search-files-output-and-ranking-research.md`

## Deliverable

- a research note that recommends ranking tiers, bounded output shape, and the next implementation slice for `search_files`

## Done When

- a first ranking policy for code-lane path discovery is explicit
- output shape is bounded and informative enough for ambiguous file discovery
- the next `search_files` implementation slice is small and Phase 2 aligned

## Completion Notes

- added [71-R-phase2-search-files-output-and-ranking-research.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/71-R-phase2-search-files-output-and-ranking-research.md)
- recommendation:
  - use tier headers plus one full relative path per result line
  - rank strong path matches before basename-only hits, and basename hits before loose component/substring matches
  - default the first shell to `20` results with a hard cap of `50`
- next safest implementation is a code-lane `search_files` shell over the existing basename and directory-component indices

## Carry Forward To Next Task

Next task:

- `72-T-phase2-search-files-code-lane-shell.md`

Carry forward:

- keep `search_files` code-lane first until a real text registry exists, but avoid closing the door on later mixed-lane ranking

Open points:

- OPEN: whether future mixed-lane `search_files` results should label lane explicitly once text-lane candidates exist
