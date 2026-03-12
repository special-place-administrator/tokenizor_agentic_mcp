---
doc_type: task
task_id: 102
title: P1 prompt_context extensionless alias line hint contract research
status: done
sprint: tokenizor-upgrade-foundation
parent_plan: 05-P-validation-and-backlog.md
prev_task: 101-T-p1-prompt-context-basename-line-hint-shell.md
next_task: 103-T-p1-prompt-context-extensionless-line-hint-shell.md
created: 2026-03-12
updated: 2026-03-12
---
# Task 102: P1 Prompt Context Extensionless Alias Line Hint Contract Research

## Objective

- define the smallest prompt-context follow-up contract that lets unique extensionless aliases like `db:2` feed the combined file+symbol exact-selector path

## Why This Exists

- task 101 adds basename-derived `file.rs:line`, but many prompts still shorten that to `db:2`
- prompt-context already has a trusted file hint by the time line extraction runs, so this is the smallest next ergonomic bridge
- this slice can improve prompt-driven disambiguation without opening global bare-token parsing

## Read Before Work

- [100-R-p1-prompt-context-basename-line-hint-contract-research.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/100-R-p1-prompt-context-basename-line-hint-contract-research.md)
- [101-T-p1-prompt-context-basename-line-hint-shell.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/101-T-p1-prompt-context-basename-line-hint-shell.md)
- [99-T-p1-prompt-context-path-line-hint-shell.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/99-T-p1-prompt-context-path-line-hint-shell.md)

## Expected Touch Points

- `docs/execution-plan/102-T-p1-prompt-context-extensionless-line-hint-contract-research.md`
- `docs/execution-plan/102-R-p1-prompt-context-extensionless-line-hint-contract-research.md`
- `docs/execution-plan/103-T-p1-prompt-context-extensionless-line-hint-shell.md`

## Deliverable

- a small research task that defines the first extensionless-alias `name:line` contract and authors the next execution slice

## Done When

- the accepted extensionless alias `:line` shape is explicit
- the relationship to existing file-hint and `symbol_line` flows is clear
- the next implementation slice is recoverable from disk

## Completion Notes

- added [102-R-p1-prompt-context-extensionless-line-hint-contract-research.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/102-R-p1-prompt-context-extensionless-line-hint-contract-research.md)
- narrowed extensionless alias parsing to aliases derived from the already resolved file hint
- preserved exact-path `path:line`, basename-derived `file.rs:line`, explicit `line N`, and ambiguity-fallback behavior
- authored the follow-on execution slice as `103-T-p1-prompt-context-extensionless-line-hint-shell.md`

## Carry Forward To Next Task

Next task:

- `103-T-p1-prompt-context-extensionless-line-hint-shell.md`

Carry forward:

- only activate extensionless aliases through the already-resolved file hint
- preserve exact-path and basename-derived `:line` behavior
- keep this slice separate from broader bare-token parsing

Open points:

- route the first shell through the existing resolved-file hint rather than introducing a new alias resolver
