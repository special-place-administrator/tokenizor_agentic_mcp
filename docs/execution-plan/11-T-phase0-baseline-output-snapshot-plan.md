---
doc_type: task
task_id: 11
title: Phase 0 baseline output snapshot plan
status: done
sprint: tokenizor-upgrade-foundation
parent_plan: 04-P-phase-plan.md
prev_task: 10-T-phase0-benchmark-scenarios.md
next_task: 12-T-phase0-regression-fixture-plan.md
created: 2026-03-11
updated: 2026-03-11
---
# Task 11: Phase 0 Baseline Output Snapshot Plan

## Objective

- define how current tool outputs will be captured for before/after comparison

## Why This Exists

- baseline latency alone is not enough
- later refactors need stable output comparison for regressions and deterministic ordering

## Read Before Work

- [00-P-task-orchestration.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/00-P-task-orchestration.md)
- [04-P-phase-plan.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/04-P-phase-plan.md)
- [10-T-phase0-benchmark-scenarios.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/10-T-phase0-benchmark-scenarios.md)

## Expected Touch Points

- `tests/`
- `src/protocol/format.rs`
- `src/protocol/tools.rs`

## Deliverable

- one note describing which tool outputs need fixtures, what should be normalized, and where snapshots should live

## Done When

- snapshot targets are named for the current high-value tools
- deterministic ordering concerns are called out explicitly

## Completion Notes

- created [11-D-phase0-baseline-output-snapshot-plan.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/11-D-phase0-baseline-output-snapshot-plan.md)
- chose plain-text golden files for Phase 0 snapshots because the repo does not currently carry a snapshot-specific test dependency
- named the must-snapshot current tools: `search_text`, `search_symbols`, `find_references`, `get_file_content`, `get_context_bundle`, and `get_file_context`
- limited `get_repo_outline` to an optional path-proxy baseline because Phase 2 is expected to change its path labeling
- defined normalization guardrails: normalize temp roots, path separators, and line endings only; do not hide ordering, counts, or section structure

## Carry Forward To Next Task

Next task:

- `12-T-phase0-regression-fixture-plan.md`

Carry forward:

- snapshot candidates chosen in [11-D-phase0-baseline-output-snapshot-plan.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/11-D-phase0-baseline-output-snapshot-plan.md)
- snapshot location proposed: `tests/snapshots/phase0/`
- likely harness location proposed: `tests/phase0_output_snapshots.rs`
- open fixture-normalization questions captured for task 12

Open points:

- OPEN: exact snapshot harness helper may depend on current test utilities, but the note keeps it out of `src/`
- OPEN: path-proxy snapshot may be better deferred until it proves stable enough to be worth keeping
