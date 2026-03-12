---
doc_type: task
task_id: 13
title: Phase 0 compatibility thresholds
status: done
sprint: tokenizor-upgrade-foundation
parent_plan: 04-P-phase-plan.md
prev_task: 12-T-phase0-regression-fixture-plan.md
next_task: 20-T-phase1-query-duplication-discovery.md
created: 2026-03-11
updated: 2026-03-11
---
# Task 13: Phase 0 Compatibility Thresholds

## Objective

- define what counts as preserving the current floor for latency, output usefulness, and deterministic behavior

## Why This Exists

- later phases need explicit pass/fail thresholds instead of vague "no regressions" language

## Read Before Work

- [00-P-task-orchestration.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/00-P-task-orchestration.md)
- [04-P-phase-plan.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/04-P-phase-plan.md)
- [05-P-validation-and-backlog.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/05-P-validation-and-backlog.md)

## Expected Touch Points

- `docs/execution-plan/`
- `tests/`

## Deliverable

- one note defining compatibility thresholds and what evidence later slices must gather before claiming improvement

## Done When

- thresholds cover latency, ordering stability, and bounded output behavior
- the note is concrete enough to be reused in later handoffs and reviews

## Completion Notes

- created [13-D-phase0-compatibility-thresholds.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/13-D-phase0-compatibility-thresholds.md)
- separated absolute thresholds already backed by tests from provisional query thresholds that still need the benchmark harness
- defined the three compatibility gates for later slices: latency, deterministic behavior, and bounded output usefulness
- defined the evidence later slices must attach before claiming compatibility or improvement

## Carry Forward To Next Task

Next task:

- `20-T-phase1-query-duplication-discovery.md`

Carry forward:

- baseline protection rules made explicit in [13-D-phase0-compatibility-thresholds.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/13-D-phase0-compatibility-thresholds.md)
- provisional thresholds called out for `search_text`, `search_symbols`, `find_references`, `get_file_content`, `get_file_context`, and the Phase 0 path proxy
- absolute threshold carried forward for `get_context_bundle` under 100ms on the existing 50-file case

Open points:

- OPEN: most query latency numbers remain provisional until the benchmark harness records the first real baseline
- OPEN: path-proxy compatibility should block regressions without freezing the later dedicated path-tool redesign
