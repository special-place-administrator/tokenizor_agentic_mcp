---
doc_type: task
task_id: 115
title: P1 prompt_context dotted qualified symbol line hint shell
status: done
sprint: tokenizor-upgrade-foundation
parent_plan: 05-P-validation-and-backlog.md
prev_task: 114-T-p1-prompt-context-dotted-qualified-symbol-line-hint-contract-research.md
next_task: 116-T-p1-prompt-context-slash-qualified-symbol-alias-contract-research.md
created: 2026-03-12
updated: 2026-03-12
---
# Task 115: P1 Prompt Context Dotted Qualified Symbol Line Hint Shell

## Objective

- let prompt-context consume exact dotted qualified symbol aliases with direct `:line` hints like `pkg.db.connect:2` and route them into the exact-selector symbol-context lane

## Why This Exists

- task 114 makes the dotted qualified-symbol `:line` form the next explicit selector contract
- dotted aliases need the same explicit disambiguation coverage that Rust qualified aliases now have

## Read Before Work

- [114-R-p1-prompt-context-dotted-qualified-symbol-line-hint-contract-research.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/114-R-p1-prompt-context-dotted-qualified-symbol-line-hint-contract-research.md)
- [114-T-p1-prompt-context-dotted-qualified-symbol-line-hint-contract-research.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/114-T-p1-prompt-context-dotted-qualified-symbol-line-hint-contract-research.md)
- [113-T-p1-prompt-context-dotted-qualified-symbol-alias-shell.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/113-T-p1-prompt-context-dotted-qualified-symbol-alias-shell.md)

## Expected Touch Points

- `src/sidecar/handlers.rs`
- `tests/sidecar_integration.rs`

## Deliverable

- a prompt-context shell that explicitly accepts dotted qualified symbol aliases with direct `:line` hints and preserves unrelated-number guardrails

## Done When

- exact dotted qualified aliases with `:line` disambiguate duplicate same-name symbols in one matched file
- unrelated colon numbers do not activate dotted exact selection
- existing dotted no-line aliases, Rust `::` aliases, and `line N` behavior stay intact
- focused tests cover the dotted `:line` route and its guardrail behavior

## Completion Notes

- added focused prompt-context coverage for exact dotted qualified symbol aliases with direct `:line` hints like `pkg.db.connect:4`
- confirmed dotted alias `:line` hints disambiguate duplicate same-name symbols in one matched file
- confirmed unrelated colon numbers do not count as dotted selector lines

## Carry Forward To Next Task

Next task:

- `116-T-p1-prompt-context-slash-qualified-symbol-alias-contract-research.md`

Carry forward:

- keep dotted alias line hints exact and boundary-aware
- preserve existing dotted no-line, Rust `::`, and `line N` behavior
- avoid broadening this slice into unlabeled-number inference

Open points:

- whether later slices should extend explicit exact qualified-symbol aliases to slash-based module conventions
