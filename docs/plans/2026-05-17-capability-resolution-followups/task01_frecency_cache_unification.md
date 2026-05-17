# /goal Capability Resolution Follow-Up Task 01: Frecency Cache Unification (M3)

/goal eliminate the two-handle frecency topology so call-time `rank_by="frecency"` reuses the cached persistent `FrecencyStore` writer when present, observes post-HEAD-reset state consistently, and does not race on cross-handle SQLite busy timeouts.

## Context

- Project: SymForge, a Rust-native MCP for code indexing, retrieval, orchestration, and recovery.
- Working directory: `C:\AI_STUFF\PROGRAMMING\symforge`.
- Current problem: `cached_store_for` (`src/live_index/frecency.rs:477-491`) opens the writeable, HEAD-reset-applying `FrecencyStore` and caches it per workspace. `ranking_scores_for_paths` (`src/live_index/frecency.rs:432-444`) opens an independent read-only handle via `FrecencyStore::open_existing_readonly` for the same DB file even when the cached writer already exists. Two connections share the file, so:
  - Concurrent `bump()` + `rank_by="frecency"` calls under `SYMFORGE_FRECENCY=1` can stall the reader on the 5s `busy_timeout`.
  - Reader can observe pre-reset rows while the cached writer is mid-HEAD-reset, because the read-only handle bypasses the cache mutex that serializes policy application.
- ADR `docs/decisions/0016-call-time-capability-resolution.md` and req `CCR-3` require deterministic `rank_by="frecency"`.
- Severity: Medium. Not a correctness break in single-threaded callers, but observable under MCP concurrency.
- Relevant source material:
  - `src/live_index/frecency.rs`
  - `src/protocol/tools.rs` (the `rank_by="frecency"` branch around `src/protocol/tools.rs:5365-5456`)
  - `docs/decisions/0011-frecency-bump-policy.md`
  - `docs/decisions/0016-call-time-capability-resolution.md`
  - `tests/call_time_frecency.rs`
  - `tests/frecency_ranking.rs`
- Requirements covered: `CCR-1`, `CCR-3`, `CCR-9`, `CCR-10`
- Depends on: prior capability resolution task pack (`docs/plans/2026-05-16-call-time-capability-resolution`).
- Expected files to modify:
  - `src/live_index/frecency.rs`
  - `tests/call_time_frecency.rs` (or a new focused test file)
- Files off limits:
  - `src/protocol/edit.rs`
  - `src/worktree.rs`
  - `src/live_index/coupling/lifecycle.rs`
  - `src/capability/*`

## Machine Metadata

- phase: `3h-capability-resolution-followups`
- plan: `01`
- wave: `1`
- type: `frecency-cache-unification`
- autonomous: `true`
- requirements: `CCR-1`, `CCR-3`, `CCR-9`, `CCR-10`
- must_haves:
  - Call-time frecency ranking reuses the cached persistent store when one exists.
  - Cross-handle race between bump-time HEAD reset and rank-time read is removed.
  - Session-only and unset-flag paths preserve existing footprint-free behavior.

## Success Criteria - All Must Be True

1. When the cached persistent `FrecencyStore` exists for the workspace, `ranking_scores_for_paths` reads through the cached handle, not a new read-only connection.
2. When no cached persistent store exists, the function still consults an existing on-disk DB via the existing read-only fallback so discovery-only sessions stay footprint-free.
3. Session-store handling is unchanged; sessions still create no persistent DB file.
4. A focused test demonstrates that a HEAD-reset applied through the cached writer is immediately visible to a subsequent `rank_by="frecency"` call (no stale pre-reset scores).
5. A focused test exercises concurrent bump + rank under persistent policy and verifies neither path errors and the cached connection is used.
6. `tests/call_time_frecency.rs` continues to pass unchanged.
7. `cargo check`, focused `cargo test --test call_time_frecency --test frecency_ranking -- --test-threads=1`, and shared `cargo test --all-targets -- --test-threads=1` pass.

## Constraints

- Do not change the persistent vs session policy mapping in `collection_policy_from_env`.
- Do not change the `FrecencyStore` public API surface.
- Do not touch coupling, worktree, or ranking-diagnostics code.
- Do not introduce a second cache map; reuse the existing `CACHE`/`session_cache` design.
- Preserve infallibility of `bump()` — read path must still degrade silently rather than panic.

## Implementation Sketch

1. Add a private `cached_persistent_for(repo_root) -> Option<Arc<FrecencyStore>>` helper that returns the existing entry from the persistent `CACHE` without inserting a new one (mirrors `cached_session_store_for`).
2. In `ranking_scores_for_paths`, before calling `FrecencyStore::open_existing_readonly`, consult `cached_persistent_for(repo_root)`. If present, use its `bulk_scores` and label source `"persistent (cached)"` or keep `"persistent"`. Only fall through to `open_existing_readonly` when the cache miss is genuine.
3. Keep the session-store branch unchanged.
4. Add tests in `tests/call_time_frecency.rs`:
   - `cached_writer_post_reset_visible_to_ranking`: with `SYMFORGE_FRECENCY=1`, seed via `bump()`, force HEAD-reset by calling the cached writer's reset path, then call `rank_by="frecency"`; assert returned scores reflect post-reset state.
   - `concurrent_bump_and_rank_under_persistent_policy_completes`: run a few `bump()` and `search_files rank_by="frecency"` calls back-to-back (single-threaded test harness is enough — the goal is to prove no extra handle path is taken when the cache is hot).

## Verification

```powershell
cargo test --test call_time_frecency -- --test-threads=1
cargo test --test frecency_ranking -- --test-threads=1
cargo check
cargo test --all-targets -- --test-threads=1
```

## Quality Bar

- One persistent connection per workspace per process, period.
- No regression in session-only discovery footprint.
- Documented in inline comments why `cached_persistent_for` is consulted first.

## Final Deliverable

- Updated `src/live_index/frecency.rs` with the cache-first read path.
- New tests proving post-reset visibility and concurrent-call resilience.
- Verification command output.
- One-line update to `docs/decisions/0016-call-time-capability-resolution.md` Migration Order if behavior intent has shifted.
