---
doc_type: task
task_id: 122
title: P1 prompt_context slash module alias line hint contract research
status: done
sprint: tokenizor-upgrade-foundation
parent_plan: 05-P-validation-and-backlog.md
prev_task: 121-T-p1-prompt-context-slash-module-alias-file-hint-shell.md
next_task: 123-T-p1-prompt-context-slash-module-alias-line-hint-shell.md
created: 2026-03-12
updated: 2026-03-12
---
# Task 122: P1 Prompt Context Slash Module Alias Line Hint Contract Research

## Objective

- define the smallest explicit follow-up contract for exact normalized slash module aliases with direct `:line` hints such as `src/utils:3 connect`

## Why This Exists

- task 121 makes exact normalized slash module aliases explicit for no-line file hints, but it does not yet pin the adjacent `:line` form with focused tests
- the shared line-hint parser already uses the file hint alias when one exists, so this syntax is the next safe selector refinement
- this keeps the slash module-alias family consistent with earlier module-alias and slash-qualified symbol work

## Read Before Work

- [121-T-p1-prompt-context-slash-module-alias-file-hint-shell.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/121-T-p1-prompt-context-slash-module-alias-file-hint-shell.md)
- [120-R-p1-prompt-context-slash-module-alias-file-hint-contract-research.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/120-R-p1-prompt-context-slash-module-alias-file-hint-contract-research.md)
- [119-T-p1-prompt-context-slash-qualified-symbol-line-hint-shell.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/119-T-p1-prompt-context-slash-qualified-symbol-line-hint-shell.md)

## Expected Touch Points

- `docs/execution-plan/122-T-p1-prompt-context-slash-module-alias-line-hint-contract-research.md`
- `docs/execution-plan/122-R-p1-prompt-context-slash-module-alias-line-hint-contract-research.md`
- `docs/execution-plan/123-T-p1-prompt-context-slash-module-alias-line-hint-shell.md`

## Deliverable

- a research task that chooses the first explicit slash module-alias `:line` contract and authors the next shell slice

## Done When

- the accepted slash module-alias `:line` syntax is explicit
- the boundary between exact slash module alias line hints and unrelated colon numbers is clear
- the next implementation slice is recoverable from disk

## Completion Notes

- added [122-R-p1-prompt-context-slash-module-alias-line-hint-contract-research.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/122-R-p1-prompt-context-slash-module-alias-line-hint-contract-research.md)
- chose exact full slash module aliases with direct `:line` suffixes as the next safe selector contract
- authored the follow-on execution slice as `123-T-p1-prompt-context-slash-module-alias-line-hint-shell.md`

## Carry Forward To Next Task

Next task:

- `123-T-p1-prompt-context-slash-module-alias-line-hint-shell.md`

Carry forward:

- keep slash module alias line hints exact and boundary-aware
- preserve the no-line slash module-alias route, slash-qualified symbol priority, and existing `line N` behavior
- avoid broadening this slice into generic slash-path number inference

Open points:

- whether slash module alias `:line` support should remain limited to normalized JS and TS module paths
