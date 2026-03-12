---
doc_type: task
task_id: 106
title: P1 prompt_context qualified module alias contract research
status: done
sprint: tokenizor-upgrade-foundation
parent_plan: 05-P-validation-and-backlog.md
prev_task: 105-T-p1-prompt-context-qualified-extensionless-path-shell.md
next_task: 107-T-p1-prompt-context-qualified-module-alias-shell.md
created: 2026-03-12
updated: 2026-03-12
---
# Task 106: P1 Prompt Context Qualified Module Alias Contract Research

## Objective

- define the smallest follow-up contract for prompt-context to accept exact module aliases like `crate::db:2` without reopening fuzzy module-name parsing

## Why This Exists

- task 105 covers repo-relative extensionless paths like `src/db:2`, but some prompts refer to logical modules instead of paths
- module aliases can be ergonomic, but only if the contract stays exact and deterministic
- this is the next decision point before prompt-context crosses from path hints into language-aware aliases

## Read Before Work

- [104-R-p1-prompt-context-qualified-extensionless-path-contract-research.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/104-R-p1-prompt-context-qualified-extensionless-path-contract-research.md)
- [105-T-p1-prompt-context-qualified-extensionless-path-shell.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/105-T-p1-prompt-context-qualified-extensionless-path-shell.md)
- [98-T-p1-prompt-context-path-line-hint-shell.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/98-T-p1-prompt-context-path-line-hint-shell.md)

## Expected Touch Points

- `docs/execution-plan/106-T-p1-prompt-context-qualified-module-alias-contract-research.md`
- `docs/execution-plan/106-R-p1-prompt-context-qualified-module-alias-contract-research.md`
- `docs/execution-plan/107-T-p1-prompt-context-qualified-module-alias-shell.md`

## Deliverable

- a research task that chooses the first exact module-alias prompt shape, if any, and authors the next shell slice

## Done When

- the accepted qualified module alias syntax is explicit
- the guardrail between exact module aliases and fuzzy module guessing is clear
- the next implementation slice is recoverable from disk

## Completion Notes

- added [106-R-p1-prompt-context-qualified-module-alias-contract-research.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/106-R-p1-prompt-context-qualified-module-alias-contract-research.md)
- chose exact qualified module aliases with explicit namespace separators as the next safe boundary
- authored the follow-on execution slice as `107-T-p1-prompt-context-qualified-module-alias-shell.md`

## Carry Forward To Next Task

Next task:

- `107-T-p1-prompt-context-qualified-module-alias-shell.md`

Carry forward:

- only accept aliases that exactly match a language-derived module path
- require an explicit namespace separator so this lane stays distinct from path and stem aliases
- preserve the current fallback behavior when no exact module alias is available

Open points:

- whether any later slice should generalize beyond explicitly qualified module aliases
