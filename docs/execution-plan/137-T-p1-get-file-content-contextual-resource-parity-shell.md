---
doc_type: task
task_id: 137
title: P1 get_file_content contextual resource parity shell
status: done
sprint: tokenizor-upgrade-foundation
parent_plan: 05-P-validation-and-backlog.md
prev_task: 136-T-p1-get-file-content-contextual-resource-parity-contract-research.md
next_task:
created: 2026-03-12
updated: 2026-03-12
---
# Task 137: P1 Get File Content Contextual Resource Parity Shell

## Objective

- let the file-content resource template request `around_line`, `around_match`, and `context_lines`

## Why This Exists

- task 135 aligned ordinary reads between the tool and the file-content resource template
- task 136 chose contextual parity as the next smallest gap on the resource surface

## Read Before Work

- [136-R-p1-get-file-content-contextual-resource-parity-contract-research.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/136-R-p1-get-file-content-contextual-resource-parity-contract-research.md)
- [136-T-p1-get-file-content-contextual-resource-parity-contract-research.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/136-T-p1-get-file-content-contextual-resource-parity-contract-research.md)
- [135-T-p1-get-file-content-ordinary-read-resource-parity-shell.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/135-T-p1-get-file-content-ordinary-read-resource-parity-shell.md)

## Expected Touch Points

- `src/protocol/resources.rs`
- `src/protocol/tools.rs`
- `README.md`

## Deliverable

- a file-content resource template that can request the existing contextual read modes without changing the current ordinary-read resource behavior

## Done When

- the file-content resource template accepts `around_line`, `around_match`, and `context_lines`
- those fields forward into `GetFileContentInput`
- ordinary-read resource behavior from task 135 stays unchanged
- focused resource tests cover both contextual modes

## Completion Notes

- extended the file-content resource template with `around_line`, `around_match`, and `context_lines`
- threaded those contextual fields through the resource URI parser into `GetFileContentInput`
- added focused resource tests for both contextual URI modes while keeping the ordinary-read resource test green

## Carry Forward To Next Task

Next task:

- `TBD`

Carry forward:

- keep the contextual parity slice separate from symbolic and chunked resource parity
- preserve the current default resource behavior for ordinary reads
