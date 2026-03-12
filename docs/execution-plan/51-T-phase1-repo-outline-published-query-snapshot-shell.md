---
doc_type: task
task_id: 51
title: Phase 1 repo outline published query snapshot shell
status: done
sprint: tokenizor-upgrade-foundation
parent_plan: 04-P-phase-plan.md
prev_task: 50-T-phase1-first-immutable-query-snapshot-research.md
next_task: 
created: 2026-03-11
updated: 2026-03-11
---
# Task 51: Phase 1 Repo Outline Published Query Snapshot Shell

## Objective

- publish the first fuller immutable query-facing snapshot using the existing repo-outline metadata view, then migrate `get_repo_outline` and `get_file_tree` to consume it

## Why This Exists

- the repo-outline / file-tree family already shares a compact owned view shape
- this makes it the lowest-risk place to prove direct published query snapshots after the operational published-state work

## Read Before Work

- [50-R-phase1-first-immutable-query-snapshot-research.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/50-R-phase1-first-immutable-query-snapshot-research.md)
- [30-T-phase1-query-read-view-capture.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/30-T-phase1-query-read-view-capture.md)
- [33-T-phase1-file-tree-read-view-capture.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/33-T-phase1-file-tree-read-view-capture.md)

## Expected Touch Points

- `src/live_index/store.rs`
- `src/live_index/query.rs`
- `src/protocol/tools.rs`

## Deliverable

- one published immutable query snapshot plus at least one direct consumer pair on top of it

## Done When

- `SharedIndexHandle` publishes an immutable repo-outline-style snapshot on mutation
- `get_repo_outline` and `get_file_tree` can consume that published snapshot without taking a live read lock
- focused tests cover the snapshot publication and migrated consumers

## Completion Notes

- `SharedIndexHandle` now publishes an immutable `RepoOutlineView` snapshot beside the lightweight operational `PublishedIndexState`
- republish happens from the same handle publication path as other mutations, so helper-based writes and compatibility-mode direct `write()` mutations both keep the repo-outline snapshot current
- migrated `get_repo_outline` and `get_file_tree` to consume the published repo-outline snapshot directly, while preserving the old empty/degraded guard behavior via published state instead of a live lock
- focused verification passed:
  - `cargo test repo_outline -- --nocapture`
  - `cargo test file_tree -- --nocapture`
  - `cargo test shared_index_handle -- --nocapture`

## Carry Forward To Next Task

Next task:

- `TBD`

Carry forward:

- the first real query-facing immutable published snapshot now exists, and it serves a small family (`get_repo_outline` plus `get_file_tree`) rather than a single isolated tool
- later query snapshots should stay comparably narrow unless a broader shared publication shape proves clearly simpler

Open points:

- OPEN: decide whether the next published query snapshot should stay metadata-only (`what_changed` timestamp mode or similar) or step into a richer read family such as symbol/detail content views
