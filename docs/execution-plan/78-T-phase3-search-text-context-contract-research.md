---
doc_type: task
task_id: 78
title: Phase 3 search_text context contract research
status: done
sprint: tokenizor-upgrade-foundation
parent_plan: 04-P-phase-plan.md
prev_task: 77-T-phase3-search-text-scope-filter-shell.md
next_task: 79-T-phase3-search-text-context-window-shell.md
created: 2026-03-12
updated: 2026-03-12
---
# Task 78: Phase 3 Search Text Context Contract Research

## Objective

- define the smallest public `search_text` context contract that can replace common `rg -n -C` workflows without turning Phase 3 into a read-surface redesign

## Why This Exists

- task 77 intentionally stopped at scope, caps, and noise suppression
- the Phase 3 plan still requires context-line support before `search_text` can replace a large share of shell grep usage
- the plan explicitly called for context-format research before landing another shell

## Read Before Work

- [04-P-phase-plan.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/04-P-phase-plan.md)
- [02-P-workstreams-and-tool-surface.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/02-P-workstreams-and-tool-surface.md)
- [05-P-validation-and-backlog.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/05-P-validation-and-backlog.md)
- [76-R-phase3-scoped-search-contract-research.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/76-R-phase3-scoped-search-contract-research.md)
- [77-T-phase3-search-text-scope-filter-shell.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/77-T-phase3-search-text-scope-filter-shell.md)
- [78-R-phase3-search-text-context-contract-research.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/78-R-phase3-search-text-context-contract-research.md)

## Expected Touch Points

- `docs/execution-plan/78-R-phase3-search-text-context-contract-research.md`
- `docs/execution-plan/78-T-phase3-search-text-context-contract-research.md`
- `docs/execution-plan/79-T-phase3-search-text-context-window-shell.md`

## Deliverable

- a research note that fixes the first context-window contract and a small next implementation task that can be executed without reopening Phase 3 scope design

## Done When

- the first public context knob is explicit
- rendering and cap semantics are explicit
- the next implementation slice is small and Phase 3 aligned

## Completion Notes

- added [78-R-phase3-search-text-context-contract-research.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/78-R-phase3-search-text-context-contract-research.md)
- recommendation:
  - add a single symmetric `context: Option<u32>` field first
  - preserve current output byte-for-byte when `context` is omitted
  - materialize merged windows in the query layer rather than in the formatter
  - keep `limit` and `max_per_file` counting matches, not rendered lines
  - defer `before`, `after`, `glob`, `exclude_glob`, `case_sensitive`, and `whole_word`
- authored the next execution slice as `79-T-phase3-search-text-context-window-shell.md`

## Carry Forward To Next Task

Next task:

- `79-T-phase3-search-text-context-window-shell.md`

Carry forward:

- keep the first context slice additive to task 77 rather than redesigning the tool surface
- preserve exact no-context behavior
- stay code-lane only until a text-lane registry exists

Open points:

- OPEN: whether later asymmetric `before` and `after` fields are still needed once real usage exists
