---
phase: 6
slug: hook-enrichment-integration
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-03-10
---

# Phase 6 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust `cargo test` (built-in) + `tokio::test` for async |
| **Config file** | none — cargo test runner |
| **Quick run command** | `cargo test --lib 2>&1 | tail -5` |
| **Full suite command** | `cargo test -- --test-threads=1 2>&1 | tail -20` |
| **Estimated runtime** | ~30 seconds |

---

## Sampling Rate

- **After every task commit:** Run `cargo test --lib 2>&1 | tail -5`
- **After every plan wave:** Run `cargo test -- --test-threads=1 2>&1 | tail -20`
- **Before `/gsd:verify-work`:** Full suite must be green
- **Max feedback latency:** 30 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|-----------|-------------------|-------------|--------|
| 06-01-01 | 01 | 0 | HOOK-04 | unit | `cargo test test_read_hook_noop_on_non_source` | ❌ W0 | ⬜ pending |
| 06-01-02 | 01 | 0 | HOOK-05 | unit | `cargo test test_symbol_diff_labels` | ❌ W0 | ⬜ pending |
| 06-01-03 | 01 | 0 | HOOK-07 | unit | `cargo test test_grep_hook_caps_at_10` | ❌ W0 | ⬜ pending |
| 06-01-04 | 01 | 0 | HOOK-09 | unit | `cargo test test_truncation_logical_boundary` | ❌ W0 | ⬜ pending |
| 06-01-05 | 01 | 0 | INFR-01 | unit | `cargo test test_init_single_post_tool_use_entry` | ❌ W0 | ⬜ pending |
| 06-01-06 | 01 | 0 | INFR-01 | unit | `cargo test test_init_migrates_old_entries` | ❌ W0 | ⬜ pending |
| 06-01-07 | 01 | 0 | INFR-04 | unit | `cargo test test_token_stats_increment` | ❌ W0 | ⬜ pending |
| 06-02-01 | 02 | 1 | HOOK-04 | integration | `cargo test test_read_hook_injects_outline -- --test-threads=1` | ❌ W0 | ⬜ pending |
| 06-02-02 | 02 | 1 | HOOK-05 | integration | `cargo test test_edit_hook_impact_diff -- --test-threads=1` | ❌ W0 | ⬜ pending |
| 06-02-03 | 02 | 1 | HOOK-06 | integration | `cargo test test_write_hook_confirms_index -- --test-threads=1` | ❌ W0 | ⬜ pending |
| 06-02-04 | 02 | 1 | HOOK-07 | integration | `cargo test test_grep_hook_annotates_matches -- --test-threads=1` | ❌ W0 | ⬜ pending |
| 06-03-01 | 03 | 2 | HOOK-08 | integration | `cargo test test_session_start_repo_map -- --test-threads=1` | ❌ W0 | ⬜ pending |
| 06-03-02 | 03 | 2 | HOOK-09 | integration | `cargo test test_read_hook_budget_enforced -- --test-threads=1` | ❌ W0 | ⬜ pending |
| 06-03-03 | 03 | 2 | INFR-04 | integration | `cargo test test_health_includes_savings -- --test-threads=1` | ❌ W0 | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [ ] `tests/hook_enrichment_integration.rs` — integration test stubs for HOOK-04 through HOOK-09
- [ ] Unit tests for `build_with_budget()` / truncation logic in sidecar handlers
- [ ] Unit tests for `symbol_diff()` (pre/post snapshot diff for HOOK-05)
- [ ] Unit tests for `parse_stdin_input()` and `relative_path()` in hook.rs
- [ ] Unit tests for new init entry format and auto-migration (INFR-01)
- [ ] Unit tests for `TokenStats` counter increment logic (INFR-04)

*Existing `tests/sidecar_integration.rs` covers HOOK-01..03 and HOOK-10 — must remain green.*
*Existing `tests/init_integration.rs` covers idempotency — will need update for new single-entry format.*

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Session start repo map appears without tool call | HOOK-08 | Requires live Claude Code session | Start session in indexed repo, verify repo map in first response |
| Token savings accessible via model report | INFR-04 | Requires interactive model query | Ask model "how many tokens have hooks saved?" in live session |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 30s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
