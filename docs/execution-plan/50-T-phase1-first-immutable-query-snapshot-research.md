---
doc_type: task
task_id: 50
title: Phase 1 first immutable query snapshot research
status: done
sprint: tokenizor-upgrade-foundation
parent_plan: 04-P-phase-plan.md
prev_task: 49-T-phase1-published-degraded-state-shell.md
next_task: 51-T-phase1-repo-outline-published-query-snapshot-shell.md
created: 2026-03-11
updated: 2026-03-11
---
# Task 50: Phase 1 First Immutable Query Snapshot Research

## Objective

- choose the first fuller immutable query snapshot candidate now that the operational published-state path is established

## Why This Exists

- tasks 45 through 49 made `SharedIndexHandle` a real published state container for operational reporting
- Phase 1 still needs the first proof that a query-facing owned snapshot can be published and consumed directly, not just captured under a short read lock

## Read Before Work

- [30-T-phase1-query-read-view-capture.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/30-T-phase1-query-read-view-capture.md)
- [33-T-phase1-file-tree-read-view-capture.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/33-T-phase1-file-tree-read-view-capture.md)
- [49-T-phase1-published-degraded-state-shell.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/49-T-phase1-published-degraded-state-shell.md)

## Expected Touch Points

- `src/live_index/`
- `src/protocol/tools.rs`
- `docs/execution-plan/`

## Deliverable

- a short research note naming the first query-facing immutable published snapshot and why it is the best next slice

## Done When

- the first immutable query snapshot candidate is explicit
- the note explains why it is smaller / safer than the other plausible readers
- carry-forward risks are captured

## Completion Notes

- the best first immutable query snapshot is `RepoOutlineView`, not search/text/xref or `what_changed`, because it already exists as a stable owned metadata view and it feeds both `get_repo_outline` and `get_file_tree`
- publishing that view is the smallest way to prove direct query-facing immutable publication after tasks 45 through 49 strengthened the operational state path
- explicitly deferred: search/text/xref snapshot publication, which is still materially larger and more coupled to query semantics

## Carry Forward To Next Task

Next task:

- `51-T-phase1-repo-outline-published-query-snapshot-shell.md`

Carry forward:

- reuse the existing `RepoOutlineView` instead of inventing a second repo-outline-specific snapshot type
- keep the first immutable publication limited to the repo-outline/file-tree metadata family

Open points:

- OPEN: after repo-outline/file-tree publication lands, decide whether later query snapshots should remain per-view or start converging on a broader shared published query substrate
