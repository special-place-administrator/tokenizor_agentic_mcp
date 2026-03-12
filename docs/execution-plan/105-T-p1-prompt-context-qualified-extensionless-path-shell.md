---
doc_type: task
task_id: 105
title: P1 prompt_context qualified extensionless path shell
status: done
sprint: tokenizor-upgrade-foundation
parent_plan: 05-P-validation-and-backlog.md
prev_task: 104-T-p1-prompt-context-qualified-extensionless-path-contract-research.md
next_task: 106-T-p1-prompt-context-qualified-module-alias-contract-research.md
created: 2026-03-12
updated: 2026-03-12
---
# Task 105: P1 Prompt Context Qualified Extensionless Path Shell

## Objective

- let prompt-context consume repo-relative extensionless path hints like `src/db:line` for combined file+symbol prompts while preserving the existing exact-path, basename, and bare stem routes

## Why This Exists

- task 104 chooses repo-relative extensionless paths as the next safe disambiguation step after `db:2`
- repeated file stems need a compact path-shaped hint that stays deterministic and file-oriented

## Read Before Work

- [104-R-p1-prompt-context-qualified-extensionless-path-contract-research.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/104-R-p1-prompt-context-qualified-extensionless-path-contract-research.md)
- [104-T-p1-prompt-context-qualified-extensionless-path-contract-research.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/104-T-p1-prompt-context-qualified-extensionless-path-contract-research.md)
- [103-T-p1-prompt-context-extensionless-line-hint-shell.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/103-T-p1-prompt-context-extensionless-line-hint-shell.md)

## Expected Touch Points

- `src/sidecar/handlers.rs`
- `tests/sidecar_integration.rs`

## Deliverable

- a prompt-context shell that accepts unique repo-relative extensionless path hints like `src/db:2` and feeds them into the exact-selector symbol-context lane

## Done When

- repo-relative extensionless path hints resolve through `symbol_line`
- ambiguous or partial path-like aliases do not activate exact selection
- existing exact-path, basename-derived, and bare stem support stay intact
- focused tests cover the new qualified extensionless path route and its guardrail behavior

## Completion Notes

- extended prompt-context file-hint matching to recognize unique repo-relative extensionless path aliases such as `src/db:2`
- gave qualified extensionless paths precedence ahead of bare stem aliases so repeated stems can still disambiguate
- preserved exact-path, basename-derived, bare stem, and explicit `line N` behavior
- added focused unit and endpoint coverage for the new path-shaped alias plus a partial-path fallback guardrail

## Carry Forward To Next Task

Next task:

- `106-T-p1-prompt-context-qualified-module-alias-contract-research.md`

Carry forward:

- keep accepted aliases repo-relative and slash-shaped
- preserve the current fallback behavior when a path hint is not exact enough
- avoid language-specific module parsing in this slice

Open points:

- OPEN: whether future prompt-context slices should accept exact module aliases only when they include an explicit namespace separator
