---
phase: 7
slug: polish-and-persistence
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-03-10
---

# Phase 7 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust built-in test harness + `cargo test` |
| **Config file** | Cargo.toml (no separate test config) |
| **Quick run command** | `cargo test --lib 2>&1` |
| **Full suite command** | `cargo test 2>&1` |
| **Estimated runtime** | ~30 seconds |

---

## Sampling Rate

- **After every task commit:** Run `cargo test --lib 2>&1`
- **After every plan wave:** Run `cargo test 2>&1`
- **Before `/gsd:verify-work`:** Full suite must be green
- **Max feedback latency:** 30 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|-----------|-------------------|-------------|--------|
| 07-01-01 | 01 | 1 | PLSH-01 | unit | `cargo test --lib trigram 2>&1` | ❌ W0 | ⬜ pending |
| 07-01-02 | 01 | 1 | PLSH-01 | unit | `cargo test --lib trigram 2>&1` | ❌ W0 | ⬜ pending |
| 07-01-03 | 01 | 1 | PLSH-02 | unit | `cargo test --lib scored_search 2>&1` | ❌ W0 | ⬜ pending |
| 07-01-04 | 01 | 1 | PLSH-03 | integration | `cargo test --test live_index_integration test_file_tree 2>&1` | ❌ W0 | ⬜ pending |
| 07-02-01 | 02 | 1 | PLSH-04 | unit | `cargo test --lib persist 2>&1` | ❌ W0 | ⬜ pending |
| 07-02-02 | 02 | 1 | PLSH-04 | unit | `cargo test --lib persist 2>&1` | ❌ W0 | ⬜ pending |
| 07-02-03 | 02 | 1 | PLSH-04 | unit | `cargo test --lib persist 2>&1` | ❌ W0 | ⬜ pending |
| 07-02-04 | 02 | 1 | PLSH-05 | unit | `cargo test --lib persist 2>&1` | ❌ W0 | ⬜ pending |
| 07-03-01 | 03 | 2 | LANG-01 | integration | `cargo test --test tree_sitter_grammars test_c_grammar 2>&1` | ❌ W0 | ⬜ pending |
| 07-03-02 | 03 | 2 | LANG-01 | unit | `cargo test --lib c_language 2>&1` | ❌ W0 | ⬜ pending |
| 07-03-03 | 03 | 2 | LANG-02 | integration | `cargo test --test tree_sitter_grammars test_cpp_grammar 2>&1` | ❌ W0 | ⬜ pending |
| 07-03-04 | 03 | 2 | LANG-02 | unit | `cargo test --lib cpp_language 2>&1` | ❌ W0 | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [ ] `src/live_index/trigram.rs` — stubs for PLSH-01 (TrigramIndex struct + unit tests)
- [ ] `src/live_index/persist.rs` — stubs for PLSH-04, PLSH-05 (serialize/deserialize + version check + stat-check)
- [ ] `src/parsing/languages/c.rs` — stubs for LANG-01 (C symbol extraction)
- [ ] `src/parsing/languages/cpp.rs` — stubs for LANG-02 (C++ symbol extraction)
- [ ] `tests/tree_sitter_grammars.rs` additions — `test_c_grammar_loads_and_parses`, `test_cpp_grammar_loads_and_parses`
- [ ] Cargo.toml additions: `postcard = { version = "1.1.3", features = ["use-std"] }`, `tree-sitter-c = "0.24.1"`, `tree-sitter-cpp = "0.23.4"`

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Server restart loads index in <100ms | PLSH-04 | Timing-sensitive, requires process restart | Start server, index a repo, stop server, restart, measure load time from logs |
| Relevance ranking is observable in search results | PLSH-02 | Requires human judgment of "relevant" ordering | Run `search_symbols "parse"`, verify exact > prefix > substring ordering visually |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 30s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
