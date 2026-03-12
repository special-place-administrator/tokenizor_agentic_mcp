---
doc_type: task
task_id: 10
title: Phase 0 benchmark scenarios
status: done
sprint: tokenizor-upgrade-foundation
parent_plan: 04-P-phase-plan.md
prev_task: 
next_task: 11-T-phase0-baseline-output-snapshot-plan.md
created: 2026-03-11
updated: 2026-03-11
---
# Task 10: Phase 0 Benchmark Scenarios

## Objective

- define the smallest useful benchmark scenario set for path lookup, text search, symbol lookup, reference lookup, and file reading

## Why This Exists

- Phase 0 needs a measurable baseline before query-surface changes begin
- this slice is discovery and scoping only, not harness implementation

## Read Before Work

- [00-P-task-orchestration.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/00-P-task-orchestration.md)
- [04-P-phase-plan.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/04-P-phase-plan.md)
- [05-P-validation-and-backlog.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/05-P-validation-and-backlog.md)

## Expected Touch Points

- `docs/execution-plan/`
- `tests/`
- `src/protocol/`

## Deliverable

- one discovery or research note that names the benchmark scenarios, why each matters, and where they will likely be measured

## Done When

- benchmark scenarios cover the major tool families named in Phase 0
- the note is granular enough to guide a later harness slice without rereading the monolith

## Completion Notes

- created [10-D-phase0-benchmark-scenarios.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/10-D-phase0-benchmark-scenarios.md)
- fixed the minimum Phase 0 benchmark set at 10 scenarios across path lookup, text search, symbol lookup, reference lookup, and file reading
- identified likely benchmark homes: `src/protocol/tools.rs`, `src/protocol/format.rs`, `src/live_index/query.rs`, `tests/live_index_integration.rs`, and `tests/xref_integration.rs`
- identified fixture shapes to carry into the next tasks: unique-path repo tree, repeated-basename repo, mixed code-text repo, common-symbol flood repo, and bounded line-range read file

## Carry Forward To Next Task

Next task:

- `11-T-phase0-baseline-output-snapshot-plan.md`

Carry forward:

- baseline scenarios chosen in [10-D-phase0-benchmark-scenarios.md](/C:/AI_STUFF/PROGRAMMING/tokenizor_agentic_mcp/docs/execution-plan/10-D-phase0-benchmark-scenarios.md)
- likely test homes identified: `src/protocol/tools.rs`, `src/protocol/format.rs`, `src/live_index/query.rs`, `tests/live_index_integration.rs`, `tests/xref_integration.rs`
- likely fixture shapes identified: unique-path repo tree, repeated-basename repo, mixed code-text repo, common-symbol flood repo, bounded line-range read file

Open points:

- OPEN: exact benchmark harness entrypoint still needs confirmation; Phase 0 should time public tool handlers first
- OPEN: path lookup remains a proxy benchmark until `search_files` or `resolve_path` exists
