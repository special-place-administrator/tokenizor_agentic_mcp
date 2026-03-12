# Research: Phase 1 First Immutable Query Snapshot

Related plan:

- [04-P-phase-plan.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/04-P-phase-plan.md)
- [30-T-phase1-query-read-view-capture.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/30-T-phase1-query-read-view-capture.md)
- [33-T-phase1-file-tree-read-view-capture.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/33-T-phase1-file-tree-read-view-capture.md)
- [49-T-phase1-published-degraded-state-shell.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/49-T-phase1-published-degraded-state-shell.md)

Goal:

- choose the first query-facing immutable published snapshot after the operational published-state path became real

## Current Code Reality

- operational published state is now consumed by:
- `health`
- sidecar `GET /health`
- daemon project health
- startup readiness / degraded logging
- many query paths already narrowed live read-lock time by capturing owned views under the lock and formatting afterward
- `RepoOutlineView` is already a stable owned query shape with:
- total file count
- total symbol count
- sorted file metadata entries
- `get_repo_outline` already renders directly from `RepoOutlineView`
- `get_file_tree` already renders from `RepoOutlineView.files`

That means the repo-outline family is already one conceptual view with two consumers.

## Candidate A: Publish `RepoOutlineView`

### Shape

- add a published immutable `RepoOutlineView` snapshot to `SharedIndexHandle`
- republish it whenever the live index mutates
- migrate `get_repo_outline` and `get_file_tree` to read the published view directly

### Advantages

- smallest query-facing publication step because the owned view already exists
- shared payoff across two tools, not one
- deterministic and metadata-only; no file bytes, no symbol bodies, no xref grouping
- low behavior risk because the current formatting surface stays the same

### Weaknesses

- duplicates a modest whole-index metadata view beside the live index
- does not yet help deeper read/search/xref tools

### Best fit

- strongest candidate

## Candidate B: Publish `WhatChangedTimestampView`

### Shape

- publish the timestamp-mode `what_changed` view
- migrate only `what_changed` timestamp mode to it

### Advantages

- also compact and deterministic

### Weaknesses

- only serves one smaller operational tool
- less user-visible leverage than repo outline and file tree

### Best fit

- weaker than Candidate A

## Candidate C: Publish Search Result Snapshots

### Shape

- publish a text or symbol search-oriented snapshot
- use it to serve `search_symbols` or `search_text`

### Advantages

- higher apparent feature value

### Weaknesses

- much larger semantic surface
- ranking / filtering behavior is more volatile
- risks coupling publication design to search semantics too early

### Best fit

- too large for the first immutable query publication

## Preferred Approach

- choose Candidate A
- publish `RepoOutlineView` first and let both `get_repo_outline` and `get_file_tree` consume it

## Why

- it is the smallest existing owned query view with multiple direct consumers
- it proves the project can publish a real immutable query-facing structure without touching file bytes or higher-risk query semantics
- it keeps the slice behavior-preserving and easy to benchmark against the existing capture-under-lock version

## Recommended Next Implementation Slice

- store an immutable published `RepoOutlineView` beside `PublishedIndexState`
- republish it from the same handle publication path used by mutations
- add direct handle accessors for the published repo-outline snapshot
- migrate `get_repo_outline` and `get_file_tree` to consume that snapshot without a live read lock
- add focused tests for publication parity and consumer migration

## Carry Forward

- immediate next slice: published repo-outline snapshot shell
- deferred on purpose:
- search/text/xref snapshot publication
- broader merged published query substrate across multiple unrelated read families
