---
doc_type: task
task_id: 103
title: P1 prompt_context extensionless alias line hint shell
status: done
sprint: tokenizor-upgrade-foundation
parent_plan: 05-P-validation-and-backlog.md
prev_task: 102-T-p1-prompt-context-extensionless-line-hint-contract-research.md
next_task: 104-T-p1-prompt-context-qualified-extensionless-path-contract-research.md
created: 2026-03-12
updated: 2026-03-12
---
# Task 103: P1 Prompt Context Extensionless Alias Line Hint Shell

## Objective

- let prompt-context consume a unique extensionless alias like `db:line` for combined file+symbol prompts while preserving exact-path and basename-derived `:line`

## Why This Exists

- task 102 narrows the next prompt-context ergonomic improvement to extensionless aliases derived from the already trusted file hint
- prompt-context already has the necessary guardrail, so this is the smallest safe follow-up

## Read Before Work

- [102-R-p1-prompt-context-extensionless-line-hint-contract-research.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/102-R-p1-prompt-context-extensionless-line-hint-contract-research.md)
- [102-T-p1-prompt-context-extensionless-line-hint-contract-research.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/102-T-p1-prompt-context-extensionless-line-hint-contract-research.md)
- [101-T-p1-prompt-context-basename-line-hint-shell.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/101-T-p1-prompt-context-basename-line-hint-shell.md)

## Expected Touch Points

- `src/sidecar/handlers.rs`
- `tests/sidecar_integration.rs`

## Deliverable

- a prompt-context shell that accepts unique extensionless alias `name:line` hints for combined file+symbol prompts and feeds them into exact-selector symbol context

## Done When

- unique extensionless alias `name:line` prompts resolve through `symbol_line`
- unrelated or ambiguous bare aliases do not activate line hints
- existing exact-path, basename-derived, and explicit `line N` support stay intact
- focused tests cover the new extensionless alias path and its guardrail behavior

## Completion Notes

- extended prompt-context file-hint matching to recognize unique extensionless `stem:line` aliases such as `db:2`
- kept exact-path, basename-derived, and explicit `line N` disambiguation behavior unchanged
- preserved ambiguity fallback for repeated stems like `src/db.rs` and `tests/db.py`
- added focused unit and endpoint coverage that proves the exact-selector lane excludes unrelated same-name hits

## Carry Forward To Next Task

Next task:

- `104-T-p1-prompt-context-qualified-extensionless-path-contract-research.md`

Carry forward:

- keep extensionless aliases anchored to the resolved file hint
- preserve the current exact-selector fallback when no usable line hint exists
- avoid broadening this slice into generic module or symbol parsing

Open points:

- OPEN: whether the next path should be repo-relative extensionless paths like `src/db:2` before any true module-style aliases
