---
doc_type: task
task_id: 47
title: Phase 1 first published state consumer or query snapshot shell
status: done
sprint: tokenizor-upgrade-foundation
parent_plan: 04-P-phase-plan.md
prev_task: 46-T-phase1-published-state-consumer-vs-query-snapshot-research.md
next_task: 
created: 2026-03-11
updated: 2026-03-11
---
# Task 47: Phase 1 First Published State Consumer Or Query Snapshot Shell

## Objective

- implement the first real `PublishedIndexState` consumer, centered on `health`, after task 46 chose that path over a first fuller immutable query snapshot

## Why This Exists

- task 46 chose `health` as the highest-value first consumer of published state
- the current immutable-query candidates already have acceptable owned-capture behavior, so the smaller win is to make the new publication shell drive real operational reporting first

## Read Before Work

- [46-R-phase1-published-state-consumer-vs-query-snapshot-research.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/46-R-phase1-published-state-consumer-vs-query-snapshot-research.md)

## Expected Touch Points

- `src/live_index/store.rs`
- `src/protocol/`
- `src/main.rs`
- `src/daemon.rs`

## Deliverable

- the first small production consumer of the newly published state substrate, with focused coverage

## Done When

- `health` no longer depends exclusively on the live lock for data that the published substrate can own
- current public behavior remains unchanged
- focused tests cover the migrated consumer path

## Completion Notes

- migrated the first real published-state consumer to `health`, with `TokenizorServer::health`, sidecar `GET /health`, and daemon project health all reading `PublishedIndexState` instead of reacquiring the live index for count and status reporting
- expanded `PublishedIndexState` to carry parse-count and load-duration fields needed by health-formatting without a live read
- closed a correctness hole in `SharedIndexHandle`: direct `write()` callers now republish published state on mutated write-guard drop, so published health cannot go stale when compatibility-mode callers mutate through the raw guard
- fixed `LiveIndex::update_file` to clear bootstrap-empty state on first incremental file insert so published status transitions from `Empty` to `Ready` after mutation
- focused verification passed:
  - `cargo test shared_index_handle -- --nocapture`
  - `cargo test health -- --nocapture`
  - `cargo test --test sidecar_integration -- --nocapture`

## Carry Forward To Next Task

Next task:

- `TBD`

Carry forward:

- startup readiness logging in `main.rs` still reads live `IndexState` because degraded-summary detail does not yet exist on `PublishedIndexState`
- if the next slice wants startup and sidecar health to be fully publication-driven, the published substrate needs a compact degraded-summary field rather than only the `Degraded` label

Open points:

- OPEN: decide whether the next small slice should finish the remaining operational consumers (`main.rs` readiness/degraded logging) or begin the first fuller immutable query snapshot candidate now that `health` is publication-backed
