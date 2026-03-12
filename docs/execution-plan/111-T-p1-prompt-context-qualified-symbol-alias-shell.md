---
doc_type: task
task_id: 111
title: P1 prompt_context qualified symbol alias shell
status: done
sprint: tokenizor-upgrade-foundation
parent_plan: 05-P-validation-and-backlog.md
prev_task: 110-T-p1-prompt-context-qualified-symbol-alias-contract-research.md
next_task: 112-T-p1-prompt-context-dotted-qualified-symbol-alias-contract-research.md
created: 2026-03-12
updated: 2026-03-12
---
# Task 111: P1 Prompt Context Qualified Symbol Alias Shell

## Objective

- let prompt-context consume exact qualified symbol aliases like `crate::db::connect` and route them into the exact-selector symbol-context lane

## Why This Exists

- task 110 chooses exact qualified symbol aliases as the next prompt-context precision bridge after module aliases
- some prompts already name the full qualified symbol, and that should map directly to the exact-selector path when possible

## Read Before Work

- [110-R-p1-prompt-context-qualified-symbol-alias-contract-research.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/110-R-p1-prompt-context-qualified-symbol-alias-contract-research.md)
- [110-T-p1-prompt-context-qualified-symbol-alias-contract-research.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/110-T-p1-prompt-context-qualified-symbol-alias-contract-research.md)
- [109-T-p1-prompt-context-module-alias-file-hint-shell.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/109-T-p1-prompt-context-module-alias-file-hint-shell.md)

## Expected Touch Points

- `src/sidecar/handlers.rs`
- `tests/sidecar_integration.rs`

## Deliverable

- a prompt-context shell that accepts exact qualified symbol aliases and routes them into the exact-selector symbol-context lane

## Done When

- exact qualified symbol aliases resolve through the exact file+symbol path
- partial or fuzzy qualified symbols do not activate exact selection
- existing exact path, module alias, and file-hint behavior stay intact
- focused tests cover the new qualified-symbol route and its guardrail behavior

## Completion Notes

- prompt-context now accepts exact qualified symbol aliases like `crate::db::connect` and routes them into the exact file+symbol selector lane
- exact qualified symbol aliases can also carry direct `:line` hints like `crate::db::connect:2`
- continued qualified paths like `crate::db::connect::helper` stay on the fallback path instead of collapsing to one exact file

## Carry Forward To Next Task

Next task:

- `112-T-p1-prompt-context-dotted-qualified-symbol-alias-contract-research.md`

Carry forward:

- keep qualified symbol aliases exact and boundary-aware
- preserve current file/module-hint fallbacks
- avoid broadening this slice into fuzzy namespace guessing or generic property-chain parsing

Open points:

- whether later slices should explicitly support additional non-Rust qualified symbol forms
