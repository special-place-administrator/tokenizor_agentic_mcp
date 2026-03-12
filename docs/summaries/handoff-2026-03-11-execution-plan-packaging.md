# Session Handoff: Execution Plan Packaging

Date: 2026-03-11
Topic: Tokenizor upgrade planning context, split execution-plan docs, and task-slicing rules
Status: safe checkpoint before context reset

## What Was Done

- pulled latest `origin/main` to local `main`
- preserved `TOKENIZOR_CONTEXT_AND_EXECUTION_PLAN.md` as the authoritative source
- split that monolith into plan files under `docs/execution-plan/`
- renamed split plan files to the `NN-P-*` convention
- added authority rules:
- source doc first
- split plan docs second
- summary docs last
- added `docs/summaries/source-tokenizor-context-and-execution-plan.md` as orientation-only summary
- added `docs/execution-plan/04-D-phase-plan-task-slicing.md` to enforce granular slices that should not require `/compact` mid-task

## Current Canonical Reading Set

- [TOKENIZOR_CONTEXT_AND_EXECUTION_PLAN.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/TOKENIZOR_CONTEXT_AND_EXECUTION_PLAN.md)
- [README.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/README.md)
- [01-P-overview-and-principles.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/01-P-overview-and-principles.md)
- [03-P-architecture-and-guardrails.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/03-P-architecture-and-guardrails.md)
- [04-P-phase-plan.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/04-P-phase-plan.md)
- [05-P-validation-and-backlog.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/05-P-validation-and-backlog.md)
- [02-P-workstreams-and-tool-surface.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/02-P-workstreams-and-tool-surface.md)

## Working Conventions

- `NN-P-*` = plan or phase docs
- `NN-T-*` = executable task docs
- `NN-D-*` = discovery notes
- `NN-R-*` = research notes
- implementation slices must be small enough that `/compact` would not be expected mid-task
- prefer discovery -> research -> small implementation slice -> focused verification
- use `python execution/task_queue.py resume docs/execution-plan` to recover or promote the active task

## Important Intent Rules

- do not let lightweight summaries override the source plan
- use split plan docs for normal work
- reopen the monolith only when exact wording or provenance matters
- preserve current useful behavior unless measured evidence supports replacement
- research is mandatory before changes to query semantics, ranking, index structure, memory profile, watcher behavior, or public tool contracts

## Known Open Point

- the source plan has one ambiguity:
- the body ties line-numbered read work to Phase 4
- the closing execution guidance says "the line-numbered subset of Phase 3"
- do not silently normalize this; flag it explicitly when planning the affected slice

## Recommended Next Step

- start with a Phase 1 discovery note focused on:
- `src/protocol/tools.rs`
- `src/protocol/format.rs`
- `src/live_index/query.rs`
- `src/live_index/store.rs`
- goal:
- define the smallest first substrate slice that supports later path discovery and scoped search without spanning too many subsystems at once

## What Not To Re-Read First

- do not reread the full 63 KB monolith unless exact source wording is needed
- do not open unrelated docs under `docs/` before the Phase 1 discovery pass

## Current Repo State Relevant To This Handoff

- documentation changes are uncommitted
- no code implementation for the upgrade has started yet
- no tests were run for this packaging step
