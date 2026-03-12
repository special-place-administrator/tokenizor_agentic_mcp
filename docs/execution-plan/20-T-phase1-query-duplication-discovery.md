---
doc_type: task
task_id: 20
title: Phase 1 query duplication discovery
status: done
sprint: tokenizor-upgrade-foundation
parent_plan: 04-P-phase-plan.md
prev_task: 13-T-phase0-compatibility-thresholds.md
next_task: 21-T-phase1-query-layer-shape-research.md
created: 2026-03-11
updated: 2026-03-11
---
# Task 20: Phase 1 Query Duplication Discovery

## Objective

- inspect where query semantics are currently split across tool parsing, formatting, query internals, and sidecar helpers

## Why This Exists

- the smallest viable query-layer refactor depends on real edit points, not guessed architecture
- this is the first direct codebase discovery step for Phase 1

## Read Before Work

- [00-P-task-orchestration.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/00-P-task-orchestration.md)
- [03-P-architecture-and-guardrails.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/03-P-architecture-and-guardrails.md)
- [04-P-phase-plan.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/04-P-phase-plan.md)

## Expected Touch Points

- `src/protocol/tools.rs`
- `src/protocol/format.rs`
- `src/live_index/query.rs`
- `src/live_index/store.rs`
- `src/sidecar/handlers.rs`

## Deliverable

- one discovery note mapping the current semantic split and naming the most likely first edit points

## Done When

- the discovery note identifies real duplication points instead of broad module summaries
- the note recommends the smallest next research slice

## Completion Notes

- wrote [20-D-phase1-query-duplication-discovery.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/20-D-phase1-query-duplication-discovery.md)
- confirmed the main semantic drift lives in `src/protocol/format.rs` and `src/sidecar/handlers.rs`, while `src/live_index/query.rs` is mostly a primitive substrate rather than the source of duplication
- identified the smallest likely first edit points as extraction of search normalization, ranking, grouped-reference selection, and prompt/path matching into a shared query-facing module under `src/live_index/`

## Carry Forward To Next Task

Next task:

- `21-T-phase1-query-layer-shape-research.md`

Carry forward:

- concrete edit points identified in [20-D-phase1-query-duplication-discovery.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/20-D-phase1-query-duplication-discovery.md)
- modules that can stay unchanged in the first substrate slice called out explicitly in the discovery note
- task 21 should focus only on query-layer file shape and ownership boundaries, not on new indices or public API expansion yet

Open points:

- OPEN: exact initial query-layer file shape may still need comparison
