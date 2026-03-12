---
doc_type: task
task_id: 48
title: Phase 1 published degraded state research
status: done
sprint: tokenizor-upgrade-foundation
parent_plan: 04-P-phase-plan.md
prev_task: 47-T-phase1-first-published-state-consumer-or-query-snapshot-shell.md
next_task: 49-T-phase1-published-degraded-state-shell.md
created: 2026-03-11
updated: 2026-03-11
---
# Task 48: Phase 1 Published Degraded State Research

## Objective

- determine whether the next smallest post-`47` slice should finish the remaining operational published-state consumers by adding compact degraded-state detail, or whether Phase 1 should now pivot to the first fuller immutable query snapshot candidate

## Why This Exists

- task 47 proved that lightweight published state can drive real production health reporting
- `main.rs` startup readiness and degraded logging still reads live `IndexState`
- the project direction favors an explicit in-memory state machine, so the next move should be chosen deliberately rather than by convenience

## Read Before Work

- [46-R-phase1-published-state-consumer-vs-query-snapshot-research.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/46-R-phase1-published-state-consumer-vs-query-snapshot-research.md)
- [47-T-phase1-first-published-state-consumer-or-query-snapshot-shell.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/47-T-phase1-first-published-state-consumer-or-query-snapshot-shell.md)

## Expected Touch Points

- `src/live_index/store.rs`
- `src/main.rs`
- `docs/execution-plan/`

## Deliverable

- a short research note that names the smallest next architectural slice and why

## Done When

- the next small step after task 47 is explicit
- the note states whether published degraded-state detail should land before the first fuller immutable query snapshot
- risks and carry-forward points are captured

## Completion Notes

- published operational state is close to complete after task 47; the one remaining live-read seam is `main.rs` startup degraded logging, which still needs the circuit-breaker summary string
- the smallest next slice is to add a compact optional degraded-summary field to `PublishedIndexState` and migrate startup readiness / degraded logging onto it
- explicitly deferred: the first fuller immutable query snapshot candidate, which should come after the operational state substrate is complete enough

## Carry Forward To Next Task

Next task:

- `49-T-phase1-published-degraded-state-shell.md`

Carry forward:

- keep `PublishedIndexStatus` as the compact state label and add only the degraded detail actually needed by startup logging
- do not jump to a fuller published `IndexState` clone unless a later slice proves it is necessary

Open points:

- OPEN: once startup logging is publication-backed, decide whether the next Phase 1 slice should be the first immutable query snapshot candidate or another narrow operational consumer
