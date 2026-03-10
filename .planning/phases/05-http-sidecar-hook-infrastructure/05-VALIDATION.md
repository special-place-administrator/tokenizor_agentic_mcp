---
phase: 5
slug: http-sidecar-hook-infrastructure
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-03-10
---

# Phase 5 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust built-in test harness + integration tests |
| **Config file** | Cargo.toml (no separate test config) |
| **Quick run command** | `cargo test --test sidecar_integration -- --nocapture` |
| **Full suite command** | `cargo test` |
| **Estimated runtime** | ~30 seconds |

---

## Sampling Rate

- **After every task commit:** Run `cargo test -p tokenizor_agentic_mcp` (~5s)
- **After every plan wave:** Run `cargo test` (~30s)
- **Before `/gsd:verify-work`:** Full suite must be green
- **Max feedback latency:** 30 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|-----------|-------------------|-------------|--------|
| 05-01-01 | 01 | 0 | HOOK-01 | integration | `cargo test --test sidecar_integration test_sidecar_binds_ephemeral_port` | ❌ W0 | ⬜ pending |
| 05-01-02 | 01 | 0 | HOOK-01 | integration | `cargo test --test sidecar_integration test_health_endpoint_latency` | ❌ W0 | ⬜ pending |
| 05-01-03 | 01 | 0 | HOOK-01 | integration | `cargo test --test sidecar_integration test_outline_endpoint` | ❌ W0 | ⬜ pending |
| 05-01-04 | 01 | 0 | HOOK-02 | integration | `cargo test --test sidecar_integration test_shared_index_mutation` | ❌ W0 | ⬜ pending |
| 05-01-05 | 01 | 0 | HOOK-03 | integration | `cargo test --test sidecar_integration test_hook_binary_latency` | ❌ W0 | ⬜ pending |
| 05-01-06 | 01 | 0 | HOOK-10 | unit | `cargo test -p tokenizor_agentic_mcp test_hook_output_is_valid_json` | ❌ W0 | ⬜ pending |
| 05-01-07 | 01 | 0 | HOOK-10 | unit | `cargo test -p tokenizor_agentic_mcp test_hook_failopen_valid_json` | ❌ W0 | ⬜ pending |
| 05-01-08 | 01 | 0 | INFR-01 | integration | `cargo test --test init_integration test_init_writes_hooks` | ❌ W0 | ⬜ pending |
| 05-01-09 | 01 | 0 | INFR-01 | integration | `cargo test --test init_integration test_init_idempotent` | ❌ W0 | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [ ] `tests/sidecar_integration.rs` — stubs for HOOK-01, HOOK-02, HOOK-03
- [ ] `tests/init_integration.rs` — stubs for tokenizor init idempotency (INFR-01)
- [ ] Unit tests in `src/cli/hook.rs` — stubs for HOOK-10 fail-open JSON validity
- [ ] tokio `sync` feature — add to Cargo.toml before Wave 1

*Existing test infrastructure (Cargo.toml test harness) covers framework setup.*

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Sidecar endpoint responds within 50ms | HOOK-01 | Latency is environment-dependent | Run `cargo test --test sidecar_integration test_health_endpoint_latency` with 200ms budget; visual check on CI |

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
