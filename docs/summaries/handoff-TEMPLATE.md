# Session Handoff Template

Date: YYYY-MM-DD
Topic: <short handoff topic>
Status: <checkpoint status>

## What Was Done

- <completed item>
- <completed item>
- <completed item>

## Current Canonical Reading Set

- `AGENTS.md`
- `docs/execution-plan/README.md`
- `<primary plan doc>`
- `<primary discovery/research doc>`
- `<current handoff doc>`

## Working Conventions

- `NN-P-*` = plan or phase docs
- `NN-D-*` = discovery notes
- `NN-R-*` = research notes
- source plan authority is higher than summaries
- implementation slices must be small enough that `/compact` would not be expected mid-task

## Current Sprint Or Milestone Context

- Sprint or milestone: <name>
- Theme: <short description>
- Session goal: <exact deliverable for next session>

## Current Task Context

- Active task: <task name>
- Why this task now: <reason>
- Definition of done for next slice:
- <done condition>
- <done condition>

## Important Intent Rules

- <rule from source plan that must not be lost>
- <rule from source plan that must not be lost>
- <rule from source plan that must not be lost>

## Discovery And Research State

- Discovery completed:
- <doc path or "none">
- Research completed:
- <doc path or "none">
- Required before coding:
- <what still needs discovery or research, if any>

## Files Most Likely Relevant Next

- `<path>`
- `<path>`
- `<path>`

## Open Questions

- OPEN: <question>
- OPEN: <question>

## Risks Or Constraints

- <constraint>
- <constraint>

## Recommended Next Smallest Slice

- <single granular slice>
- Verification:
- <focused test or benchmark>

## What Not To Re-Read First

- <large file or irrelevant area to avoid reopening immediately>
- <large file or irrelevant area to avoid reopening immediately>

## Resume Prompt

```text
Resume work on `tokenizor_agentic_mcp`.

First read:
1. `AGENTS.md`
2. `<this handoff path>`
3. `docs/execution-plan/README.md`
4. `docs/execution-plan/00-P-task-orchestration.md`

Then load only the minimum relevant files using this authority order:
1. `TOKENIZOR_CONTEXT_AND_EXECUTION_PLAN.md`
2. `docs/execution-plan/NN-P-*.md`
3. `docs/execution-plan/NN-T-*.md`
4. `docs/execution-plan/NN-D-*.md`
5. `docs/execution-plan/NN-R-*.md`
6. `docs/summaries/*.md`

Before choosing work:
- run `python execution/task_queue.py resume docs/execution-plan`
- if a task is already `in_progress`, continue it
- if no task is `in_progress`, the helper promotes the next `pending` task
- read the selected `NN-T-*` file and the linked `NN-P-*`, `NN-D-*`, and `NN-R-*` docs

Current sprint/context:
- Sprint: optional if not already encoded in the active task file
- Task: infer from the active `NN-T-*` file
- Goal of this session: infer from the active `NN-T-*` file

Rules:
- Do not assume prior conversation context exists.
- Use split plan docs first; open the monolith only if exact wording or provenance is needed.
- Preserve the intent of the source plan.
- Do discovery/research first when the task changes query semantics, ranking, index structure, memory profile, watcher behavior, or public tool contracts.
- Keep implementation slices small enough that `/compact` would not be expected mid-task.
- Prefer: discovery -> research -> one small implementation slice -> focused verification.
- When the active task completes, update its notes, mark it `done`, and advance the queue with `python execution/task_queue.py complete docs/execution-plan <task-id-or-file> --advance`.

Start by summarizing the current state from the handoff and the active task file, then continue that task.
```
