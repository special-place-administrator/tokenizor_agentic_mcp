---
doc_type: task
task_id: 63
title: Phase 1 remaining substrate priority research
status: done
sprint: tokenizor-upgrade-foundation
parent_plan: 04-P-phase-plan.md
prev_task: 62-T-phase1-file-local-view-compatibility-labeling-shell.md
next_task: 64-T-phase1-file-classification-heuristics-research.md
created: 2026-03-11
updated: 2026-03-11
---
# Task 63: Phase 1 Remaining Substrate Priority Research

## Objective

- decide the correct next Phase 1 move now that the shared-file migration wave is complete and the seeded queue is empty

## Why This Exists

- tasks 42 through 62 strengthened the live-state publication path and moved the hot file-local readers onto shared `Arc<IndexedFile>` capture
- that work may have made another published-snapshot increment less urgent than unfinished source-plan substrate, but that should be decided explicitly rather than assumed

## Read Before Work

- [04-P-phase-plan.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/04-P-phase-plan.md)
- [20-D-phase1-query-duplication-discovery.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/20-D-phase1-query-duplication-discovery.md)
- [21-R-phase1-query-layer-shape-research.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/21-R-phase1-query-layer-shape-research.md)
- [23-R-phase1-text-lane-boundary-research.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/23-R-phase1-text-lane-boundary-research.md)
- [52-R-phase1-next-published-query-family-research.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/52-R-phase1-next-published-query-family-research.md)
- [53-R-phase1-shared-file-read-substrate-research.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/53-R-phase1-shared-file-read-substrate-research.md)
- [61-R-phase1-file-local-view-compatibility-research.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/61-R-phase1-file-local-view-compatibility-research.md)

## Expected Touch Points

- `docs/execution-plan/04-P-phase-plan.md`
- `src/discovery/mod.rs`
- `src/domain/index.rs`
- `src/live_index/search.rs`
- `src/live_index/store.rs`
- `src/protocol/tools.rs`

## Deliverable

- a research note that ranks the remaining credible Phase 1 directions and chooses the next one

## Done When

- the note compares further published-snapshot work, compatibility cleanup, and unfinished Phase 1 substrate from the source plan
- the recommendation is tied to concrete code reality, not only the plan text
- the immediate next task is clear

## Completion Notes

- the next highest-value Phase 1 move is not more published snapshot work or compatibility cleanup
- the remaining source-plan substrate is still incomplete in code:
- file classification metadata does not exist on the current discovery, parse, or live-index file models
- shared internal query option structs do not exist yet either
- the right ordering is:
- file classification heuristics research
- then a small file classification metadata shell
- then shared internal query option structs on top of that substrate

## Carry Forward To Next Task

Next task:

- `64-T-phase1-file-classification-heuristics-research.md`

Carry forward:

- shared-file capture and lightweight published state were the right recent steps, but they are no longer the main unfinished Phase 1 gap
- file classification should come before shared internal option structs because future scope and noise controls depend on those flags

Open points:

- OPEN: decide the smallest deterministic rule set for `is_code`, `is_text`, `is_binary`, `is_generated`, `is_test`, and `is_vendor`
