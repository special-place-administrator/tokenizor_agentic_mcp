# Story 1.1: Validate Local Runtime Readiness

Status: done

<!-- Note: Validation is optional. Run validate-create-story for quality check before dev-story. -->

## Story

As an operator,
I want to validate Tokenizor's local runtime and dependency readiness,
so that I can trust the environment before starting or initializing the system.

## Acceptance Criteria

1. Readiness is evaluated from current config plus observed local runtime state only; the check does not rely on hidden bootstrap, migration, or indexing work.

2. Failures are classified explicitly as bootstrap, dependency, configuration, compatibility, or storage issues rather than generic non-ready output.

3. Each failed or degraded prerequisite includes explicit remediation guidance suitable for operator-facing CLI output.

4. Running readiness checks mutates nothing: no project/workspace registration, no control-plane writes, no bootstrap/init side effects, no migration execution, and no indexing work.

## Tasks / Subtasks

- [x] Extend the readiness domain model so doctor-style checks can express category, severity, and remediation guidance without inventing a second reporting system. (AC: 1, 2, 3, 4)
  - [x] Reuse and evolve `HealthStatus`, `ComponentHealth`, `HealthReport`, and `DeploymentReport` in `src/domain/health.rs` instead of creating parallel readiness structs.
  - [x] Add explicit issue categorization for bootstrap, dependency, configuration, compatibility, and storage failures.
  - [x] Preserve a deterministic machine-readable output shape for CLI and later MCP/operator reuse.

- [x] Upgrade deployment/readiness aggregation so config validity, runtime/service reachability, schema/module compatibility, and storage readiness are all represented from the existing application layer. (AC: 1, 2, 3, 4)
  - [x] Extend `DeploymentService` in `src/application/deployment.rs` to aggregate richer prerequisite checks and remediation text.
  - [x] Keep `doctor` non-mutating by continuing to use report/check paths, not bootstrap paths.
  - [x] Treat this story as the readiness foundation for Story 1.2; report deployment/runtime readiness blockers through `doctor`, and do not change guarded startup behavior here.

- [x] Expand control-plane prerequisite checks to cover real SpacetimeDB readiness expectations without performing bootstrap or migration work. (AC: 1, 2, 3, 4)
  - [x] Build on `SpacetimeControlPlane::deployment_checks()` in `src/storage/control_plane.rs` rather than adding a separate probe layer.
  - [x] Verify CLI presence, endpoint reachability, configured database/module path, and schema-compatibility expectations exposed by current config.
  - [x] Return actionable remediation messages tied to the exact failed prerequisite.

- [x] Preserve storage-readiness boundaries and non-mutating behavior. (AC: 1, 3, 4)
  - [x] Reuse `LocalCasBlobStore::health_check()` for non-mutating storage validation.
  - [x] Do not call `initialize()` from doctor/readiness flows; bootstrap mutations remain the responsibility of `init`.
  - [x] Ensure readiness checks do not create project/workspace records, do not write control-plane state, and do not start run/indexing state.

- [x] Wire the operator-facing doctor output through the enriched readiness report and cover it with tests. (AC: 1, 2, 3, 4)
  - [x] Update `src/main.rs::doctor()` to emit the richer report and fail clearly when readiness is not achieved.
  - [x] Keep `src/main.rs::init()` and `src/main.rs::run()` out of scope except for sharing the same read-only readiness primitives or reporting surfaces.
  - [x] Add or extend inline unit tests in the existing module files (`src/storage/control_plane.rs`, `src/storage/local_cas.rs`, `src/domain/health.rs`) instead of inventing a new external test layout unless necessary.
  - [x] Cover missing CLI, unreachable endpoint, empty database/module config, schema mismatch or compatibility failure, missing CAS directories, and non-mutating doctor behavior.

## Dev Notes

### Story Requirements

- Scope this story to `doctor` and readiness evaluation only.
- Readiness must be derived from current config and observed local state, not from optimistic assumptions.
- Report whether startup would be blocked, but do not redesign `run` startup behavior in this story.
- Do not add project/workspace registration, `init`, migration execution, indexing, or bootstrap side effects.

### Current Implementation Baseline

- `src/main.rs` already exposes `doctor`, `init`, and `run`.
- `doctor()` currently calls `ApplicationContext::deployment_report()` and prints JSON.
- `init()` currently calls `bootstrap_report()` and is the bootstrap/mutation path; keep that boundary intact.
- `run()` currently initializes local storage and enforces runtime readiness; Story 1.2 should own guarded startup behavior, not Story 1.1.
- `src/application/deployment.rs` already aggregates control-plane deployment checks plus blob-store health/initialize behavior.
- `src/storage/control_plane.rs` already probes SpacetimeDB CLI presence, endpoint reachability, configured database, and module path existence.
- `src/storage/local_cas.rs` already distinguishes non-mutating `health_check()` from mutating `initialize()`.

### Developer Context

- Reuse the existing readiness/reporting path. Do not create a parallel `doctor`-only subsystem.
- The likely center of change is `src/domain/health.rs`, `src/application/deployment.rs`, `src/storage/control_plane.rs`, `src/storage/local_cas.rs`, and `src/main.rs`.
- The current gap is semantic richness, not the absence of a readiness path. The scaffold already knows how to probe several prerequisites; it does not yet classify them cleanly or attach remediation guidance.
- Because this is Epic 1 / Story 1.1, there is no prior story file to mine for implementation learnings. This story establishes the operator-readiness contract consumed by later startup and lifecycle stories.

### Technical Requirements

- Validate config-derived inputs explicitly before network or filesystem probes when possible.
- At minimum, classify readiness across:
  - configuration validity
  - local SpacetimeDB runtime/service reachability
  - CLI/bootstrap surface presence when it affects operator guidance
  - schema/module compatibility
  - CAS/storage root readiness
- Keep the output machine-readable and deterministic.
- Prefer explicit modeled categories or fields over forcing operators to parse free-form detail strings.
- If schema compatibility cannot be fully verified yet, surface the limitation explicitly as compatibility-related status rather than hiding it.
- Remediation guidance must tell the operator what to do next, not only what failed.

### Architecture Compliance

- Preserve the layered flow `main.rs` -> `application` -> `storage/domain`.
- Tokenizor remains authoritative for operational truth; readiness checks are observers in this story, not mutators.
- Keep raw-storage concerns in the CAS layer and control-plane/runtime concerns in the control-plane layer.
- Maintain the documented split between read-only readiness checks and bootstrap/mutation flows (`doctor` vs `init`).
- Do not implement migration execution here. If compatibility is wrong, report it and point to the future/operator path instead of fixing it automatically.

### Library / Framework Requirements

- Stay on the current dependency set in `Cargo.toml` for this story. Do not turn Story 1.1 into a dependency-upgrade task.
- Continue using `anyhow`, `thiserror`, `serde`, and `serde_json` for the current CLI/reporting pattern unless a localized refactor is strictly required.
- Keep `rmcp` out of the critical path for readiness implementation. Story 1.1 is an operator CLI concern first, even if later MCP surfaces may reuse the same readiness model.

### File Structure Requirements

- Prefer edits in existing files:
  - `src/domain/health.rs`
  - `src/application/deployment.rs`
  - `src/application/mod.rs`
  - `src/storage/control_plane.rs`
  - `src/storage/local_cas.rs`
  - `src/main.rs`
- Do not prematurely create the full future architecture tree from the planning docs; the current repo is still a compact scaffold.
- Follow the existing inline-test pattern (`#[cfg(test)]` inside source files) unless there is a compelling need for a new test file.

### Testing Requirements

- Add unit coverage for readiness classification and remediation rendering.
- Add control-plane probe tests for:
  - missing CLI
  - unreachable endpoint
  - empty or invalid database/module config
  - compatibility/schema failure or unverified-compatibility reporting
- Add CAS readiness tests proving missing directories are reported without being created.
- Add doctor/report tests proving the readiness path performs no writes or bootstrap side effects.
- Keep tests deterministic and local; do not depend on an actual running SpacetimeDB instance in unit tests when a probe abstraction already exists.

### Git Intelligence Summary

- Current git history contains only the initial scaffold commit (`477399c Initial commit`).
- There are no prior implementation patterns beyond the scaffold itself, so reuse decisions should be driven by the existing source layout and the docs-led architecture rather than commit archaeology.

### Latest Technical Information

- The repository currently pins Rust 2024 edition and `rmcp = 1.1.0` in `Cargo.toml`; do not upgrade the MCP SDK as part of readiness work.
- Current official SpacetimeDB docs emphasize a CLI-managed local runtime and operator commands such as install/start/CLI reference flows. Remediation text should point operators toward those official surfaces instead of inventing Tokenizor-specific magic.
- Keep readiness checks aligned with the current SpacetimeDB operator model: detect and explain missing prerequisites, but do not silently install, start, publish, or migrate anything in this story.

### Project Context Reference

- No `project-context.md` was found in the repository.
- Use `docs/index.md` as the brownfield document entry point.
- Use `docs/architecture.md`, `docs/tokenizor_project_direction.md`, and `docs/development-guide.md` as the target-state/operator-behavior references.

### Project Structure Notes

- The planning architecture describes a larger future module tree, but the actual repo currently has a compact scaffold with `application`, `domain`, `storage`, `protocol`, `indexing`, `parsing`, and `observability` roots.
- Story 1.1 should strengthen the existing scaffold, not introduce speculative directories such as future `runtime/` or deep provider trees.
- Inline tests already exist in `src/storage/control_plane.rs`, `src/storage/local_cas.rs`, and `src/storage/sha256.rs`; matching that pattern will minimize churn.

### References

- [Source: _bmad-output/planning-artifacts/epics.md#Story 1.1: Validate Local Runtime Readiness]
- [Source: _bmad-output/planning-artifacts/prd.md#Repository & Workspace Lifecycle]
- [Source: _bmad-output/planning-artifacts/architecture.md#API / Protocol & Communication]
- [Source: _bmad-output/planning-artifacts/architecture.md#Infrastructure / Runtime / Deployment Model]
- [Source: docs/development-guide.md#Key Commands]
- [Source: docs/development-guide.md#Environment Configuration]
- [Source: docs/source-tree-analysis.md#Critical Directories]
- [Source: docs/project-overview.md#Current Implementation Status]
- [Source: src/main.rs]
- [Source: src/application/deployment.rs]
- [Source: src/domain/health.rs]
- [Source: src/storage/control_plane.rs]
- [Source: src/storage/local_cas.rs]
- [External: https://spacetimedb.com/docs/install]
- [External: https://spacetimedb.com/docs/cli/reference]
- [External: https://docs.rs/crate/rmcp/1.1.0]

## Dev Agent Record

### Agent Model Used

GPT-5 Codex

### Implementation Plan

- Extend the shared health/reporting domain with explicit readiness metadata so `doctor`, storage checks, and future MCP surfaces can reuse one deterministic contract.
- Keep `doctor` on read-only report paths by enriching `DeploymentService::report()` and the existing control-plane/CAS probes instead of adding bootstrap-specific logic.
- Prove the boundary with inline tests for classification, remediation rendering, control-plane failure cases, CAS no-write checks, and deployment-service non-mutation.

### Debug Log References

- Missing BMAD validator task noted during story generation: `_bmad/core/tasks/validate-workflow.xml` is absent in this install.

### Completion Notes List

- Story manually validated against the create-story checklist because the referenced BMAD validator task is missing.
- Scope tightened to read-only readiness evaluation only.
- Current scaffold reuse points and out-of-scope boundaries captured explicitly for the dev agent.
- Implemented structured readiness metadata on the shared health model: category, severity, remediation guidance, and `ready_for_run` semantics.
- Expanded SpacetimeDB deployment checks to classify CLI, endpoint, database, module-path, and schema-compatibility prerequisites with actionable operator remediation.
- Kept `doctor` non-mutating by exercising `deployment_report()` only, while `bootstrap_report()` remains the storage-initializing path used by `init`.
- Added deterministic inline tests for domain readiness behavior, control-plane probe scenarios, CAS no-write health validation, deployment read-only behavior, and doctor failure messaging.
- Verified with `cargo test` and a `cargo run -- doctor` smoke run on 2026-03-07; the smoke run correctly reported this machine's missing `spacetimedb` CLI and unreachable `http://127.0.0.1:3007` endpoint as blocking prerequisites.

### File List

- _bmad-output/implementation-artifacts/sprint-status.yaml
- _bmad-output/implementation-artifacts/1-1-validate-local-runtime-readiness.md
- src/application/deployment.rs
- src/config.rs
- src/domain/health.rs
- src/domain/mod.rs
- src/main.rs
- src/storage/control_plane.rs
- src/storage/local_cas.rs

### Change Log

- 2026-03-07: Implemented structured local runtime readiness reporting, explicit prerequisite remediation, non-mutating doctor validation, and inline coverage for the new readiness contract.
- 2026-03-07: Code review follow-up reordered doctor readiness output so configuration-derived findings are reported before runtime and filesystem probe findings, and deferred startup/readiness contract alignment to Story 1.2.

## Senior Developer Review (AI)

### Reviewer

GPT-5 Codex

### Review Date

2026-03-07

### Outcome

Approve

### Summary

- Verified the Story 1.1 implementation against the changed source files and the story acceptance criteria.
- Fixed the in-scope review issue: `doctor` now reports configuration-derived readiness findings before runtime and filesystem probe findings.
- Reworded the story task text to reflect the actual Story 1.1 scope: `doctor` reports deployment/runtime readiness blockers, while `run()` startup gating remains Story 1.2 work.

### Findings

- Resolved in Story 1.1: deployment readiness checks were emitted in probe-first order instead of config-first order.
- Deferred to Story 1.2: align `run()` with the richer readiness evaluator before serving MCP.
- Deferred to Story 1.2: decide whether the shared `health_report()` surface should become readiness-grade or remain a narrower health check.

### Deferred Follow-ups For Story 1.2

- Align `run()` and its runtime gate with the richer readiness evaluator before serving MCP.
- Decide whether the shared health surface should be promoted to readiness-grade coverage or intentionally remain narrower than deployment readiness.
