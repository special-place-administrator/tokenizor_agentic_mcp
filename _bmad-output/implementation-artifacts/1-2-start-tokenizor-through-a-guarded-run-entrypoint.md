# Story 1.2: Start Tokenizor Through a Guarded Run Entrypoint

Status: done

<!-- Note: Validation is optional. Run validate-create-story for quality check before dev-story. -->

## Story

As an operator,
I want `run` to start Tokenizor only when prerequisites are healthy,
so that MCP is not served from an unsafe or degraded local state.

## Acceptance Criteria

1. Given local runtime, schema, and storage prerequisites are healthy
   When I run the Tokenizor operator entrypoint
   Then Tokenizor starts serving MCP through the baseline operator entrypoint
   And startup records clear healthy-state feedback

2. Given readiness checks fail
   When I run Tokenizor
   Then Tokenizor refuses to start serving MCP
   And it returns explicit and actionable failure output instead of partial startup

## Tasks / Subtasks

- [x] Align the guarded `run` startup gate with the richer deployment readiness evaluator from Story 1.1. (AC: 1, 2)
  - [x] Rework `src/application/mod.rs::ensure_runtime_ready()` so it uses deployment/runtime readiness that matches `doctor`, not the narrower health-only surface.
  - [x] Preserve the `TOKENIZOR_REQUIRE_READY_CONTROL_PLANE` guard behavior so startup gating can still be explicitly bypassed by config when intended.
  - [x] Keep `doctor()` and `run()` consuming the same read-only readiness primitives rather than diverging again.

- [x] Make `run` fail closed before serving MCP when readiness is not achieved. (AC: 2)
  - [x] Update `src/main.rs::run()` so MCP serving never starts when the guarded readiness gate fails.
  - [x] Return operator-facing failure detail that is explicit and actionable instead of a generic non-ready message.
  - [x] Avoid partial startup behavior: no MCP serve loop, no degraded serve mode, and no hidden auto-bootstrap or migration attempts.

- [x] Preserve bootstrap and mutation boundaries while supporting successful startup. (AC: 1, 2)
  - [x] Keep `init()` as the mutation/bootstrap path; `run()` should validate and serve, not bootstrap control-plane state.
  - [x] Keep storage initialization behavior intentional and justified; if local CAS layout creation remains part of `run`, document why that does not violate the guarded-startup contract.
  - [x] Do not add project/workspace registration, migration execution, indexing work, or control-plane writes to startup gating.

- [x] Cover guarded startup behavior with deterministic tests. (AC: 1, 2)
  - [x] Add or extend inline tests in `src/application/mod.rs`, `src/application/deployment.rs`, and/or `src/main.rs` for readiness-pass and readiness-fail startup paths.
  - [x] Prove that failed readiness blocks MCP startup and surfaces actionable detail.
  - [x] Prove that healthy readiness continues through the baseline startup path.

## Dev Notes

### Story Requirements

- Scope this story to guarded `run()` startup behavior and readiness alignment.
- Reuse the richer deployment/runtime readiness model established in Story 1.1.
- Fail closed: if guarded readiness is not achieved, do not start serving MCP.
- Return actionable operator-facing failure output; avoid generic "not ready" messaging.
- Do not expand this story into project/workspace lifecycle, migrations, or indexing.

### Current Implementation Baseline

- `src/main.rs::run()` currently:
  - builds config and `ApplicationContext`
  - calls `initialize_local_storage()`
  - calls `ensure_runtime_ready()`
  - immediately serves MCP with `TokenizorServer::new(application).serve(stdio())`
- `src/main.rs::doctor()` already calls `ApplicationContext::deployment_report()` and prints the richer readiness JSON from Story 1.1.
- `src/application/mod.rs::ensure_runtime_ready()` still uses `health_report()`, which is narrower than the deployment readiness evaluator.
- `src/application/mod.rs::health_report()` uses `HealthService`, which aggregates `control_plane.health_check()` and blob-store health only.
- `src/storage/control_plane.rs::health_check()` still checks endpoint reachability only, while `deployment_checks()` carries the richer config, compatibility, CLI, endpoint, and module-path readiness.

### Developer Context

- Story 1.1 intentionally stopped at doctor/reporting scope.
- The two review follow-ups deferred into Story 1.2 were:
  - align `run()` with the richer readiness evaluator before serving MCP
  - decide whether the shared `health_report()` surface becomes readiness-grade or remains intentionally narrower
- Resolve those follow-ups here in the smallest coherent way that satisfies guarded startup.
- Prefer changing the startup gate and its supporting application-layer contract over broad redesign of unrelated health consumers.

### Technical Requirements

- Use the richer deployment/runtime readiness evaluator for guarded startup decisions.
- Keep config-derived readiness failures visible before runtime/file-system probe failures where ordering matters.
- Preserve deterministic, machine-readable readiness reporting for `doctor`.
- Startup failure output should include the concrete blocking prerequisites and remediation, not only a summary string.
- If `TOKENIZOR_REQUIRE_READY_CONTROL_PLANE` is false, preserve the explicit opt-out behavior rather than silently removing the config escape hatch.

### Architecture Compliance

- Preserve the layered flow `main.rs` -> `application` -> `storage/domain`.
- Keep the control-plane/runtime readiness contract in the application/domain layers, not embedded as ad hoc CLI logic in `main.rs`.
- Maintain the split between read-only readiness checks and mutation/bootstrap flows (`doctor`/guarded `run` vs `init`).
- Do not silently bootstrap, migrate, publish, or repair on `run` failure.
- Ensure MCP is either served from a ready state or not served at all.

### Library / Framework Requirements

- Stay on the current dependency set in `Cargo.toml`.
- Continue using the existing `anyhow`, `serde`, and `serde_json` CLI/reporting approach unless a small refactor is needed for better guarded-startup output.
- Keep the server startup path compatible with the existing `rmcp` stdio scaffold.

### File Structure Requirements

- Prefer edits in existing files:
  - `src/main.rs`
  - `src/application/mod.rs`
  - `src/application/deployment.rs`
  - `src/domain/health.rs`
  - `src/storage/control_plane.rs`
  - `src/storage/local_cas.rs`
- Follow the current inline-test pattern unless a new focused test file is clearly justified.

### Testing Requirements

- Add deterministic tests covering guarded startup pass/fail behavior.
- Cover the `TOKENIZOR_REQUIRE_READY_CONTROL_PLANE` opt-out path if it remains part of the runtime gate.
- Ensure tests prove that failed readiness does not reach the MCP serve path.
- Keep tests local and hermetic; do not require a live SpacetimeDB instance.

### Git Intelligence Summary

- Current git history remains minimal, so implementation guidance should come from the live scaffold and Story 1.1 learnings rather than commit archaeology.
- Story 1.1 already established the richer deployment readiness contract and documented the 1.2 follow-up scope.

### Latest Technical Information

- The current local CLI surface remains `cargo run -- doctor`, `cargo run -- init`, and `cargo run -- run`.
- SpacetimeDB operator flows are still local-runtime and CLI driven; guarded `run` should report missing prerequisites clearly, not try to install or publish automatically.

### Project Context Reference

- No `project-context.md` was found in the repository.
- Use `docs/index.md` as the document entry point and `docs/development-guide.md` for operator commands and environment configuration.

### Project Structure Notes

- The repo is still a compact Rust scaffold, not the full future module tree from planning artifacts.
- Strengthen the existing startup/readiness contract in place instead of introducing speculative runtime orchestration layers.
- Story 1.2 should consume the readiness foundation from Story 1.1 rather than duplicating it.

### References

- [Source: _bmad-output/planning-artifacts/epics.md#Story 1.2: Start Tokenizor Through a Guarded Run Entrypoint]
- [Source: _bmad-output/planning-artifacts/prd.md#Functional Requirements]
- [Source: _bmad-output/planning-artifacts/architecture.md#Infrastructure / Runtime / Deployment Model]
- [Source: docs/development-guide.md#Key Commands]
- [Source: docs/development-guide.md#Environment Configuration]
- [Source: _bmad-output/implementation-artifacts/1-1-validate-local-runtime-readiness.md]
- [Source: src/main.rs]
- [Source: src/application/mod.rs]
- [Source: src/application/deployment.rs]
- [Source: src/domain/health.rs]
- [Source: src/storage/control_plane.rs]
- [Source: src/storage/local_cas.rs]

## Dev Agent Record

### Agent Model Used

GPT-5 Codex

### Debug Log References

- Missing BMAD validator task noted during story generation: `_bmad/core/tasks/validate-workflow.xml` is absent in this install.

### Completion Notes List

- Story manually validated against the create-story checklist because the referenced BMAD validator task is missing.
- Story scope constrained to guarded startup and readiness alignment only.
- Story inherits the two deferred follow-ups from Story 1.1 and treats them as the implementation center for guarded `run()`.
- Guarded startup now reuses the richer deployment readiness report from Story 1.1 instead of the narrower `health_report()` surface.
- `run()` no longer initializes local CAS storage before readiness gating, so failed startup does not mutate bootstrap state on the way to refusal.
- The shared `health_report()` surface remains intentionally narrower for now; Story 1.2 aligns `run()` with deployment readiness without promoting all health consumers to readiness-grade checks.
- Added application-layer tests for readiness failure, success, and explicit runtime-gate opt-out, plus binary tests proving the serve path is skipped when readiness fails.
- Verified with `cargo fmt`, `cargo test`, and `cargo run -- run` on 2026-03-07; the smoke run now exits before serving MCP and reports the missing local `spacetimedb` CLI plus unreachable `http://127.0.0.1:3007` endpoint as blocking prerequisites.

### File List

- _bmad-output/implementation-artifacts/1-2-start-tokenizor-through-a-guarded-run-entrypoint.md
- _bmad-output/implementation-artifacts/sprint-status.yaml
- src/application/mod.rs
- src/main.rs
- src/storage/local_cas.rs

### Change Log

- 2026-03-07: Created Story 1.2 from sprint context and captured the deferred guarded-startup follow-ups from Story 1.1.
- 2026-03-07: Aligned `run()` gating with deployment readiness, removed pre-gate CAS initialization from `run()`, and added guarded-startup tests for failure, success, and opt-out behavior.
- 2026-03-07: Review follow-up hardened CAS readiness with non-destructive write probes so guarded startup blocks on non-writable storage, not only missing layout directories.

## Senior Developer Review (AI)

### Reviewer

GPT-5 Codex

### Review Date

2026-03-07

### Outcome

Approve

### Summary

- Fixed the in-scope review issue: CAS readiness now verifies the existing layout is writable before `run()` is allowed to serve MCP.
- Kept the MCP `health` tool contract unchanged in Story 1.2; that alignment is explicitly deferred because it widens protocol/reporting scope beyond guarded startup.
- Left startup success logging as-is; the current signal is acceptable for this story and richer operator logging remains optional polish.

### Findings

- Resolved in Story 1.2: `LocalCasBlobStore::health_check()` was too optimistic because it only checked directory existence and could allow startup against a non-writable CAS layout.
- Deferred beyond Story 1.2: align the MCP `health` tool with deployment-grade readiness if the product wants protocol-visible parity with `doctor` and guarded `run()`.
- Deferred as optional polish: expand healthy startup logging with endpoint/database/module-path detail if later operator diagnostics need more context.

### Deferred Follow-ups

- Decide whether the MCP `health` tool should remain a narrower runtime health surface or be promoted to the richer deployment/readiness contract used by `doctor` and guarded `run()`.
