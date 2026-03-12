---
doc_type: task
task_id: 83
title: Phase 3 search_text match semantics shell
status: done
sprint: tokenizor-upgrade-foundation
parent_plan: 04-P-phase-plan.md
prev_task: 82-T-phase3-search-text-match-semantics-contract-research.md
next_task: 84-T-p1-search-symbols-scope-contract-research.md
created: 2026-03-12
updated: 2026-03-12
---
# Task 83: Phase 3 Search Text Match Semantics Shell

## Objective

- add the first public `case_sensitive` and `whole_word` `search_text` shell

## Why This Exists

- Phase 3 still needs the remaining literal match-semantics knobs after scope, context, and glob filtering
- task 82 fixed the smallest stable contract, so implementation can stay narrow and deterministic

## Read Before Work

- [82-R-phase3-search-text-match-semantics-contract-research.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/82-R-phase3-search-text-match-semantics-contract-research.md)
- [82-T-phase3-search-text-match-semantics-contract-research.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/82-T-phase3-search-text-match-semantics-contract-research.md)
- [81-T-phase3-search-text-glob-filter-shell.md](/E:/project/tokenizor_agentic_mcp/docs/execution-plan/81-T-phase3-search-text-glob-filter-shell.md)

## Expected Touch Points

- `src/protocol/tools.rs`
- `src/live_index/search.rs`
- `src/protocol/format.rs`

## Deliverable

- a `search_text` shell that supports first-pass case-sensitive and whole-word matching without disturbing the current scoped output contract

## Done When

- `search_text` accepts `case_sensitive` and `whole_word`
- literal search preserves its current default case-insensitive behavior unless overridden
- regex search can opt into case-insensitive matching without changing its current default
- literal whole-word matching works with identifier-style boundaries
- `regex=true` plus `whole_word=true` returns a stable user-facing error
- focused tests cover literal, regex, and invalid-combination behavior

## Completion Notes

- extended the public `search_text` input with `case_sensitive` and `whole_word`
- preserved current defaults by keeping literal search case-insensitive unless overridden while leaving regex search case-sensitive unless explicitly opted out
- added identifier-style whole-word matching for literal queries and `terms` using an escaped word-boundary matcher
- added case-insensitive regex opt-in via `case_sensitive=false`
- rejected `regex=true` plus `whole_word=true` with a stable user-facing error instead of guessing at partial regex word-boundary semantics
- kept scope filters, glob filters, caps, and context rendering behavior unchanged
- verification run for this task:
  - `cargo test test_search_module_text_search_with_options_respects_case_sensitive_literal_matching -- --nocapture`
  - `cargo test test_search_module_text_search_with_options_respects_whole_word_boundaries -- --nocapture`
  - `cargo test test_search_module_text_search_regex_can_opt_into_case_insensitive_matching -- --nocapture`
  - `cargo test test_search_module_text_search_rejects_regex_whole_word_combination -- --nocapture`
  - `cargo test search_text -- --nocapture`
  - `cargo test`

## Carry Forward To Next Task

Next task:

- `84-T-p1-search-symbols-scope-contract-research.md`

Carry forward:

- keep the current trigram prefilter unchanged unless focused verification shows a concrete regression
- preserve context rendering and glob behavior byte-for-byte outside the new semantics
- keep regex whole-word support deferred

Open points:

- OPEN: whether later work should widen whole-word matching beyond identifier-style boundaries
