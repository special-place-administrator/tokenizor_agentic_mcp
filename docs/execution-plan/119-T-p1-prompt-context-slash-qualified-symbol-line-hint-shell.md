---
doc_type: task
task_id: 119
title: P1 prompt_context slash qualified symbol line hint shell
status: done
sprint: tokenizor-upgrade-foundation
parent_plan: 05-P-validation-and-backlog.md
prev_task: 118-T-p1-prompt-context-slash-qualified-symbol-line-hint-contract-research.md
next_task: 120-T-p1-prompt-context-slash-module-alias-file-hint-contract-research.md
created: 2026-03-12
updated: 2026-03-12
---
# Task 119: P1 Prompt Context Slash Qualified Symbol Line Hint Shell

## Objective

- let prompt-context consume exact slash-qualified symbol aliases with direct `:line` hints like `src/utils/connect:2` and route them into the exact-selector symbol-context lane

## Why This Exists

- task 118 makes the slash-qualified symbol `:line` form the next explicit selector contract
- slash-qualified aliases need the same explicit disambiguation coverage already added for the Rust and dotted families

## Read Before Work

- [118-R-p1-prompt-context-slash-qualified-symbol-line-hint-contract-research.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/118-R-p1-prompt-context-slash-qualified-symbol-line-hint-contract-research.md)
- [118-T-p1-prompt-context-slash-qualified-symbol-line-hint-contract-research.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/118-T-p1-prompt-context-slash-qualified-symbol-line-hint-contract-research.md)
- [117-T-p1-prompt-context-slash-qualified-symbol-alias-shell.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/117-T-p1-prompt-context-slash-qualified-symbol-alias-shell.md)

## Expected Touch Points

- `src/sidecar/handlers.rs`
- `tests/sidecar_integration.rs`

## Deliverable

- a prompt-context shell that explicitly accepts slash-qualified symbol aliases with direct `:line` hints and preserves unrelated-number guardrails

## Done When

- exact slash-qualified aliases with `:line` disambiguate duplicate same-name symbols in one matched file
- unrelated colon numbers do not activate slash exact selection
- existing slash no-line aliases, earlier alias families, and `line N` behavior stay intact
- focused tests cover the slash `:line` route and its guardrail behavior

## Completion Notes

- added focused handler coverage for exact slash-qualified aliases with direct `:line` hints such as `src/utils/connect:3`
- added a guardrail test that unrelated colon numbers like `build:3` do not disambiguate slash-qualified symbol aliases
- added endpoint coverage for the exact slash-qualified alias `:line` route
- verified the shared exact-alias line-hint parser already satisfied this contract, so the shell landed as test-only coverage

## Carry Forward To Next Task

Next task:

- `120-T-p1-prompt-context-slash-module-alias-file-hint-contract-research.md`

Carry forward:

- keep slash alias line hints exact and boundary-aware
- preserve existing slash no-line, earlier alias-family, and `line N` behavior
- avoid broadening this slice into generic path-number inference

Open points:

- whether normalized slash module aliases like `src/utils` should also act as no-line file hints
