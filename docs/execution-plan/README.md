# Tokenizor Execution Plan Split

This directory is the low-context reading path for `TOKENIZOR_CONTEXT_AND_EXECUTION_PLAN.md`.

The root-level file remains the authoritative source document.
These split files exist to reduce context cost and make targeted reading easier.

Authority order:

1. `TOKENIZOR_CONTEXT_AND_EXECUTION_PLAN.md`
2. the split files in this directory
3. the summary in `docs/summaries/source-tokenizor-context-and-execution-plan.md`

Intent-preservation note:

- each split file contains a generated five-line header followed by a verbatim slice of the original source
- the five split files reconstruct the original document byte-for-byte once those generated headers are removed
- implementation decisions should be made from the split files or the original source, not from the summary alone

Recommended reading order:

1. [01-P-overview-and-principles.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/01-P-overview-and-principles.md)
2. [03-P-architecture-and-guardrails.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/03-P-architecture-and-guardrails.md)
3. [04-P-phase-plan.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/04-P-phase-plan.md)
4. [05-P-validation-and-backlog.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/05-P-validation-and-backlog.md)
5. [02-P-workstreams-and-tool-surface.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/02-P-workstreams-and-tool-surface.md)

Use the split set by intent:

- `01-P-overview-and-principles.md`: product intent, success hierarchy, blockers, and current code observations
- `02-P-workstreams-and-tool-surface.md`: workstreams, public tool design, routing guidance, and target steady-state tool surface
- `03-P-architecture-and-guardrails.md`: required refactors, dual-lane model, and performance/reliability constraints
- `04-P-phase-plan.md`: dependency-ordered execution sequence from Phase 0 through Phase 9
- `05-P-validation-and-backlog.md`: tests, data model additions, output standards, backlog, anti-goals, and success definition

Adjacent working-note convention:

- plan or phase documents should live beside the source as `NN-P-name.md`
- task documents should live beside the source as `NN-T-name.md`
- discovery notes for a split doc should live beside it as `NN-D-name.md`
- research notes for a split doc should live beside it as `NN-R-name.md`
- example:
- `01-P-overview-and-principles.md`
- `10-T-phase0-benchmark-scenarios.md`
- `01-D-overview-and-principles.md`
- `01-R-overview-and-principles.md`
- if a document needs multiple rounds, append a short topic suffix instead of changing the numbering scheme

Execution slicing rule:

- no implementation task should be sized such that `/compact` would be expected mid-task
- if a task would require broad rereading, split it before coding
- default unit of work: one public tool addition or upgrade, one internal substrate change, or one focused ranking/read-path improvement
- each slice should end at a stable checkpoint with tests, benchmarks, or a written note explaining why they are deferred
- if a slice grows beyond a few core files plus its tests and docs, split it again
- if a slice needs multiple independent design decisions, write discovery or research first and break it into follow-on slices
- prefer more small completed checkpoints over one large partially-finished change

Practical triggers to split further:

- you need to reread more than two split plan files in the same implementation pass
- you expect to touch more than one major subsystem at once, such as protocol plus query engine plus storage plus watcher behavior
- you cannot describe the deliverable in one sentence without using "and" several times
- you cannot verify the slice with a focused test or benchmark pass

Task workflow:

- executable work should live in `NN-T-*` files with explicit `status`
- only one task may be `in_progress` at a time
- use `python execution/task_queue.py resume docs/execution-plan` to recover or promote the active task
- use `python execution/task_queue.py complete docs/execution-plan <task-id-or-file> --advance` when a task is fully complete
- task bodies should carry enough forward context that the next task can resume without relying on chat history

Use the original monolith when exact wording or full provenance matters:

- [TOKENIZOR_CONTEXT_AND_EXECUTION_PLAN.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/TOKENIZOR_CONTEXT_AND_EXECUTION_PLAN.md)
