---
doc_type: task
task_id: 46
title: Phase 1 published state consumer vs query snapshot research
status: done
sprint: tokenizor-upgrade-foundation
parent_plan: 04-P-phase-plan.md
prev_task: 45-T-phase1-published-handle-state-shell.md
next_task: 47-T-phase1-first-published-state-consumer-or-query-snapshot-shell.md
created: 2026-03-11
updated: 2026-03-11
---
# Task 46: Phase 1 Published State Consumer Vs Query Snapshot Research

## Objective

- decide whether the next small architectural move should be the first real consumer of `PublishedIndexState` or the first fuller published immutable query snapshot for one targeted read path

## Why This Exists

- task 45 proved that `SharedIndexHandle` now publishes lightweight authoritative state from real mutation paths
- the next decision changes either reader behavior or snapshot structure, so it needs an explicit research pass before coding
- this is the point where the project should avoid drifting into structural work without a clear first consumer or first immutable-read win

## Read Before Work

- [04-P-phase-plan.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/04-P-phase-plan.md)
- [26-R-phase1-live-state-snapshot-research.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/26-R-phase1-live-state-snapshot-research.md)
- [44-R-phase1-published-handle-state-research.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/44-R-phase1-published-handle-state-research.md)
- [45-T-phase1-published-handle-state-shell.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/45-T-phase1-published-handle-state-shell.md)

## Expected Touch Points

- `docs/execution-plan/`
- `src/live_index/store.rs`
- `src/protocol/`
- `src/main.rs`
- `src/daemon.rs`

## Deliverable

- a short research note comparing “consume published lightweight state now” against “publish the first immutable query snapshot now”, with a concrete recommended next slice

## Done When

- the note names the most valuable first consumer candidates for `PublishedIndexState`
- the note names the smallest plausible first immutable query snapshot candidate if that path is preferred
- one immediate implementation slice is chosen and recorded

## Completion Notes

- reviewed the current publication shell against the real reader candidates in `src/protocol/tools.rs`, `src/protocol/format.rs`, `src/main.rs`, `src/sidecar/handlers.rs`, and `src/daemon.rs`
- compared two real next moves: make one production reader consume `PublishedIndexState`, or publish the first fuller immutable query snapshot for a targeted read path such as `repo_outline`
- chose the first `PublishedIndexState` consumer as the next slice, centered on `health`, because the current immutable-query candidates already use owned capture under short locks while `health` and related operational surfaces still reacquire the live lock for state/count reporting
- identified one concrete prerequisite for that slice: extend `PublishedIndexState` with parse counts and load duration so `health` can switch without degrading current output

## Carry Forward To Next Task

Next task:

- `47-T-phase1-first-published-state-consumer-or-query-snapshot-shell.md`

Carry forward:

- next implementation should center on `health` as the first real consumer of `PublishedIndexState`
- startup logging, sidecar `GET /health`, and daemon project health are the best secondary consumers after the same published-state expansion
- the first fuller immutable query snapshot remains a later step, with `repo_outline` still the cleanest candidate when that work becomes worth doing

Open points:

- OPEN: avoid bundling both the first consumer and the first immutable query snapshot into one slice unless the research proves they are inseparable
