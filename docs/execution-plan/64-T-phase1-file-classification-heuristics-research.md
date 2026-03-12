---
doc_type: task
task_id: 64
title: Phase 1 file classification heuristics research
status: done
sprint: tokenizor-upgrade-foundation
parent_plan: 04-P-phase-plan.md
prev_task: 63-T-phase1-remaining-substrate-priority-research.md
next_task: 65-T-phase1-file-classification-metadata-shell.md
created: 2026-03-11
updated: 2026-03-11
---
# Task 64: Phase 1 File Classification Heuristics Research

## Objective

- decide the smallest reliable rule set and ownership boundary for Phase 1 file classification metadata

## Why This Exists

- the Phase 1 plan still calls for `is_code`, `is_text`, `is_binary`, `is_generated`, `is_test`, and `is_vendor`
- current research already says the exact classification rule is still unresolved
- getting those flags wrong would distort future path discovery, scoped search, text-lane reads, and noise suppression

## Read Before Work

- [03-P-architecture-and-guardrails.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/03-P-architecture-and-guardrails.md)
- [04-P-phase-plan.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/04-P-phase-plan.md)
- [23-R-phase1-text-lane-boundary-research.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/23-R-phase1-text-lane-boundary-research.md)
- [63-R-phase1-remaining-substrate-priority-research.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/63-R-phase1-remaining-substrate-priority-research.md)

## Expected Touch Points

- `src/discovery/mod.rs`
- `src/domain/index.rs`
- `src/live_index/store.rs`
- `src/watcher/mod.rs`
- `src/parsing/mod.rs`

## Deliverable

- a research note that defines which classification flags are decided at discovery/load time, which remain query-time or deferred, and where the metadata should live

## Done When

- the note gives deterministic rules for the six planned flags or explicitly scopes any deferral
- the first implementation boundary is small enough for one shell task
- watcher and text-lane implications are called out

## Completion Notes

- the right model is not six unrelated booleans
- use a mutually exclusive `FileClass` axis (`Code`, `Text`, `Binary`) plus orthogonal generated/test/vendor tags, with `is_code`, `is_text`, and `is_binary` derived from the class
- keep the first implementation scoped to the current semantic lane:
- classify currently indexed files as `Code`
- add deterministic path-based generated/test/vendor tags
- explicitly defer broader text-lane and binary population until the lightweight text registry exists

## Carry Forward To Next Task

Next task:

- `TBD`

Carry forward:

- first classification shell should thread metadata through discovery, parse results, live index storage, watcher updates, and snapshot persistence
- watcher/discovery must remain code-only in this slice; text-lane expansion is a later task

Open points:

- OPEN: keep the initial generated-file heuristic to strong path/filename matches only, or add banner-based heuristics in a later pass
