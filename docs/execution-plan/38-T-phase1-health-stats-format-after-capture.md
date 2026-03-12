---
doc_type: task
task_id: 38
title: Phase 1 health stats format after capture
status: done
sprint: tokenizor-upgrade-foundation
parent_plan: 04-P-phase-plan.md
prev_task: 37-T-phase1-what-changed-timestamp-view-capture.md
next_task: 
created: 2026-03-11
updated: 2026-03-11
---
# Task 38: Phase 1 Health Stats Format After Capture

## Objective

- move `health` to the capture-then-format pattern so report rendering happens after the index and watcher guards are released

## Why This Exists

- `health` already computes owned `HealthStats`, but the tool still calls the formatter while both the index read lock and watcher mutex are held
- this is the last low-risk operational formatter path before the remaining heavier xref/context work

## Read Before Work

- [26-R-phase1-live-state-snapshot-research.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/26-R-phase1-live-state-snapshot-research.md)
- [37-T-phase1-what-changed-timestamp-view-capture.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/37-T-phase1-what-changed-timestamp-view-capture.md)

## Expected Touch Points

- `src/protocol/tools.rs`
- `src/protocol/format.rs`
- `src/live_index/`

## Deliverable

- a pure formatter over captured health stats, with the tool path updated so report formatting happens after guard release

## Done When

- `health` captures the stats it needs under the necessary locks and formats after the guards are dropped
- current public output remains unchanged
- focused tests cover parity or the migrated tool path

## Completion Notes

- migrated `health` to capture status and owned `HealthStats` under the index and watcher locks, then format after both guards are released
- added `format::health_report_from_stats()` as the pure rendering helper used by both the existing formatter wrappers and the tool path
- kept token-savings rendering unchanged and outside the index/watch locks
- added focused parity coverage and reran `cargo test health`

## Carry Forward To Next Task

Next task:

- `TBD`

Carry forward:

- the operational lane is now largely narrowed
- the main remaining borrowed formatter-held read paths are the xref/context family: `find_references`, `find_dependents`, and `get_context_bundle`

Open points:

- OPEN: choose the smallest xref/context migration path before introducing broader owned reference/context result structures
