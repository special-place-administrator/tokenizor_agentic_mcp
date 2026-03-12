---
doc_type: task
task_id: 120
title: P1 prompt_context slash module alias file hint contract research
status: done
sprint: tokenizor-upgrade-foundation
parent_plan: 05-P-validation-and-backlog.md
prev_task: 119-T-p1-prompt-context-slash-qualified-symbol-line-hint-shell.md
next_task: 121-T-p1-prompt-context-slash-module-alias-file-hint-shell.md
created: 2026-03-12
updated: 2026-03-12
---
# Task 120: P1 Prompt Context Slash Module Alias File Hint Contract Research

## Objective

- define the smallest follow-up contract that lets exact normalized slash module aliases like `src/utils` act as prompt-context file hints even without a symbol-qualified segment or `:line`

## Why This Exists

- task 119 makes exact slash-qualified symbol aliases explicit with direct `:line` hints, but prompts often name a JS/TS module path and symbol separately
- normalized slash module aliases can identify `index.ts` and `index.js` files deterministically in the same way earlier module-alias tasks did for Rust and Python
- this is the next small ergonomic bridge before any broader slash-path guessing discussion

## Read Before Work

- [119-T-p1-prompt-context-slash-qualified-symbol-line-hint-shell.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/119-T-p1-prompt-context-slash-qualified-symbol-line-hint-shell.md)
- [117-T-p1-prompt-context-slash-qualified-symbol-alias-shell.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/117-T-p1-prompt-context-slash-qualified-symbol-alias-shell.md)
- [109-T-p1-prompt-context-module-alias-file-hint-shell.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/109-T-p1-prompt-context-module-alias-file-hint-shell.md)

## Expected Touch Points

- `docs/execution-plan/120-T-p1-prompt-context-slash-module-alias-file-hint-contract-research.md`
- `docs/execution-plan/120-R-p1-prompt-context-slash-module-alias-file-hint-contract-research.md`
- `docs/execution-plan/121-T-p1-prompt-context-slash-module-alias-file-hint-shell.md`

## Deliverable

- a research task that decides whether exact normalized slash module aliases should activate file hints without `:line` and authors the next shell slice

## Done When

- the no-line slash module-alias prompt shape is explicit
- the exact-boundary rule that separates `src/utils` from `src/utilsx` and `src/utils/more` is clear
- the next implementation slice is recoverable from disk

## Completion Notes

- added [120-R-p1-prompt-context-slash-module-alias-file-hint-contract-research.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/120-R-p1-prompt-context-slash-module-alias-file-hint-contract-research.md)
- chose exact normalized slash module aliases as no-line file hints when they match a full module-path boundary
- authored the follow-on execution slice as `121-T-p1-prompt-context-slash-module-alias-file-hint-shell.md`

## Carry Forward To Next Task

Next task:

- `121-T-p1-prompt-context-slash-module-alias-file-hint-shell.md`

Carry forward:

- keep slash module aliases exact and boundary-aware
- preserve current slash-qualified symbol alias priority and path-shaped fallback behavior
- avoid broadening this slice into fuzzy slash-path guessing

Open points:

- whether a later slice should add an explicit slash module-alias `:line` contract
