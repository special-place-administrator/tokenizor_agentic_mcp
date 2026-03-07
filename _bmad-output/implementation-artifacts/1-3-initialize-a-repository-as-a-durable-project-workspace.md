# Story 1.3: Initialize a Repository as a Durable Project Workspace

Status: done

<!-- Note: Validation is optional. Run validate-create-story for quality check before dev-story. -->

## Story

As a power user,
I want to initialize a local repository into Tokenizor,
so that the project and current workspace become durable identities across sessions.

## Acceptance Criteria

1. Given I am inside a supported local repository or provide an explicit repository path
   When I run the initialization flow
   Then Tokenizor creates or reuses a durable project identity and registers the current workspace
   And the result remains stable across later sessions

2. Given the same repository is initialized again with equivalent inputs
   When the operation is replayed
   Then Tokenizor behaves idempotently
   And it does not create duplicate project or workspace records

## Tasks / Subtasks

- [x] Add minimal durable project/workspace identity models and reports for the current scaffold. (AC: 1, 2)
  - [x] Extend the domain model with a workspace identity type and a machine-readable initialization report.
  - [x] Reuse the existing repository model as the durable project/repository identity anchor for this slice rather than inventing a parallel naming system disconnected from the current scaffold.
  - [x] Model whether repository/workspace registration was created or reused so idempotent replay is visible in CLI output.

- [x] Implement repository/workspace initialization as an application-layer workflow. (AC: 1, 2)
  - [x] Resolve the target repository from the current working directory or an explicit `init` path argument.
  - [x] Detect a supported local repository/workspace root deterministically and derive stable durable ids from canonical local state.
  - [x] Persist repository/workspace registration in Tokenizor-owned local state so the result survives later sessions in this scaffold.

- [x] Keep initialization idempotent and side-effect-bounded. (AC: 2)
  - [x] Re-running `init` for equivalent inputs must reuse the same repository/workspace identities instead of creating duplicates.
  - [x] Persist only the intended registry state; do not start indexing or mutate unrelated run/checkpoint/idempotency state.
  - [x] Keep the current local storage bootstrap behavior intact where it supports the initialization flow.

- [x] Wire the operator CLI through the new initialization workflow and cover it with deterministic tests. (AC: 1, 2)
  - [x] Update `src/main.rs::init()` to accept an optional explicit path argument while preserving current bootstrap/readiness feedback.
  - [x] Emit machine-readable initialization output that includes repository/workspace registration results and any remaining deployment blockers.
  - [x] Add inline unit coverage for repository resolution, durable replay/idempotency, and persisted registry reload behavior across fresh application contexts.

## Dev Notes

### Story Requirements

- Scope this story to repository/workspace initialization and durable registration only.
- `init` should support current-directory initialization and an explicit repository path override.
- The result must survive later sessions, so this story needs real local durability rather than in-memory-only registration.
- Do not expand into indexing, active workspace resolution, listing, or migration flows yet; those belong to later stories in Epic 1.

### Current Implementation Baseline

- `src/main.rs::init()` currently calls `ApplicationContext::bootstrap_report()` and prints a deployment/bootstrap report only.
- `src/application/mod.rs` currently exposes storage bootstrap, health, deployment, and guarded runtime readiness, but no repository/workspace initialization workflow.
- `src/domain/repository.rs` already defines a `Repository` record with durable-looking identity fields.
- The current source tree contains no workspace model yet.
- `src/storage/control_plane.rs` exposes `upsert_repository()`, but the default `SpacetimeControlPlane` write path still returns the pending-write error because full persistence is not wired yet.

### Developer Context

- Story 1.1 established read-only readiness reporting.
- Story 1.2 aligned guarded startup with the richer readiness evaluator and intentionally left MCP health-contract widening deferred.
- Story 1.3 is the first story that requires durable project/workspace state, so it must introduce a pragmatic persistence path within the current scaffold instead of pretending the pending SpacetimeDB write path is already complete.
- In this slice, the smallest coherent approach is a Tokenizor-owned local registry under the existing local state root, while keeping the code structured so later SpacetimeDB control-plane persistence can replace or subsume it cleanly.

### Technical Requirements

- Resolve the repository/workspace root deterministically from:
  - current working directory when no explicit path is supplied
  - an explicit path argument when provided
- For Git repositories, prefer the repository root as the durable identity root when the current directory is nested inside it.
- For non-Git local folders, support initialization of the explicit folder/root as a local repository identity.
- Derive stable ids from canonical local state so replaying equivalent inputs yields the same ids.
- Keep output machine-readable and explicit about whether repository/workspace records were created or reused.
- Preserve the current bootstrap/reporting behavior enough that operators can still see remaining deployment blockers after registration.

### Architecture Compliance

- Preserve the layered flow `main.rs` -> `application` -> `domain/storage`.
- Keep Tokenizor authoritative for project/workspace identity instead of delegating identity state to provider clients.
- Do not let repository initialization silently become indexing or active-session resolution.
- Treat any local durable registry introduced here as a scaffold for authoritative Tokenizor state, not as an excuse to push identity ownership into provider or client state.

### Library / Framework Requirements

- Stay on the current dependency set in `Cargo.toml`.
- Continue using `serde` / `serde_json` for the machine-readable local registry and CLI output unless a strictly localized refactor is needed.
- Do not turn Story 1.3 into a dependency-addition or SpacetimeDB SDK integration task.

### File Structure Requirements

- Prefer edits in existing compact scaffold areas plus minimal new domain/application files:
  - `src/main.rs`
  - `src/application/mod.rs`
  - `src/domain/mod.rs`
  - `src/domain/repository.rs`
  - `src/storage/control_plane.rs`
- Minimal new files are acceptable if they sharpen boundaries for initialization or workspace identity:
  - `src/domain/workspace.rs`
  - `src/domain/init.rs`
  - `src/application/init.rs`
- Follow the existing inline-test pattern unless a new focused test file is clearly justified.

### Testing Requirements

- Add deterministic unit coverage for:
  - resolving current-directory vs explicit-path initialization
  - stable id reuse across equivalent init replays
  - persisted registry reload behavior across fresh service/application instances
  - duplicate avoidance for repository/workspace records
- Keep tests hermetic and local; do not depend on a live SpacetimeDB runtime.

### Git Intelligence Summary

- Current git history remains minimal, so implementation guidance should come from the live scaffold and the completed Story 1.1 / 1.2 learnings rather than commit archaeology.
- Story 1.2 left startup/readiness aligned and complete, so Story 1.3 can focus cleanly on the `init` lifecycle path.

### Latest Technical Information

- The current operator CLI surface remains `cargo run -- doctor`, `cargo run -- init`, and `cargo run -- run`.
- Current docs still describe `init` mainly as bootstrap of local storage plus deployment blockers; Story 1.3 should move it toward repository/workspace registration while remaining honest about any remaining deployment limitations.

### Project Context Reference

- No `project-context.md` was found in the repository.
- Use `docs/index.md`, `docs/development-guide.md`, `docs/data-models.md`, and `docs/api-contracts.md` as the current brownfield references for how the scaffold is described today.

### Project Structure Notes

- The repo is still a compact Rust scaffold rather than the fully expanded architecture tree.
- Build forward from the existing `Repository` model and CLI bootstrap path instead of introducing the full future `application/services/project_registry.rs` tree all at once.
- Keep the implementation path replaceable by later true control-plane persistence.

### References

- [Source: _bmad-output/planning-artifacts/epics.md#Story 1.3: Initialize a Repository as a Durable Project Workspace]
- [Source: _bmad-output/planning-artifacts/prd.md#Repository & Workspace Lifecycle]
- [Source: _bmad-output/planning-artifacts/architecture.md#Requirements to Structure Mapping]
- [Source: _bmad-output/planning-artifacts/architecture.md#Integration Points]
- [Source: docs/api-contracts.md#CLI Commands]
- [Source: docs/data-models.md#Current Implemented Models]
- [Source: docs/project-overview.md#Current Architecture Constraints]
- [Source: docs/development-guide.md#Key Commands]
- [Source: src/main.rs]
- [Source: src/application/mod.rs]
- [Source: src/domain/repository.rs]
- [Source: src/storage/control_plane.rs]

## Dev Agent Record

### Agent Model Used

GPT-5 Codex

### Debug Log References

- Missing BMAD validator task noted during story generation: `_bmad/core/tasks/validate-workflow.xml` is absent in this install.

### Completion Notes List

- Story manually validated against the create-story checklist because the referenced BMAD validator task is missing.
- Story scope constrained to durable repository/workspace initialization only.
- Story explicitly records the current scaffold gap: durable registration needs a Tokenizor-owned local persistence path before full SpacetimeDB writes are wired.
- Added `Workspace`, `InitializationReport`, and explicit `created` / `reused` registration outcomes so `init` can report idempotent durable identity behavior in machine-readable output.
- Implemented an application-layer initialization workflow that resolves the current or explicit repository path, prefers the Git root when present, and persists repository/workspace state under `.tokenizor/control-plane/project-workspace-registry.json`.
- Updated `init` to accept an optional explicit path argument and to print repository/workspace registration plus the existing deployment blockers in one JSON payload.
- Updated operator docs so `init` is documented as repository/workspace registration plus bootstrap, not only local storage setup.
- Verified with `cargo fmt`, `cargo test`, and two `cargo run -- init .` smoke runs on 2026-03-07. The first run created the repository/workspace ids for this repo; the second reused them, while both runs still exited non-zero because this machine's `spacetimedb` CLI and local runtime remain unavailable.
- Hardened the local bootstrap registry with an atomic temp-file replace path, a cross-process lock around read/modify/write, explicit bootstrap provenance metadata, and stricter Git root detection so repository/workspace resolution stays deterministic.

### File List

- _bmad-output/implementation-artifacts/1-3-initialize-a-repository-as-a-durable-project-workspace.md
- _bmad-output/implementation-artifacts/sprint-status.yaml
- README.md
- docs/api-contracts.md
- docs/data-models.md
- docs/development-guide.md
- src/application/init.rs
- src/application/mod.rs
- src/domain/init.rs
- src/domain/mod.rs
- src/domain/workspace.rs
- src/main.rs
- src/storage/mod.rs

### Change Log

- 2026-03-07: Created Story 1.3 from sprint context and scoped durable repository/workspace initialization to the current scaffold.
- 2026-03-07: Implemented Tokenizor-owned local registry persistence for repository/workspace initialization, idempotent replay reporting, optional explicit init path handling, and matching operator documentation updates.
- 2026-03-07: Hardened Story 1.3 after review with atomic registry writes, lock-based concurrent update protection, explicit local-bootstrap provenance metadata, stricter Git marker validation, and a clean re-review.

## Senior Developer Review (AI)

### Reviewer

GPT-5 Codex

### Date

2026-03-07

### Outcome

Approved after follow-up fixes.

### Notes

- Re-reviewed the local registry authority boundary, deterministic path resolution, and `init` failure contract.
- Confirmed the registry now uses a locked read/modify/write path with atomic replacement semantics for the snapshot file.
- Confirmed `.git` root detection now requires a real Git marker instead of any arbitrary `.git` entry.
- Confirmed the snapshot schema now advertises local bootstrap provenance instead of silently looking like the final authoritative control plane.
- Re-ran `cargo fmt` and `cargo test`; all tests passed.
