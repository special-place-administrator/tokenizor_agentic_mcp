---
phase: 3
slug: file-watcher-freshness
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-03-10
---

# Phase 3 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust built-in + cargo test |
| **Config file** | Cargo.toml (test integration) |
| **Quick run command** | `cargo test --lib 2>/dev/null` |
| **Full suite command** | `cargo test 2>/dev/null` |
| **Estimated runtime** | ~30 seconds |

---

## Sampling Rate

- **After every task commit:** Run `cargo test --lib 2>/dev/null`
- **After every plan wave:** Run `cargo test 2>/dev/null`
- **Before `/gsd:verify-work`:** Full suite must be green
- **Max feedback latency:** 30 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|-----------|-------------------|-------------|--------|
| 03-01-01 | 01 | 1 | FRSH-01 | integration | `cargo test test_watcher_detects_change_within_200ms` | ❌ W0 | ⬜ pending |
| 03-01-02 | 01 | 1 | FRSH-02 | unit | `cargo test test_single_file_reparse_under_50ms` | ❌ W0 | ⬜ pending |
| 03-01-03 | 01 | 1 | FRSH-03 | integration | `cargo test test_watcher_updates_index_after_edit` | ❌ W0 | ⬜ pending |
| 03-01-04 | 01 | 1 | FRSH-04 | integration | `cargo test test_watcher_indexes_new_file` | ❌ W0 | ⬜ pending |
| 03-01-05 | 01 | 1 | FRSH-05 | integration | `cargo test test_watcher_removes_deleted_file` | ❌ W0 | ⬜ pending |
| 03-01-06 | 01 | 1 | FRSH-06 | integration | `cargo test test_symbol_freshness_after_rename` | ❌ W0 | ⬜ pending |
| 03-01-07 | 01 | 1 | RELY-03 | unit | `cargo test test_enoent_handled_gracefully` | ❌ W0 | ⬜ pending |
| 03-01-08 | 01 | 1 | — | unit | `cargo test test_hash_skip_prevents_reparse` | ❌ W0 | ⬜ pending |
| 03-01-09 | 01 | 1 | — | unit | `cargo test test_health_report_shows_watcher_state` | ❌ W0 | ⬜ pending |
| 03-01-10 | 01 | 1 | — | unit | `cargo test test_windows_path_normalization` | ❌ W0 | ⬜ pending |
| 03-01-11 | 01 | 1 | — | unit | `cargo test test_burst_tracker_extends_window` | ❌ W0 | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [ ] `tests/watcher_integration.rs` — stubs for FRSH-01, FRSH-03, FRSH-04, FRSH-05, FRSH-06 (real FS + timing)
- [ ] `src/watcher/mod.rs` unit tests — stubs for RELY-03, content_hash skip, burst tracker, Windows path normalization
- [ ] Framework install: none needed (cargo test already present)

*Existing infrastructure covers framework requirements.*

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Editor burst produces exactly 1 re-index | FRSH-01 (SC-1) | Requires real editor saving behavior | Open VS Code, edit file, save, check re-index log entries |

*All other phase behaviors have automated verification.*

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 30s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
