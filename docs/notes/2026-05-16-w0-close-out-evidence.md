---
title: Wave 0 Close-out Evidence
type: verification
date: 2026-05-16
status: released
---

# Wave 0 Close-out Evidence

## Scope

Wave 0 trust-restoration units from `docs/plans/2026-05-15-symforge-post-h-roadmap.md`.

This records the software-side close-out evidence and final release confirmation. The roadmap's original `v7.8.1` target was superseded after `v7.8.1` shipped Unit 0.2; the final Wave 0 close-out release is `v7.8.2`.

## Landed Units

| Unit | Commit | Evidence |
|---|---:|---|
| 0.1 `find_references` method-call false-positive fix | `cfc261f` | `tests/qualified_usages_method_call.rs`; AAP truncate repro verified before this close-out pass |
| 0.2 `edit_plan` / `find_references` symbol-line drift fix | `c8ba267` | `tests/edit_plan_symbol_line.rs`; released and installed as `symforge@7.8.1` |
| 0.3 frecency discovery DB creation fix | `6ee9f6d` | `tests/frecency_ranking.rs::search_files_frecency_rank_does_not_create_db_when_empty`; full frecency file passed |
| 0.4 sidecar Windows port-pool fix | `e77b009` | `SO_REUSEADDR` bind path in `src/sidecar/server.rs`; `shutdown_and_join` across sidecar tests; 10/10 sidecar integration stress runs passed |

## Release Confirmation

Checked from `C:\AI_STUFF\PROGRAMMING\symforge` on 2026-05-16 after `git fetch origin main --tags`.

| Check | Result |
|---|---|
| `git log --oneline --decorate -5 origin/main` | `9ec2765` is `origin/main`, `origin/HEAD`, and tag `v7.8.2` |
| `gh run view 25945780958` | PASS; prepared release PR after `b5186cf` |
| `gh run view 25946082407` | PASS; release merge workflow published cargo, npm, and release assets |
| `gh pr view 234 --json number,state,mergedAt,mergeCommit,title,url` | PR #234 `chore(main): release 7.8.2` MERGED at `2026-05-15T23:22:06Z`, merge commit `9ec2765` |
| `gh release list --limit 5` | `v7.8.2` is Latest, published `2026-05-15T23:32:00Z` |
| `npm view symforge version` | `7.8.2` |
| `npm list -g symforge --depth=0` | global install is `symforge@7.8.2` |
| `symforge --version` | `symforge 7.8.2` |

## Resume Verification

Run from `C:\AI_STUFF\PROGRAMMING\symforge` on 2026-05-16 after local `main` fast-forwarded to release merge `9ec2765` and with this close-out docs patch in the working tree.

| Gate | Result |
|---|---|
| `cargo check` | PASS |
| `cargo test --test qualified_usages_method_call -- --test-threads=1` | PASS, 1 passed |
| `cargo test --test edit_plan_symbol_line -- --test-threads=1` | PASS, 1 passed |
| `cargo test --test frecency_ranking search_files_frecency_rank_does_not_create_db_when_empty -- --test-threads=2` | PASS, 1 passed, 20 filtered out |
| `cargo test sidecar::server::tests::so_reuseaddr_listener_rebinds_on_recently_freed_port -- --test-threads=1` | PASS, 1 passed, 1682 filtered out |
| `cargo test --all-targets -- --test-threads=1` | PASS |
| `cargo build --release` | PASS |
| `cargo clippy -- -D warnings` | PASS |
| `cargo test --all-targets` | PASS |
| `npm test` from `npm/` | PASS, 24 passed, 1 skipped |

## Gate Commands

Run from `C:\AI_STUFF\PROGRAMMING\symforge\.worktrees\w0-frecency-contract` after Unit 0.3 commit `6ee9f6d`.

| Gate | Result |
|---|---|
| `cargo test --test frecency_ranking search_files_frecency_rank_does_not_create_db_when_empty -- --test-threads=2` | PASS |
| `cargo test --test frecency_ranking` | PASS, 21 passed |
| `cargo test --test edit_hook_behavior` | PASS, 24 passed |
| `cargo test sidecar::server::tests::so_reuseaddr_listener_rebinds_on_recently_freed_port -- --test-threads=1` | PASS |
| `for ($i = 1; $i -le 10; $i++) { cargo test --test sidecar_integration -- --test-threads=10 --include-ignored }` with `10048` / address-in-use scan | PASS, 10/10 runs, zero matches |
| `cargo check` | PASS |
| `cargo test --all-targets -- --test-threads=1` | PASS |
| `cargo clippy -- -D warnings` | PASS |
| `cargo test --all-targets` | PASS |
| `cargo build --release` | PASS |
| `git diff --check` before Unit 0.3 commit | PASS |

## Notes

- `cargo fmt --check` was not used as a completion gate because it reports broad pre-existing formatting drift across unrelated files.
- Unit 0.4 required no new patch in this pass; audit found it already satisfied on `main` by `e77b009`.
- Final release follow-up completed as `v7.8.2` via release-please PR #234.
