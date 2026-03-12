---
doc_type: task
task_id: 117
title: P1 prompt_context slash qualified symbol alias shell
status: done
sprint: tokenizor-upgrade-foundation
parent_plan: 05-P-validation-and-backlog.md
prev_task: 116-T-p1-prompt-context-slash-qualified-symbol-alias-contract-research.md
next_task: 118-T-p1-prompt-context-slash-qualified-symbol-line-hint-contract-research.md
created: 2026-03-12
updated: 2026-03-12
---
# Task 117: P1 Prompt Context Slash Qualified Symbol Alias Shell

## Objective

- let prompt-context consume exact slash-qualified symbol aliases derived from normalized module paths and route them into the exact-selector symbol-context lane

## Why This Exists

- task 116 chooses exact slash-qualified symbol aliases as the next language-expansion candidate after Rust `::` and dotted aliases
- slash-qualified aliases need an explicit contract so they do not collide with ordinary file and path hints

## Read Before Work

- [116-R-p1-prompt-context-slash-qualified-symbol-alias-contract-research.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/116-R-p1-prompt-context-slash-qualified-symbol-alias-contract-research.md)
- [116-T-p1-prompt-context-slash-qualified-symbol-alias-contract-research.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/116-T-p1-prompt-context-slash-qualified-symbol-alias-contract-research.md)
- [115-T-p1-prompt-context-dotted-qualified-symbol-line-hint-shell.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/115-T-p1-prompt-context-dotted-qualified-symbol-line-hint-shell.md)

## Expected Touch Points

- `src/sidecar/handlers.rs`
- `tests/sidecar_integration.rs`

## Deliverable

- a prompt-context shell that explicitly accepts exact slash-qualified symbol aliases and preserves plain-path fallback behavior

## Done When

- exact slash-qualified symbol aliases resolve through the exact file+symbol path
- plain path hints and partial slash aliases do not activate exact selection
- existing file hints, Rust `::`, and dotted alias behavior stay intact
- focused tests cover the slash exact route and its guardrail behavior

## Completion Notes

- prompt-context now accepts exact slash-qualified symbol aliases derived from normalized JS/TS module paths such as `src/utils/connect`
- slash-qualified aliases route through the exact file+symbol selector lane without widening general file-hint module matching
- continued slash aliases like `src/utils/connect/more` stay on the fallback path instead of collapsing to one exact file

## Carry Forward To Next Task

Next task:

- `118-T-p1-prompt-context-slash-qualified-symbol-line-hint-contract-research.md`

Carry forward:

- keep slash-qualified symbol aliases exact and boundary-aware
- preserve existing path/file hints and earlier alias routes
- avoid broadening this slice into fuzzy import-string parsing

Open points:

- whether slash-qualified symbol `:line` hints should become an explicit contract next
