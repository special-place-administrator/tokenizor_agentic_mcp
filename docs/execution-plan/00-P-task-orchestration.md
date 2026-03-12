# Task Orchestration

This document defines how executable task slices are stored and advanced on disk.

Purpose:

- make session resets safe
- remove the need to restate the current task manually
- keep execution state recoverable from files instead of chat memory

## Task File Convention

- `NN-T-name.md` = executable task slice
- `status` must be one of: `pending`, `in_progress`, `done`
- only one task may be `in_progress` at a time
- tasks should be granular enough that they do not require `/compact` mid-task

Recommended task header fields:

- `doc_type`
- `task_id`
- `title`
- `status`
- `sprint`
- `parent_plan`
- `prev_task`
- `next_task`
- `created`
- `updated`

## Daisy-Chain Rule

Each task file should point backward and forward:

- `prev_task`: the immediately preceding task in the execution chain
- `next_task`: the next intended task if the current one completes cleanly

Each task body should also include a carry-forward section so the next session does not have to infer the context transition.

## Status Transition Rule

At session start:

1. run `python execution/task_queue.py resume docs/execution-plan`
2. if a task is already `in_progress`, continue it
3. if no task is `in_progress`, the script promotes the next `pending` task
4. read that task plus its linked plan/discovery/research docs

When a task starts:

- ensure its status is `in_progress`
- update `updated`

When a task completes:

1. fill in the completion notes and carry-forward section
2. run `python execution/task_queue.py complete docs/execution-plan <task-id-or-file> --advance`
3. this marks the task `done`
4. if a next pending task exists, it is promoted to `in_progress`

## Selection Rule

The deterministic selection order is:

1. current `in_progress` task, if one exists
2. otherwise the explicit `next_task` of the task just completed, if it is still `pending`
3. otherwise the first `pending` task by numeric task id

## Authoring Rule

- use plan docs (`NN-P-*`) for intent and acceptance criteria
- use discovery docs (`NN-D-*`) for edit-point finding and task shaping
- use research docs (`NN-R-*`) for design tradeoffs
- use task docs (`NN-T-*`) only for one small executable slice at a time

## Resume Prompt Rule

Fresh sessions should not ask the user to restate the task if the task queue is current.

The prompt should:

1. read `AGENTS.md`
2. read the latest handoff
3. run the task queue helper
4. read the selected task and its linked context docs
5. continue work from there
