---
doc_type: task
task_id: 113
title: P1 prompt_context dotted qualified symbol alias shell
status: done
sprint: tokenizor-upgrade-foundation
parent_plan: 05-P-validation-and-backlog.md
prev_task: 112-T-p1-prompt-context-dotted-qualified-symbol-alias-contract-research.md
next_task: 114-T-p1-prompt-context-dotted-qualified-symbol-line-hint-contract-research.md
created: 2026-03-12
updated: 2026-03-12
---
# Task 113: P1 Prompt Context Dotted Qualified Symbol Alias Shell

## Objective

- let prompt-context consume exact dotted qualified symbol aliases like `pkg.db.connect` and route them into the exact-selector symbol-context lane

## Why This Exists

- task 112 makes exact dotted qualified symbol aliases the next explicit non-Rust prompt-context boundary
- dotted aliases should only become a contract when they are covered by focused route and guardrail tests

## Read Before Work

- [112-R-p1-prompt-context-dotted-qualified-symbol-alias-contract-research.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/112-R-p1-prompt-context-dotted-qualified-symbol-alias-contract-research.md)
- [112-T-p1-prompt-context-dotted-qualified-symbol-alias-contract-research.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/112-T-p1-prompt-context-dotted-qualified-symbol-alias-contract-research.md)
- [111-T-p1-prompt-context-qualified-symbol-alias-shell.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/111-T-p1-prompt-context-qualified-symbol-alias-shell.md)

## Expected Touch Points

- `src/sidecar/handlers.rs`
- `tests/sidecar_integration.rs`

## Deliverable

- a prompt-context shell that explicitly accepts exact dotted qualified symbol aliases and keeps dotted continuation guardrails intact

## Done When

- exact dotted qualified symbol aliases resolve through the exact file+symbol path
- continued or partial dotted aliases do not activate exact selection
- existing Rust `::` aliases and file/module-hint behavior stay intact
- focused tests cover the dotted exact route and its guardrail behavior

## Completion Notes

- added focused prompt-context coverage for exact dotted qualified symbol aliases like `pkg.db.connect`
- confirmed continued dotted chains like `pkg.db.connect.more` do not activate exact selection
- the existing exact qualified-symbol handler route from task 111 already covered dotted aliases once the dependent fixture included the module import edge that exact reference collection requires

## Carry Forward To Next Task

Next task:

- `114-T-p1-prompt-context-dotted-qualified-symbol-line-hint-contract-research.md`

Carry forward:

- keep dotted qualified symbol aliases exact and boundary-aware
- preserve the existing Rust `::` route and file/module fallbacks
- avoid broadening this slice into generic dotted property-chain parsing

Open points:

- whether later slices should make dotted qualified symbol `:line` hints an explicit contract
