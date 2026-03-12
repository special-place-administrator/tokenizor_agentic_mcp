---
doc_type: task
task_id: 72
title: Phase 2 search_files code-lane shell
status: done
sprint: tokenizor-upgrade-foundation
parent_plan: 04-P-phase-plan.md
prev_task: 71-T-phase2-search-files-output-and-ranking-research.md
next_task: 73-T-phase2-repo-outline-path-rich-label-research.md
created: 2026-03-12
updated: 2026-03-12
---
# Task 72: Phase 2 Search Files Code-Lane Shell

## Objective

- implement the first public `search_files` surface over the current semantic code lane with bounded tiered path output

## Why This Exists

- Phase 2 still needs the broader discovery tool after `resolve_path`
- task 71 defined the initial output and ranking policy
- the basename and directory-component indices already exist and are ready to drive the first shell

## Read Before Work

- [04-P-phase-plan.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/04-P-phase-plan.md)
- [22-R-phase1-path-index-options-research.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/22-R-phase1-path-index-options-research.md)
- [69-R-phase2-path-discovery-lane-defaults-research.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/69-R-phase2-path-discovery-lane-defaults-research.md)
- [71-R-phase2-search-files-output-and-ranking-research.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/71-R-phase2-search-files-output-and-ranking-research.md)
- [71-T-phase2-search-files-output-and-ranking-research.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/71-T-phase2-search-files-output-and-ranking-research.md)

## Expected Touch Points

- `src/live_index/query.rs`
- `src/protocol/format.rs`
- `src/protocol/tools.rs`

## Deliverable

- a first `search_files` tool/query path that returns bounded tiered full-path results over the current code lane

## Done When

- `search_files` exists on the MCP surface
- the shell uses basename/component indices plus cheap fallback scanning
- result ordering and overflow behavior are covered by focused tests

## Completion Notes

- added the first public `search_files` tool over the current semantic code lane
- implementation touches:
  - `src/live_index/query.rs`
  - `src/protocol/format.rs`
  - `src/protocol/tools.rs`
  - `src/live_index/mod.rs`
- current shell behavior:
  - normalizes slash variants and rejects empty queries with a bounded explicit message
  - ranks results into strong path, basename, and loose path tiers
  - uses basename and directory-component indices first, with cheap normalized path scanning only for the remaining loose fallback
  - clamps user-visible limits to a default of `20` with a hard cap of `50`
  - returns one full relative path per line plus an overflow summary instead of noisy per-result reason labels
- verification:
  - `cargo test search_files -- --nocapture`
  - `cargo test exactly_20_tools_registered -- --nocapture`
  - `cargo test resolve_path -- --nocapture`

## Carry Forward To Next Task

Next task:

- `73-T-phase2-repo-outline-path-rich-label-research.md`

Carry forward:

- preserve code-lane semantics and tiered path output so later mixed-lane expansion stays additive
- keep `search_files` and upgraded `repo_outline` aligned on path-rich disambiguation without collapsing into full-path noise everywhere

Open points:

- OPEN: whether `repo_outline` should render full relative paths directly or adopt a shorter path-rich label that stays unambiguous on repeated basenames
