# Phase H C-2 Close-Out Evidence Matrix (2026-05-15)

Audit basis: local `main` at `4d939f142bb95e51605b4767a8feb728d0dce80e`.

## Gate criteria (per plan-doc lines 161-171)

| Gate | Description | Commit(s) | Regression test (file::name) | Status |
| --- | --- | --- | --- | --- |
| G1.H.2 | health unification | `be509a7` | `src/protocol/format/tests.rs::health_report_consistency::both_paths_agree_on_watcher_state_and_load_time`; `src/protocol/format/tests.rs::health_renders_rejected_stale_mutations_counter` | PASS |
| G1.H.3 | structural envelope label | `4d939f1` | `src/protocol/tools.rs::tests::test_search_text_structural_envelope_reports_ast_grep_match_type` | PASS |
| G1.H.4 | `find_dependents` Pass 2 collision filter | `251d7f0` | `tests/find_dependents_pass2.rs::constructor_name_collision_no_false_positive`; `tests/find_dependents_pass2.rs::real_qualified_call_dependent_still_reported`; `tests/find_dependents_pass2.rs::cross_language_method_name_collision`; `tests/find_dependents_pass2.rs::synthetic_large_method_collision_false_positive_count_under_limit` | PASS |
| G1.H.5 | `find_references` qualified-path coverage | `42c8e16` | `src/protocol/tools/tests/find_references.rs::qualified_call_via_full_path_returned` | PASS |
| G1.H.6 | `get_symbol_context` / `get_file_context` budget enforcement | `e691f10` | `src/protocol/tools.rs::tests::get_symbol_context::large_file_respects_budget`; `src/protocol/tools.rs::tests::get_file_context::nested_test_module_collapsed_by_default`; `src/protocol/tools.rs::tests::get_file_context::include_tests_true_restores_full_outline` | PASS |
| G1.H.7 | `batch_rename` timeout regression coverage | `9e5a93f` profile, `cabe312` regression test; source fix landed as side effect of `42c8e16` shared collector consolidation | `tests/batch_rename_perf.rs::batch_rename_health_dry_run_stays_under_h7_budget` | PASS |
| G2 | ADR 0014 watcher-subsystem spawn-blocking discipline | `766c72c` | n/a (docs) | PASS |
| G3 | B-P1-1 `batch_rename` dry-run completes within 5s on evaluator 13-site repro | `42c8e16`, `9e5a93f`, `cabe312` | `tests/batch_rename_perf.rs::batch_rename_health_dry_run_stays_under_h7_budget` | PASS |
| G4 | B-P1-2 `find_dependents` AAP-shape repro has `<5` false positives | `251d7f0` | `tests/find_dependents_pass2.rs::synthetic_large_method_collision_false_positive_count_under_limit` | PASS |
| G5 | B-P1-3 `find_references` fully-qualified Rust call returns site | `42c8e16` | `src/protocol/tools/tests/find_references.rs::qualified_call_via_full_path_returned` | PASS |
| G6 | B-P1-4 + B-P1-5 `get_symbol_context` / `get_file_context` complete within 5s on large/budgeted context cases | `e691f10` | `src/protocol/tools.rs::tests::get_symbol_context::large_file_respects_budget`; `src/protocol/tools.rs::tests::get_file_context::nested_test_module_collapsed_by_default`; `src/protocol/tools.rs::tests::get_file_context::include_tests_true_restores_full_outline` | PASS |
| G7 | B-P1-6 `health` and `health_compact` agree on watcher state and `load_duration_ms` | `be509a7` | `src/protocol/format/tests.rs::health_report_consistency::both_paths_agree_on_watcher_state_and_load_time` | PASS |
| G8 | B-P1-7 `search_text(structural=true)` envelope shows `structural (ast-grep)` | `4d939f1` | `src/protocol/tools.rs::tests::test_search_text_structural_envelope_reports_ast_grep_match_type` | PASS |
| G9 | All four cargo verification commands green with `--test-threads=1` and default parallelism | `4d939f1` final verification | `docs/notes/2026-05-15-c2-final-verification.txt` | PASS |
| G10 | No regressions on 1640+ pre-existing lib tests | `4d939f1` final verification | `docs/notes/2026-05-15-c2-final-verification.txt` | PASS |
| G11 | Master plan-doc updated with C-2 completion timestamp | `<pending>` | n/a (docs close-out action) | PENDING |

## Appendix: bonus correctness fix

- `71feb4f`: innermost enclosing symbol resolution for `search_text` nested-item matches. Out-of-scope for the original H.3 structural-envelope spec, which is tracked separately by G1.H.3 and G8. Treated as a valuable correctness improvement that landed under the H.3 task slot in error; not a gate criterion. Regression test: `src/protocol/tools.rs::tests::test_search_text_uses_innermost_enclosing_symbol_for_nested_items`.

## Cargo verification provenance

Consolidated close-out verification is captured in `docs/notes/2026-05-15-c2-final-verification.txt` on `main` at `4d939f142bb95e51605b4767a8feb728d0dce80e`. The transcript records the orchestrator-requested V1-V4 command set for this close-out slice.

Summary:

- V1 `cargo test --all-targets -- --test-threads=1`: PASS under WSL fallback; 1969 passed, 0 failed, 4 ignored. Windows first attempt hit the allowed libgit2 lockfile flake.
- V2 `cargo test --all-targets`: PASS under WSL; 1969 passed, 0 failed, 4 ignored.
- V3 `cargo clippy -- -D warnings`: PASS under WSL.
- V4 `cargo check`: PASS under WSL.

## Open items for C-2 close-out commit (orchestrator action)

- G11: write C-2 completion timestamp into master plan-doc `docs/plans/2026-05-08-symforge-improvements-master.md`.
