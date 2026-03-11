---
phase: 1
slug: liveindex-foundation
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-03-10
---

# Phase 1 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust built-in test harness (`cargo test`) |
| **Config file** | Cargo.toml `[dev-dependencies]` |
| **Quick run command** | `cargo test --lib` |
| **Full suite command** | `cargo test` |
| **Estimated runtime** | ~30 seconds |

---

## Sampling Rate

- **After every task commit:** Run `cargo test --lib`
- **After every plan wave:** Run `cargo test`
- **Before `/gsd:verify-work`:** Full suite must be green
- **Max feedback latency:** 30 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|-----------|-------------------|-------------|--------|
| 01-01-01 | 01 | 1 | LIDX-01 | integration | `cargo test live_index_integration` | ❌ W0 | ⬜ pending |
| 01-01-02 | 01 | 1 | LIDX-02 | unit | `cargo test --lib live_index::store` | ❌ W0 | ⬜ pending |
| 01-01-03 | 01 | 1 | LIDX-03 | unit | `cargo test --lib live_index::store::test_content_in_memory` | ❌ W0 | ⬜ pending |
| 01-01-04 | 01 | 1 | LIDX-04 | unit | `cargo test --lib live_index::store::test_concurrent_readers` | ❌ W0 | ⬜ pending |
| 01-02-01 | 02 | 1 | RELY-01 | unit | `cargo test --lib live_index::store::test_circuit_breaker_trips` | ❌ W0 | ⬜ pending |
| 01-02-02 | 02 | 1 | RELY-02 | unit | `cargo test --lib live_index::store::test_partial_parse_retained` | ❌ W0 | ⬜ pending |
| 01-02-03 | 02 | 2 | RELY-04 | integration | `cargo test live_index_integration::test_stdout_purity` | ❌ W0 | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [ ] `tests/live_index_integration.rs` — stubs for LIDX-01, RELY-04 (startup load from tempdir, stdout purity via subprocess spawn)
- [ ] `src/live_index/store.rs` inline tests — stubs for LIDX-02, LIDX-03, LIDX-04, RELY-01, RELY-02
- [ ] `src/discovery/mod.rs` tests — file walking behavior (reuse v1 `discovery.rs` test patterns)

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| stdout JSON purity under real MCP load | RELY-04 | Subprocess spawn + pipe needed | Start binary, pipe stdout to `jq`, verify exit 0 |

*Note: This has an automated integration test analog, but the real-world subprocess test is the definitive check.*

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 30s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
