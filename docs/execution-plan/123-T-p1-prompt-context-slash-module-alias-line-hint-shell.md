---
doc_type: task
task_id: 123
title: P1 prompt_context slash module alias line hint shell
status: done
sprint: tokenizor-upgrade-foundation
parent_plan: 05-P-validation-and-backlog.md
prev_task: 122-T-p1-prompt-context-slash-module-alias-line-hint-contract-research.md
next_task: 124-T-p1-get-file-content-around-line-contract-research.md
created: 2026-03-12
updated: 2026-03-12
---
# Task 123: P1 Prompt Context Slash Module Alias Line Hint Shell

## Objective

- let prompt-context consume exact normalized slash module aliases with direct `:line` hints like `src/utils:3 connect` and route them into the exact-selector symbol-context lane

## Why This Exists

- task 122 makes the slash module-alias `:line` form the next explicit selector contract
- slash module aliases need the same explicit disambiguation coverage already added for other module-alias and slash-qualified selector families

## Read Before Work

- [122-R-p1-prompt-context-slash-module-alias-line-hint-contract-research.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/122-R-p1-prompt-context-slash-module-alias-line-hint-contract-research.md)
- [122-T-p1-prompt-context-slash-module-alias-line-hint-contract-research.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/122-T-p1-prompt-context-slash-module-alias-line-hint-contract-research.md)
- [121-T-p1-prompt-context-slash-module-alias-file-hint-shell.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/121-T-p1-prompt-context-slash-module-alias-file-hint-shell.md)

## Expected Touch Points

- `src/sidecar/handlers.rs`
- `tests/sidecar_integration.rs`

## Deliverable

- a prompt-context shell that explicitly accepts slash module aliases with direct `:line` hints and preserves unrelated-number guardrails

## Done When

- exact slash module aliases with `:line` disambiguate duplicate same-name symbols in one matched file
- unrelated colon numbers do not activate slash module-alias line selection
- existing no-line slash module aliases, slash-qualified symbol priority, and `line N` behavior stay intact
- focused tests cover the slash module-alias `:line` route and its guardrail behavior

## Completion Notes

- added focused handler coverage for exact slash module-alias `:line` prompts such as `src/utils:3 connect`
- added a guardrail test that unrelated colon numbers like `build:3` do not disambiguate slash module aliases
- added endpoint coverage for the exact slash module-alias `:line` route
- verified the shared file-hint line parser already satisfied this contract, so the shell landed as test-only coverage

## Carry Forward To Next Task

Next task:

- `124-T-p1-get-file-content-around-line-contract-research.md`

Carry forward:

- keep slash module alias line hints exact and boundary-aware
- preserve existing no-line slash module-alias behavior, slash-qualified symbol priority, and `line N` fallback
- avoid broadening this slice into generic slash-path number inference

Open points:

- whether `get_file_content` should next add a centered `around_line` read mode before broader `around_match` and chunking work
