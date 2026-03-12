---
doc_type: task
task_id: 70
title: Phase 2 resolve_path code-lane shell
status: done
sprint: tokenizor-upgrade-foundation
parent_plan: 04-P-phase-plan.md
prev_task: 69-T-phase2-path-discovery-lane-defaults-research.md
next_task: 71-T-phase2-search-files-output-and-ranking-research.md
created: 2026-03-12
updated: 2026-03-12
---
# Task 70: Phase 2 Resolve Path Code-Lane Shell

## Objective

- implement the first `resolve_path` surface over the existing semantic-lane basename and directory-component indices

## Why This Exists

- Phase 2 path discovery should start by reducing the most common shell escape: resolving one intended file
- task 22 chose basename plus directory-component indices as the first path substrate
- task 69 concluded that `resolve_path` is the best first path discovery surface and should stay code-lane only in its first shell

## Read Before Work

- [04-P-phase-plan.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/04-P-phase-plan.md)
- [22-R-phase1-path-index-options-research.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/22-R-phase1-path-index-options-research.md)
- [69-R-phase2-path-discovery-lane-defaults-research.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/69-R-phase2-path-discovery-lane-defaults-research.md)
- [69-T-phase2-path-discovery-lane-defaults-research.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/69-T-phase2-path-discovery-lane-defaults-research.md)

## Expected Touch Points

- `src/live_index/query.rs`
- `src/protocol/format.rs`
- `src/protocol/tools.rs`

## Deliverable

- a first `resolve_path` tool/query path that handles exact path hits, exact basename hits, basename-plus-directory narrowing, and bounded ambiguous output over the current code lane

## Done When

- the code-lane `resolve_path` shell exists and uses the existing path indices
- repeated basenames produce deterministic disambiguation instead of silent guessing
- behavior is covered by focused tests

## Completion Notes

- added the first public `resolve_path` tool over the current semantic code lane
- implementation touches:
  - `src/live_index/query.rs`
  - `src/protocol/format.rs`
  - `src/protocol/tools.rs`
  - `src/live_index/mod.rs`
- current shell behavior:
  - exact normalized path match returns immediately
  - exact basename plus directory-component narrowing resolves repeated names deterministically
  - partial path fallback works through bounded code-lane path scanning when basename lookup alone is insufficient
  - ambiguous results return bounded disambiguation output instead of silently guessing
- updated tool registration coverage for the new MCP surface

## Carry Forward To Next Task

Next task:

- `71-T-phase2-search-files-output-and-ranking-research.md`

Carry forward:

- preserve code-first ranking and keep mixed-lane expansion for a later explicit slice

Open points:

- OPEN: whether `resolve_path` ambiguous output should eventually include lane labels once text-lane candidates exist
