---
title: SymForge Code Review and Planning Packet
type: handoff
status: active
date: 2026-05-16
---

# SymForge Code Review and Planning Packet

Audience: code reviewer or planning agent reviewing latest SymForge `main`.

This packet consolidates the current repo state, what landed locally, what still
needs planning, and which Markdown artifacts should be attached for context.
It exists so review can start from one document instead of reconstructing the
state from Obsidian and scattered planning notes.

## Repo State At Packet Authoring

- Branch: `main`
- Base observed before this packet commit: `origin/main` at `51810d7`
- Local state before this packet commit: clean worktree, `main...origin/main [ahead 6]`
- Local commits ahead of `origin/main` before this packet commit:
  - `c90a757 feat(search-files): fuse cochange ranking`
  - `416d1da docs(search-files): deprecate changed_with`
  - `58ffeab chore(cochange): close out wave 2`
  - `d3dda0e docs(rtk): audit once-lock candidates`
  - `afc41eb docs(rtk): investigate match_output short-circuit`
  - `1a4fa86 refactor(parsing): use automod for language modules`

## Primary Review Requests

1. Review the six commits above plus this packet/roadmap status update.
2. Validate that Wave 2 CoChange ranker fusion is coherent and safe to use as
   the base for later RTK work.
3. Validate that `automod` use in `src/parsing/languages/mod.rs` is acceptable
   and does not hide build or module-order risks.
4. Validate the remaining roadmap sequence and dependency order.
5. Decide whether SQLite analytics (T2.2) remains in scope as a standalone
   observability feature after the `match_output` investigation found no real
   hook.
6. Decide whether to push/release the current local commits now or continue
   accumulating Wave 3 work before release-please opens the next release PR.

## Current Truth Sources

- Main roadmap: `docs/plans/2026-05-15-symforge-post-h-roadmap.md`
- Current Wave 2 close-out evidence: `docs/notes/2026-05-16-w2-close-out-evidence.md`
- RTK OnceLock audit: `docs/notes/2026-05-16-rtk-once-lock-audit.md`
- RTK `match_output` investigation: `docs/notes/2026-05-16-rtk-match-output-investigation.md`
- External evaluator archive: `docs/notes/external-evaluations/2026-05-11/`
- Obsidian backlog intake was distilled into this packet and the roadmap. The
  vault note title is `SymForge Backlog Intake 2026-05-16`, but it is not in
  this git repo.

## Recently Completed Work

Completed and should not be replanned unless review finds a defect:

- Wave 0: Trust restoration hotfixes.
- Wave 1: Index hygiene close-out.
- Wave 2: CoChange ranker fusion.
- Wave 3a.1: OnceLock audit.
- Wave 3a.2: `match_output` investigation.
- Wave 3b.1: `automod` for `src/parsing/languages/`.
- Wave 3e.4: `match_output` short-circuit is N/A as written because no real
  `match_output` symbol exists.

## Immediate Integration Bookkeeping

These were handled in this packet commit:

- Roadmap status was updated for completed Wave 3a.1, 3a.2, 3b.1, and 3e.4.
- Roadmap contradiction was resolved: SQLite analytics (T2.2) is now a
  standalone product decision if retained, not dependent on `match_output`.

Still needed after review:

- If current commits are accepted, push/release handling should be decided.
- If release-please opens a PR, verify the generated version and changelog
  before publishing npm.

## Remaining Work By Dependency Order

### P1 - Current Critical Path: Wave 3

These are the next implementation units for v7.10.0-class work.

1. Unit 3b.2 - Pre-edit tee snapshots module.
   - Dependency: none remaining from completed 3b.1.
   - Importance: high; foundational edit-safety primitive.
2. Unit 3c.1 - `automod` for `src/parsing/config_extractors/`.
   - Dependency: 3b.1 is complete.
   - Importance: medium; mechanical consistency cleanup.
3. Unit 3c.2 - Inline test framework for extractors.
   - Dependency: 3b.1 complete; best after module surface stabilizes.
   - Importance: high for parser regression safety.
4. Unit 3d.1 - Trust-gating for `.symforge/` plus ADR 0015.
   - Dependency: 3b.2 because it uses the edit-safety module.
   - Importance: high, new safety surface; default should be cautious/log-only.
5. Unit 3d.2 - Compression-ratio CI assertion.
   - Dependency: standalone.
   - Importance: medium; protects `get_file_context` value proposition.
6. Unit 3e.1 - Graceful degradation tiers.
   - Dependency: after earlier Wave 3 module/safety work.
   - Importance: high for robust agent behavior on partial indexes.
7. Unit 3e.2 - SQLite analytics.
   - Dependency: product decision required.
   - Importance: optional/medium unless observability is prioritized.
8. Unit 3e.3 - CLI correction learning.
   - Dependency: analytics/failure corpus if T2.2 is retained.
   - Importance: optional/medium.
9. Unit 3f.1 - Wave 3 close-out and release evidence.
   - Dependency: all retained Wave 3 units.

### P2 - Required Later: Wave 4 Stability Polish

Starts after Wave 3 closes. The roadmap commitment is a one-week sprint within
30 days of Wave 3 close.

1. Unit 4.1 - Bump `tree-sitter-rust` to 0.25+ for Rust raw refs.
2. Unit 4.2 - Deepest-error walk for `validate_file_syntax`.
3. Unit 4.3 - Untracked-file search diagnostic.
4. Unit 4.4 - Sidecar PID/alive state in `health`.
5. Unit 4.5 - `NoisePolicy` covers `.obsidian/`.
6. Unit 4.6 - Regression-suite gap audit.
7. Unit 4.7 - `search_text(group_by="usage")` filters docs/comments.
8. Unit 4.8 - Truncation phrasing unification.
9. Unit 4.9 - Structural-pattern cookbook.
10. Unit 4.10 - Wave 4 close-out and patch release.

### P2/P3 - Evaluation Backlog As Acceptance Evidence

These are not separate roadmap forks unless a current repro fails. Use them as
acceptance/regression evidence while implementing Waves 3 and 4:

- Windows `index_folder` self-destruction long-run verification.
- `batch_rename` bounded-time dry-run verification.
- `get_symbol_context` and `get_file_context` hard budget verification.
- `find_dependents` common-method false-positive checks.
- `find_references` fully-qualified Rust path completeness.
- Health snapshot consistency and watcher reason checks.
- Parser diagnostic correctness for `//!` docs and Rust raw refs.
- Untracked-file search behavior.
- `group_by="usage"` semantics.
- Structural-search label correctness.

### P3 - Strategic Product Line: Structural Governance

This should be a future milestone, not interleaved with the Wave 3/4 critical
path unless the owner explicitly reprioritizes it.

1. Structural core graph from LiveIndex.
2. `structural_health`.
3. `structural_baseline`.
4. `structural_delta`.
5. `structural_gate` plus `.symforge/architecture-rules.toml`.
6. `test_gap_risk`.
7. `evolution_risk`.
8. `what_if_structure`.
9. Downstream AAP integration remains out of SymForge scope.

## Attachment List

### Minimal Attachment Set

Attach these if the review system prefers a concise packet:

1. `docs/summaries/2026-05-16-code-review-planning-packet.md`
2. `docs/plans/2026-05-15-symforge-post-h-roadmap.md`
3. `docs/notes/2026-05-16-w2-close-out-evidence.md`
4. `docs/notes/2026-05-16-rtk-once-lock-audit.md`
5. `docs/notes/2026-05-16-rtk-match-output-investigation.md`

### Expanded Evidence Set

Attach these when the reviewer needs full history and evidence:

1. `docs/notes/2026-05-16-w0-close-out-evidence.md`
2. `docs/notes/2026-05-15-c2-close-out-evidence.md`
3. `docs/notes/2026-05-15-c2-final-verification.txt`
4. `docs/plans/2026-05-12-symforge-stability-hotfix.md`
5. `docs/plans/2026-05-08-symforge-improvements-master.md`
6. `docs/research/coupling-calibration-2026-04-18.md`
7. `docs/decisions/0011-frecency-bump-policy.md`
8. `docs/decisions/0012-edit-and-ranker-hook-architecture.md`
9. `docs/decisions/0013-coupling-signal-contract.md`
10. `docs/decisions/0014-watcher-subsystem-spawn-blocking-discipline.md`
11. `docs/notes/external-evaluations/2026-05-11/SYMFORGE_EVALUATION_2026-05-11.md`
12. `docs/notes/external-evaluations/2026-05-11/SYMFORGE_TEST_REPORT_2026-05-11_01.md`
13. `docs/notes/external-evaluations/2026-05-11/SYMFORGE_TEST_REPORT_2026-05-11_02.md`
14. `docs/notes/external-evaluations/2026-05-11/INVESTIGATION_B-P0-1.md`
15. `docs/notes/external-evaluations/2026-05-11/INVESTIGATION_HEALTH_REFS.md`
16. `docs/notes/external-evaluations/2026-05-11/PROFILE_BATCH_RENAME.md`

## Reviewer Operating Notes

- Treat high-level SymForge dependency/context outputs as hypotheses until the
  relevant Wave 4 polish items close; cross-check surprising results with raw
  `rg`.
- For Rust/source changes, expected verification remains:
  `cargo check`, `cargo test --all-targets -- --test-threads=1`, and
  `cargo build --release`.
- For `npm/` changes, expected verification is `cd npm && npm test`.
- For mixed Rust + `npm/` changes, run both paths.
- No source-code verification was rerun solely for this packet commit because
  the packet changes docs/roadmap status only.
