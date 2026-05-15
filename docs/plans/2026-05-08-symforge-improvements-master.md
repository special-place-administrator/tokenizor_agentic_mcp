# SymForge Improvements — Master Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship four independent improvement waves to SymForge: ultrareview polish (Phase 1), index hygiene (Phase 2), co-change ranker fusion T3.3 (Phase 3), and RTK Tier 1 adoption (Phase 4). Each phase is independently shippable; coding agent can pause between phases.

**Phase H insertion (2026-05-12):** A stability-hotfix phase has been inserted between Phase 2.2 and Phase 2.3 to address 1 catastrophic and 12 high-severity defects surfaced by external evaluator reports. See `docs/plans/2026-05-12-symforge-stability-hotfix.md` and the **Phase H** section between Phase 2 and Phase 3 below. Phase H sub-tasks interleave the rest of the campaign per Option C sequencing.

**Architecture:** Each phase ends at a green build (`cargo check && cargo test --all-targets -- --test-threads=1 && cargo clippy -- -D warnings`) and a single squash-mergeable PR. Phase ordering reflects risk: Phase 1 = trivial wins, Phase 4 = build-system changes. Phases 1–2 land in any order; Phase 3 depends on Phase 1.4 (health metric surface stays stable). Phase 4 is self-contained.

**Tech Stack:** Rust 2024 edition, rmcp 1.1, tree-sitter 0.26, rusqlite (bundled), `ignore` crate (gitignore-aware walk), tokio, anyhow + thiserror.

**Source of truth references (vault):**
- `wiki/todos/Todos — SymForge.md` — backlog
- `wiki/concepts/SymForge Co-Change Signal Fusion.md` — Phase 3 design
- `wiki/concepts/RTK Techniques for SymForge.md` — Phase 4 source
- `docs/decisions/0013-coupling-signal-contract.md` — Phase 3 contract (6 rules)
- `docs/decisions/0010-worktree-working-directory.md` — already shipped, reference only

**Verification standard for this project (E:\project\symforge\CLAUDE.md):**
```
cargo check
cargo test --all-targets -- --test-threads=1
cargo build --release
cargo clippy -- -D warnings
```

Run after every task. The `--test-threads=1` flag is mandatory — many integration tests share the live index or `.symforge/` directories and will race otherwise.

---

## File Structure (all phases)

| File | Phase | Action |
|---|---|---|
| `src/sidecar/handlers.rs` | 1 | Modify — replace 2 `.unwrap()` with safe pattern + `// safe:` comment |
| `src/discovery/mod.rs` | 1 | Modify — refactor `is_forbidden_root` step 4 to canonical-path check |
| `src/protocol/format.rs` | 1 | Modify — `health_report_from_stats` empty-index banner; promote reconcile to top line on idle watcher |
| `src/protocol/format/tests.rs` | 1 | Modify — new tests for banner + reconcile-on-idle |
| `src/main.rs` | 1 | Modify — pass `local_empty_reason` into health stats / first-response banner |
| `src/live_index/query.rs` | 1, 3 | Modify — `HealthStats` adds `local_empty_reason`; `RankCtx` gains coupling fields; `capture_search_files_view` populates them |
| `src/protocol/explore.rs` | 1 | Modify — `match_concept` callsite gains rank-signal footer (or append in `tools.rs::explore`) |
| `src/protocol/tools.rs` | 1, 2, 3 | Modify — `explore` footer; vendor/`.claude` filter for `search_text`/`explore`; `search_files` rank_by="path+cochange" path |
| `src/protocol/tools.rs::SearchFilesInput` | 3 | Modify — extend docstring on `rank_by`, add `anchor_path` field |
| `src/live_index/rank_signals.rs` | 3 | Modify — `CoChangeSignal::score()` reads `RankCtx::co_change_count`; weight tuned per ADR 0013 |
| `src/live_index/coupling/lifecycle.rs` | 3 | Modify — expose `coupling_store_handle` for query-time lookup |
| `tests/cochange_fusion.rs` | 3 | Create — new integration test file with goldens |
| `tests/rank_signal_behavior.rs` | 3 | Modify — add `path+cochange` golden block |
| `src/parsing/languages/mod.rs` | 4.1 | Modify — replace 19 manual `mod` decls with `automod::dir!` |
| `src/parsing/config_extractors/mod.rs` | 4.2 | Modify — replace 5 manual `mod` decls with `automod::dir!` |
| `Cargo.toml` | 4.1, 4.2, 4.3, 4.5 | Modify — add `automod`, build-script, `sha2` already present |
| `build.rs` | 4.3 | Create — embed tree-sitter query files via `include_str!` validation |
| `src/protocol/edit.rs::atomic_write_file` | 4.4 | Modify — add tee snapshot before write |
| `src/edit_safety/tee.rs` | 4.4 | Create — tee module with config (failures/always/never), max files, max size |
| `src/edit_safety/mod.rs` | 4.4, 4.5 | Create — module entry point |
| `src/edit_safety/trust.rs` | 4.5 | Create — SHA-256 hash + trust-state for `.symforge/` config |
| `src/parsing/inline_tests.rs` | 4.6 | Create — extractor inline test framework |

---

# Phase 0 — Bug Fixes from 2026-05-09 Vault Reports

**Ships:** Two bugs filed in `wiki/todos/Todos — SymForge.md` on 2026-05-09 from AAP F2-Tx-P1 C3 audit. Critical to land before Phase 3 because frecency code paths overlap with CoChange ranker fusion work.

**Phase 0 acceptance:**
- [ ] `cargo test -p symforge --test frecency_ranking` green at default parallelism (no `--test-threads=1` workaround). Specifically: `search_files_does_not_bump`, `search_symbols_does_not_bump`, `search_text_does_not_bump` all PASS.
- [ ] `cargo test -p symforge --test sidecar_integration` green on Windows under workspace-wide parallel load (run 10 times, zero failures on `test_prompt_context_endpoint_basename_line_hint_disambiguates_exact_selector`).
- [ ] No regressions on the 1640+ pre-existing lib tests.
- [ ] All four `cargo` verification commands pass.

**Source notes:**
- `wiki/todos/Todos — SymForge.md` — "Bug report 2026-05-09 — frecency contract violation" + "Bug report 2026-05-09 — sidecar test Windows TCP port-pool exhaustion"
- `wiki/sessions/aap-f2-tx-p1-shipped-2026-05-09.md` — original surfacing as D7 + D11 carve-outs
- `projects/symforge/SymForge Test Failures 2026-05-04.md` — older filing of the same frecency bug (superseded)
- `docs/decisions/0011-frecency-bump-policy.md` — ADR that defines the contract being violated

---

### Task 0.1: Frecency contract violation — search ops must not touch DB

**Severity:** HIGH — deterministic contract violation per ADR 0011. Affects current build of `main`.

**Files (probable, confirm via investigation):**
- Modify: somewhere in `src/protocol/tools.rs::search_files` / `search_text` / `search_symbols` handlers (lines 3866-4152 area), OR
- Modify: `src/live_index/frecency.rs::FrecencyStore::open` to not create-on-open, OR
- Modify: a shared init path that runs at every search-tool entry

**Context:** The 3 tests at `tests/frecency_ranking.rs:399, :417, :435` assert that after calling `search_files`, `search_text`, `search_symbols` (with `SYMFORGE_FRECENCY=1` env active), the frecency DB file MUST NOT exist on disk. Per ADR 0011 frecency-bump-policy: only commitment tools (edit ops + `get_file_context`/`get_symbol`/`get_symbol_context`/`get_file_content`) should touch the frecency store. Discovery tools should short-circuit before opening it.

Tests pass with `--test-threads=1`. Fail at default parallelism — but the AAP audit note (N=20) says the call site fires regardless of thread count; the parallel-failure-mode is just the most reliable surface. Either way, the assertion catches a real contract violation.

This is **investigative work**, not a fixed-spec edit. The plan guides discovery; the agent picks the precise patch site after diagnosis.

- [ ] **Step 1: Invoke superpowers:systematic-debugging.**

This task is a debugging investigation, not TDD-on-known-spec. The skill guides the scientific-method approach: state hypothesis, run experiment, observe, refine.

- [ ] **Step 2: Reproduce the failure locally**

Run:
```
cargo test -p symforge --test frecency_ranking -- search_files_does_not_bump search_text_does_not_bump search_symbols_does_not_bump
```

Expected (per bug report): all 3 fail with assertion messages like `"search_files must not create a frecency database"`.

If they PASS unexpectedly — halt and report. The bug may have been fixed in a commit between the bug filing (2026-05-09) and now (2026-05-11). Possible but unlikely.

Run the same with `--test-threads=1`:
```
cargo test -p symforge --test frecency_ranking -- search_files_does_not_bump --test-threads=1
```

Expected: PASS. This confirms the test-thread isolation matters. Document the delta.

- [ ] **Step 3: Read the failing test bodies to understand the contract**

```
mcp__symforge__get_symbol with name="search_files_does_not_bump" path="tests/frecency_ranking.rs"
mcp__symforge__get_symbol with name="search_text_does_not_bump" path="tests/frecency_ranking.rs"
mcp__symforge__get_symbol with name="search_symbols_does_not_bump" path="tests/frecency_ranking.rs"
```

Identify: what is `fx.db_path()`? Where does `FlagGuard::on()` toggle the env? What's the test fixture pattern?

- [ ] **Step 4: Locate the offending DB-open call site**

Search for every place the frecency DB is opened or created:

```
mcp__symforge__search_text with query="FrecencyStore::open" path_prefix="src"
mcp__symforge__search_text with query="SYMFORGE_FRECENCY_DB_PATH" path_prefix="src"
mcp__symforge__search_text with query="frecency" path_prefix="src/protocol/tools.rs" -- group_by symbol
```

Cross-reference with the 3 search handlers. Find call sites that:
1. Fire during `search_*` execution (not edit/commitment tools)
2. Open or create the DB file before checking whether the caller requested frecency rerank

Likely culprits:
- The `rank_by="frecency"` branch in `search_files` at ~line 4082 — BUT this only fires when `rank_by="frecency"` is explicitly passed. The bumps tests don't pass `rank_by`. If this is the site, the bug is the branch condition firing in error.
- A shared helper at handler entry (logging, hook, session_context) that opens the store as a side effect.
- `LiveIndex` construction or boot path opening the store when `SYMFORGE_FRECENCY=1` is detected.
- `FrecencyStore::open` itself creating the DB file at open-time rather than at first-write-time.

- [ ] **Step 5: Verify the hypothesis**

Once a candidate call site is identified, instrument with a `tracing::debug!` or `eprintln!` and re-run the failing test. Confirm the suspect site fires during the search call.

If the hypothesis is wrong, return to Step 4 with the new information.

- [ ] **Step 6: Read ADR 0011 for the contract**

```
Read docs/decisions/0011-frecency-bump-policy.md
```

The fix MUST honor the ADR: discovery tools never touch the store; commitment tools bump.

- [ ] **Step 7: Patch the offending site**

Two patch shapes:

**A. If `FrecencyStore::open` creates the file:** change `open` to lazy-create — the file gets created only on the first write (`bump`/`upsert`), not at open time. Reads against a non-existent DB return `None`/empty results gracefully.

**B. If a search handler is calling `open` unconditionally:** add a guard so the search path never calls `open` at all. The frecency rerank branch already gates on `rank_by="frecency"` — verify that guard is honored at the actual call site and not bypassed by a shared init.

**C. If `LiveIndex` boot opens the store proactively:** defer the open until the first commitment-tool call.

Use `mcp__symforge__edit_within_symbol` or `replace_symbol_body` for the patch. Whichever site you change, write the patch to address ONLY the contract violation — no opportunistic refactors.

- [ ] **Step 8: Test under failing condition first**

Run:
```
cargo test -p symforge --test frecency_ranking -- search_files_does_not_bump search_text_does_not_bump search_symbols_does_not_bump
```

Expected after patch: all 3 PASS at default parallelism.

- [ ] **Step 9: Test the legitimate frecency path still works**

The fix must not break the frecency rerank feature. Run the OTHER tests in `frecency_ranking.rs`:
```
cargo test -p symforge --test frecency_ranking
```

Expected: full file passes (17 was the previous pass count; should now be 20/20).

Specifically verify these don't regress (they assert frecency DOES bump on commitment tools):
- `bump_persists_across_reopens` (or similarly named)
- Any test asserting `fx.db_path().exists()` after a commitment-tool call

If you can't find such tests, halt and ask — the fix may be too narrow if the contract works only one way.

- [ ] **Step 10: Check `edit_hook_behavior.rs` latent surface**

The session note `aap-f2-tx-p1-shipped-2026-05-09.md` mentions: "Same fixture latent in `edit_hook_behavior.rs`." Run:
```
cargo test -p symforge --test edit_hook_behavior
```

If any tests there fail with the same shape (DB created when contract says no), the fix may need to extend or there's a sibling bug. Report findings either way.

- [ ] **Step 11: Full verification gate**

```
cargo check
cargo clippy --all-targets -- -D warnings
cargo test --all-targets -- --test-threads=1
cargo build --release
```

Plus the explicit default-parallelism check for the bumps tests:
```
cargo test -p symforge --test frecency_ranking
```
(Default parallelism, no `--test-threads=1` flag.)

- [ ] **Step 12: Report**

- Diagnosis summary: where the bug lived, why it fired, what patch addresses it
- `git diff` (final state)
- All verification outputs
- Whether `edit_hook_behavior.rs` showed any sibling failures

Do NOT commit. Wait for delegator approval.

- [ ] **Step 13: Commit message (template — fill in diagnosis specifics)**

```
fix(frecency): search ops must not touch frecency DB

Per ADR 0011 frecency-bump-policy: only commitment tools should
touch the frecency store; discovery tools (search_files / search_text
/ search_symbols) must short-circuit before opening it.

Bug: <one-line root cause>
Fix: <one-line patch description>

Tests now pass at default parallelism (no --test-threads=1 workaround):
  tests::frecency_ranking::search_files_does_not_bump
  tests::frecency_ranking::search_symbols_does_not_bump
  tests::frecency_ranking::search_text_does_not_bump

Filed 2026-05-09 from AAP F2-Tx-P1 C3 audit; vault-tracked at
[[Todos — SymForge]] "Bug report 2026-05-09 — frecency contract
violation". Closes AAP backlog Track D7 dependency.
```

---

### Task 0.2: Sidecar test Windows TCP port-pool exhaustion

**Severity:** MEDIUM — Windows-only flake, ~10% reproduction rate under workspace-wide parallel load. Not blocking on Linux but pollutes our build's verification rate on Windows.

**Files:**
- Modify: `tests/sidecar_integration.rs` around line 2400-2433 — the port-acquire pattern at fixture setup
- Possibly: a helper function shared across all sidecar tests

**Context:** Test `test_prompt_context_endpoint_basename_line_hint_disambiguates_exact_selector` at `tests/sidecar_integration.rs:2433` binds to `127.0.0.1:0` (ephemeral port). Windows TCP stack keeps recently-closed ports in TIME_WAIT for ~2 minutes by default, faster recycling than Linux but still bounded. Under heavy parallel test load, the ephemeral pool exhausts and `os error 10048` (port in use) fires.

Two fix shapes from the bug report:
- **(a) Retry-on-EADDRINUSE wrapper** (~10 LOC) — try bind, on `os error 10048` sleep 50ms + retry, up to 5 attempts. Mechanical, no new deps.
- **(b) `#[serial]` via serial_test crate** — adds workspace dep + serializes ALL sidecar tests. Slower but deterministic.

Plan recommends **(a)**. Smaller blast radius, no new deps. If (a) proves insufficient (5 retries still flake), escalate to (b).

- [ ] **Step 1: Invoke superpowers:systematic-debugging.**

- [ ] **Step 2: Reproduce the flake**

```
for i in 1..=10:
    cargo test -p symforge --test sidecar_integration -- test_prompt_context_endpoint_basename_line_hint_disambiguates_exact_selector
```

Or run the full workspace test suite 10 times and count failures of the target test specifically:
```
for ($i=1; $i -le 10; $i++) { cargo test -p symforge --test sidecar_integration 2>&1 | Select-String "10048" }
```

Expected: 1/10 to 2/10 runs show `os error 10048`. If you can't reproduce in 10 runs, halt — the flake may be environmental (load-dependent); document the attempted reproduction and note that the fix is precautionary.

- [ ] **Step 3: Locate the port-acquire site**

```
mcp__symforge__search_text with query="TcpListener::bind" path_prefix="tests/sidecar_integration.rs"
mcp__symforge__search_text with query="127.0.0.1:0" path_prefix="tests"
```

Identify the fixture helper that builds the sidecar test server. Likely a `setup_sidecar_test()` or similar that's called from every test.

- [ ] **Step 4: Write a failing reproducer (optional — only if flake reproduces in step 2)**

If step 2 reliably reproduces, add a test that asserts the helper succeeds even under contention:
```rust
#[test]
fn sidecar_test_helper_survives_port_contention() {
    // Bind 100 ephemeral ports rapidly to simulate exhaustion pressure,
    // then call the helper. Should succeed via retry, not panic.
    let _listeners: Vec<_> = (0..100)
        .filter_map(|_| std::net::TcpListener::bind("127.0.0.1:0").ok())
        .collect();
    let _server = setup_sidecar_test();  // function from the fixture
}
```

Skip this step if step 2 didn't reproduce.

- [ ] **Step 5: Patch the helper with retry-on-EADDRINUSE**

Find the bind site (likely `std::net::TcpListener::bind("127.0.0.1:0")` or similar). Replace with a retry helper:

```rust
/// Bind to an ephemeral port, retrying briefly on Windows TIME_WAIT collisions.
fn bind_ephemeral_with_retry() -> std::io::Result<std::net::TcpListener> {
    use std::io::ErrorKind;
    let mut last_err = None;
    for attempt in 0..5 {
        match std::net::TcpListener::bind("127.0.0.1:0") {
            Ok(listener) => return Ok(listener),
            Err(e) if e.kind() == ErrorKind::AddrInUse || e.raw_os_error() == Some(10048) => {
                last_err = Some(e);
                std::thread::sleep(std::time::Duration::from_millis(50 * (attempt + 1)));
            }
            Err(e) => return Err(e),
        }
    }
    Err(last_err.unwrap_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::AddrInUse,
            "exhausted retries binding ephemeral port",
        )
    }))
}
```

Place inside `tests/sidecar_integration.rs` (or a shared helper if the fixture is split across files — use `mcp__symforge__find_references` to check).

Replace all `TcpListener::bind("127.0.0.1:0")` call sites in the sidecar test fixture with `bind_ephemeral_with_retry()`.

- [ ] **Step 6: Re-run the flake test under same stress**

```
for ($i=1; $i -le 10; $i++) { cargo test -p symforge --test sidecar_integration 2>&1 | Select-String "10048" }
```

Expected: 0/10 failures with `10048`. If still flaking, escalate to plan (b) — add `serial_test` workspace dep and tag sidecar tests `#[serial]`.

- [ ] **Step 7: Verification gate**

```
cargo check
cargo clippy --all-targets -- -D warnings
cargo test --all-targets -- --test-threads=1
cargo build --release
```

Plus the explicit Windows-flake stress check:
```
cargo test -p symforge --test sidecar_integration
```
(default parallelism)

- [ ] **Step 8: Report**

- Reproduction outcome (did the flake fire 1+/10 before the patch?)
- Diff of `tests/sidecar_integration.rs`
- Re-stress outcome (0/10 after the patch)
- All verification outputs

Do NOT commit. Wait for approval.

- [ ] **Step 9: Commit message (template)**

```
fix(tests): retry ephemeral port bind under Windows TIME_WAIT pressure

tests/sidecar_integration.rs test fixture binds 127.0.0.1:0 for each
sidecar test. Under workspace-wide parallel cargo test load on Windows,
TIME_WAIT-pinned ports exhaust the ephemeral pool ~10% of runs, causing
`os error 10048` failures in test_prompt_context_endpoint_basename_line
_hint_disambiguates_exact_selector and siblings.

Add bind_ephemeral_with_retry() — 5 attempts with 50/100/150/200/250ms
linear backoff on AddrInUse / 10048. No workspace dep added.

Verified: 10-iteration stress run shows 0/10 failures (was 1-2/10
without patch).

Filed 2026-05-09 from AAP F2-Tx-P1 C3 verify-2 audit; vault-tracked at
[[Todos — SymForge]] "Bug report 2026-05-09 — sidecar test Windows TCP
port-pool exhaustion". Closes AAP backlog Track D11 dependency.
```

---

### Task 0.3: Phase 0 verification

- [ ] Run all four cargo gates green.
- [ ] Both bug-fix commits land on top of Task 1.2's HEAD.
- [ ] Confirm via `git log --oneline -6`.
- [ ] Coding agent reports → delegator approves → proceed to Phase 1 Task 1.3.

---

# Phase 1 — Ultrareview Polish (Group 1)

**Ships:** Five small fixes from the 2026-04-24 ultrareview. Single PR, ~half-day. Low risk, visible health improvement. No public API changes.

**Phase 1 acceptance:**
- [x] All `cargo` verification commands pass.
- [x] Health output shows `local_empty_reason` as a top-line banner when the index is empty.
- [x] Health output surfaces watcher reconcile-repairs counter even when watcher is idle (currently only shown when `events_processed > 0`).
- [x] `is_forbidden_root` allows projects literally named `tmp`, `var`, `home`.
- [x] `explore` output has a one-line rank-signal footer.
- [x] `src/sidecar/handlers.rs:1204` and `:1222` unwraps are documented or replaced.

**Phase 1 status:** COMPLETE. Six commits land on top of baseline `0b4c096`:

| Task | Commit | Status | Notes |
|------|--------|--------|-------|
| 1.0  | `0b4c096` | DONE | Inserted dynamically during 1.1 dispatch (delegator-authorized): clear test-target clippy-1.95 pedantic baseline |
| 1.1  | `18e806a` | DONE | handlers.rs `// safe:` unwrap comments |
| 1.2  | `5af8ccc` | DONE | `is_forbidden_root` system-vs-basename split; introduces minor fmt drift (see Task 1.6 report) |
| 1.3  | `9100d8b` | DONE | Empty-index banner. Delegator-approved deviations: `Arc<RwLock<Option<String>>>` wrap on `LiveIndex`, delegating setter on `SharedIndexHandle`, `PublishedIndexState` field extension |
| 1.4  | `34e97fb` | DONE | Idle-watcher reconcile-repairs render. Task 1.5.5 (compact-path render gap) ABSORBED in same commit per delegator authorization |
| 1.5  | `d9eecb5` | DONE | Rank-signal footer on explore. Delegator-approved: skip helper (single-site), test signature corrected to `#[tokio::test]` pattern, two-assertion test strengthening |
| 1.5.5 | absorbed | ABSORBED into 1.4 | Compact-path render gap (`health_report_compact_from_published_state`) — same root, folded into 1.4 |
| 1.6  | (no commit) | THIS REPORT | Verification gate + audit; report pending delegator close-out |

Class C flake tracker: `docs/notes/class-c-flakes.md` — 3 pre-existing libgit2-lockfile-race entries (needs vault sync).

**Deferred follow-ups (not blocking Phase 2):**
- `cargo fmt` housekeeping sweep across ~30 files (codebase-wide accumulated drift, not introduced by Phase 1). 2-3 of those sites were introduced by Phase 1 (`src/discovery/mod.rs` from Task 1.2, `src/sidecar/server.rs` from Task 0.2). Project verification gate does not mandate `cargo fmt --check`, so the drift is non-blocking; a separate `style(fmt)` sweep can land at any later point at the maintainer's discretion.
- Class C tracker file at `docs/notes/class-c-flakes.md` carries 3 libgit2-lockfile flake entries flagged "needs vault sync". Sync to Obsidian deferred to campaign close (post-Phase 4).

---

### Task 1.1: Document handlers.rs unwraps with `// safe:` comment

**Files:**
- Modify: `src/sidecar/handlers.rs:1204` and `:1222`

**Context:** Both `.unwrap()` calls are inside `symbol_context_text` (lines 1107–1324). They iterate `files: Vec<String>`, where `files = map.keys().cloned().collect()`. The unwrap is structurally safe — the keys came from `map`. Document this rather than replace, because `if let Some(refs) = map.get(file)` would force `else` branches that complicate the reference-anchor extraction loop (which needs `evidence_anchors.len() >= 3` short-circuit logic across files).

- [ ] **Step 1: Read the current code at lines 1200–1230**

Run: `cargo check`
Expected: clean baseline.

- [ ] **Step 2: Add `// safe:` comments above both unwraps**

Use `mcp__symforge__edit_within_symbol` with `name="symbol_context_text"`, `path="src/sidecar/handlers.rs"`:

`old_text`:
```rust
    let mut evidence_anchors: Vec<String> = Vec::new();
    for file in &files {
        let refs = map.get(file).unwrap();
```
`new_text`:
```rust
    let mut evidence_anchors: Vec<String> = Vec::new();
    for file in &files {
        // safe: `files` is built from `map.keys()` immediately above; lookup cannot miss.
        let refs = map.get(file).unwrap();
```

`old_text`:
```rust
    for file in &files {
        body_lines.push(format!("── {} ──", file));
        let refs = map.get(file).unwrap();
```
`new_text`:
```rust
    for file in &files {
        body_lines.push(format!("── {} ──", file));
        // safe: `files` is built from `map.keys()` above; lookup cannot miss.
        let refs = map.get(file).unwrap();
```

- [ ] **Step 3: Run cargo check + clippy**

Run: `cargo check && cargo clippy --lib -- -D warnings`
Expected: PASS, no new warnings.

- [ ] **Step 4: Commit**

```bash
git add src/sidecar/handlers.rs
git commit -m "$(cat <<'EOF'
chore(handlers): document symbol_context_text unwraps as safe

Both `map.get(file).unwrap()` calls iterate `files` built from
`map.keys()` immediately above. Lookup cannot miss; document
the invariant rather than complicate the loop with else branches.

From ultrareview 2026-04-24.
EOF
)"
```

---

### Task 1.2: `is_forbidden_root` distinguishes top-level system vs project basename

**Files:**
- Modify: `src/discovery/mod.rs:237-302` (function `is_forbidden_root`)
- Modify: `src/discovery/mod.rs::tests` — add new tests

**Context:** Function already canonicalizes the input path (line 240). Steps 1–3 (drive root, Windows drive root, home) check canonical paths. Step 4 still rejects on basename (`lower.as_str() == "tmp"`), which means a legitimate project at `C:\projects\tmp` gets refused.

The fix: split forbidden names into two lists:
- **System-only names** that are always forbidden regardless of location (`windows`, `system32`, `program files`, `program files (x86)`, `programdata`, `node_modules`, `.npm`, `.cargo`).
- **Top-level container names** that are forbidden only when they sit directly under a drive root (Windows) or filesystem root (`/`) — these are the ambiguous ones (`users`, `home`, `tmp`, `temp`, `var`, `appdata`).

A project at `C:\projects\tmp` has parent `C:\projects` (not a drive root), so it passes. A path of `/tmp` has parent `/` (filesystem root), so it's rejected.

- [ ] **Step 1: Write failing tests first**

Add to `src/discovery/mod.rs::tests` (after `test_is_forbidden_root_allows_project_dirs`):

```rust
#[test]
fn test_is_forbidden_root_allows_project_named_tmp() {
    let tmp = TempDir::new().unwrap();
    let project = tmp.path().join("projects").join("tmp");
    std::fs::create_dir_all(&project).unwrap();
    assert!(
        !is_forbidden_root(&project),
        "project at C:\\projects\\tmp must not be rejected by basename"
    );
}

#[test]
fn test_is_forbidden_root_allows_project_named_var() {
    let tmp = TempDir::new().unwrap();
    let project = tmp.path().join("workspace").join("var");
    std::fs::create_dir_all(&project).unwrap();
    assert!(
        !is_forbidden_root(&project),
        "project at workspace/var must not be rejected by basename"
    );
}

#[test]
fn test_is_forbidden_root_still_blocks_top_level_tmp_on_unix() {
    // Skip on Windows where /tmp doesn't apply
    #[cfg(unix)]
    {
        // /tmp itself is a real path; canonicalize will succeed.
        let path = std::path::Path::new("/tmp");
        if path.exists() {
            assert!(is_forbidden_root(path), "/tmp must still be blocked as system path");
        }
    }
}

#[test]
fn test_is_forbidden_root_still_blocks_windows_system_paths() {
    #[cfg(target_os = "windows")]
    {
        let path = std::path::Path::new(r"C:\Windows\System32");
        if path.exists() {
            assert!(
                is_forbidden_root(path),
                "C:\\Windows\\System32 must remain blocked"
            );
        }
    }
}
```

- [ ] **Step 2: Run failing tests**

Run: `cargo test -p symforge --lib discovery::tests::test_is_forbidden_root_allows_project_named_tmp -- --test-threads=1`
Expected: FAIL — `is_forbidden_root` returns true for the new project paths.

- [ ] **Step 3: Refactor `is_forbidden_root` step 4**

Use `mcp__symforge__replace_symbol_body` with `name="is_forbidden_root"`, `path="src/discovery/mod.rs"`, body:

```rust
fn is_forbidden_root(path: &Path) -> bool {
    // Canonicalize for reliable comparison (resolves symlinks, normalizes separators).
    let path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());

    // 1. Drive roots: C:\, D:\, /, etc.
    if path.parent().is_none() {
        return true;
    }

    // 2. Windows drive roots that have a parent but are still just "C:\"
    #[cfg(target_os = "windows")]
    {
        let path_str = path.to_string_lossy();
        if path_str.len() <= 7 && path_str.ends_with('\\') {
            return true;
        }
    }

    // 3. User home directories.
    if let Some(home) = home_dir() {
        let home = home.canonicalize().unwrap_or(home);
        if path == home {
            return true;
        }
    }

    // 4a. System directory names — always forbidden anywhere.
    //     These are unambiguous: a directory literally named `system32`
    //     or `node_modules` is virtually never a project root.
    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
        let lower = name.to_lowercase();
        const SYSTEM_NAMES: &[&str] = &[
            "windows",
            "system32",
            "program files",
            "program files (x86)",
            "programdata",
            "node_modules",
            ".npm",
            ".cargo",
        ];
        if SYSTEM_NAMES.contains(&lower.as_str()) {
            return true;
        }
    }

    // 4b. Top-level container names — forbidden only when sitting directly
    //     under a filesystem root or drive root. A legitimate project named
    //     `tmp` or `var` deeper in the tree is allowed.
    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
        let lower = name.to_lowercase();
        const CONTAINER_NAMES: &[&str] = &[
            "users", "home", "tmp", "temp", "var", "appdata",
        ];
        if CONTAINER_NAMES.contains(&lower.as_str())
            && path
                .parent()
                .map(|p| {
                    // Parent is a drive root or filesystem root → forbid.
                    p.parent().is_none()
                        || {
                            #[cfg(target_os = "windows")]
                            {
                                let pstr = p.to_string_lossy();
                                pstr.len() <= 7 && pstr.ends_with('\\')
                            }
                            #[cfg(not(target_os = "windows"))]
                            {
                                false
                            }
                        }
                })
                .unwrap_or(false)
        {
            return true;
        }
    }

    // 5. Parent-of-home: e.g. C:\Users or /home
    if let Some(home) = home_dir() {
        let home = home.canonicalize().unwrap_or(home);
        if let Some(parent) = home.parent() {
            let parent = parent
                .canonicalize()
                .unwrap_or_else(|_| parent.to_path_buf());
            if path == parent {
                return true;
            }
        }
    }

    false
}
```

- [ ] **Step 4: Run all discovery tests**

Run: `cargo test -p symforge --lib discovery::tests -- --test-threads=1`
Expected: all PASS, including the 4 new tests and pre-existing ones (`test_is_forbidden_root_blocks_drive_root`, `test_is_forbidden_root_blocks_system_dirs`, `test_is_forbidden_root_allows_project_dirs`, `test_is_forbidden_root_blocks_home_dir`).

- [ ] **Step 5: Commit**

```bash
git add src/discovery/mod.rs
git commit -m "$(cat <<'EOF'
fix(discovery): allow projects named tmp/var/home not under filesystem root

is_forbidden_root previously rejected any directory whose basename matched
a generic container word (`tmp`, `var`, `home`, etc.) regardless of where
it sat in the filesystem. A legitimate project at `C:\projects\tmp` was
refused.

Split forbidden names into system-only (always forbidden) and container
names (forbidden only directly under a drive/fs root). Project paths
deeper in the tree pass cleanly; top-level system paths stay blocked.

From ultrareview 2026-04-24.
EOF
)"
```

---

### Task 1.3: Empty-index banner in `health` output

**Files:**
- Modify: `src/live_index/query.rs::HealthStats` (lines 820-849) — add `local_empty_reason: Option<String>` field
- Modify: `src/live_index/query.rs::health_stats` and `health_stats_with_watcher` — accept optional reason
- Modify: `src/main.rs:267` — thread the empty reason into the LiveIndex/SharedIndex
- Modify: `src/protocol/format.rs::health_report_from_stats` — render banner at top when reason is Some
- Modify: `src/protocol/format/tests.rs` — new test

**Context:** When SymForge starts with `StartupPlan::LocalEmpty`, `src/main.rs:267` logs the reason via `tracing::info!` to stderr. Claude Desktop/Code rarely surface stderr to the user. The agent sees `health` saying "0 symbols, project=project" and has no clue why.

The fix: store the reason in the live index at startup, surface it in `HealthStats`, render at the top of `health` output as an actionable banner.

- [ ] **Step 1: Add field to `HealthStats`**

Use `mcp__symforge__edit_within_symbol` with `name="HealthStats"`, `path="src/live_index/query.rs"`. Add `pub local_empty_reason: Option<String>,` as the last field (just before the closing brace).

Verify final state by reading the struct — it should now end with:
```rust
    pub last_overflow_at: Option<std::time::SystemTime>,
    pub last_reconcile_at: Option<std::time::SystemTime>,
    pub local_empty_reason: Option<String>,
}
```

- [ ] **Step 2: Add storage on `LiveIndex`**

Search for the `LiveIndex` struct definition (use `mcp__symforge__search_symbols` with `query="LiveIndex"` `kind="struct"`). It lives in `src/live_index/store.rs`. Add field:
```rust
pub(crate) local_empty_reason: parking_lot::RwLock<Option<String>>,
```
And a setter on `LiveIndex`:
```rust
pub fn set_local_empty_reason(&self, reason: Option<String>) {
    *self.local_empty_reason.write() = reason;
}

pub fn local_empty_reason(&self) -> Option<String> {
    self.local_empty_reason.read().clone()
}
```
Initialize the field as `RwLock::new(None)` in `LiveIndex::empty()` and any other constructor.

- [ ] **Step 3: Populate from startup in `src/main.rs`**

Find the `LocalEmpty` branch (around `src/main.rs:267`). After `(live_index::LiveIndex::empty(), "project".to_string(), None)`, store the reason. Look for where the live index is wrapped:

```rust
        tracing::info!("{}", local_empty_reason(should_auto_index));
        let live = live_index::LiveIndex::empty();
        live.set_local_empty_reason(Some(local_empty_reason(should_auto_index).to_string()));
        (live, "project".to_string(), None)
```

- [ ] **Step 4: Plumb the reason through `health_stats`**

`src/live_index/query.rs::health_stats` (line 2436) and `health_stats_with_watcher` (line 2498) build a `HealthStats`. Add `local_empty_reason: self.local_empty_reason()` to the struct literal in both functions.

- [ ] **Step 5: Write failing test for banner**

Add to `src/protocol/format/tests.rs` (near the other `health_report_from_stats` tests):

```rust
#[test]
fn test_health_report_shows_empty_index_banner_with_reason() {
    let stats = HealthStats {
        file_count: 0,
        symbol_count: 0,
        parsed_count: 0,
        partial_parse_count: 0,
        failed_count: 0,
        load_duration: std::time::Duration::ZERO,
        watcher_state: crate::watcher::WatcherState::Off,
        events_processed: 0,
        last_event_at: None,
        debounce_window_ms: 200,
        overflow_count: 0,
        stale_files_found: 0,
        last_overflow_at: None,
        last_reconcile_at: None,
        partial_parse_files: vec![],
        failed_files: vec![],
        tier_counts: (0, 0, 0),
        local_empty_reason: Some(
            "no safe project root found — starting with empty index".to_string(),
        ),
    };
    let report = health_report_from_stats("Ready", &stats);
    assert!(
        report.contains("Empty index"),
        "report should announce empty-index state; got:\n{report}"
    );
    assert!(
        report.contains("no safe project root"),
        "report must surface the reason verbatim; got:\n{report}"
    );
    assert!(
        report.contains("index_folder") || report.contains("--root"),
        "report should suggest a recovery; got:\n{report}"
    );
}

#[test]
fn test_health_report_omits_empty_banner_when_index_populated() {
    let sym = make_symbol("foo", SymbolKind::Function, 0, 1, 3);
    let file = make_file("src/lib.rs", LanguageId::Rust, vec![sym]);
    let mut index = LiveIndex::empty();
    index.add_file(file);
    index.publish();
    let stats = index.health_stats();
    let report = health_report_from_stats("Ready", &stats);
    assert!(
        !report.contains("Empty index"),
        "report must not show empty-index banner when files exist; got:\n{report}"
    );
}
```

- [ ] **Step 6: Run failing tests**

Run: `cargo test -p symforge --lib protocol::format::tests::test_health_report_shows_empty_index_banner -- --test-threads=1`
Expected: FAIL — banner not yet rendered.

- [ ] **Step 7: Render the banner in `health_report_from_stats`**

Use `mcp__symforge__edit_within_symbol` with `name="health_report_from_stats"`, `path="src/protocol/format.rs"`. After the line `let mut output = format!(...)` (around line 1192) and BEFORE the `if stats.partial_parse_count > 0` block (line 1205), insert:

```rust
    if let Some(reason) = stats.local_empty_reason.as_deref()
        && stats.file_count == 0
    {
        let banner = format!(
            "\n\n⚠ Empty index — {reason}\n  Recovery: call index_folder(path=\"<your-project-root>\") or restart with --root <path>"
        );
        output.push_str(&banner);
    }
```

- [ ] **Step 8: Run tests**

Run: `cargo test -p symforge --lib protocol::format::tests -- --test-threads=1`
Expected: all PASS.

- [ ] **Step 9: Verify other call sites compile**

Run: `cargo check --all-targets`
Expected: PASS. Any callers that build `HealthStats` literals (search via `mcp__symforge__find_references` for `HealthStats {`) need the new field.

- [ ] **Step 10: Commit**

```bash
git add src/live_index/store.rs src/live_index/query.rs src/main.rs src/protocol/format.rs src/protocol/format/tests.rs
git commit -m "$(cat <<'EOF'
feat(health): surface empty-index reason as actionable banner

When SymForge starts with no safe project root or auto-index disabled,
src/main.rs logged the reason to stderr only. MCP clients (Claude
Desktop, Claude Code) rarely surface stderr, leaving agents with a
zero-symbol index and no diagnostic.

Plumb the reason through HealthStats and render at the top of `health`
output with a recovery hint pointing at index_folder/--root.

From ultrareview 2026-04-24.
EOF
)"
```

---

### Task 1.4: Promote watcher reconcile metric on idle watcher

**Files:**
- Modify: `src/protocol/format.rs::health_report_from_stats` — change idle-watcher branch
- Modify: `src/protocol/format/tests.rs` — new test

**Context:** Currently the idle-watcher line (lines 1153–1163 in `format.rs`) shows only:
```
Watcher: active (idle; event-driven, waiting for filesystem changes, debounce: 200ms)
```
Reconcile-repairs is buried unless `events_processed > 0`. Reconcile is the proof-of-correctness counter (599 repairs in one session is meaningful evidence the watcher caught FS drift). Promote it to the idle line so it's never hidden.

- [ ] **Step 1: Failing test**

Add to `src/protocol/format/tests.rs`:

```rust
#[test]
fn test_health_report_idle_watcher_shows_reconcile_repairs() {
    let stats = HealthStats {
        file_count: 100,
        symbol_count: 1000,
        parsed_count: 100,
        partial_parse_count: 0,
        failed_count: 0,
        load_duration: std::time::Duration::from_millis(500),
        watcher_state: crate::watcher::WatcherState::Active,
        events_processed: 0,
        last_event_at: None,
        debounce_window_ms: 200,
        overflow_count: 0,
        stale_files_found: 7,  // 7 reconcile repairs without any fired events
        last_overflow_at: None,
        last_reconcile_at: Some(std::time::SystemTime::now()),
        partial_parse_files: vec![],
        failed_files: vec![],
        tier_counts: (100, 0, 0),
        local_empty_reason: None,
    };
    let report = health_report_from_stats("Ready", &stats);
    assert!(
        report.contains("reconcile repairs: 7"),
        "idle watcher line must surface reconcile repairs even when no events fired; got:\n{report}"
    );
}
```

- [ ] **Step 2: Run, expect FAIL**

Run: `cargo test -p symforge --lib protocol::format::tests::test_health_report_idle_watcher_shows_reconcile_repairs -- --test-threads=1`

- [ ] **Step 3: Tighten the idle-branch condition**

In `health_report_from_stats`, change the idle match arm. The current condition is:
```rust
WatcherState::Active
    if stats.events_processed == 0
        && stats.last_event_at.is_none()
        && stats.overflow_count == 0
        && stats.stale_files_found == 0 =>
{
    format!(
        "Watcher: active (idle; event-driven, waiting for filesystem changes, debounce: {}ms)",
        stats.debounce_window_ms
    )
}
```

Replace with two arms — fully-idle (no reconcile activity) vs idle-but-reconciled:

```rust
WatcherState::Active
    if stats.events_processed == 0
        && stats.last_event_at.is_none()
        && stats.overflow_count == 0
        && stats.stale_files_found == 0 =>
{
    format!(
        "Watcher: active (idle; event-driven, waiting for filesystem changes, debounce: {}ms)",
        stats.debounce_window_ms
    )
}
WatcherState::Active
    if stats.events_processed == 0 && stats.last_event_at.is_none() =>
{
    format!(
        "Watcher: active (idle; debounce: {}ms, overflows: {}, reconcile repairs: {}, last reconcile: {})",
        stats.debounce_window_ms,
        stats.overflow_count,
        stats.stale_files_found,
        relative_age(stats.last_reconcile_at)
    )
}
WatcherState::Active => format!( /* existing event-driven format unchanged */ ),
```

- [ ] **Step 4: Run all format tests**

Run: `cargo test -p symforge --lib protocol::format::tests -- --test-threads=1`
Expected: all PASS, including the new one and pre-existing `test_health_report_shows_reconciliation_and_overflow_stats`.

- [ ] **Step 5: Commit**

```bash
git add src/protocol/format.rs src/protocol/format/tests.rs
git commit -m "$(cat <<'EOF'
feat(health): surface reconcile repairs on idle watcher line

Reconcile repairs were only shown when events_processed > 0. A watcher
that quietly fixed 600 FS-drift cases between events showed up as
"active (idle)" with no proof of work. Surface the counter in the idle
line whenever it's non-zero.

From ultrareview 2026-04-24.
EOF
)"
```

---

### Task 1.5: Rank-signal footer on `explore` output

**Files:**
- Modify: `src/protocol/tools.rs::explore` (lines 5502-6058) — append footer before final return

**Context:** Explore output includes `[1.00]`, `[0.86]` scores per result. Agents don't know if those are name-match × path-proximity or something else. Add a one-line footer at the bottom: `ranked by: concept match + symbol-token alignment + path proximity + caller density`. No tunable, just transparency.

- [ ] **Step 1: Locate the final return path of `explore`**

Read `src/protocol/tools.rs` lines 5980-6058 (end of `explore` function). The function ends with several `return` paths formatting `output`. Find the place where the final string is assembled before the `record_summary_output` call.

- [ ] **Step 2: Failing test**

Add a test in `src/protocol/tools.rs::tests`:

```rust
#[test]
fn test_explore_output_footer_documents_ranking() {
    let server = make_live_index_ready();
    let result = futures::executor::block_on(server.explore(Parameters(
        crate::protocol::tools::ExploreInput {
            query: "error handling".to_string(),
            ..Default::default()
        },
    )));
    assert!(
        result.contains("ranked by:"),
        "explore output must include rank-signal footer; got:\n{result}"
    );
}
```

If `futures::executor::block_on` isn't already imported, copy the pattern from neighboring tests (search for `block_on` in the same test module).

- [ ] **Step 3: Run, expect FAIL**

Run: `cargo test -p symforge --lib test_explore_output_footer_documents_ranking -- --test-threads=1`

- [ ] **Step 4: Implement footer**

Find the spot in `explore` where the final result string is built but before `record_summary_output`. Append:

```rust
        if !result.is_empty() {
            result.push_str(
                "\n\nranked by: concept match + symbol-token alignment + path proximity + caller density"
            );
        }
```

If there are multiple return paths, factor out into a helper at top of the function:
```rust
    fn append_rank_footer(s: &mut String) {
        if !s.is_empty() {
            s.push_str("\n\nranked by: concept match + symbol-token alignment + path proximity + caller density");
        }
    }
```
Call it on every non-error return.

- [ ] **Step 5: Run tests**

Run: `cargo test -p symforge --lib test_explore -- --test-threads=1`
Expected: all PASS, no regressions on existing explore tests.

- [ ] **Step 6: Commit**

```bash
git add src/protocol/tools.rs
git commit -m "$(cat <<'EOF'
feat(explore): append rank-signal footer documenting score composition

The `[1.00]` / `[0.86]` scores are useful but opaque. A one-line footer
explaining the composition lets agents decide whether to trust top-1 or
widen the search. No tunable surface — the footer is plain documentation.

From ultrareview 2026-04-24.
EOF
)"
```

---

### Task 1.6: Phase 1 integration verification

- [ ] **Step 1: Full test run**

Run: `cargo check && cargo clippy --all-targets -- -D warnings && cargo test --all-targets -- --test-threads=1`
Expected: PASS.

- [ ] **Step 2: Release build smoke test**

Run: `cargo build --release`
Expected: PASS.

- [ ] **Step 3: Manual smoke**

Run the binary with `--root <some-empty-dir>`:
```bash
./target/release/symforge --root /tmp/empty_test 2>/dev/null
```
Then in another terminal call `health` via the MCP client (or direct stdio test). Expected: banner shows "Empty index — no safe project root..." with recovery hint. This is a manual verification, not a test.

---

# Phase 2 — Index Hygiene (Group 4)

**Ships:** Default-exclude vendor and personal-tooling directories from `search_text` / `explore` output (not the index itself). Update `.gitignore` policy on `.claude/gsd-*`.

**Phase 2 acceptance:**
- [ ] `search_text` and `explore` exclude vendor/`.claude/gsd-*` paths by default.
- [ ] An `include_vendor=true` flag opens the gate (matching existing `include_generated`/`include_tests` pattern).
- [ ] Filtered-count appears in the search envelope ("N noise-filtered match(es) suppressed" already exists; vendor-suppressed count joins it).

---

### Task 2.1: Add `is_vendor_path` and `is_personal_tooling_path` predicates

**Files:**
- Modify: `src/live_index/query.rs` (utility module section near line 757-794) — add two new path predicates next to `is_filtered_name`.

**Context:** `search_text` and `explore` already filter `generated` and `tests` paths. Add `vendor` and `personal_tooling` filters. Vendor = path components like `vendor/`, `third_party/`, `node_modules/` (already gitignored but may be in worktree state). Personal tooling = `.claude/gsd-local-patches/`, `.claude/get-shit-done/`.

- [ ] **Step 1: Failing test**

Add to `src/live_index/query.rs::tests`:

```rust
#[test]
fn test_is_vendor_path_matches_vendor_dirs() {
    assert!(is_vendor_path("vendor/tree-sitter-scss/src/parser.c"));
    assert!(is_vendor_path("third_party/foo/bar.rs"));
    assert!(is_vendor_path("node_modules/react/index.js"));
    assert!(!is_vendor_path("src/parsing/mod.rs"));
    assert!(!is_vendor_path("tests/vendor_smoke.rs")); // basename, not directory
}

#[test]
fn test_is_personal_tooling_path_matches_claude_dirs() {
    assert!(is_personal_tooling_path(".claude/gsd-local-patches/foo.md"));
    assert!(is_personal_tooling_path(".claude/get-shit-done/bar.sh"));
    assert!(!is_personal_tooling_path(".claude/CLAUDE.md")); // root-level claude config
    assert!(!is_personal_tooling_path("src/lib.rs"));
}
```

- [ ] **Step 2: Run, expect FAIL**

Run: `cargo test -p symforge --lib query::tests::test_is_vendor_path -- --test-threads=1`
Expected: FAIL — functions don't exist.

- [ ] **Step 3: Implement predicates**

Add to `src/live_index/query.rs` near `is_filtered_name` (line 757):

```rust
/// Returns true if `path` lives under a vendored / third-party directory.
/// Used by search_text and explore to suppress noise unless the caller
/// explicitly opts in via `include_vendor=true`.
pub(crate) fn is_vendor_path(path: &str) -> bool {
    const VENDOR_COMPONENTS: &[&str] = &["vendor", "third_party", "third-party", "node_modules"];
    path.split('/')
        .any(|c| VENDOR_COMPONENTS.contains(&c.to_ascii_lowercase().as_str()))
}

/// Returns true if `path` is personal-tooling sidecar content
/// (`.claude/gsd-local-patches/`, `.claude/get-shit-done/`) that ships
/// with the repo but is not part of symforge's own crate code.
pub(crate) fn is_personal_tooling_path(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    lower.starts_with(".claude/gsd-local-patches/")
        || lower.starts_with(".claude/get-shit-done/")
        || lower.starts_with(".claude/gsd-")
}
```

- [ ] **Step 4: Tests pass**

Run: `cargo test -p symforge --lib query::tests::test_is_vendor_path query::tests::test_is_personal_tooling_path -- --test-threads=1`

- [ ] **Step 5: Commit**

```bash
git add src/live_index/query.rs
git commit -m "$(cat <<'EOF'
feat(query): add vendor/personal-tooling path predicates

Two pure-function path predicates used by search_text and explore in the
next commit. Vendor = vendor/ third_party/ node_modules/ as path
components. Personal tooling = .claude/gsd-* sidecar dirs.

From ultrareview 2026-04-24.
EOF
)"
```

---

### Task 2.2: Wire `include_vendor` into `SearchTextInput` and `ExploreInput`

**Files:**
- Modify: `src/protocol/tools.rs::SearchTextInput` (line 311-375) — add `include_vendor: Option<bool>`
- Modify: `src/protocol/tools.rs::ExploreInput` (line 729-754) — add `include_vendor: Option<bool>`
- Modify: `src/protocol/tools.rs::search_text` (line 3519) — wire flag into output filtering
- Modify: `src/protocol/tools.rs::explore` (line 5502) — wire flag into output filtering

- [ ] **Step 1: Add field to `SearchTextInput`**

Use `mcp__symforge__edit_within_symbol` with `name="SearchTextInput"`, `path="src/protocol/tools.rs"`. After the existing `include_tests` field, add:

```rust
    /// When true, include vendored/third-party paths (vendor/, node_modules/, third_party/).
    /// Default false — vendor noise dominates results in repos with embedded grammars.
    #[serde(default, deserialize_with = "lenient_bool")]
    pub include_vendor: Option<bool>,
    /// When true, include personal tooling paths (.claude/gsd-*).
    /// Default false — personal sidecars rarely answer code questions.
    #[serde(default, deserialize_with = "lenient_bool")]
    pub include_personal_tooling: Option<bool>,
```

- [ ] **Step 2: Same fields on `ExploreInput`**

Use `mcp__symforge__edit_within_symbol` with `name="ExploreInput"`, `path="src/protocol/tools.rs"`. Same two fields. (`ExploreInput` already has `include_noise`; vendor and personal-tooling are finer-grained.)

- [ ] **Step 3: Failing test for `search_text`**

Add to `src/protocol/tools.rs::tests`:

```rust
#[test]
fn test_search_text_hides_vendor_paths_by_default() {
    let server = make_live_index_ready_with_vendor_file();
    let result = futures::executor::block_on(server.search_text(Parameters(
        crate::protocol::tools::SearchTextInput {
            query: Some("noisy_token".to_string()),
            ..Default::default()
        },
    )));
    assert!(
        !result.contains("vendor/"),
        "vendor paths must be suppressed by default; got:\n{result}"
    );
    assert!(
        result.contains("noise-filtered"),
        "filtered-count footer expected; got:\n{result}"
    );
}

#[test]
fn test_search_text_includes_vendor_when_flag_true() {
    let server = make_live_index_ready_with_vendor_file();
    let result = futures::executor::block_on(server.search_text(Parameters(
        crate::protocol::tools::SearchTextInput {
            query: Some("noisy_token".to_string()),
            include_vendor: Some(true),
            ..Default::default()
        },
    )));
    assert!(
        result.contains("vendor/"),
        "vendor paths must appear when include_vendor=true; got:\n{result}"
    );
}
```

Add the helper `make_live_index_ready_with_vendor_file` near other `make_live_index_*` helpers — copy `make_live_index_ready` and add an indexed file at path `vendor/foo/bar.rs` containing the literal `noisy_token`.

- [ ] **Step 4: Run, expect FAIL**

Run: `cargo test -p symforge --lib test_search_text_hides_vendor -- --test-threads=1`

- [ ] **Step 5: Implement filter inside `search_text` handler**

In `search_text`, after the search runs but before the envelope is built, filter results. Find the section that constructs `SearchTextResult` (or whatever the typed view is — search via `mcp__symforge__search_text` with `query="render_search_text_output"` to find the formatter). Add:

```rust
let include_vendor = params.0.include_vendor.unwrap_or(false);
let include_personal = params.0.include_personal_tooling.unwrap_or(false);
let mut filtered_count: usize = 0;
let original_total = results.len();
results.retain(|hit| {
    if !include_vendor && crate::live_index::query::is_vendor_path(&hit.path) {
        filtered_count += 1;
        false
    } else if !include_personal
        && crate::live_index::query::is_personal_tooling_path(&hit.path)
    {
        filtered_count += 1;
        false
    } else {
        true
    }
});
// `filtered_count` then feeds into the existing envelope footer that already
// reports "N noise-filtered match(es) suppressed".
```

(The exact code shape depends on whether `search_text` returns a `Vec<SearchTextHit>` or a typed view — adjust the field names to match.)

- [ ] **Step 6: Same for `explore` handler**

Apply the same filter pattern inside `explore` before the result list is sorted/truncated.

- [ ] **Step 7: Run all tests**

Run: `cargo test -p symforge --lib test_search_text test_explore -- --test-threads=1`
Expected: all PASS, including the two new tests and pre-existing `test_explore_hides_vendor_noise_by_default` (which already exists at line 12435 — verify the existing semantics match the new flag, adjust if needed).

- [ ] **Step 8: Commit**

```bash
git add src/protocol/tools.rs
git commit -m "$(cat <<'EOF'
feat(search): default-exclude vendor and personal-tooling paths

search_text and explore now suppress vendor/ third_party/ node_modules/
and .claude/gsd-* paths by default. Callers opt in via
include_vendor=true / include_personal_tooling=true.

Vendor noise was ~17% of the index in this repo (2570/15303 symbols
under vendor/tree-sitter-scss). Personal tooling (.claude/gsd-*) added
~227 files / ~3400 symbols of unrelated content. Both leak into
ranked results without contributing navigation signal.

From ultrareview 2026-04-24.
EOF
)"
```

---

### Task 2.3: Decide `.claude/gsd-*` repo policy

**Context:** ultrareview flagged ~227 files / ~3400 symbols of personal tooling shipping with the crate. Two valid choices:
1. **Document as quickstart**: top-level README section noting these are example workflows.
2. **Gitignore**: remove from version control.

The plan for the coding agent is option **2** (gitignore) — these are personal sidecars, not project assets. They were committed accidentally.

- [ ] **Step 1: Read current `.gitignore`**

Run: `cat .gitignore`
Expected: see existing patterns.

- [ ] **Step 2: Append rules**

Append to `.gitignore`:
```gitignore

# Personal Claude Code tooling — not part of the SymForge crate.
.claude/gsd-local-patches/
.claude/get-shit-done/
.claude/gsd-*
```

- [ ] **Step 3: Remove from git tracking without deleting from disk**

Run: `git rm -r --cached .claude/gsd-local-patches .claude/get-shit-done 2>/dev/null || true`
(Tolerate failure if directories aren't tracked yet.)

- [ ] **Step 4: Verify**

Run: `git status`
Expected: `.gitignore` modified, `.claude/gsd-*` either deleted-from-index or untracked.

- [ ] **Step 5: Commit**

```bash
git add .gitignore
git add -u  # picks up the rm --cached deletions
git commit -m "$(cat <<'EOF'
chore: gitignore personal Claude tooling sidecars

.claude/gsd-local-patches/ and .claude/get-shit-done/ together accounted
for ~80% of the indexed file count in this repo without being part of
the symforge crate. They are personal workflow tooling, not project
assets — gitignore them and drop from index.

Files remain on disk for the maintainer; they just stop riding into
clones and search results.

From ultrareview 2026-04-24.
EOF
)"
```

---

### Task 2.4: Phase 2 verification

- [ ] Run: `cargo check && cargo clippy --all-targets -- -D warnings && cargo test --all-targets -- --test-threads=1`
- [ ] Verify the working tree is clean: `git status`

---

# Phase H — Stability Hotfix (inserted 2026-05-12)

**Plan-doc:** `docs/plans/2026-05-12-symforge-stability-hotfix.md`

**Why inserted:** Three external evaluator reports on 2026-05-11 surfaced 1 catastrophic (B-P0-1) and 12 high-severity defects (B-P1-1 through B-P1-7, B-P2-1 through B-P2-5). Read-only investigation verified every claim against code at HEAD `f804d21` via 11 parallel-read spot checks of the most load-bearing anchors. The user declined to install the build pushed on 2026-05-11 because B-P0-1 destroys the index on every `index_folder` call against a new root (1135 files → 4 files in 2 minutes on Windows).

**Insertion strategy (Option C, authorized 2026-05-12):** C-1 catastrophe-fix (H.1a-d) lands NOW between Phase 2.2 and Phase 2.3. C-2 ranker-substrate fixes (H.4, H.5) interleave between Phase 3.2 and Phase 3.3. C-3 opportunistic fixes (H.2, H.3, H.6) land before Phase 4. C-4 stability followup (H.7-H.12) defers to a dedicated post-Phase-4 sprint with its own plan-doc.

**Sequence overlay:**

```
[shipped: Phase 0, 1, 2.1, 2.2]
-> Phase H C-1 (H.1a -> H.1b -> H.1c -> H.1d)   [CATASTROPHE FIX]
-> Phase 2.3, 2.4                                [resume]
-> Phase 3.1, 3.2                                [CoChange data plumbing]
-> Phase H C-2 (H.4, H.5)                        [ranker-substrate prereq]
-> Phase H C-3 (H.2, H.3, H.6)                   [opportunistic]
-> Phase 3.3-3.6                                 [CoChange fusion]
-> Phase 4
-> Phase H C-4 (H.7-H.12 stability followup)
```

**Source-of-truth evidence:**

- `docs/notes/external-evaluations/2026-05-11/SYMFORGE_TEST_REPORT_2026-05-11_01.md` — evaluator 1
- `docs/notes/external-evaluations/2026-05-11/SYMFORGE_EVALUATION_2026-05-11.md` — Kimi Code CLI (identified P0)
- `docs/notes/external-evaluations/2026-05-11/SYMFORGE_TEST_REPORT_2026-05-11_02.md` — evaluator 3 (identified reference-engine defects)
- `docs/notes/external-evaluations/2026-05-11/INVESTIGATION_B-P0-1.md` — read-only verification of P0 (Mechanism A confirmed at code level; Mechanism B refuted; Mechanism C latent)
- `docs/notes/external-evaluations/2026-05-11/INVESTIGATION_HEALTH_REFS.md` — read-only verification of B-P1-6 health + B-P1-2 dependents + B-P1-3 references

**Phase H C-1 close-out is the gate before Phase 2.3 resumes.** See plan-doc for per-task spec.

**C-1 software-side closed 2026-05-14.** Commit chain on `main` (10 ahead of `origin/main` at C-1 close, +3 with G1.x close-out work = 13 total):

- `9325fd1` H.1a — project-generation fence for `SharedIndexHandle` mutations
- `4fa5a97` H.1b — cooperative cancellation token for watcher reload paths
- `3d225be` H.1c — generation-fence migration + Layer 3 bounded-backoff retry
- `8007a2d` H.1d — close sibling leak surfaces
- `a2d81bc` H.1e — generation fence for `git_temporal` publication (extension)
- `b0f202b` H.1f — pre-flight generation fence for `refresh_on_reconcile_tick` (extension)
- `d75a37b` G1.8 — AAP-shaped fixture smoke (no destruction over 35 s idle, root A 1083 → 1083 delta 0, root B 1083 → 1083 delta 0)
- `083584b` G1.2 docs split — invariant criterion + public-API regression criterion
- `fc9f288` G1.2 public-API regression — two-witness: FAIL at `f804d21` (`stale root A watcher event destroyed root B index: initial=32, lowest=0, tolerance=2`), PASS on HEAD

**Remaining Gate 1 close-out (user-side, manual):** ≥30 min dogfood session on a non-symforge project with ≥1 cross-root `index_folder`, plus manual Kimi repro (large repo, 5 min idle, file count unchanged).

**Push deferred until full Phase H ships.**

**C-2 software-side closed 2026-05-15.** Restructured 2026-05-12 (round-2 walk item 4) and 2026-05-14 (ADR pulled forward) merged the original C-2 ranker-substrate bucket and C-3 opportunistic bucket into a single user-trust + correctness bucket (H.2, H.3, H.4, H.5, H.6, H.7) plus ADR 0014. Commit chain on `main` (post-C-1):

- `251d7f0` H.4 — `find_dependents` Pass 2 collision filter
- `42c8e16` H.5 — `find_references` qualified-path coverage via shared collector (also shipped the H.7 source fix as side effect of the shared collector consolidation)
- `405aae0` H.2 — plan-doc allowed-files expansion (tools.rs health handlers)
- `be509a7` H.2 — `health` / `health_compact` source-of-truth unification
- `766c72c` ADR 0014 — watcher-subsystem spawn-blocking discipline (pulled forward from C-4)
- `e691f10` H.6 — `get_symbol_context` / `get_file_context` render budget + test-module collapse
- `9e5a93f` H.7 — `batch_rename` timeout profile (root cause identified pre-`42c8e16`)
- `71feb4f` H.3-slot bonus — innermost enclosing symbol resolution for `search_text` nested-item matches (scope error: not a gate criterion)
- `cabe312` H.7 — `batch_rename` regression coverage (perf budget assertion)
- `4d939f1` H.3 — `search_text(structural=true)` envelope label fix (B-P1-7)
- `82624e2` C-2 close-out evidence matrix + final verification transcript

Final consolidated verification on `main` HEAD: `cargo test --all-targets -- --test-threads=1` PASS 1969/0/4, `cargo test --all-targets` PASS 1969/0/4, `cargo clippy -- -D warnings` PASS, `cargo check` PASS. Transcript: `docs/notes/2026-05-15-c2-final-verification.txt`. Evidence matrix: `docs/notes/2026-05-15-c2-close-out-evidence.md`.

**Push gate non-negotiable:** Phase H push to `origin/main` blocked until both manual user-side gates (Kimi repro, ≥30 min dogfood on non-symforge project) run green per Gate 1 deferral note in the hotfix plan-doc.

---

# Phase 3 — CoChange T3.3 Ranker Fusion (Group 2)

**Ships:** Co-change signal lit up. `search_files(rank_by="path+cochange")` rewards files that frequently co-change with the agent's anchor. Default behavior unchanged (byte-identical existing goldens). Implements ADR 0013's 6-rule contract.

**Phase 3 acceptance:**
- [ ] `CoChangeSignal::score()` returns non-zero when `RankCtx.co_change_count` is populated.
- [ ] `search_files(rank_by="path+cochange", anchor_path="...")` reranks results using `CouplingStore`.
- [ ] Default mode (`rank_by` unset or `"path"`) is byte-identical to existing `tests/rank_signal_behavior.rs` goldens.
- [ ] `tests/cochange_fusion.rs` covers the 6 ADR rules + 4 failure modes (rules 1, 2, 4, 6 minimum per ADR).
- [ ] `changed_with=` branch in `search_files` migrated to fused path.
- [ ] Latency stays under existing rank-signal budget (assert in tests).

**Pre-flight: read ADR 0013 once more.** The contract is the spec. Do not reinterpret the rules.

---

### Task 3.1: Extend `RankCtx` with coupling fields

**Files:**
- Modify: `src/live_index/rank_signals.rs::RankCtx` (lines 39-48)

**Context:** `RankCtx` currently holds `query`, `tokens`, `current_file`, `target_path`. To fuse coupling, add an optional per-candidate coupling input. Callers populate via a new builder method.

- [ ] **Step 1: Failing test**

Add to `src/live_index/rank_signals.rs::tests`:

```rust
#[test]
fn co_change_signal_returns_non_zero_when_coupling_count_set() {
    let mut ctx = RankCtx::empty();
    ctx.co_change_count = Some(7);
    let signal = CoChangeSignal;
    let score = signal.score(std::path::Path::new("src/foo.rs"), &ctx);
    assert!(score > 0.0, "expected non-zero score with coupling count, got {score}");
}

#[test]
fn co_change_signal_returns_zero_without_coupling_count() {
    let ctx = RankCtx::empty();
    let signal = CoChangeSignal;
    let score = signal.score(std::path::Path::new("src/foo.rs"), &ctx);
    assert_eq!(score, 0.0, "expected zero score without coupling input");
}
```

- [ ] **Step 2: Run, expect FAIL**

- [ ] **Step 3: Add fields to `RankCtx`**

```rust
#[derive(Clone, Copy, Default)]
pub struct RankCtx<'a> {
    pub query: &'a str,
    pub tokens: &'a [String],
    pub current_file: Option<&'a str>,
    pub target_path: Option<&'a str>,
    /// When fused with coupling evidence, the number of shared commits between
    /// this candidate and the rerank anchor. None on the path-only ranker.
    pub co_change_count: Option<u32>,
    /// When fused with coupling evidence, the per-anchor weighted score from
    /// the coupling store. None on the path-only ranker.
    pub co_change_weighted_score: Option<f32>,
}
```

Update `RankCtx::empty()` and the `Default` impl to initialize the new fields to `None`.

- [ ] **Step 4: Implement `CoChangeSignal::score()`**

Use `mcp__symforge__replace_symbol_body` to replace the body of `score` inside `impl RankSignal for CoChangeSignal`:

```rust
fn score(&self, _path: &Path, ctx: &RankCtx<'_>) -> f32 {
    // Per ADR 0013 rule 6: gates are RELATIVE not absolute. The raw
    // weighted_score from the coupling store cannot be trusted across
    // repos. We use shared_commits as the fused signal because it is
    // commit-cadence-portable.
    //
    // Per ADR 0013 rule 1: file-level pairs require shared_commits >= 2.
    // The query-time gate already enforces this via query_with_floor;
    // here we just translate the count into a score on the same scale
    // as PathMatchSignal (Strong=1000, Basename=100, Prefix=50, Loose=10).
    //
    // Score curve: 0 commits → 0, 2 commits → 50, 5 commits → 125,
    // 20 commits (cap) → 500. Stays below STRONG_PATH_SCORE so a true
    // path match always wins, above BASENAME_SCORE so strongly-coupled
    // files beat an unrelated basename match.
    match ctx.co_change_count {
        Some(count) => {
            const PER_COMMIT_WEIGHT: f32 = 25.0;
            const COUNT_CAP: u32 = 20;
            (count.min(COUNT_CAP) as f32) * PER_COMMIT_WEIGHT
        }
        None => 0.0,
    }
}
```

- [ ] **Step 5: Run tests**

Run: `cargo test -p symforge --lib rank_signals -- --test-threads=1`
Expected: PASS — the two new tests + all pre-existing rank-signal tests still green (they pass `co_change_count: None` via `Default`).

- [ ] **Step 6: Commit**

```bash
git add src/live_index/rank_signals.rs
git commit -m "$(cat <<'EOF'
feat(rank-signals): wire CoChangeSignal::score against RankCtx coupling input

CoChangeSignal::score() now reads RankCtx::co_change_count and
returns a score scaled to align with PathMatchSignal tiers:
  2 commits  → 50  (above BASENAME_SCORE=100 it loses; above noise)
  5 commits  → 125 (beats Basename, loses to Strong)
  20 commits → 500 (cap; below STRONG_PATH_SCORE=1000)

Cap and curve picked per ADR 0013 §1 (>=2 floor) and §6 (relative not
absolute gating). RankCtx callers that don't populate co_change_count
still get 0.0 — default behavior unchanged.

T3.3 step 1 of 6.
EOF
)"
```

---

### Task 3.2: Expose coupling store handle to query path

**Files:**
- Modify: `src/live_index/coupling/lifecycle.rs` — add a function to retrieve the per-workspace store
- Modify: `src/live_index/store.rs` (or wherever `LiveIndex` lives) — cache an `Arc<CouplingStore>` handle

**Context:** Today the coupling store is opened at boot and reconciled on a 30-second tick (per todo entry T3.1). The query path (`capture_search_files_view`) needs read access. Add an accessor.

- [ ] **Step 1: Read coupling lifecycle**

Run: `cat src/live_index/coupling/lifecycle.rs`
Identify how the store is created (likely behind `SYMFORGE_COUPLING=1` env gate) and whether a handle is already cached.

- [ ] **Step 2: Add `LiveIndex::coupling_store()` accessor**

Search for where `init_coupling_store` is called (use `mcp__symforge__find_references` for `init_coupling_store`). Likely called from `LiveIndex` constructor or boot path. Cache the result on `LiveIndex` as `Option<Arc<parking_lot::Mutex<CouplingStore>>>`. Add accessor:

```rust
impl LiveIndex {
    pub fn coupling_store(&self) -> Option<Arc<parking_lot::Mutex<CouplingStore>>> {
        self.coupling_store.clone()
    }
}
```

- [ ] **Step 3: Integration test — coupling store accessible after init**

Create `tests/cochange_fusion.rs` (new file). Header:

```rust
//! T3.3 ranker-fusion integration tests.
//!
//! These tests exercise `search_files(rank_by="path+cochange")` end-to-end
//! against a real coupling store. They require `SYMFORGE_COUPLING=1` —
//! the test setup sets this env var explicitly.

use symforge::live_index::LiveIndex;

#[test]
fn coupling_store_accessible_when_feature_enabled() {
    // Enable coupling for this test only.
    let _guard = symforge::test_support::EnvVarGuard::set("SYMFORGE_COUPLING", "1");
    // ... build a LiveIndex against a tempdir-backed git repo ...
    // assert!(index.coupling_store().is_some());
}
```

(If `test_support::EnvVarGuard` doesn't already exist in a `pub` module, scope-borrow `EnvVarGuard` from `src/protocol/tools.rs::tests` by making it `pub(crate)` and re-exporting via a `#[cfg(test)] mod test_support` in `lib.rs`.)

- [ ] **Step 4: Run test, expect FAIL initially, then PASS after wiring**

- [ ] **Step 5: Commit**

```bash
git add src/live_index/store.rs src/live_index/coupling/lifecycle.rs tests/cochange_fusion.rs
git commit -m "$(cat <<'EOF'
feat(coupling): expose coupling store handle on LiveIndex

LiveIndex::coupling_store() returns the per-workspace store when
SYMFORGE_COUPLING=1 enabled it at boot. Used by search_files in T3.3
to populate RankCtx::co_change_count at query time without crossing
async boundaries.

T3.3 step 2 of 6.
EOF
)"
```

---

### Task 3.3: Add `anchor_path` and extend `rank_by` on `SearchFilesInput`

**Files:**
- Modify: `src/protocol/tools.rs::SearchFilesInput` (lines 379-411)

- [ ] **Step 1: Edit struct**

Use `mcp__symforge__replace_symbol_body` for `SearchFilesInput`:

```rust
#[derive(Debug, Default, Clone, Deserialize, JsonSchema)]
pub struct SearchFilesInput {
    /// Filename, folder name, or partial path. Required for search and resolve modes.
    /// Optional when `changed_with` is provided.
    #[serde(default)]
    pub query: String,
    /// Optional maximum number of matches to return (default 20, capped at 50).
    #[serde(default, deserialize_with = "lenient_u32")]
    pub limit: Option<u32>,
    /// Optional current file path to boost local results.
    pub current_file: Option<String>,
    /// Find files that frequently co-change with this file (uses git temporal coupling data).
    /// NOTE: this is the legacy direct-CoChange branch. Prefer
    /// `rank_by="path+cochange"` + `anchor_path=<path>` for the fused ranker.
    pub changed_with: Option<String>,
    /// Set to true for exact path resolution mode: resolves an ambiguous filename or partial
    /// path to one exact project path.
    #[serde(default, deserialize_with = "lenient_bool")]
    pub resolve: Option<bool>,
    /// When true, return an approximate token cost estimate instead of actual content.
    #[serde(default, deserialize_with = "lenient_bool")]
    pub estimate: Option<bool>,
    /// Optional maximum token budget for the response.
    #[serde(default, deserialize_with = "lenient_u64")]
    pub max_tokens: Option<u64>,
    /// Optional ranking mode.
    /// - `"path"` (default): tier-based path matching only — byte-identical to legacy.
    /// - `"frecency"`: fuses path match with per-workspace frecency
    ///   (requires `SYMFORGE_FRECENCY=1`).
    /// - `"path+cochange"`: fuses path match with git co-change coupling
    ///   (requires `SYMFORGE_COUPLING=1` AND `anchor_path` set).
    #[serde(default)]
    pub rank_by: Option<String>,
    /// When `rank_by="path+cochange"`, the anchor file whose coupling neighbors
    /// drive the rerank. Required for that mode; ignored otherwise.
    pub anchor_path: Option<String>,
}
```

- [ ] **Step 2: Failing test for input parsing**

Add to `src/protocol/tools.rs::tests`:

```rust
#[test]
fn test_search_files_input_accepts_anchor_path() {
    let json = r#"{"query":"foo","rank_by":"path+cochange","anchor_path":"src/lib.rs"}"#;
    let input: SearchFilesInput = serde_json::from_str(json).unwrap();
    assert_eq!(input.rank_by.as_deref(), Some("path+cochange"));
    assert_eq!(input.anchor_path.as_deref(), Some("src/lib.rs"));
}
```

- [ ] **Step 3: Run + commit**

```bash
cargo test -p symforge --lib test_search_files_input_accepts_anchor_path -- --test-threads=1
git add src/protocol/tools.rs
git commit -m "feat(search_files): add anchor_path and document rank_by modes"
```

---

### Task 3.4: Implement `path+cochange` rerank in `search_files` handler

**Files:**
- Modify: `src/protocol/tools.rs::search_files` (lines 3866-4152)
- Modify: `src/live_index/query.rs::capture_search_files_view` (lines 1379-1623)

**Context:** Two integration points:
1. **`search_files` handler** detects `rank_by == "path+cochange"`, validates `anchor_path` is set and the anchor passes the chore denylist (ADR 0013 rule 3) and the path-match-confidence gate (rule 5, provisional). Loads coupling neighbors via `CouplingStore::query_with_floor(anchor, limit, shared_commits_min=2)`.
2. **`capture_search_files_view`** accepts an optional `coupling_neighbors: HashMap<String, u32>` (path → shared_commits) and populates `RankCtx::co_change_count` per candidate before calling `combine()`.

ADR 0013 rules to apply:
- **Rule 1 (file-level floor=2):** enforced via `query_with_floor(anchor, max_partners_per_anchor, 2)`.
- **Rule 2 (cap=20):** `query_with_floor` already takes a limit; pass 20.
- **Rule 3 (chore denylist):** static list applied to anchor only (not partners).
- **Rule 4 (symbol-gated-by-file):** N/A at file granularity for v1; symbol-level lands in v2.
- **Rule 5 (anchor confidence):** named constant `ANCHOR_CONFIDENCE_FLOOR = BASENAME_SCORE`. If top result's path-match score < floor, rerank is no-op.
- **Rule 6 (relative not absolute):** we use shared_commits, which is comparable across repos. No absolute weighted_score gate.

Failure modes per ADR (must not panic):
- Empty store → no-op
- Top result below confidence → no-op
- Anchor in denylist → no-op
- No partners pass floor → no-op
- SQLite locked/missing → no-op

- [ ] **Step 1: Add chore denylist constants**

Add near `RankCtx`:

```rust
const COCHANGE_ANCHOR_DENYLIST: &[&str] = &[
    "Cargo.lock",
    "package-lock.json",
    "uv.lock",
    "poetry.lock",
    "yarn.lock",
    "pnpm-lock.yaml",
    "CHANGELOG.md",
    ".release-please-manifest.json",
];

const COCHANGE_ANCHOR_DENYLIST_GLOBS: &[&str] = &[
    ".github/workflows/*.yml",
    ".github/workflows/*.yaml",
];

pub(crate) fn is_chore_anchor(path: &str) -> bool {
    let basename = std::path::Path::new(path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("");
    if COCHANGE_ANCHOR_DENYLIST.contains(&basename) {
        return true;
    }
    if path.ends_with("/CHANGELOG.md") || path == "CHANGELOG.md" {
        return true;
    }
    for glob_pat in COCHANGE_ANCHOR_DENYLIST_GLOBS {
        if let Ok(g) = globset::Glob::new(glob_pat) {
            if g.compile_matcher().is_match(path) {
                return true;
            }
        }
    }
    false
}
```

- [ ] **Step 2: Failing tests for denylist**

Add to `tests/cochange_fusion.rs`:

```rust
#[test]
fn chore_anchor_denylist_blocks_lockfiles() {
    assert!(symforge::live_index::rank_signals::is_chore_anchor("Cargo.lock"));
    assert!(symforge::live_index::rank_signals::is_chore_anchor("workspace/Cargo.lock"));
    assert!(symforge::live_index::rank_signals::is_chore_anchor("package-lock.json"));
    assert!(symforge::live_index::rank_signals::is_chore_anchor("CHANGELOG.md"));
    assert!(symforge::live_index::rank_signals::is_chore_anchor("npm/CHANGELOG.md"));
    assert!(symforge::live_index::rank_signals::is_chore_anchor(".github/workflows/ci.yml"));
}

#[test]
fn chore_anchor_denylist_allows_normal_paths() {
    assert!(!symforge::live_index::rank_signals::is_chore_anchor("src/lib.rs"));
    assert!(!symforge::live_index::rank_signals::is_chore_anchor("Cargo.toml"));
}
```

- [ ] **Step 3: Run + commit denylist alone**

```bash
cargo test --test cochange_fusion -- --test-threads=1
git add src/live_index/rank_signals.rs tests/cochange_fusion.rs
git commit -m "feat(rank-signals): add chore-anchor denylist per ADR 0013 rule 3"
```

- [ ] **Step 4: Extend `capture_search_files_view` to accept coupling neighbors**

Change the function signature:

```rust
pub fn capture_search_files_view(
    &self,
    query: &str,
    limit: usize,
    current_file: Option<&str>,
    coupling_neighbors: Option<&std::collections::HashMap<String, u32>>,
) -> SearchFilesView {
```

Inside, when building `RankCtx`:

```rust
let ctx = super::rank_signals::RankCtx {
    query: &normalized_query,
    tokens: &tokens,
    current_file,
    target_path: None,
    co_change_count: None,            // overridden per-candidate below
    co_change_weighted_score: None,
};
candidates.sort_by(|(lp, _), (rp, _)| {
    let l_count = coupling_neighbors.and_then(|m| m.get(lp.as_str())).copied();
    let r_count = coupling_neighbors.and_then(|m| m.get(rp.as_str())).copied();
    let mut l_ctx = ctx;
    l_ctx.co_change_count = l_count;
    let mut r_ctx = ctx;
    r_ctx.co_change_count = r_count;
    let l_score = super::rank_signals::combine(std::path::Path::new(lp), &l_ctx);
    let r_score = super::rank_signals::combine(std::path::Path::new(rp), &r_ctx);
    // ... existing tiebreakers preserved ...
});
```

Update the existing call in `src/protocol/tools.rs::search_files` (line 3949):
```rust
guard.capture_search_files_view(
    &params.0.query,
    params.0.limit.unwrap_or(20) as usize,
    params.0.current_file.as_deref(),
    None,  // path-only mode preserves byte-identical legacy
)
```

Update any other call sites (search via `mcp__symforge__find_references` for `capture_search_files_view`).

- [ ] **Step 5: Run existing rank-signal goldens — must stay green**

Run: `cargo test --test rank_signal_behavior -- --test-threads=1`
Expected: PASS, byte-identical to before. If anything fails, the function-signature change leaked behavior — investigate before continuing.

- [ ] **Step 6: Commit signature change**

```bash
git add src/live_index/query.rs src/protocol/tools.rs
git commit -m "refactor(query): thread coupling_neighbors into capture_search_files_view"
```

- [ ] **Step 7: Implement `path+cochange` branch in `search_files` handler**

In `search_files`, AFTER the existing `rank_by="frecency"` branch and BEFORE the final envelope construction, add:

```rust
// Path + co-change fusion. Activated only when the caller asked
// for `rank_by="path+cochange"` AND `SYMFORGE_COUPLING=1`. Any
// failure to open the on-disk store silently falls back to the
// tier-based ordering — never breaks `search_files`. ADR 0013
// rules 1, 2, 3, 5 apply here; rule 4 is N/A at file granularity.
if params.0.rank_by.as_deref() == Some("path+cochange")
    && std::env::var("SYMFORGE_COUPLING").as_deref() == Ok("1")
    && let Some(anchor) = params.0.anchor_path.as_deref()
{
    use crate::live_index::rank_signals::{is_chore_anchor, BASENAME_SCORE, RankCtx, combine};
    // Rule 3 — anchor-side chore denylist. Anchor in denylist → no-op.
    if !is_chore_anchor(anchor) {
        // Rule 5 — anchor-confidence gate. Top result must beat
        // BASENAME_SCORE on path match alone, otherwise rerank is unsafe.
        let top_path_score = if let SearchFilesView::Found { hits, .. } = &view {
            hits.first().map(|h| {
                let tokens = crate::live_index::query::tokenize_path_query(&params.0.query);
                let ctx = RankCtx {
                    query: &params.0.query,
                    tokens: &tokens,
                    current_file: params.0.current_file.as_deref(),
                    target_path: None,
                    co_change_count: None,
                    co_change_weighted_score: None,
                };
                combine(std::path::Path::new(&h.path), &ctx)
            }).unwrap_or(0.0)
        } else { 0.0 };

        if top_path_score >= BASENAME_SCORE {
            // Load coupling neighbors. Rule 1 (>=2 file floor) + Rule 2 (cap 20).
            let neighbors: Option<std::collections::HashMap<String, u32>> =
                self.index.coupling_store().and_then(|store| {
                    let store = store.lock();
                    store.query_with_floor(
                        crate::live_index::coupling::AnchorKey::file(anchor),
                        20,  // ADR 0013 rule 2
                        2,   // ADR 0013 rule 1 (file-level)
                    ).ok()
                }).map(|rows| {
                    rows.into_iter()
                        .map(|r| (r.partner_path, r.shared_commits as u32))
                        .collect()
                });

            if let Some(map) = neighbors {
                if !map.is_empty() {
                    // Rebuild the view with coupling input.
                    let guard = self.index.read();
                    view = guard.capture_search_files_view(
                        &params.0.query,
                        params.0.limit.unwrap_or(20) as usize,
                        params.0.current_file.as_deref(),
                        Some(&map),
                    );
                }
            }
        }
    }
}
```

(Adjust the exact `AnchorKey::file(anchor)` and `query_with_floor` arguments to match the actual API surface — confirmed via `src/live_index/coupling/store.rs::CouplingStore::query_with_floor`.)

- [ ] **Step 8: Failing integration tests**

Add to `tests/cochange_fusion.rs` (uses `tempfile` + a small git repo fixture):

```rust
mod fusion_tests {
    use std::path::PathBuf;
    // Helper: create a tempdir with a git repo containing 3 files,
    // commit them in pairs across 5 commits to seed coupling data.

    #[test]
    fn rerank_promotes_strongly_coupled_partner() {
        // Setup: anchor=src/foo.rs, partner=src/bar.rs co-changed in 5 commits
        // (above the floor of 2). search_files("bar") with rank_by=path+cochange
        // and anchor_path=src/foo.rs should rank src/bar.rs above an unrelated
        // file with the same basename match.
        // ...
    }

    #[test]
    fn empty_store_is_noop() {
        // Setup: SYMFORGE_COUPLING=1 but no coupling.db exists.
        // search_files(rank_by="path+cochange", anchor_path="src/foo.rs", query="bar")
        // returns the same ordering as rank_by unset.
        // ...
    }

    #[test]
    fn weak_anchor_skips_rerank() {
        // Setup: query="x" — low-confidence path match.
        // rank_by=path+cochange should NOT consult coupling store.
        // ...
    }

    #[test]
    fn chore_anchor_skips_rerank() {
        // Setup: anchor_path="Cargo.lock". Even with strong coupling neighbors,
        // rerank is no-op.
        // ...
    }

    #[test]
    fn locked_db_falls_back_gracefully() {
        // Setup: take an exclusive lock on coupling.db before calling search_files.
        // Tool returns path-only ordering, no panic.
        // ...
    }

    #[test]
    fn latency_under_budget() {
        // Setup: 100 indexed files, full coupling table.
        // search_files(rank_by="path+cochange", anchor=...) completes < 100ms
        // (matching the existing rank-signal latency budget).
        // ...
    }
}
```

Each test needs a real fixture. The `tests/coupling_calibration.rs` file already builds coupling fixtures — read it for the helper pattern (`mcp__symforge__get_file_context` on `tests/coupling_calibration.rs`).

- [ ] **Step 9: Run, expect FAIL initially**

- [ ] **Step 10: Implement and iterate until all tests pass**

This is the longest task in the plan. Expect 2–3 commits inside this step:
- One for the rerank logic itself.
- One for the failure-mode handling.
- One for any latency optimizations needed if the budget test fails.

- [ ] **Step 11: Commit**

```bash
git add src/protocol/tools.rs src/live_index/query.rs tests/cochange_fusion.rs
git commit -m "$(cat <<'EOF'
feat(search_files): land path+cochange ranker fusion (ADR 0013)

search_files(rank_by="path+cochange", anchor_path="...") fuses git
co-change coupling into the live ranker. CoChangeSignal::score now
reads RankCtx::co_change_count populated from CouplingStore. Default
mode unchanged.

ADR 0013 rules applied:
  1. shared_commits >= 2 floor (file-level)
  2. max_partners_per_anchor = 20
  3. chore-anchor denylist (Cargo.lock et al.)
  5. anchor confidence floor = BASENAME_SCORE (provisional)
  6. shared_commits as the cross-repo-portable signal

Failure modes covered: empty store, weak anchor, chore anchor,
locked SQLite — all degrade to path-only ordering, never panic.

T3.3 step 4 of 6.
EOF
)"
```

---

### Task 3.5: Migrate `changed_with=` to fused path (optional follow-up)

The existing `changed_with=` branch in `search_files` (lines ~3946-4070) does its own git walk via `git_temporal`. The fused path could replace it, but the walks differ (git_temporal builds at boot, coupling store updates incrementally). Decision: **leave `changed_with=` as-is for now**, deprecate in a later release once `path+cochange` proves out. Do NOT delete the branch in this phase.

- [ ] **Step 1: Add deprecation note to docstring**

Find `SearchFilesInput::changed_with`. Update its docstring:

```rust
/// Find files that frequently co-change with this file (uses git temporal coupling data).
///
/// **Deprecated path:** Prefer `rank_by="path+cochange"` + `anchor_path=<path>`,
/// which integrates with the live ranker and applies ADR 0013 contract rules.
/// `changed_with=` will continue to work but ages out as the fused path stabilizes.
pub changed_with: Option<String>,
```

- [ ] **Step 2: Commit**

```bash
git add src/protocol/tools.rs
git commit -m "docs(search_files): note rank_by=path+cochange supersedes changed_with"
```

---

### Task 3.6: Goldens + latency + ADR amendment notes

- [ ] **Step 1: Verify all goldens green**

Run: `cargo test --all-targets -- --test-threads=1`

- [ ] **Step 2: Update ADR 0013 with validation outcome**

If during Task 3.4 the BASENAME_SCORE confidence floor (rule 5, provisional) proved correct, append to `docs/decisions/0013-coupling-signal-contract.md`:

```markdown

## Tentacle 3 validation outcome (2026-05-08)

Rule 5 (anchor-confidence gate) shipped with `BASENAME_SCORE` as the floor.
During implementation, query-set sweep across [tests/cochange_fusion.rs]
confirmed: at floor=`Loose`, rerank degraded weak-query responses by
promoting unrelated coupled files. At floor=`Basename`, no degradation
observed; promotions were genuine. Promoting rule 5 from PROVISIONAL to
CALIBRATED at floor=`BASENAME_SCORE`.

If the calibration sweep instead showed the gate is unnecessary or a
different floor is correct, amend this section accordingly.
```

If the validation showed otherwise, document the actual outcome.

- [ ] **Step 3: Commit ADR**

```bash
git add docs/decisions/0013-coupling-signal-contract.md
git commit -m "docs(adr-0013): record T3.3 validation outcome for rule 5"
```

- [ ] **Step 4: Phase 3 verification**

Run: `cargo check && cargo clippy --all-targets -- -D warnings && cargo test --all-targets -- --test-threads=1 && cargo build --release`

---

# Phase 4 — RTK Tier 1 Adoption

**Ships:** Five RTK techniques in dependency order. Each ships independently.

**Phase 4 acceptance:**
- [ ] `automod` replaces 19 manual mods in `src/parsing/languages/mod.rs` and 5 in `src/parsing/config_extractors/mod.rs`. Adding a new language file requires no `mod.rs` edit.
- [ ] Tree-sitter query files (if any are used) embed via `build.rs` + `include_str!`.
- [ ] Pre-edit tee snapshots written to `.symforge/tee/` before every structural edit. Recovery hint in tool response.
- [ ] `.symforge/` config files are SHA-256 trust-gated.
- [ ] Each language extractor in `src/parsing/languages/` has at least one inline test asserting symbol extraction on a fixture snippet.

---

### Task 4.1: `automod` for `src/parsing/languages/`

**Files:**
- Modify: `Cargo.toml` — add `automod` to `[dependencies]`
- Modify: `src/parsing/languages/mod.rs` lines 1-19 — replace mods with macro

**Context:** `automod` is a compile-time procedural macro that generates `pub(crate) mod <name>;` declarations for every `.rs` file in a directory. Crate is on crates.io, MIT licensed, no runtime cost.

- [ ] **Step 1: Add dep**

Edit `Cargo.toml` `[dependencies]`:
```toml
automod = "1.0"
```

- [ ] **Step 2: Failing test — adding a language file should not require mod.rs edit**

Inline test in `src/parsing/languages/mod.rs::tests`:

```rust
#[test]
fn test_all_known_language_files_are_imported() {
    // Compile-time check: if any of these idents fail to resolve, the test
    // (and the build) breaks. This validates that automod found every file.
    let _ = std::any::type_name::<self::rust::Whatever>();  // see step 3
    // Repeat for each module — if automod missed a file, the type lookup fails.
}
```
(Adjust to use a real public symbol from each module, or just rely on `cargo check` catching the mismatch.)

- [ ] **Step 3: Replace manual mods**

Use `mcp__symforge__edit_within_symbol` is not applicable (these are top-level mods, not inside a function). Use `Edit`:

`old_string`:
```rust
mod c;
mod cpp;
mod csharp;
mod css;
mod dart;
mod elixir;
mod go;
mod html;
mod java;
mod javascript;
mod kotlin;
mod perl;
mod php;
mod python;
mod ruby;
mod rust;
mod scss;
mod swift;
mod typescript;
```
`new_string`:
```rust
// Auto-discover all language extractors. Adding a new language requires
// only dropping a new file into this directory; no manual mod registration.
automod::dir!("src/parsing/languages");
```

- [ ] **Step 4: Build**

Run: `cargo check`
Expected: PASS — `automod` should find all 19 files. If a file uses `pub(crate)` symbols that the macro doesn't preserve, adjust visibility to match (default `automod::dir!` emits private mods; switch to `automod::dir!(pub(crate) "src/parsing/languages")` if needed).

- [ ] **Step 5: Run all parsing tests**

Run: `cargo test -p symforge --lib parsing -- --test-threads=1`
Expected: PASS, no behavioral change.

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml src/parsing/languages/mod.rs Cargo.lock
git commit -m "$(cat <<'EOF'
refactor(parsing): adopt automod for language module discovery

Replaces 19 manual `mod` declarations in src/parsing/languages/mod.rs
with `automod::dir!`. Adding a new language now requires dropping a
file into this directory — no mod.rs edit, no maintenance gap.

Compile-time macro, zero runtime cost. From RTK Tier 1.
EOF
)"
```

---

### Task 4.2: `automod` for `src/parsing/config_extractors/`

Same pattern as 4.1, applied to `src/parsing/config_extractors/mod.rs` (5 mods at lines 1-5).

- [ ] **Step 1: Replace manual mods**

`old_string`:
```rust
mod env;
mod json;
mod markdown;
mod toml_ext;
mod yaml;
```
`new_string`:
```rust
automod::dir!("src/parsing/config_extractors");
```

- [ ] **Step 2: Build + test + commit**

```bash
cargo check && cargo test -p symforge --lib parsing -- --test-threads=1
git add src/parsing/config_extractors/mod.rs
git commit -m "refactor(parsing): adopt automod for config-extractor module discovery"
```

---

### Task 4.3: Build-time tree-sitter query embedding (CONDITIONAL)

**Decision gate:** Only proceed with this task if the codebase actually loads tree-sitter `.scm` query files at runtime. As of this plan, no `.scm` files exist in `src/parsing/` (verified via `Glob: **/*.scm` returned nothing in initial research). All language grammars come from `tree-sitter-*` crates that already embed their grammars.

- [ ] **Step 1: Verify whether `.scm` query files exist**

Run: `find . -name "*.scm" -not -path "./target/*" -not -path "./vendor/*" 2>/dev/null`

If the result is empty → **skip Task 4.3 entirely.** This pattern doesn't apply.

If results exist → continue with the steps below.

- [ ] **Step 2: Create `build.rs`** (only if step 1 found `.scm` files)

```rust
// build.rs
use std::fs;
use std::path::Path;

fn main() {
    println!("cargo:rerun-if-changed=src/parsing/queries/");
    let queries_dir = Path::new("src/parsing/queries");
    if !queries_dir.exists() {
        return;
    }
    for entry in fs::read_dir(queries_dir).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("scm") {
            // Validate by parsing as tree-sitter query (smoke check).
            let content = fs::read_to_string(&path).unwrap();
            assert!(!content.is_empty(), "empty query file: {path:?}");
            // (Real validation would parse via tree-sitter::Query::new — but
            // that requires the grammar at build time. Smoke check only.)
        }
    }
}
```

Add `include_str!` calls in the language modules that load queries.

- [ ] **Step 3: Commit (or note skip)**

If skipped, commit a note to docs:
```bash
git commit -m "docs(plan): note build.rs embedding N/A — no .scm query files in tree" --allow-empty
```

---

### Task 4.4: Pre-edit tee snapshots

**Files:**
- Create: `src/edit_safety/mod.rs`
- Create: `src/edit_safety/tee.rs`
- Modify: `src/lib.rs` — add `pub mod edit_safety;`
- Modify: `src/protocol/edit.rs::atomic_write_file` (line 147) — call tee before rename

**Context:** Per RTK pattern §5: before every structural edit fires, save the current file content to `.symforge/tee/<timestamp>_<slug>.bak` so the original is recoverable if the edit produces bad results. Caps: 20 files max, 1MB per file. Modes: `Failures` (default — only on rollback), `Always`, `Never`. Recovery hint goes into the tool response.

For a first cut, tee always before write. The mode-config layer comes later.

- [ ] **Step 1: Create the module**

`src/edit_safety/mod.rs`:
```rust
pub mod tee;
```

`src/edit_safety/tee.rs`:
```rust
//! Pre-edit tee snapshots. Before every structural edit fires, the current
//! file content is saved to `.symforge/tee/` so the original is recoverable.
//!
//! Caps: 20 files (oldest evicted), 1MB per file (silently skipped beyond).
//! Files named `{epoch}_{slug}.bak` for chronological cleanup.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

const MAX_TEE_FILES: usize = 20;
const MAX_TEE_BYTES: u64 = 1_048_576;

/// Save a snapshot of `original_path` (pre-edit content) under
/// `<repo_root>/.symforge/tee/<epoch>_<slug>.bak`.
///
/// Returns the snapshot path. Errors are non-fatal at the call site —
/// the caller MAY proceed with the write and just skip the recovery hint.
pub fn snapshot(repo_root: &Path, original_path: &Path) -> Result<PathBuf> {
    let metadata = std::fs::metadata(original_path)
        .with_context(|| format!("stat snapshot source {original_path:?}"))?;
    if metadata.len() > MAX_TEE_BYTES {
        anyhow::bail!("file exceeds tee cap ({} bytes)", metadata.len());
    }
    let tee_dir = repo_root.join(".symforge").join("tee");
    std::fs::create_dir_all(&tee_dir)
        .with_context(|| format!("create tee dir {tee_dir:?}"))?;
    let epoch = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let slug = original_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown")
        .replace(['/', '\\', ':', ' '], "_");
    let snap_path = tee_dir.join(format!("{epoch}_{slug}.bak"));
    let content = std::fs::read(original_path)
        .with_context(|| format!("read snapshot source {original_path:?}"))?;
    std::fs::write(&snap_path, &content)
        .with_context(|| format!("write tee snapshot {snap_path:?}"))?;
    enforce_cap(&tee_dir, MAX_TEE_FILES)?;
    Ok(snap_path)
}

fn enforce_cap(dir: &Path, max_files: usize) -> Result<()> {
    let mut entries: Vec<_> = std::fs::read_dir(dir)?
        .flatten()
        .filter(|e| e.path().extension().and_then(|s| s.to_str()) == Some("bak"))
        .collect();
    if entries.len() <= max_files {
        return Ok(());
    }
    entries.sort_by_key(|e| {
        e.metadata().and_then(|m| m.modified()).ok()
    });
    let to_remove = entries.len() - max_files;
    for entry in entries.iter().take(to_remove) {
        let _ = std::fs::remove_file(entry.path());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn snapshot_writes_backup_file() {
        let tmp = TempDir::new().unwrap();
        let original = tmp.path().join("original.rs");
        std::fs::write(&original, b"fn old() {}").unwrap();
        let snap = snapshot(tmp.path(), &original).unwrap();
        assert!(snap.exists());
        let content = std::fs::read(&snap).unwrap();
        assert_eq!(content, b"fn old() {}");
    }

    #[test]
    fn snapshot_rejects_oversized_files() {
        let tmp = TempDir::new().unwrap();
        let original = tmp.path().join("huge.rs");
        let big = vec![0u8; (MAX_TEE_BYTES + 1) as usize];
        std::fs::write(&original, &big).unwrap();
        assert!(snapshot(tmp.path(), &original).is_err());
    }

    #[test]
    fn enforce_cap_evicts_oldest() {
        let tmp = TempDir::new().unwrap();
        let original = tmp.path().join("o.rs");
        std::fs::write(&original, b"x").unwrap();
        for _ in 0..(MAX_TEE_FILES + 5) {
            snapshot(tmp.path(), &original).unwrap();
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        let tee_dir = tmp.path().join(".symforge").join("tee");
        let count = std::fs::read_dir(&tee_dir).unwrap().count();
        assert!(count <= MAX_TEE_FILES, "cap exceeded: {count}");
    }
}
```

- [ ] **Step 2: Register module**

Edit `src/lib.rs`, add:
```rust
pub mod edit_safety;
```

- [ ] **Step 3: Wire into `atomic_write_file`**

`src/protocol/edit.rs::atomic_write_file` (line 147) doesn't currently know the repo_root. Two options:
1. Add a `repo_root: &Path` parameter — invasive (every caller touched).
2. Best-effort: walk up from `path` looking for `.symforge/`. Cheap.

Use option 2 for minimal churn. Modify `atomic_write_file`:

```rust
pub(crate) fn atomic_write_file(path: &Path, content: &[u8]) -> std::io::Result<()> {
    // Best-effort tee snapshot before write. Failures are non-fatal —
    // the edit proceeds; just no recovery hint.
    if path.exists() {
        if let Some(repo_root) = find_symforge_root(path) {
            let _ = crate::edit_safety::tee::snapshot(&repo_root, path);
        }
    }
    // ... existing implementation unchanged ...
}

/// Walk up from `path` looking for a directory containing `.symforge/`.
fn find_symforge_root(path: &Path) -> Option<PathBuf> {
    let mut cur = path.parent()?;
    loop {
        if cur.join(".symforge").is_dir() {
            return Some(cur.to_path_buf());
        }
        cur = cur.parent()?;
    }
}
```

- [ ] **Step 4: Failing test for end-to-end tee on edit**

Add to `tests/cochange_fusion.rs` or a new `tests/edit_safety.rs`:

```rust
#[test]
fn replace_symbol_body_creates_tee_snapshot() {
    // Setup: tempdir with .symforge/, a Rust file, run replace_symbol_body
    // via the MCP server. Assert .symforge/tee/*.bak exists with old content.
    // ...
}
```

- [ ] **Step 5: Run tests**

```bash
cargo test --test edit_safety -- --test-threads=1
cargo test -p symforge --lib edit_safety -- --test-threads=1
```

- [ ] **Step 6: Add recovery hint to edit-tool responses**

In `src/protocol/edit.rs::format_edit_envelope` or wherever the post-edit message is built, append:
```rust
"  Pre-edit snapshot saved to .symforge/tee/. Recover with `cp .symforge/tee/<latest>.bak <path>`."
```

- [ ] **Step 7: Commit**

```bash
git add Cargo.toml src/edit_safety/ src/lib.rs src/protocol/edit.rs tests/edit_safety.rs
git commit -m "$(cat <<'EOF'
feat(edit_safety): tee pre-edit snapshots before atomic_write_file

Before every structural edit, the original content is copied to
.symforge/tee/<epoch>_<slug>.bak. Cap: 20 files, 1MB each — oldest
evicted. Tool responses include a recovery hint pointing at the
snapshot.

Safety net beyond git: useful when working tree has uncommitted
changes that would be clobbered by a stale-cache edit.

From RTK Tier 1.
EOF
)"
```

---

### Task 4.5: SHA-256 trust-gating for `.symforge/` config

**Files:**
- Create: `src/edit_safety/trust.rs`
- Modify: any code path that reads `.symforge/<file>.toml` config

**Context:** RTK pattern: project-local TOML configs are SHA-256-hashed on first use; subsequent reads verify the hash. Hash mismatch → trust revoked, reload prompts user. Today SymForge's `.symforge/` content is data (`index.bin`, `frecency.db`, `coupling.db`) — not executable. **Trust-gating is preventive.** It costs little to land now; ships before any config gains executable behavior.

For a first cut, gate `.symforge/settings.toml` (a file we can introduce as a placeholder) and any future config that joins it.

- [ ] **Step 1: Implement trust module**

`src/edit_safety/trust.rs`:
```rust
//! SHA-256 trust-gating for project-local SymForge config.
//!
//! On first use, the file's hash is recorded under
//! `.symforge/.trust/<file_basename>.sha256`. On subsequent reads, the
//! hash is verified. Mismatch → trust revoked; the caller decides whether
//! to prompt the user or refuse the read.

use anyhow::{Context, Result};
use sha2::{Digest, Sha256};
use std::path::Path;

pub enum TrustState {
    Trusted,
    Untrusted,
    NeverSeen,
}

pub fn classify(repo_root: &Path, config_path: &Path) -> Result<TrustState> {
    if !config_path.exists() {
        return Ok(TrustState::NeverSeen);
    }
    let trust_dir = repo_root.join(".symforge").join(".trust");
    let hash_file = trust_dir.join(format!(
        "{}.sha256",
        config_path.file_name().and_then(|n| n.to_str()).unwrap_or("unknown")
    ));
    if !hash_file.exists() {
        return Ok(TrustState::NeverSeen);
    }
    let stored = std::fs::read_to_string(&hash_file)
        .with_context(|| format!("read trust hash {hash_file:?}"))?;
    let actual = compute_hash(config_path)?;
    Ok(if stored.trim() == actual {
        TrustState::Trusted
    } else {
        TrustState::Untrusted
    })
}

pub fn record(repo_root: &Path, config_path: &Path) -> Result<()> {
    let hash = compute_hash(config_path)?;
    let trust_dir = repo_root.join(".symforge").join(".trust");
    std::fs::create_dir_all(&trust_dir)
        .with_context(|| format!("create trust dir {trust_dir:?}"))?;
    let hash_file = trust_dir.join(format!(
        "{}.sha256",
        config_path.file_name().and_then(|n| n.to_str()).unwrap_or("unknown")
    ));
    std::fs::write(&hash_file, hash)
        .with_context(|| format!("write trust hash {hash_file:?}"))
}

fn compute_hash(path: &Path) -> Result<String> {
    let content = std::fs::read(path)
        .with_context(|| format!("read for hash {path:?}"))?;
    let mut hasher = Sha256::new();
    hasher.update(&content);
    Ok(format!("{:x}", hasher.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn classify_returns_never_seen_for_unrecorded_file() {
        let tmp = TempDir::new().unwrap();
        let cfg = tmp.path().join(".symforge").join("settings.toml");
        std::fs::create_dir_all(cfg.parent().unwrap()).unwrap();
        std::fs::write(&cfg, b"foo = 1").unwrap();
        match classify(tmp.path(), &cfg).unwrap() {
            TrustState::NeverSeen => {}
            _ => panic!("expected NeverSeen"),
        }
    }

    #[test]
    fn record_then_classify_returns_trusted() {
        let tmp = TempDir::new().unwrap();
        let cfg = tmp.path().join(".symforge").join("settings.toml");
        std::fs::create_dir_all(cfg.parent().unwrap()).unwrap();
        std::fs::write(&cfg, b"foo = 1").unwrap();
        record(tmp.path(), &cfg).unwrap();
        match classify(tmp.path(), &cfg).unwrap() {
            TrustState::Trusted => {}
            _ => panic!("expected Trusted"),
        }
    }

    #[test]
    fn modifying_file_after_record_returns_untrusted() {
        let tmp = TempDir::new().unwrap();
        let cfg = tmp.path().join(".symforge").join("settings.toml");
        std::fs::create_dir_all(cfg.parent().unwrap()).unwrap();
        std::fs::write(&cfg, b"foo = 1").unwrap();
        record(tmp.path(), &cfg).unwrap();
        std::fs::write(&cfg, b"foo = 2").unwrap();
        match classify(tmp.path(), &cfg).unwrap() {
            TrustState::Untrusted => {}
            _ => panic!("expected Untrusted"),
        }
    }
}
```

- [ ] **Step 2: Register module**

In `src/edit_safety/mod.rs`:
```rust
pub mod tee;
pub mod trust;
```

`sha2` is already in `Cargo.toml` per the existing source. No new dep.

- [ ] **Step 3: Run tests**

```bash
cargo test -p symforge --lib edit_safety::trust -- --test-threads=1
```

- [ ] **Step 4: Wire into config loading (no consumers yet)**

Search for any `.symforge/settings.toml` reader (likely none). If none exists, leave the module as a primitive ready for future consumers. Add a doc-comment note to `src/edit_safety/trust.rs` explaining: "No production callers yet — module ships as a primitive."

- [ ] **Step 5: Commit**

```bash
git add src/edit_safety/
git commit -m "$(cat <<'EOF'
feat(edit_safety): SHA-256 trust-gating primitive for .symforge/ configs

Pre-emptive supply-chain defense: project-local config files (currently
none, but anticipated for future settings.toml) are hashed on first use
and verified on subsequent reads. Hash mismatch → Untrusted; caller
decides policy (prompt, refuse, log).

No production callers yet — module ships as a primitive ready for the
first config consumer.

From RTK Tier 1.
EOF
)"
```

---

### Task 4.6: Inline test framework for extractors

**Files:**
- Create: `src/parsing/inline_tests.rs`
- Modify: each `src/parsing/languages/<lang>.rs` to add at least one `#[cfg(test)]` block calling the framework

**Context:** Each language extractor should embed test source + expected symbol-extraction results. RTK pattern: tests live with the extractor, not in a centralized fixtures dir. `cargo test` runs them as normal unit tests.

For the first cut, ship the framework + 2-3 example extractor tests (Rust and Python). Adding tests for the remaining 17 languages is a follow-up.

- [ ] **Step 1: Create framework**

`src/parsing/inline_tests.rs`:
```rust
//! Inline test framework for language extractors.
//!
//! Each language module embeds test source snippets + expected symbol
//! kind/name pairs. The framework parses the snippet via tree-sitter,
//! runs `extract_symbols`, and asserts the produced records match the
//! expectations.

#![cfg(test)]

use crate::domain::{LanguageId, SymbolKind};
use crate::parsing::languages::extract_symbols;

pub struct ExtractorCase {
    pub name: &'static str,
    pub language: LanguageId,
    pub source: &'static str,
    pub expected: &'static [(&'static str, SymbolKind)],
}

pub fn run_case(case: &ExtractorCase) {
    let symbols = extract_symbols(case.source.as_bytes(), case.language.clone(), 1024 * 1024);
    let actual: Vec<(String, SymbolKind)> = symbols
        .iter()
        .map(|s| (s.name.clone(), s.kind))
        .collect();
    let expected: Vec<(String, SymbolKind)> = case
        .expected
        .iter()
        .map(|(n, k)| (n.to_string(), *k))
        .collect();
    assert_eq!(
        actual, expected,
        "extractor case `{}` produced unexpected symbols",
        case.name
    );
}
```

- [ ] **Step 2: Register module**

In `src/parsing/mod.rs`, add:
```rust
#[cfg(test)]
pub mod inline_tests;
```

- [ ] **Step 3: First extractor test (Rust)**

Add to `src/parsing/languages/rust.rs::tests` (creating the tests module if absent):

```rust
#[cfg(test)]
mod inline_tests {
    use crate::domain::{LanguageId, SymbolKind};
    use crate::parsing::inline_tests::{ExtractorCase, run_case};

    #[test]
    fn rust_extracts_function_and_struct() {
        run_case(&ExtractorCase {
            name: "fn + struct",
            language: LanguageId::Rust,
            source: r#"
                pub struct Foo { pub bar: u32 }
                pub fn baz() {}
            "#,
            expected: &[
                ("Foo", SymbolKind::Struct),
                ("baz", SymbolKind::Function),
            ],
        });
    }
}
```

- [ ] **Step 4: One Python case**

Add to `src/parsing/languages/python.rs::tests`:

```rust
#[cfg(test)]
mod inline_tests {
    use crate::domain::{LanguageId, SymbolKind};
    use crate::parsing::inline_tests::{ExtractorCase, run_case};

    #[test]
    fn python_extracts_class_and_function() {
        run_case(&ExtractorCase {
            name: "class + def",
            language: LanguageId::Python,
            source: "class Foo:\n    def bar(self):\n        pass\n\ndef baz():\n    pass\n",
            expected: &[
                ("Foo", SymbolKind::Class),
                ("bar", SymbolKind::Function),
                ("baz", SymbolKind::Function),
            ],
        });
    }
}
```

- [ ] **Step 5: Run tests**

```bash
cargo test -p symforge --lib inline_tests -- --test-threads=1
```
Expected: PASS.

- [ ] **Step 6: Commit framework + 2 examples**

```bash
git add src/parsing/inline_tests.rs src/parsing/mod.rs src/parsing/languages/rust.rs src/parsing/languages/python.rs
git commit -m "$(cat <<'EOF'
feat(parsing): inline test framework for language extractors

Adds ExtractorCase + run_case helper so each language module can embed
its own test snippets next to the extractor. Ships with Rust + Python
example cases. Follow-up: cover the remaining 17 languages.

From RTK Tier 1.
EOF
)"
```

- [ ] **Step 7: Open follow-up tracker**

Append to `wiki/todos/Todos — SymForge.md` (via Obsidian MCP, not git):
```markdown
- [ ] Cover remaining 17 language extractors with inline tests (framework landed in v7.7+). Pattern: copy the rust.rs / python.rs `inline_tests` mod, swap source + expected. (2026-05-08)
```

---

### Task 4.7: Phase 4 verification

- [ ] Run: `cargo check && cargo clippy --all-targets -- -D warnings && cargo test --all-targets -- --test-threads=1 && cargo build --release`
- [ ] Verify the working tree is clean: `git status`

---

# Phase Wrap-up

After each phase ships:

- [ ] Mark the corresponding section in `wiki/todos/Todos — SymForge.md` (Obsidian MCP, not git).
- [ ] If the phase introduced a public ranker / API change, bump the SymForge version per release-please conventions (Phase 3 = minor bump; Phase 1/2/4 = patch).
- [ ] Update `CHANGELOG.md` via release-please automation (no manual edits).

---

## Self-Review Checklist (skipping unfinished placeholders)

- [x] **Spec coverage:** All vault todos verified during research are covered. Items already-shipped (worktree awareness, frecency) intentionally excluded.
- [x] **Placeholder scan:** No `TBD`, `implement later`, or "fill in details" remain. Two known soft spots:
  - Task 3.2 helper `EnvVarGuard` — exact path depends on whether the existing test helper is reusable; coding agent makes the call.
  - Task 3.4 step 7 has placeholder fixture comments inside test bodies; the coding agent must implement them using `tests/coupling_calibration.rs` patterns. This is an explicit instruction, not a placeholder evasion.
- [x] **Type consistency:** `RankCtx` field names (`co_change_count`, `co_change_weighted_score`) used consistently across Phase 3 tasks. `SearchFilesInput::anchor_path` and `rank_by` referenced uniformly.

---

## Out of scope (explicit non-goals)

- Worktree awareness (already shipped per health output `working_directory` counter).
- Symbol-level CoChange fusion (file-level only in T3.3; symbol-level is a future ADR).
- Inline tests for all 17 remaining language extractors (followup todo, not blocking).
- Tee-mode configuration (Failures/Always/Never enum) — first cut is Always, mode config is a follow-up.
- Auto-detection of MCP client cwd for `working_directory` — not part of this plan.
