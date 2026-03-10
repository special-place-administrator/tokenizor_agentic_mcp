---
phase: 2
slug: mcp-tools-v1-parity
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-03-10
---

# Phase 2 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust built-in `#[test]` + `#[tokio::test]`, `cargo test` |
| **Config file** | none (Cargo.toml `[dev-dependencies]` controls) |
| **Quick run command** | `cargo test --lib 2>/dev/null` |
| **Full suite command** | `cargo test 2>/dev/null` |
| **Estimated runtime** | ~15 seconds |

---

## Sampling Rate

- **After every task commit:** Run `cargo test --lib 2>/dev/null`
- **After every plan wave:** Run `cargo test 2>/dev/null`
- **Before `/gsd:verify-work`:** Full suite must be green
- **Max feedback latency:** 15 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|-----------|-------------------|-------------|--------|
| 2-01-01 | 01 | 1 | LIDX-05 | integration | `cargo test --test live_index_integration test_load_perf 2>/dev/null` | ❌ W0 | ⬜ pending |
| 2-01-02 | 01 | 1 | TOOL-01 | unit | `cargo test --lib protocol::tools::tests::test_get_symbol 2>/dev/null` | ❌ W0 | ⬜ pending |
| 2-01-03 | 01 | 1 | TOOL-02 | unit | `cargo test --lib protocol::tools::tests::test_get_symbols_batch 2>/dev/null` | ❌ W0 | ⬜ pending |
| 2-01-04 | 01 | 1 | TOOL-03 | unit | `cargo test --lib protocol::format::tests::test_file_outline_format 2>/dev/null` | ❌ W0 | ⬜ pending |
| 2-01-05 | 01 | 1 | TOOL-04 | unit | `cargo test --lib protocol::format::tests::test_repo_outline_format 2>/dev/null` | ❌ W0 | ⬜ pending |
| 2-01-06 | 01 | 1 | TOOL-05 | unit | `cargo test --lib protocol::tools::tests::test_search_symbols 2>/dev/null` | ❌ W0 | ⬜ pending |
| 2-01-07 | 01 | 1 | TOOL-06 | unit | `cargo test --lib protocol::tools::tests::test_search_text 2>/dev/null` | ❌ W0 | ⬜ pending |
| 2-01-08 | 01 | 1 | TOOL-07 | unit | `cargo test --lib protocol::format::tests::test_health_format 2>/dev/null` | ❌ W0 | ⬜ pending |
| 2-01-09 | 01 | 1 | TOOL-08 | integration | `cargo test --test live_index_integration test_index_folder_reload 2>/dev/null` | ❌ W0 | ⬜ pending |
| 2-01-10 | 01 | 1 | TOOL-12 | unit | `cargo test --lib protocol::tools::tests::test_what_changed 2>/dev/null` | ❌ W0 | ⬜ pending |
| 2-01-11 | 01 | 1 | TOOL-13 | unit | `cargo test --lib protocol::tools::tests::test_get_file_content 2>/dev/null` | ❌ W0 | ⬜ pending |
| 2-01-12 | 01 | 1 | INFR-02 | integration | `cargo test --test live_index_integration test_auto_index_behavior 2>/dev/null` | ❌ W0 | ⬜ pending |
| 2-01-13 | 01 | 1 | INFR-03 | unit | `cargo test --lib protocol::format::tests 2>/dev/null` | ❌ W0 | ⬜ pending |
| 2-01-14 | 01 | 1 | INFR-05 | integration | `cargo test --test live_index_integration test_tool_list_no_v1_tools 2>/dev/null` | ❌ W0 | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [ ] `src/protocol/mod.rs` — server struct stubs, SharedIndex integration point
- [ ] `src/protocol/tools.rs` — tool handler stubs for all 10 tools
- [ ] `src/protocol/format.rs` — formatter stubs
- [ ] `tests/live_index_integration.rs` — add LIDX-05 perf, INFR-02, TOOL-08, INFR-05 test stubs
- [ ] `LiveIndex::empty()` constructor — needed for TOKENIZOR_AUTO_INDEX=false tests
- [ ] `LiveIndex::reload()` method — needed for index_folder reload tests

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| v1 tools absent from MCP listing | INFR-05 | Also automatable but worth visual spot-check | Start server, list tools, confirm no cancel_index_run etc. |
| Compact text feels natural to model | INFR-03 | Subjective quality | Feed tool output to Claude, check readability |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 15s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
