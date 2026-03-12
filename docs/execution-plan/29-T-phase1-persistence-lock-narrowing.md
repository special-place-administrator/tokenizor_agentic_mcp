---
doc_type: task
task_id: 29
title: Phase 1 persistence lock narrowing
status: done
sprint: tokenizor-upgrade-foundation
parent_plan: 04-P-phase-plan.md
prev_task: 28-T-phase1-root-aware-snapshot-mtime-capture.md
next_task: 30-T-phase1-query-read-view-capture.md
created: 2026-03-11
updated: 2026-03-11
---
# Task 29: Phase 1 Persistence Lock Narrowing

## Objective

- reduce broad read-lock hold times in snapshot serialization and background verification by capturing owned snapshot data under a short lock and doing heavier work after the guard is released

## Why This Exists

- task 26 identified lock breadth as the next structural risk after provenance and persistence correctness
- current persistence flows still keep read access tied too closely to snapshot building and verification work

## Read Before Work

- [26-R-phase1-live-state-snapshot-research.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/26-R-phase1-live-state-snapshot-research.md)
- [27-T-phase1-snapshot-provenance-and-verify-state.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/27-T-phase1-snapshot-provenance-and-verify-state.md)
- [28-T-phase1-root-aware-snapshot-mtime-capture.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/28-T-phase1-root-aware-snapshot-mtime-capture.md)

## Expected Touch Points

- `src/live_index/persist.rs`
- `src/main.rs`

## Deliverable

- owned-snapshot or equivalent lock-narrowing path for persistence and verify flows, with focused tests

## Done When

- shutdown serialization no longer holds a read lock while doing disk I/O
- background verification no longer couples long metadata/hash work to a borrowed live-index view
- focused tests cover the lock-narrowing helper behavior

## Completion Notes

- added owned persistence capture helpers in `src/live_index/persist.rs` so serialization can separate index capture from later mtime lookup and disk write work
- added `serialize_shared_index` and updated shutdown in `src/main.rs` so clean shutdown no longer holds an index read lock while serializing and writing `.tokenizor/index.bin`
- added owned verify-view capture for `background_verify`, so stat-check and spot-hash work now run from copied metadata instead of from a borrowed live-index view
- made verify-view ordering deterministic by sorting captured paths and added focused helper coverage for that behavior
- reran the full `live_index::persist::tests::` suite after the change

## Carry Forward To Next Task

Next task:

- `30-T-phase1-query-read-view-capture.md`

Carry forward:

- persistence-side owned views now exist in `src/live_index/persist.rs` as the first lock-narrowing substrate
- the same owned-view capture pattern should be extended into heavier query read paths next

Open points:

- OPEN: a later full published-snapshot refactor may replace this helper layer, but should not invalidate its tests
