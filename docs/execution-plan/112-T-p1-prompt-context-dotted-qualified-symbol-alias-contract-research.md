---
doc_type: task
task_id: 112
title: P1 prompt_context dotted qualified symbol alias contract research
status: done
sprint: tokenizor-upgrade-foundation
parent_plan: 05-P-validation-and-backlog.md
prev_task: 111-T-p1-prompt-context-qualified-symbol-alias-shell.md
next_task: 113-T-p1-prompt-context-dotted-qualified-symbol-alias-shell.md
created: 2026-03-12
updated: 2026-03-12
---
# Task 112: P1 Prompt Context Dotted Qualified Symbol Alias Contract Research

## Objective

- define the first explicit non-Rust qualified-symbol prompt contract for exact dotted aliases like `pkg.db.connect`

## Why This Exists

- task 111 makes exact qualified symbol aliases work for the current namespaced route, but the contract is still documented with Rust-style `::` examples
- prompt-context already derives dotted module aliases for Python, so the next small decision is whether to bless those exact dotted forms explicitly
- this keeps the prompt-context precision lane language-aware without broadening into arbitrary dotted property parsing

## Read Before Work

- [111-T-p1-prompt-context-qualified-symbol-alias-shell.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/111-T-p1-prompt-context-qualified-symbol-alias-shell.md)
- [110-R-p1-prompt-context-qualified-symbol-alias-contract-research.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/110-R-p1-prompt-context-qualified-symbol-alias-contract-research.md)
- [106-R-p1-prompt-context-qualified-module-alias-contract-research.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/106-R-p1-prompt-context-qualified-module-alias-contract-research.md)

## Expected Touch Points

- `docs/execution-plan/112-T-p1-prompt-context-dotted-qualified-symbol-alias-contract-research.md`
- `docs/execution-plan/112-R-p1-prompt-context-dotted-qualified-symbol-alias-contract-research.md`
- `docs/execution-plan/113-T-p1-prompt-context-dotted-qualified-symbol-alias-shell.md`

## Deliverable

- a research task that chooses the first dotted qualified-symbol prompt shape and authors the next shell slice

## Done When

- the accepted dotted qualified-symbol syntax is explicit
- the boundary between exact dotted aliases and generic member-access text is clear
- the next implementation slice is recoverable from disk

## Completion Notes

- added [112-R-p1-prompt-context-dotted-qualified-symbol-alias-contract-research.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/112-R-p1-prompt-context-dotted-qualified-symbol-alias-contract-research.md)
- chose exact dotted qualified symbol aliases rooted in current module-alias derivation as the next safe non-Rust extension
- authored the follow-on execution slice as `113-T-p1-prompt-context-dotted-qualified-symbol-alias-shell.md`

## Carry Forward To Next Task

Next task:

- `113-T-p1-prompt-context-dotted-qualified-symbol-alias-shell.md`

Carry forward:

- keep dotted qualified symbol aliases exact and boundary-aware
- preserve Rust `::` aliases and the existing file/module-hint fallbacks
- avoid broadening this slice into generic dotted property-chain parsing

Open points:

- whether later slices should add explicit dotted alias derivation for languages beyond Python
