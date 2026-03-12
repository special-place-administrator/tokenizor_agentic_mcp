---
doc_type: task
task_id: 49
title: Phase 1 published degraded state shell
status: done
sprint: tokenizor-upgrade-foundation
parent_plan: 04-P-phase-plan.md
prev_task: 48-T-phase1-published-degraded-state-research.md
next_task: 
created: 2026-03-11
updated: 2026-03-11
---
# Task 49: Phase 1 Published Degraded State Shell

## Objective

- add the smallest published degraded-state detail needed so startup readiness and degraded logging can consume `PublishedIndexState` instead of reacquiring the live index

## Why This Exists

- task 47 made `health` publication-backed but left `main.rs` startup logging on the live lock
- the handle is increasingly acting like the repo’s in-memory state machine, so operational state reporting should continue to move onto the published substrate before the first fuller immutable query snapshot

## Read Before Work

- [48-R-phase1-published-degraded-state-research.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/48-R-phase1-published-degraded-state-research.md)
- [47-T-phase1-first-published-state-consumer-or-query-snapshot-shell.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/47-T-phase1-first-published-state-consumer-or-query-snapshot-shell.md)

## Expected Touch Points

- `src/live_index/store.rs`
- `src/main.rs`
- `src/live_index/persist.rs`

## Deliverable

- published degraded-state detail plus at least one startup/readiness consumer migrated onto it

## Done When

- `PublishedIndexState` carries enough compact degraded-state data for startup logging
- `main.rs` startup readiness / degraded logging no longer requires a live read for data the published substrate owns
- focused tests cover the new published-state detail and migrated startup path

## Completion Notes

- extended `PublishedIndexState` with a compact optional `degraded_summary`, derived from the circuit-breaker summary when the live index is degraded
- migrated `main.rs` startup readiness / degraded logging to `published_state()` so startup operational reporting no longer needs a live `IndexState` read for data the handle already owns
- kept the published payload small: status still flows through `PublishedIndexStatus`, with the summary string added only for the degraded path instead of publishing a fuller parallel `IndexState`
- focused verification passed:
  - `cargo test shared_index_handle -- --nocapture`
  - `cargo test startup_index_log_view -- --nocapture`
  - `cargo test health -- --nocapture`

## Carry Forward To Next Task

Next task:

- `TBD`

Carry forward:

- the operational published-state path is now strong enough that the next Phase 1 slice can turn back toward the first fuller immutable query snapshot candidate instead of another health/status expansion

Open points:

- OPEN: choose the first fuller immutable query snapshot candidate now that `health`, sidecar health, daemon project health, and startup logging are publication-backed
