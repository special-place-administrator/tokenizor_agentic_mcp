---
doc_type: task
task_id: 88
title: P1 find_references exact selector contract research
status: done
sprint: tokenizor-upgrade-foundation
parent_plan: 05-P-validation-and-backlog.md
prev_task: 87-T-p1-search-symbols-noise-defaults-shell.md
next_task: 89-T-p1-find-references-exact-selector-shell.md
created: 2026-03-12
updated: 2026-03-12
---
# Task 88: P1 Find References Exact Selector Contract Research

## Objective

- define the smallest exact-symbol follow-up contract that lets `search_symbols` feed `find_references` without reopening stable-id design work

## Why This Exists

- backlog P1 still calls for exact-symbol reference query flow
- `search_symbols` now exposes path, kind, and line context tightly enough that exact follow-up is the next precision bottleneck
- Phase 5 eventually needs a broader identity model, but the smallest useful shell may be a `{path,name,kind}` selector first

## Read Before Work

- [04-P-phase-plan.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/04-P-phase-plan.md)
- [05-P-validation-and-backlog.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/05-P-validation-and-backlog.md)
- [84-R-p1-search-symbols-scope-contract-research.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/84-R-p1-search-symbols-scope-contract-research.md)
- [86-R-p1-search-symbols-noise-defaults-research.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/86-R-p1-search-symbols-noise-defaults-research.md)
- [87-T-p1-search-symbols-noise-defaults-shell.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/87-T-p1-search-symbols-noise-defaults-shell.md)

## Expected Touch Points

- `docs/execution-plan/88-T-p1-find-references-exact-selector-contract-research.md`
- `docs/execution-plan/88-R-p1-find-references-exact-selector-contract-research.md`
- `docs/execution-plan/89-T-p1-find-references-exact-selector-shell.md`

## Deliverable

- a small research task that fixes the first exact-selector follow-up contract for `find_references` and authors the next execution slice

## Done When

- the first exact-selector contract is explicit
- the relationship between name-only mode and exact follow-up mode is clear
- the next implementation slice is recoverable from disk

## Completion Notes

- added [88-R-p1-find-references-exact-selector-contract-research.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/88-R-p1-find-references-exact-selector-contract-research.md)
- recommendation:
  - preserve current name-only behavior unless an exact selector is provided
  - add `path`, `symbol_kind`, and `symbol_line` so `search_symbols` output can chain directly into `find_references`
  - keep `kind` as the reference-kind filter, not the symbol-kind selector
  - make the first shell exact about symbol selection but dependency-scoped for cross-file reference matching
  - keep successful formatter output unchanged and use stable error strings for missing or ambiguous selectors
- authored the follow-on execution slice as `89-T-p1-find-references-exact-selector-shell.md`

## Carry Forward To Next Task

Next task:

- `89-T-p1-find-references-exact-selector-shell.md`

Carry forward:

- keep this research separate from stable symbol-id substrate work
- preserve current name-only `find_references` behavior unless explicit disambiguation is provided
- prefer a contract that chains cleanly from current `search_symbols` output

Resolved point:

- include `symbol_line` in the first exact-selector shell as a secondary disambiguator
