---
doc_type: task
task_id: 114
title: P1 prompt_context dotted qualified symbol line hint contract research
status: done
sprint: tokenizor-upgrade-foundation
parent_plan: 05-P-validation-and-backlog.md
prev_task: 113-T-p1-prompt-context-dotted-qualified-symbol-alias-shell.md
next_task: 115-T-p1-prompt-context-dotted-qualified-symbol-line-hint-shell.md
created: 2026-03-12
updated: 2026-03-12
---
# Task 114: P1 Prompt Context Dotted Qualified Symbol Line Hint Contract Research

## Objective

- define the smallest explicit dotted-alias follow-up contract for exact `:line` hints such as `pkg.db.connect:2`

## Why This Exists

- task 113 makes exact dotted qualified symbol aliases explicit, but it does not yet pin dotted `:line` forms with focused tests
- task 111 already introduced the generic full-alias line-hint path, so the next slice can stay narrow and mostly contractual
- this closes the remaining dotted exact-selector ergonomics gap without opening broader unlabeled-number parsing

## Read Before Work

- [113-T-p1-prompt-context-dotted-qualified-symbol-alias-shell.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/113-T-p1-prompt-context-dotted-qualified-symbol-alias-shell.md)
- [112-R-p1-prompt-context-dotted-qualified-symbol-alias-contract-research.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/112-R-p1-prompt-context-dotted-qualified-symbol-alias-contract-research.md)
- [111-T-p1-prompt-context-qualified-symbol-alias-shell.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/111-T-p1-prompt-context-qualified-symbol-alias-shell.md)

## Expected Touch Points

- `docs/execution-plan/114-T-p1-prompt-context-dotted-qualified-symbol-line-hint-contract-research.md`
- `docs/execution-plan/114-R-p1-prompt-context-dotted-qualified-symbol-line-hint-contract-research.md`
- `docs/execution-plan/115-T-p1-prompt-context-dotted-qualified-symbol-line-hint-shell.md`

## Deliverable

- a research task that chooses the first explicit dotted qualified-symbol `:line` contract and authors the next shell slice

## Done When

- the accepted dotted qualified-symbol `:line` syntax is explicit
- the boundary between exact dotted alias line hints and unrelated colon numbers is clear
- the next implementation slice is recoverable from disk

## Completion Notes

- added [114-R-p1-prompt-context-dotted-qualified-symbol-line-hint-contract-research.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/114-R-p1-prompt-context-dotted-qualified-symbol-line-hint-contract-research.md)
- chose exact full dotted aliases with direct `:line` suffixes as the next safe dotted-selector contract
- authored the follow-on execution slice as `115-T-p1-prompt-context-dotted-qualified-symbol-line-hint-shell.md`

## Carry Forward To Next Task

Next task:

- `115-T-p1-prompt-context-dotted-qualified-symbol-line-hint-shell.md`

Carry forward:

- keep dotted alias line hints exact and boundary-aware
- preserve the no-line dotted alias route and existing `line N` behavior
- avoid broadening this slice into unlabeled-number inference

Open points:

- whether later slices should add explicit dotted alias line-hint coverage for languages beyond Python-style module aliases
