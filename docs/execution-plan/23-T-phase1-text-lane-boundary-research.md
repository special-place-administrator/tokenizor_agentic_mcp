---
doc_type: task
task_id: 23
title: Phase 1 text lane boundary research
status: done
sprint: tokenizor-upgrade-foundation
parent_plan: 04-P-phase-plan.md
prev_task: 22-T-phase1-path-index-options-research.md
next_task: 
created: 2026-03-11
updated: 2026-03-11
---
# Task 23: Phase 1 Text Lane Boundary Research

## Objective

- determine the lightest reliable boundary between the semantic code lane and the non-binary text lane

## Why This Exists

- the source plan treats non-code text support as necessary for read/search parity, but warns against turning the semantic engine into an all-files indexer

## Read Before Work

- [00-P-task-orchestration.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/00-P-task-orchestration.md)
- [03-P-architecture-and-guardrails.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/03-P-architecture-and-guardrails.md)
- [04-P-phase-plan.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/04-P-phase-plan.md)

## Expected Touch Points

- `src/live_index/store.rs`
- `src/live_index/query.rs`
- `src/protocol/tools.rs`

## Deliverable

- one research note comparing lazy reads, bounded caching, and lightweight registry options for non-code text

## Done When

- the note recommends a boundary that preserves code-first behavior
- watcher and memory-profile implications are called out explicitly

## Completion Notes

- wrote [23-R-phase1-text-lane-boundary-research.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/23-R-phase1-text-lane-boundary-research.md)
- compared pure lazy reads, bounded cache only, and a lightweight text registry with optional bounded cache
- selected a lightweight text registry plus bounded content cache as the smallest reliable boundary because it preserves code-first behavior without routing non-code text through the semantic `IndexedFile` mutation path

## Carry Forward To Next Task

Next task:

- `24-T-phase1-shared-search-module-text-symbol.md`

Carry forward:

- chosen text-lane boundary recommendation is in [23-R-phase1-text-lane-boundary-research.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/23-R-phase1-text-lane-boundary-research.md): lightweight text registry plus bounded cache, with lazy reads only on cache miss
- unresolved risks that should shape the first implementation slice are called out there, especially file-classification rules and cache-sizing or invalidation behavior

Open points:

- OPEN: first implementation slice after this research should be chosen from the accumulated Phase 0 and Phase 1 notes
