# /goal Capability Resolution Follow-Up Task 02: Ranking Diagnostics Policy Enforcement (M1 + L1)

/goal close the contract gap so `RankingDiagnosticsPolicy::Disabled` is honored at call time: operator can disable ranking diagnostics via `SYMFORGE_DEBUG_RANKING=disabled/off`, requested `debug_ranking=true` then returns explicit `disabled by policy` evidence, and the same env parsing flows through one shared policy helper.

## Context

- Project: SymForge, a Rust-native MCP for code indexing, retrieval, orchestration, and recovery.
- Working directory: `C:\AI_STUFF\PROGRAMMING\symforge`.
- Current problem: `RankingDiagnosticsPolicy` enum exists in `src/capability/policy.rs:56-71` with variants `CallTimeExplain`, `DefaultOn`, `Disabled`. Three places read `SYMFORGE_DEBUG_RANKING` directly without policy mapping:
  - `src/protocol/tools.rs:2449-2452` (`search_files_debug_ranking_requested`)
  - `src/protocol/tools.rs:2695-2701` (`ranking_diagnostics_health_status`)
  - `src/protocol/tools.rs:5636` (legacy `last_10_bumps` health gate)
- No call site consults the `Disabled` variant. ADR `docs/decisions/0016-call-time-capability-resolution.md` §Decision says policy may "disable a capability for this process or workspace"; ranking diagnostics is the only capability without that path.
- Plan req `CCR-1`, `CCR-2`, `CCR-6`, `CCR-8` require uniform policy semantics across capabilities.
- Severity: Medium. Dead enum variant + asymmetric contract.
- Relevant source material:
  - `src/capability/policy.rs`
  - `src/capability/mod.rs`
  - `src/protocol/tools.rs`
  - `docs/decisions/0016-call-time-capability-resolution.md`
  - `tests/search_files_ranking_debug.rs`
  - `tests/capability_status_integration.rs`
  - `README.md` (env-var table)
- Requirements covered: `CCR-1`, `CCR-2`, `CCR-6`, `CCR-8`, `CCR-9`
- Depends on: prior capability resolution task pack.
- Expected files to modify:
  - `src/capability/mod.rs` (re-export if a new helper lives in policy or a new module)
  - `src/live_index/` (new helper, optional — preferred location next to other `policy_from_env` helpers in their feature modules)
  - `src/protocol/tools.rs`
  - `src/protocol/format.rs` (only if a capability evidence helper needs adjustment)
  - `tests/search_files_ranking_debug.rs`
  - `tests/capability_status_integration.rs`
  - `README.md`
- Files off limits:
  - `src/live_index/frecency.rs`
  - `src/live_index/coupling/lifecycle.rs`
  - `src/worktree.rs`
  - `src/protocol/edit.rs`

## Machine Metadata

- phase: `3h-capability-resolution-followups`
- plan: `02`
- wave: `1`
- type: `ranking-diagnostics-policy`
- autonomous: `true`
- requirements: `CCR-1`, `CCR-2`, `CCR-6`, `CCR-8`, `CCR-9`
- must_haves:
  - One env-to-policy helper for ranking diagnostics, mirroring frecency/coupling/worktree.
  - `SYMFORGE_DEBUG_RANKING=disabled|off|0|false|no|disable` → `RankingDiagnosticsPolicy::Disabled`.
  - Requested `debug_ranking=true` under disabled policy returns explicit capability evidence, no explanation block.
  - Health surfaces report `ranking diagnostics: disabled by policy` when policy is disabled.

## Success Criteria - All Must Be True

1. A `ranking_diagnostics_policy_from_env() -> RankingDiagnosticsPolicy` helper exists in a module owned by ranking diagnostics (e.g. `src/protocol/tools.rs` near other ranking helpers, or a new `src/live_index/ranking_diagnostics.rs`). Mapping matches the other three policy helpers: unset → `CallTimeExplain`; `1`/`true`/`on`/`yes`/`default-on` → `DefaultOn`; `0`/`false`/`no`/`off`/`disabled`/`disable` → `Disabled`; unknown → `Disabled` (matches frecency convention).
2. `search_files_debug_ranking_requested` and any other call sites consult the policy helper, not raw env.
3. When policy is `Disabled` and the caller passes `debug_ranking=true`, the response includes `Capability: ranking diagnostics disabled by policy - <detail>` evidence and omits the ranking explanation block.
4. When policy is `DefaultOn` and the caller omits `debug_ranking`, behavior is unchanged from today (explanation appears).
5. When policy is `CallTimeExplain` (default) and the caller passes `debug_ranking=true`, behavior is unchanged from today (explanation appears, no extra evidence noise).
6. `ranking_diagnostics_health_status()` reports `disabled by policy` / `call-time explain available/default-on` / `call-time explain available/default-off` matching policy state.
7. README env-var row for `SYMFORGE_DEBUG_RANKING` documents the `disabled` value.
8. Tests prove all three policy states from a request perspective and from a health perspective, with env unset, `=1`, and `=disabled`.
9. `cargo check`, focused `cargo test --test search_files_ranking_debug --test capability_status_integration -- --test-threads=1`, and shared `cargo test --all-targets -- --test-threads=1` pass.

## Constraints

- Do not change `RankingDiagnosticsPolicy` enum variants.
- Do not change `search_files` request schema (still `debug_ranking: bool`).
- Do not change frecency, coupling, or worktree policy mapping.
- Do not add per-call score dumps; preserve compactness of the explanation block.
- Preserve the existing `search_files` response when diagnostics are disabled — only the evidence line is added; ranking explanation is suppressed.

## Implementation Sketch

1. Add `ranking_diagnostics_policy_from_env()`. Place it next to the other ranking helpers in `src/protocol/tools.rs` to keep policy-vs-feature locality consistent with how `routing_policy_from_env` lives in `worktree.rs`.
2. Update `search_files_debug_ranking_requested` to:
   - Read the policy.
   - If `Disabled`: return `false` AND signal to the handler that a `DisabledByPolicy` capability-evidence line should be appended.
   - If `DefaultOn`: return `true` regardless of request.
   - If `CallTimeExplain`: return `input.debug_ranking.unwrap_or(false)`.
3. Adjust the `search_files` handler so a disabled-policy request produces a `capability_evidence_line` with `CapabilityName::RankingDiagnostics` + `CapabilityStatus::DisabledByPolicy`, and the existing `if debug_ranking { append explanation }` branch stays off.
4. Update `ranking_diagnostics_health_status()` to read the same policy helper.
5. Add tests in `tests/search_files_ranking_debug.rs`:
   - `debug_ranking_disabled_by_policy_returns_evidence_and_omits_explanation`
   - `debug_ranking_default_on_policy_explains_even_without_request_field` (rename or extend existing test that uses `SYMFORGE_DEBUG_RANKING=1`)
6. Add a test in `tests/capability_status_integration.rs`:
   - `health_reports_ranking_diagnostics_disabled_by_policy_when_env_disabled`
7. Update README env-var row.

## Verification

```powershell
cargo test --test search_files_ranking_debug -- --test-threads=1
cargo test --test capability_status_integration -- --test-threads=1
cargo check
cargo test --all-targets -- --test-threads=1
```

## Quality Bar

- `RankingDiagnosticsPolicy::Disabled` is no longer dead code.
- Capability contract vocabulary is symmetric across all four advertised capabilities.
- README env-var documentation matches actual parsing.
- Disabled policy is fail-quiet at the response level: explicit evidence, no explanation, no panic.

## Final Deliverable

- Code changes plumbing the policy helper into the three call sites.
- New tests + updated existing ones.
- Verification command output.
- README diff for the env-var row.
- Note in `docs/notes/2026-05-16-call-time-capability-resolution-close-out.md` (or new dated follow-up note) recording the gap and its close.
