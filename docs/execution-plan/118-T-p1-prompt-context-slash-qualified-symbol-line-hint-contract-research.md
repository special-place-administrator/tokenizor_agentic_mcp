---
doc_type: task
task_id: 118
title: P1 prompt_context slash qualified symbol line hint contract research
status: done
sprint: tokenizor-upgrade-foundation
parent_plan: 05-P-validation-and-backlog.md
prev_task: 117-T-p1-prompt-context-slash-qualified-symbol-alias-shell.md
next_task: 119-T-p1-prompt-context-slash-qualified-symbol-line-hint-shell.md
created: 2026-03-12
updated: 2026-03-12
---
# Task 118: P1 Prompt Context Slash Qualified Symbol Line Hint Contract Research

## Objective

- define the smallest explicit follow-up contract for exact slash-qualified symbol aliases with direct `:line` hints such as `src/utils/connect:2`

## Why This Exists

- task 117 makes slash-qualified symbol aliases explicit for normalized JS/TS module paths, but it does not yet pin slash `:line` forms with focused tests
- the exact qualified-symbol lane already supports alias-attached `:line` hints for the earlier Rust and dotted families
- this is the next adjacent selector refinement before any broader slash-import or path semantics

## Read Before Work

- [117-T-p1-prompt-context-slash-qualified-symbol-alias-shell.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/117-T-p1-prompt-context-slash-qualified-symbol-alias-shell.md)
- [116-R-p1-prompt-context-slash-qualified-symbol-alias-contract-research.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/116-R-p1-prompt-context-slash-qualified-symbol-alias-contract-research.md)
- [115-T-p1-prompt-context-dotted-qualified-symbol-line-hint-shell.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/115-T-p1-prompt-context-dotted-qualified-symbol-line-hint-shell.md)

## Expected Touch Points

- `docs/execution-plan/118-T-p1-prompt-context-slash-qualified-symbol-line-hint-contract-research.md`
- `docs/execution-plan/118-R-p1-prompt-context-slash-qualified-symbol-line-hint-contract-research.md`
- `docs/execution-plan/119-T-p1-prompt-context-slash-qualified-symbol-line-hint-shell.md`

## Deliverable

- a research task that chooses the first explicit slash-qualified symbol `:line` contract and authors the next shell slice

## Done When

- the accepted slash-qualified symbol `:line` syntax is explicit
- the boundary between exact slash alias line hints and unrelated colon numbers is clear
- the next implementation slice is recoverable from disk

## Completion Notes

- added [118-R-p1-prompt-context-slash-qualified-symbol-line-hint-contract-research.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/118-R-p1-prompt-context-slash-qualified-symbol-line-hint-contract-research.md)
- chose exact full slash-qualified aliases with direct `:line` suffixes as the next safe slash-selector contract
- authored the follow-on execution slice as `119-T-p1-prompt-context-slash-qualified-symbol-line-hint-shell.md`

## Carry Forward To Next Task

Next task:

- `119-T-p1-prompt-context-slash-qualified-symbol-line-hint-shell.md`

Carry forward:

- keep slash alias line hints exact and boundary-aware
- preserve the no-line slash alias route and existing `line N` behavior
- avoid broadening this slice into generic path-number inference

Open points:

- whether slash-qualified symbol line hints should stay limited to JS/TS module-path derivation
