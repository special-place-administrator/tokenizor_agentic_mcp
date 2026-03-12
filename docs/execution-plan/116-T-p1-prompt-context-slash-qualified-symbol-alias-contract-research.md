---
doc_type: task
task_id: 116
title: P1 prompt_context slash qualified symbol alias contract research
status: done
sprint: tokenizor-upgrade-foundation
parent_plan: 05-P-validation-and-backlog.md
prev_task: 115-T-p1-prompt-context-dotted-qualified-symbol-line-hint-shell.md
next_task: 117-T-p1-prompt-context-slash-qualified-symbol-alias-shell.md
created: 2026-03-12
updated: 2026-03-12
---
# Task 116: P1 Prompt Context Slash Qualified Symbol Alias Contract Research

## Objective

- define whether prompt-context should add exact slash-qualified symbol aliases for module conventions that resolve to normalized slash paths

## Why This Exists

- tasks 111 through 115 make exact qualified-symbol routing explicit for Rust `::` and Python dotted forms
- the remaining adjacent namespace style in the current query layer is slash-based module resolution used by JavaScript and TypeScript
- this is the next natural language-expansion question before any broader semantic prompt parsing

## Read Before Work

- [115-T-p1-prompt-context-dotted-qualified-symbol-line-hint-shell.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/115-T-p1-prompt-context-dotted-qualified-symbol-line-hint-shell.md)
- [112-R-p1-prompt-context-dotted-qualified-symbol-alias-contract-research.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/112-R-p1-prompt-context-dotted-qualified-symbol-alias-contract-research.md)
- [111-T-p1-prompt-context-qualified-symbol-alias-shell.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/111-T-p1-prompt-context-qualified-symbol-alias-shell.md)

## Expected Touch Points

- `docs/execution-plan/116-T-p1-prompt-context-slash-qualified-symbol-alias-contract-research.md`
- `docs/execution-plan/116-R-p1-prompt-context-slash-qualified-symbol-alias-contract-research.md`
- `docs/execution-plan/117-T-p1-prompt-context-slash-qualified-symbol-alias-shell.md`

## Deliverable

- a research task that chooses the first exact slash-qualified symbol prompt shape and authors the next shell slice

## Done When

- the accepted slash-qualified symbol syntax is explicit
- the boundary between exact slash aliases and ordinary path/file hints is clear
- the next implementation slice is recoverable from disk

## Completion Notes

- added [116-R-p1-prompt-context-slash-qualified-symbol-alias-contract-research.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/116-R-p1-prompt-context-slash-qualified-symbol-alias-contract-research.md)
- chose exact slash-qualified symbol aliases rooted in normalized module-path derivation as the next language-expansion candidate
- authored the follow-on execution slice as `117-T-p1-prompt-context-slash-qualified-symbol-alias-shell.md`

## Carry Forward To Next Task

Next task:

- `117-T-p1-prompt-context-slash-qualified-symbol-alias-shell.md`

Carry forward:

- keep slash-qualified symbol aliases exact and boundary-aware
- preserve existing file hints, Rust `::`, and dotted alias routes
- avoid broadening this slice into fuzzy path or import-string parsing

Open points:

- whether slash-qualified symbol aliases should be limited to JavaScript and TypeScript module-path derivation
