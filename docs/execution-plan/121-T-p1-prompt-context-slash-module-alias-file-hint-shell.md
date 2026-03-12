---
doc_type: task
task_id: 121
title: P1 prompt_context slash module alias file hint shell
status: done
sprint: tokenizor-upgrade-foundation
parent_plan: 05-P-validation-and-backlog.md
prev_task: 120-T-p1-prompt-context-slash-module-alias-file-hint-contract-research.md
next_task: 122-T-p1-prompt-context-slash-module-alias-line-hint-contract-research.md
created: 2026-03-12
updated: 2026-03-12
---
# Task 121: P1 Prompt Context Slash Module Alias File Hint Shell

## Objective

- let prompt-context consume exact normalized slash module aliases like `src/utils` as file hints for file-only and combined file+symbol prompts while preserving exact slash-qualified symbol alias priority

## Why This Exists

- task 120 chooses exact no-line slash module aliases as the next small prompt-context improvement after the slash-qualified symbol family
- exact normalized module aliases should behave like existing exact file hints when they identify one indexed JS or TS module file

## Read Before Work

- [120-R-p1-prompt-context-slash-module-alias-file-hint-contract-research.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/120-R-p1-prompt-context-slash-module-alias-file-hint-contract-research.md)
- [120-T-p1-prompt-context-slash-module-alias-file-hint-contract-research.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/120-T-p1-prompt-context-slash-module-alias-file-hint-contract-research.md)
- [117-T-p1-prompt-context-slash-qualified-symbol-alias-shell.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/117-T-p1-prompt-context-slash-qualified-symbol-alias-shell.md)

## Expected Touch Points

- `src/sidecar/handlers.rs`
- `tests/sidecar_integration.rs`

## Deliverable

- a prompt-context shell that accepts exact normalized slash module aliases as file hints and routes matching prompts into the exact file or file+symbol lanes

## Done When

- exact normalized slash module aliases activate file hints without `:line`
- partial or continued slash aliases do not activate exact selection
- exact slash-qualified symbol aliases keep priority over the new file-hint route
- focused tests cover the new slash module-alias file-hint path and its guardrails

## Completion Notes

- extended prompt-context so exact normalized slash module aliases like `src/utils` can activate the file-hint lane for JS and TS modules
- shared the normalized slash module-alias helper between the file-hint lane and the slash-qualified symbol lane while preserving exact boundary behavior
- added focused handler coverage for file-only slash module aliases, combined file+symbol prompts, and partial or continued guardrails
- added endpoint coverage for the combined slash module-alias route

## Carry Forward To Next Task

Next task:

- `122-T-p1-prompt-context-slash-module-alias-line-hint-contract-research.md`

Carry forward:

- keep slash module aliases exact and boundary-aware
- preserve slash-qualified symbol alias priority and existing path-shaped fallback behavior
- avoid broadening this slice into fuzzy slash-path guessing

Open points:

- whether exact normalized slash module aliases should also support direct `:line` hints like `src/utils:3`
