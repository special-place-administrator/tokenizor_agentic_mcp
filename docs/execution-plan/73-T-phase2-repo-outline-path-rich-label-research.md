---
doc_type: task
task_id: 73
title: Phase 2 repo_outline path-rich label research
status: done
sprint: tokenizor-upgrade-foundation
parent_plan: 04-P-phase-plan.md
prev_task: 72-T-phase2-search-files-code-lane-shell.md
next_task: 74-T-phase2-repo-outline-unique-suffix-label-shell.md
created: 2026-03-12
updated: 2026-03-12
---
# Task 73: Phase 2 Repo Outline Path-Rich Label Research

## Objective

- define the smallest safe path-rich label upgrade for `repo_outline` so repeated basenames stop being ambiguous without bloating whole-index output

## Why This Exists

- Phase 2 explicitly includes upgrading `repo_outline` away from ambiguous basename-only output
- `search_files` and `resolve_path` now exist, so `repo_outline` is the remaining major path-discovery surface still carrying basename ambiguity
- this surface already has a published immutable snapshot shared with `file_tree`, so label changes should be deliberate rather than incidental

## Read Before Work

- [04-P-phase-plan.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/04-P-phase-plan.md)
- [02-P-workstreams-and-tool-surface.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/02-P-workstreams-and-tool-surface.md)
- [11-D-phase0-baseline-output-snapshot-plan.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/11-D-phase0-baseline-output-snapshot-plan.md)
- [51-T-phase1-repo-outline-published-query-snapshot-shell.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/51-T-phase1-repo-outline-published-query-snapshot-shell.md)
- [69-R-phase2-path-discovery-lane-defaults-research.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/69-R-phase2-path-discovery-lane-defaults-research.md)
- [71-R-phase2-search-files-output-and-ranking-research.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/71-R-phase2-search-files-output-and-ranking-research.md)

## Expected Touch Points

- `docs/execution-plan/73-R-phase2-repo-outline-path-rich-label-research.md`
- `docs/execution-plan/73-T-phase2-repo-outline-path-rich-label-research.md`

## Deliverable

- a research note that recommends the first path-rich `repo_outline` label shape, its compatibility posture with `file_tree`, and the next smallest implementation slice

## Done When

- the target `repo_outline` label shape is explicit
- the interaction with the published repo-outline snapshot and `file_tree` is addressed
- the next implementation slice is small and Phase 2 aligned

## Completion Notes

- added [73-R-phase2-repo-outline-path-rich-label-research.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/73-R-phase2-repo-outline-path-rich-label-research.md)
- recommendation:
  - keep the published `RepoOutlineView` snapshot schema unchanged
  - derive display labels in the formatter from `relative_path`
  - use basename-only for unique files and shortest unique path suffixes for repeated basenames
  - keep `file_tree` unchanged because its hierarchical output is already path-rich
- next safest implementation is a formatter-local `repo_outline` label upgrade with focused ambiguity tests

## Carry Forward To Next Task

Next task:

- `74-T-phase2-repo-outline-unique-suffix-label-shell.md`

Carry forward:

- preserve code-lane-only repo discovery until a real text registry exists

Open points:

- OPEN: whether later hook-oriented compact outlines should reuse the same unique-suffix helper or adopt a stricter token budget format
