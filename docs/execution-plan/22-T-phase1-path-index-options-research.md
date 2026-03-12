---
doc_type: task
task_id: 22
title: Phase 1 path index options research
status: done
sprint: tokenizor-upgrade-foundation
parent_plan: 04-P-phase-plan.md
prev_task: 21-T-phase1-query-layer-shape-research.md
next_task: 23-T-phase1-text-lane-boundary-research.md
created: 2026-03-11
updated: 2026-03-11
---
# Task 22: Phase 1 Path Index Options Research

## Objective

- compare basename maps, directory token maps, path trigrams, and hybrid options for fast path discovery

## Why This Exists

- Phase 2 depends on path discovery, but Phase 1 should choose the lightest substrate that still supports it well

## Read Before Work

- [00-P-task-orchestration.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/00-P-task-orchestration.md)
- [03-P-architecture-and-guardrails.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/03-P-architecture-and-guardrails.md)
- [20-T-phase1-query-duplication-discovery.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/20-T-phase1-query-duplication-discovery.md)

## Expected Touch Points

- `src/live_index/store.rs`
- `src/live_index/query.rs`
- `src/live_index/trigram.rs`

## Deliverable

- one research note comparing candidate path index structures and recommending the smallest first implementation

## Done When

- the note ties the recommendation to actual Phase 2 needs
- memory and update-cost tradeoffs are named explicitly

## Completion Notes

- wrote [22-R-phase1-path-index-options-research.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/22-R-phase1-path-index-options-research.md)
- compared basename maps, directory-component maps, a dedicated path trigram, and a small hybrid grounded in the current `store.rs` and `trigram.rs` implementation
- selected basename map plus directory-component map as the lightest first substrate, with dedicated path trigram explicitly deferred until benchmark evidence shows the cheaper hybrid is insufficient

## Carry Forward To Next Task

Next task:

- `23-T-phase1-text-lane-boundary-research.md`

Carry forward:

- chosen path index approach is in [22-R-phase1-path-index-options-research.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/22-R-phase1-path-index-options-research.md): basename map plus directory-component map
- assumptions needing benchmark validation are called out there, especially whether basename stem indexing is needed and whether narrowed-candidate fallback scans remain cheap enough without a path trigram

Open points:

- OPEN: final ranking heuristics may remain deferred until Phase 2
