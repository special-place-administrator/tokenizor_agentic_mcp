---
doc_type: task
task_id: 90
title: P1 get_context_bundle exact selector contract research
status: done
sprint: tokenizor-upgrade-foundation
parent_plan: 05-P-validation-and-backlog.md
prev_task: 89-T-p1-find-references-exact-selector-shell.md
next_task: 91-T-p1-get-context-bundle-exact-selector-shell.md
created: 2026-03-12
updated: 2026-03-12
---
# Task 90: P1 Get Context Bundle Exact Selector Contract Research

## Objective

- define the smallest exact-selector follow-up contract that lets `search_symbols` feed `get_context_bundle` without reopening stable symbol-id work

## Why This Exists

- `get_context_bundle` is still one of the highest-value current tools and the backlog explicitly says to preserve it
- task 89 improved `find_references`, but `get_context_bundle` still picks the first same-name symbol in a file and still computes callers/type usages from global same-name matches
- current `search_symbols` output already exposes `path`, `kind`, and `line`, so the next precision gap is local symbol disambiguation plus exact-section matching

## Read Before Work

- [05-P-validation-and-backlog.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/05-P-validation-and-backlog.md)
- [41-T-phase1-context-bundle-read-view-capture.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/41-T-phase1-context-bundle-read-view-capture.md)
- [88-R-p1-find-references-exact-selector-contract-research.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/88-R-p1-find-references-exact-selector-contract-research.md)
- [89-T-p1-find-references-exact-selector-shell.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/89-T-p1-find-references-exact-selector-shell.md)

## Expected Touch Points

- `docs/execution-plan/90-T-p1-get-context-bundle-exact-selector-contract-research.md`
- `docs/execution-plan/90-R-p1-get-context-bundle-exact-selector-contract-research.md`
- `docs/execution-plan/91-T-p1-get-context-bundle-exact-selector-shell.md`

## Deliverable

- a small research task that fixes the first exact-selector follow-up contract for `get_context_bundle` and authors the next execution slice

## Done When

- the first exact-selector contract for `get_context_bundle` is explicit
- the relationship between current `{path,name,kind}` behavior and line-disambiguated exact mode is clear
- the next implementation slice is recoverable from disk

## Completion Notes

- pending

## Carry Forward To Next Task

Next task:

- `91-T-p1-get-context-bundle-exact-selector-shell.md`

Carry forward:

- preserve current successful `get_context_bundle` output shape
- keep this research separate from stable symbol-id substrate work
- keep the existing under-100ms guardrail in scope

Open points:

- OPEN: whether the first shell should return raw error strings or extend the owned context-bundle view with an ambiguity variant
