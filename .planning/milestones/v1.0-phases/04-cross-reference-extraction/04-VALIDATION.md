---
phase: 4
slug: cross-reference-extraction
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-03-10
---

# Phase 4 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust's built-in `#[test]` + `#[tokio::test]` + cargo test |
| **Config file** | none (Cargo.toml dev-dependencies) |
| **Quick run command** | `cargo test --lib 2>&1 \| tail -20` |
| **Full suite command** | `cargo test 2>&1 \| tail -30` |
| **Estimated runtime** | ~15 seconds |

---

## Sampling Rate

- **After every task commit:** Run `cargo test --lib 2>&1 | tail -20`
- **After every plan wave:** Run `cargo test 2>&1 | tail -30`
- **Before `/gsd:verify-work`:** Full suite must be green
- **Max feedback latency:** 15 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|-----------|-------------------|-------------|--------|
| 04-01-01 | 01 | 0 | XREF-01..03 | unit stubs | `cargo test xref:: -q` | ❌ W0 | ⬜ pending |
| 04-01-02 | 01 | 1 | XREF-01 | unit | `cargo test xref:: -q` | ❌ W0 | ⬜ pending |
| 04-01-03 | 01 | 1 | XREF-02 | unit | `cargo test xref:: -q` | ❌ W0 | ⬜ pending |
| 04-01-04 | 01 | 1 | XREF-03 | unit | `cargo test xref:: -q` | ❌ W0 | ⬜ pending |
| 04-01-05 | 01 | 1 | XREF-07 | unit | `cargo test enclosing_symbol -q` | ❌ W0 | ⬜ pending |
| 04-02-01 | 02 | 1 | XREF-04 | unit | `cargo test find_references -q` | ❌ W0 | ⬜ pending |
| 04-02-02 | 02 | 1 | XREF-05 | unit | `cargo test alias_map -q` | ❌ W0 | ⬜ pending |
| 04-02-03 | 02 | 1 | XREF-06 | unit | `cargo test generic_filter -q` | ❌ W0 | ⬜ pending |
| 04-02-04 | 02 | 1 | XREF-08 | unit+integration | `cargo test xref_incremental -q` | ❌ W0 | ⬜ pending |
| 04-03-01 | 03 | 2 | TOOL-09 | unit | `cargo test find_references_kind_filter -q` | ❌ W0 | ⬜ pending |
| 04-03-02 | 03 | 2 | TOOL-10 | unit | `cargo test find_dependents -q` | ❌ W0 | ⬜ pending |
| 04-03-03 | 03 | 2 | TOOL-11 | unit | `cargo test get_context_bundle -q` | ❌ W0 | ⬜ pending |
| 04-03-04 | 03 | 2 | TOOL-11 | perf | `cargo test context_bundle_perf -q` | ❌ W0 | ⬜ pending |
| 04-04-01 | 04 | 3 | XREF-04 | integration | `cargo test --test xref_integration ts_builtin_filter -q` | ❌ W0 | ⬜ pending |
| 04-04-02 | 04 | 3 | XREF-08 | integration | `cargo test --test xref_integration incremental_update -q` | ❌ W0 | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [ ] `src/parsing/xref.rs` — cross-reference extraction module stubs (XREF-01..03, XREF-07)
- [ ] `tests/xref_integration.rs` — integration test stubs (XREF-04, XREF-05, XREF-06, XREF-08)
- [ ] Unit test stubs in `src/live_index/query.rs` for reverse index queries
- [ ] Unit test stubs in `src/protocol/format.rs` for formatter functions (TOOL-09, TOOL-10, TOOL-11)

*Existing test infrastructure (cargo test) covers framework needs. No new framework install required.*

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| *None* | — | — | — |

*All phase behaviors have automated verification.*

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 15s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
