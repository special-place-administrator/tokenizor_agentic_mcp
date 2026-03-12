---
doc_type: task
task_id: 12
title: Phase 0 regression fixture plan
status: done
sprint: tokenizor-upgrade-foundation
parent_plan: 04-P-phase-plan.md
prev_task: 11-T-phase0-baseline-output-snapshot-plan.md
next_task: 13-T-phase0-compatibility-thresholds.md
created: 2026-03-11
updated: 2026-03-11
---
# Task 12: Phase 0 Regression Fixture Plan

## Objective

- identify the smallest useful regression fixtures for repeated filenames, noisy common symbols, generated files, and mixed code/text repositories

## Why This Exists

- the upgrade plan depends on fixture-backed evidence, not intuition
- this slice should isolate fixture design from harness implementation

## Read Before Work

- [00-P-task-orchestration.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/00-P-task-orchestration.md)
- [04-P-phase-plan.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/04-P-phase-plan.md)
- [05-P-validation-and-backlog.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/05-P-validation-and-backlog.md)

## Expected Touch Points

- `tests/`
- `src/live_index/`
- `src/parsing/`

## Deliverable

- one note that names fixture shapes, their purpose, and likely storage location

## Done When

- all high-risk regression categories from the source plan are mapped to fixture ideas
- the fixture list is small enough to implement incrementally

## Completion Notes

- created [12-D-phase0-regression-fixture-plan.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/12-D-phase0-regression-fixture-plan.md)
- fixed the minimum regression-fixture set at four reusable fixtures: repeated basenames, common-symbol flood, generated-noise overlay, and mixed code-text repo
- chose a hybrid storage model: structural and byte-sensitive fixtures checked in under `tests/fixtures/phase0/`, high-volume noise expanded by later test helpers
- tied the fixture shapes back to current repo anchors in `src/live_index/query.rs`, `tests/xref_integration.rs`, `tests/watcher_integration.rs`, and `src/parsing/mod.rs`

## Carry Forward To Next Task

Next task:

- `13-T-phase0-compatibility-thresholds.md`

Carry forward:

- regression fixture priorities chosen in [12-D-phase0-regression-fixture-plan.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/12-D-phase0-regression-fixture-plan.md)
- custom repo fixtures called out: repeated basenames and mixed code-text
- generated-content or builder-backed cases called out: common-symbol flood and generated-noise overlay

Open points:

- OPEN: fixture consumers may live in existing integration tests or a dedicated Phase 0 harness, but fixture storage should stay under `tests/fixtures/phase0/`
- OPEN: generated-noise expansion helper still needs a concrete home when the harness is implemented
