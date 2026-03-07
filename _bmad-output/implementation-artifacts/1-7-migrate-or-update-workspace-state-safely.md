# Story 1.7: Migrate or Update Workspace State Safely

Status: done

<!-- Note: Validation is optional. Run validate-create-story for quality check before dev-story. -->

## Story

As an operator,
I want to migrate or update workspace state when lifecycle changes occur,
so that Tokenizor remains accurate without corrupting durable context.

## Acceptance Criteria

1. Given a pre-1.6 local bootstrap registry entry does not yet have the canonical project identity fields
   When I run the migration flow
   Then Tokenizor upgrades that entry to the current project-identity model where it can prove the result safely
   And preserves continuity for later sessions where possible

2. Given an old checkout path no longer exists but surviving local workspace/worktree evidence can still prove the same underlying project identity
   When migration runs
   Then Tokenizor upgrades the legacy entry safely
   And it does not mint an unrelated duplicate project

3. Given an old checkout path no longer exists and migration cannot prove identity uniquely
   When migration runs
   Then Tokenizor fails or reports the entry as unresolved deterministically
   And it does not silently merge, rewrite, or split project/workspace identity

4. Given a registered workspace path or local lifecycle state has changed and the update can be proven safely
   When I run the migration or update flow
   Then Tokenizor updates durable state safely
   And preserves continuity for later sessions where possible

5. Given the same migration or update request is run again with equivalent local evidence and inputs
   When the migration flow is replayed
   Then Tokenizor returns a deterministic unchanged or already-migrated outcome
   And it does not rewrite, split, merge, or duplicate state

6. Given a requested migration or update cannot be completed safely
   When the operation fails
   Then Tokenizor reports the failure explicitly with actionable guidance
   And it does not silently corrupt project or workspace state

## Tasks / Subtasks

- [x] Add an explicit operator-facing migration/update path for bootstrap registry maintenance. (AC: 1, 2, 3, 4, 5, 6)
  - [x] Add a dedicated `migrate` CLI flow instead of hiding migration inside opportunistic reads.
  - [x] Return machine-readable JSON summarizing migrated, updated, unchanged, and unresolved records.
  - [x] Keep migration/update behavior local-first and independent from provider-owned context.
  - [x] Make repeated equivalent migration runs deterministic and idempotent rather than rewrite-driven.

- [x] Implement deterministic legacy project-identity upgrade for pre-1.6 registry entries. (AC: 1, 2, 3, 5)
  - [x] Upgrade legacy repository records that are missing `project_identity` / `project_identity_kind` when a canonical identity can be proven safely.
  - [x] Use surviving local Git workspace/worktree evidence to recover a shared canonical Git identity when the original checkout path is gone.
  - [x] Do not silently merge or rewrite identity when multiple plausible canonical identities remain.
  - [x] Re-running the same safe upgrade must report unchanged or already-migrated state instead of mutating again.

- [x] Implement safe workspace-state update handling for changed lifecycle paths. (AC: 4, 5, 6)
  - [x] Detect when a registered workspace path is missing, moved, or otherwise lifecycle-stale.
  - [x] Update workspace/project state only when the new mapping is proven from explicit operator-supplied path input or surviving local Git/workspace evidence.
  - [x] Leave unresolved or stale state explicit when safe update is not possible.

- [x] Preserve deterministic failure and review surfaces for ambiguous migration cases. (AC: 3, 6)
  - [x] Surface candidate project/workspace paths and the reason migration could not prove a safe identity decision.
  - [x] Ensure unresolved migration state remains inspectable rather than being hidden or auto-healed.
  - [x] Keep no-silent-merge behavior explicit in both code and operator messaging.

- [x] Add deterministic tests and operator docs for migration/update behavior. (AC: 1, 2, 3, 4, 5, 6)
  - [x] Cover pre-1.6 registry upgrade when current paths still exist.
  - [x] Cover legacy upgrade when the original checkout path is gone but another surviving worktree proves identity.
  - [x] Cover deterministic unresolved/failed migration when identity cannot be proven safely.
  - [x] Cover safe workspace-path update from explicit operator input or surviving local evidence, plus stale-state reporting when proof is insufficient.
  - [x] Cover idempotent rerun of an already-completed migration/update request.
  - [x] Update operator docs and CLI contract notes for the new migration/update flow.

## Dev Notes

### Story Requirements

- Keep Story 1.7 scoped to migration/update behavior for Tokenizor's current local bootstrap registry.
- Treat this as the right home for the deferred 1.6 legacy-registry identity drift risk.
- The migration/update flow must be explicit and operator-visible; do not hide state-changing migration behind normal read paths alone.
- Preserve deterministic no-silent-merge behavior whenever migration cannot prove identity safely.
- Treat migration/update as an idempotent operator flow: equivalent reruns must return stable outcomes without rewriting or duplicating state.

### Current Implementation Baseline

- `src/main.rs` currently exposes `run`, `doctor`, `init`, `attach`, `inspect`, and `resolve`.
- Story 1.6 introduced `project_identity` and `project_identity_kind` on repository records plus the canonical Git common-directory rule for new registrations.
- Legacy registry hydration currently tries to infer missing identity fields opportunistically during snapshot load.
- There is not yet a dedicated `migrate` operator command or an explicit migration/update report model.

### Developer Context

- The specific deferred risk from Story 1.6 is: if a pre-1.6 Git registration is being upgraded and the original checkout path is gone, current hydration can reconstruct divergent identities and later split one logical project into duplicates. Story 1.7 should fix that through an explicit migration/update flow rather than by stretching Story 1.6 further.
- Story 1.6 already established the bootstrap-era canonical Git identity rule as the normalized shared Git common-directory path. Story 1.7 should reuse that rule for legacy upgrade when it can be proven from surviving local evidence.
- `inspect` and `resolve` already assume registry truth is explicit and deterministic. Migration/update work should preserve that assumption instead of burying unresolved state.

### Technical Requirements

- Explicitly include legacy registry upgrade/migration of pre-1.6 project identity in this story.
- Explicitly handle cases where old checkout paths no longer exist.
- If migration cannot prove identity uniquely, do not silently merge; report unresolved or failed migration deterministically.
- For moved workspace-path updates, only accept the new mapping when it is proven from explicit operator-supplied path input or surviving local Git/workspace evidence.
- Keep migration/update decisions based on Tokenizor-owned local evidence, not provider/client hints.
- Keep results machine-readable and operator-actionable.
- Preserve existing registry durability protections: lock, atomic replace, and provenance fields.
- Equivalent reruns of the same migration/update must be deterministic and idempotent.

### Architecture Compliance

- Preserve the layered flow `main.rs` -> `application` -> `domain/storage`.
- Keep Tokenizor authoritative for project/workspace identity and migration truth.
- Reuse the existing local bootstrap registry rather than introducing a new migration side-store.
- Keep authoritative SpacetimeDB migration out of scope; this story is about the current bootstrap-era state model only.

### Library / Framework Requirements

- Stay on the current Rust dependency set in `Cargo.toml` unless implementation proves a new crate is required.
- Continue using `serde` / `serde_json` for machine-readable CLI output and migration/update reports.
- No network or live SpacetimeDB dependency should be introduced for this story's tests.

### File Structure Requirements

- Likely files to edit:
  - `src/application/init.rs`
  - `src/application/mod.rs`
  - `src/domain/init.rs`
  - `src/domain/repository.rs`
  - `src/main.rs`
- Possible additional file:
  - `src/domain/registry.rs`
- Likely docs to update:
  - `README.md`
  - `docs/api-contracts.md`
  - `docs/data-models.md`
  - `docs/development-guide.md`
- A small new domain/report type is acceptable if it sharpens migration/update outcomes cleanly.

### Testing Requirements

- Add deterministic automated coverage for:
  - pre-1.6 registry upgrade with still-existing checkout paths
  - legacy upgrade when the original checkout path is gone but a surviving worktree proves identity
  - deterministic unresolved migration when identity cannot be proven safely
  - no-silent-merge behavior when multiple plausible identities exist
  - safe workspace-state update or explicit stale-state reporting
- Keep tests hermetic and filesystem-local. Do not depend on live SpacetimeDB or external Git commands.
- Preserve coverage around `inspect` and `resolve` so migrated state remains readable and authoritative after the update flow.

### Previous Story Intelligence

- Story 1.3 hardened registry durability with atomic writes, locking, and explicit provenance fields.
- Story 1.5 required context resolution to fail explicitly rather than relying on heuristic fallback.
- Story 1.6 introduced canonical Git common-directory identity for new registrations and explicitly deferred the legacy pre-1.6 identity drift risk to this story.

### Git Intelligence Summary

- Git remains non-informative in this repository state because the full tree shows as untracked.
- Use the live source and Story 1.6 review notes as the continuity reference.

### Latest Technical Information

- No external web research is required for Story 1.7. The relevant technical surface is the current local Rust/bootstrap-registry implementation already in this repository.

### Project Context Reference

- No `project-context.md` was found in the repository.
- Use `docs/project-overview.md`, `docs/provider_cli_runtime_architecture.md`, `docs/data-models.md`, and `docs/api-contracts.md` as the current brownfield references.

### References

- [Source: _bmad-output/planning-artifacts/epics.md#Story 1.7: Migrate or Update Workspace State Safely]
- [Source: _bmad-output/planning-artifacts/prd.md#Repository & Workspace Lifecycle]
- [Source: _bmad-output/planning-artifacts/architecture.md#Data Architecture]
- [Source: _bmad-output/planning-artifacts/architecture.md#API / Protocol & Communication]
- [Source: docs/project-overview.md#ADR-006: Project and Workspace Tracking Is Required]
- [Source: docs/provider_cli_runtime_architecture.md#Project Tracking Model]
- [Source: docs/data-models.md#Current Implemented Models]
- [Source: docs/api-contracts.md#CLI Commands]
- [Source: _bmad-output/implementation-artifacts/1-6-add-additional-workspaces-or-worktrees-to-an-existing-project.md#Senior Developer Review (AI)]
- [Source: src/application/init.rs]
- [Source: src/domain/repository.rs]
- [Source: src/domain/registry.rs]
- [Source: src/main.rs]

## Dev Agent Record

### Agent Model Used

GPT-5 Codex

### Debug Log References

- Missing BMAD validator task noted during story generation: `_bmad/core/tasks/validate-workflow.xml` is absent in this install.
- 2026-03-07: `cargo fmt --all`
- 2026-03-07: `cargo test` (55 library tests, 3 main tests)

### Completion Notes List

- Created Story 1.7 from Epic 1 planning context, current registry identity implementation, and the deferred 1.6 review follow-up.
- Explicitly included legacy pre-1.6 project-identity migration, missing old checkout-path handling, and deterministic no-silent-merge behavior when migration cannot prove identity safely.
- Kept Story 1.7 scoped to bootstrap-era registry migration/update behavior rather than broadening into authoritative SpacetimeDB migration.
- Implemented an explicit `migrate` CLI flow plus a machine-readable `MigrationReport` model for scan and explicit old/new path update requests.
- Removed hidden legacy identity hydration from read paths and made `init`/`attach` fail explicitly when matching legacy bootstrap state requires migration first.
- Added deterministic migration/update coverage for current-path legacy upgrades, surviving-worktree recovery, ambiguous unresolved cases, explicit workspace moves, idempotent reruns, and the legacy-registration guardrail.
- Updated operator-facing docs for the new migration flow and legacy-state behavior across README, API contracts, data models, and the development guide.

### File List

- _bmad-output/implementation-artifacts/1-7-migrate-or-update-workspace-state-safely.md
- _bmad-output/implementation-artifacts/sprint-status.yaml
- README.md
- docs/api-contracts.md
- docs/data-models.md
- docs/development-guide.md
- src/application/init.rs
- src/application/mod.rs
- src/domain/migration.rs
- src/domain/mod.rs
- src/main.rs

### Change Log

- 2026-03-07: Created Story 1.7 from sprint context, Story 1.6 follow-up review notes, and current local bootstrap-registry implementation state.
- 2026-03-07: Implemented explicit registry migration/update flows, deterministic legacy identity upgrade handling, explicit workspace-path updates, and migration guardrails for `init`/`attach`.
- 2026-03-07: Added migration-focused tests plus operator documentation for the new `migrate` CLI contract and JSON report surface.
- 2026-03-07: Code review (Claude Opus 4.6). Fixed 3 MEDIUM issues: replaced 2 production `expect()` calls with proper error propagation, decomposed 402-line `apply_explicit_path_update` into 4 focused functions, added `// SAFETY:` documentation to `unsafe` Windows FFI block. Fixed 3 LOW issues: added test for single-arg `migrate` rejection, documented schema version constants, documented `is_successful` semantics. All 59 tests pass.
