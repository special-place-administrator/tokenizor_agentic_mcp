---
doc_type: task
task_id: 104
title: P1 prompt_context qualified extensionless path contract research
status: done
sprint: tokenizor-upgrade-foundation
parent_plan: 05-P-validation-and-backlog.md
prev_task: 103-T-p1-prompt-context-extensionless-line-hint-shell.md
next_task: 105-T-p1-prompt-context-qualified-extensionless-path-shell.md
created: 2026-03-12
updated: 2026-03-12
---
# Task 104: P1 Prompt Context Qualified Extensionless Path Contract Research

## Objective

- define the smallest follow-up contract that lets prompt-context accept repo-relative extensionless paths like `src/db:2` without expanding into language-specific module parsing

## Why This Exists

- task 103 covers unique file stems like `db:2`, but repeated stems still need a more explicit prompt shape
- repo-relative extensionless paths stay file-oriented and deterministic, unlike broader module aliases
- this is the narrowest next prompt-context improvement that can resolve real collisions without opening fuzzy parsing

## Read Before Work

- [102-R-p1-prompt-context-extensionless-line-hint-contract-research.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/102-R-p1-prompt-context-extensionless-line-hint-contract-research.md)
- [103-T-p1-prompt-context-extensionless-line-hint-shell.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/103-T-p1-prompt-context-extensionless-line-hint-shell.md)
- [99-T-p1-prompt-context-path-line-hint-shell.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/99-T-p1-prompt-context-path-line-hint-shell.md)

## Expected Touch Points

- `docs/execution-plan/104-T-p1-prompt-context-qualified-extensionless-path-contract-research.md`
- `docs/execution-plan/104-R-p1-prompt-context-qualified-extensionless-path-contract-research.md`
- `docs/execution-plan/105-T-p1-prompt-context-qualified-extensionless-path-shell.md`

## Deliverable

- a research task that decides whether repo-relative extensionless paths should be the next prompt-context hint family and authors the next shell slice

## Done When

- the accepted prompt shape for qualified extensionless paths is explicit
- the boundary between file-path aliases and true module aliases is clear
- the next implementation slice is recoverable from disk

## Completion Notes

- added [104-R-p1-prompt-context-qualified-extensionless-path-contract-research.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/104-R-p1-prompt-context-qualified-extensionless-path-contract-research.md)
- chose repo-relative extensionless paths like `src/db:2` as the next safe follow-up instead of language-specific module aliases
- authored the follow-on execution slice as `105-T-p1-prompt-context-qualified-extensionless-path-shell.md`

## Carry Forward To Next Task

Next task:

- `105-T-p1-prompt-context-qualified-extensionless-path-shell.md`

Carry forward:

- keep matching path-shaped and repo-relative, not language-specific
- preserve exact-path, basename, and bare stem `:line` behavior
- keep ambiguous or partial labels on the existing fallback path

Open points:

- whether any later slice should support true module-style aliases such as `crate::db`
