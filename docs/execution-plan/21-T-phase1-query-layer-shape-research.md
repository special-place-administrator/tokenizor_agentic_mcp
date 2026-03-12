---
doc_type: task
task_id: 21
title: Phase 1 query layer shape research
status: done
sprint: tokenizor-upgrade-foundation
parent_plan: 04-P-phase-plan.md
prev_task: 20-T-phase1-query-duplication-discovery.md
next_task: 22-T-phase1-path-index-options-research.md
created: 2026-03-11
updated: 2026-03-11
---
# Task 21: Phase 1 Query Layer Shape Research

## Objective

- compare the smallest plausible internal query-layer shapes and choose one for the first substrate slice

## Why This Exists

- Phase 1 requires a shared query layer, but the plan explicitly warns against broad rewrites
- this research slice should bound the first implementation shape before code changes start

## Read Before Work

- [00-P-task-orchestration.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/00-P-task-orchestration.md)
- [03-P-architecture-and-guardrails.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/03-P-architecture-and-guardrails.md)
- [20-T-phase1-query-duplication-discovery.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/20-T-phase1-query-duplication-discovery.md)

## Expected Touch Points

- `src/live_index/`
- `src/protocol/`

## Deliverable

- one research note comparing candidate query-layer placements, responsibilities, and migration risk

## Done When

- the note picks one preferred shape and explains why it is the smallest safe starting point
- migration risk and benchmark implications are called out explicitly

## Completion Notes

- wrote [21-R-phase1-query-layer-shape-research.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/21-R-phase1-query-layer-shape-research.md)
- compared three placement options: expanding `src/live_index/query.rs`, adding `src/live_index/search.rs`, and introducing a `QueryEngine` abstraction
- selected `src/live_index/search.rs` as the smallest safe starting point because it creates a shared semantic owner without forcing a broader lock or snapshot redesign yet
- recorded the first implementation boundary: keep `src/protocol/format.rs` and `src/sidecar/handlers.rs` as presentation layers while moving shared query semantics underneath them

## Carry Forward To Next Task

Next task:

- `22-T-phase1-path-index-options-research.md`

Carry forward:

- chosen query-layer shape is [21-R-phase1-query-layer-shape-research.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/21-R-phase1-query-layer-shape-research.md): add `src/live_index/search.rs` as a sibling module
- formatting should stay in `src/protocol/format.rs` and sidecar budget rendering should stay in `src/sidecar/handlers.rs`
- path-index research should assume the first semantic extraction will sit above existing `LiveIndex` primitives, not replace them

Open points:

- OPEN: exact API boundaries may still need implementation-time refinement
