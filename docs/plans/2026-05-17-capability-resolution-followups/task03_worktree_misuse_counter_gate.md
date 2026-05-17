# /goal Capability Resolution Follow-Up Task 03: Worktree Misuse Counter Gate (M2)

/goal restore the worktree-misuse observability signal under the new default policy so the rolling 1-hour `edit tool calls without working_directory` counter actually fires when worktree routing is in `ExplicitCallTime` mode, instead of staying silently zero unless an operator explicitly sets `SYMFORGE_WORKTREE_AWARE=1`.

## Context

- Project: SymForge, a Rust-native MCP for code indexing, retrieval, orchestration, and recovery.
- Working directory: `C:\AI_STUFF\PROGRAMMING\symforge`.
- Current problem: After the call-time capability resolution refactor, `SYMFORGE_WORKTREE_AWARE` unset now means `WorktreeRoutingPolicy::ExplicitCallTime` (active). However, `worktree::feature_flag_enabled()` (`src/worktree.rs:379-381`) still uses legacy `=="1"` semantics, and `note_worktree_misuse_if_flag_on` (`src/protocol/mod.rs:176-181`) gates the misuse counter on that legacy flag. Result:
  - Default deployments now route worktree edits but never count omissions.
  - Health output line `edit tool calls without working_directory (last hour): {N}` is stuck at 0 unless an operator manually opts in to the legacy flag.
  - The original purpose of the counter — keep agent regressions visible after the feature ships — is silently defeated by the very refactor that made the feature default-on.
- README env-var table line `src/README.md:286` advertises env-unset as enabling explicit routing; the misuse counter contradicts that.
- Severity: Medium observability regression. No correctness impact.
- Relevant source material:
  - `src/worktree.rs`
  - `src/protocol/mod.rs`
  - `src/protocol/tools.rs` (the seven `note_worktree_misuse_if_flag_on` call sites near `src/protocol/tools.rs:8087-8904`, and the health rendering at `src/protocol/tools.rs:5607-5612` and `src/protocol/tools.rs:5697-5701`)
  - `tests/worktree_awareness.rs` (`health_surfaces_worktree_misuse_counter`)
  - `docs/notes/2026-05-16-rtk-once-lock-audit.md` (audit references `feature_flag_enabled` rationale)
- Requirements covered: `CCR-2`, `CCR-5`, `CCR-7`, `CCR-8`, `CCR-9`
- Depends on: prior capability resolution task pack.
- Expected files to modify:
  - `src/worktree.rs`
  - `src/protocol/mod.rs`
  - `tests/worktree_awareness.rs`
  - Possibly `README.md` if the env-var row needs to clarify that the counter is no longer keyed on `=1`.
- Files off limits:
  - `src/live_index/frecency.rs`
  - `src/live_index/coupling/lifecycle.rs`
  - `src/capability/*`
  - `src/protocol/edit.rs`

## Machine Metadata

- phase: `3h-capability-resolution-followups`
- plan: `03`
- wave: `1`
- type: `worktree-misuse-gate`
- autonomous: `true`
- requirements: `CCR-2`, `CCR-5`, `CCR-7`, `CCR-8`, `CCR-9`
- must_haves:
  - Misuse counter fires whenever routing policy is `ExplicitCallTime` and an edit omits `working_directory`.
  - Misuse counter stays silent when routing policy is `Disabled`.
  - Counter does not fire when `working_directory` is supplied (no regression).
  - Health output reflects the new gating accurately.

## Success Criteria - All Must Be True

1. The misuse counter is gated on `routing_policy_from_env() == WorktreeRoutingPolicy::ExplicitCallTime` (default), not on the legacy `=="1"` check.
2. When policy is `Disabled`, omissions are not counted (nothing to observe — feature is off).
3. When `working_directory` is supplied, no counter bump happens regardless of policy.
4. `feature_flag_enabled()` is either removed (preferred) or repurposed and documented as a different observability knob; no dead public function left behind.
5. Health output continues to render the counter line; existing `health_surfaces_worktree_misuse_counter` test continues to pass against the new gate.
6. A new focused test proves the counter increments under env-unset (`ExplicitCallTime` default) when an edit omits `working_directory`.
7. A new focused test proves the counter stays at zero under `SYMFORGE_WORKTREE_AWARE=disabled`.
8. README env-var row for `SYMFORGE_WORKTREE_AWARE` documents the new counter semantics if any user-visible behavior shifted.
9. `cargo check`, focused `cargo test --test worktree_awareness -- --test-threads=1`, and shared `cargo test --all-targets -- --test-threads=1` pass.

## Constraints

- Do not change `WorktreeRoutingPolicy` enum variants or `routing_policy_from_env` mapping.
- Do not change the seven edit-tool handler call sites except to align them with the renamed/retained helper.
- Do not change the rolling 1-hour `WorktreeMisuseCounter` window logic.
- Do not introduce a new env var.
- Preserve fail-safe routing behavior — disabled policy still errors before write per Task 05 of the prior pack.

## Implementation Sketch

1. Replace `feature_flag_enabled()` with a more accurate predicate, or delete it and inline the policy check in `note_worktree_misuse_if_flag_on`. Preferred shape:
   ```rust
   pub(crate) fn note_worktree_misuse_if_active(&self, working_directory: Option<&str>) {
       if working_directory.is_none()
           && crate::worktree::routing_policy_from_env()
               == crate::capability::WorktreeRoutingPolicy::ExplicitCallTime
       {
           self.worktree_misuse.record_missing_working_directory();
       }
   }
   ```
   Rename call sites accordingly.
2. Delete `worktree::feature_flag_enabled()` once no callers remain. If any external callers exist (search `tests/`), update them too.
3. Update `tests/worktree_awareness.rs`:
   - Confirm `health_surfaces_worktree_misuse_counter` still constructs the right env state.
   - Add `misuse_counter_increments_under_env_unset_default_policy`.
   - Add `misuse_counter_stays_zero_under_disabled_policy`.
4. README pass: ensure the env-var row matches the actual behavior. The counter is no longer keyed on `=1`; either drop the implicit claim or make the row explicit.

## Verification

```powershell
cargo test --test worktree_awareness -- --test-threads=1
cargo check
cargo test --all-targets -- --test-threads=1
```

## Quality Bar

- No dead public function left over from the old gating.
- Health output is accurate under all three env states (unset, `1`, `disabled`).
- Misuse counter regains its post-refactor purpose without expanding scope.

## Final Deliverable

- Updated `src/worktree.rs` and `src/protocol/mod.rs` with the new gate.
- Updated and new tests.
- Verification command output.
- README diff if env-var row wording changed.
- Note appended to `docs/notes/2026-05-16-call-time-capability-resolution-close-out.md` (or new dated follow-up note) recording the fix.
