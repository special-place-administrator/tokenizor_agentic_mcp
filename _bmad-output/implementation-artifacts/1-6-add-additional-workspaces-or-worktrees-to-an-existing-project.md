# Story 1.6: Add Additional Workspaces or Worktrees to an Existing Project

Status: done

<!-- Note: Validation is optional. Run validate-create-story for quality check before dev-story. -->

## Story

As an operator,
I want to associate additional workspaces or worktrees with an existing project,
so that Tokenizor preserves one durable project identity across related working copies.

## Acceptance Criteria

1. Given an added workspace resolves to the same underlying Git project/worktree family as an existing project
   When registration runs
   Then Tokenizor attaches it to the existing project
   And preserves a distinct workspace identity for that working copy

2. Given an added workspace does not resolve to the same underlying project identity
   When registration runs
   Then Tokenizor does not attach it to the existing project
   And it fails explicitly with guidance that separate initialization is required

3. Given more than one existing project could match
   When attachment is attempted
   Then Tokenizor fails deterministically
   And reports the candidate project/workspace paths, not only ids

4. Given the same workspace is attached again with equivalent inputs
   When the operation is replayed
   Then Tokenizor reuses the existing association
   And it does not mint a duplicate workspace or duplicate attachment

5. Given multiple workspaces are associated with one project
   When I inspect the registry
   Then all linked workspaces are shown clearly
   And their relationship to the shared project is explicit

## Tasks / Subtasks

- [x] Define and implement the bootstrap-era project identity heuristic explicitly. (AC: 1, 2, 3)
  - [x] Document one concrete local-bootstrap matching rule for when an added workspace belongs to an existing Git project/worktree family.
  - [x] Refactor the current registration target resolution so shared project identity is not derived only from the workspace root path.
  - [x] Reuse an existing project identity only when the new workspace matches that documented canonical Git project identity.
  - [x] Fail explicitly when Tokenizor cannot determine exactly one existing project association.

- [x] Extend the mutating registration flow to add a workspace without duplicating the project. (AC: 1, 2, 4)
  - [x] Preserve workspace-specific identity for each registered workspace root while keeping the shared project identity stable.
  - [x] Make replay of the same attach request idempotent for an already attached workspace.
  - [x] Keep registry writes atomic/locked and preserve the existing local-bootstrap provenance fields.
  - [x] Do not introduce a second registry or provider-owned source of truth.

- [x] Preserve a clear registry inspection model for multi-workspace projects. (AC: 5)
  - [x] Ensure `inspect` shows multiple workspaces grouped under one shared project deterministically.
  - [x] Keep orphan or conflict states explicit instead of silently regrouping or auto-healing them.

- [x] Add deterministic tests and operator docs for multi-workspace registration. (AC: 1, 2, 3, 4, 5)
  - [x] Cover registering an additional worktree/workspace to an existing project under the documented canonical Git identity rule.
  - [x] Cover idempotent re-attach of the same workspace with equivalent inputs.
  - [x] Cover unrelated-project separation and explicit failure or separate-init requirement.
  - [x] Cover ambiguity failure that reports candidate project/workspace paths, not just ids.
  - [x] Update operator-facing docs and CLI contract notes so same-project multi-workspace behavior is discoverable and scriptable.

## Dev Notes

### Story Requirements

- Keep Story 1.6 scoped to preserving one Tokenizor project identity across related local workspaces/worktrees.
- Treat Tokenizor's local bootstrap registry as the current authority for this slice; do not expand into authoritative SpacetimeDB-backed identity persistence yet.
- Keep conflict and failure states explicit. Do not silently choose a project when multiple associations are plausible.
- Preserve current operator behavior where bootstrap state remains scriptable JSON and local-first.
- Do not leave the matching rule abstract in implementation. This story must define and document one bootstrap-era project identity heuristic before coding.

### Current Implementation Baseline

- `src/main.rs` currently exposes `run`, `doctor`, `init`, `inspect`, and `resolve`.
- The only current mutating registration path is `init`, which flows through `InitializationService::initialize_repository`.
- `initialize_repository()` currently resolves one `repo_root`, builds one `Repository`, then builds one `Workspace`, and upserts both into the local bootstrap registry.
- `inspect` already groups workspaces by `repo_id`, and `resolve` already treats overlapping workspace matches as explicit conflicts.

### Developer Context

- The current implementation will duplicate project identity across related worktrees because `build_repository()` derives `repo_id` from `root_uri`, and `root_uri` currently comes from the resolved repository root path. Different worktree roots therefore mint different `repo_id` values even when they represent one underlying project. [Source: src/application/init.rs]
- `build_workspace()` already derives `workspace_id` from the workspace root path, which is appropriate to keep distinct per working copy. [Source: src/application/init.rs]
- Story 1.6 should therefore separate shared project identity from workspace-root identity, without weakening the registry durability hardening from Story 1.3 or the explicit conflict behavior from Story 1.5.
- The registry remains intentionally local bootstrap state with explicit provenance fields (`registry_kind`, `authority_mode`, `control_plane_backend`), so any new identity fields or fingerprints should reinforce that transitional model rather than blur it.
- For this local-bootstrap slice, the story should define a concrete canonical Git project identity rule and use that rule consistently for attach, reject, and ambiguity cases. Path similarity alone is not acceptable.

### Technical Requirements

- Never mint an unrelated duplicate project record for the same underlying Git project.
- Keep workspace/worktree association deterministic and based on local Tokenizor-owned evidence, not provider/client hints.
- Define and document the bootstrap-era project identity heuristic explicitly in the implementation. For this story, attach only when the added workspace resolves to the same canonical Git project identity as an existing project.
- Do not infer shared-project association from path similarity alone.
- Unknown, ambiguous, or unsupported association cases must fail explicitly.
- Ambiguity errors must report candidate project/workspace paths, not only ids.
- Preserve machine-readable CLI output and stable `inspect` grouping semantics.
- Keep per-workspace identity distinct from shared project identity so multiple workspaces remain individually addressable.

### Architecture Compliance

- Preserve the layered flow `main.rs` -> `application` -> `domain/storage`.
- Keep Tokenizor authoritative for project/workspace identity; provider clients remain consumers only.
- Reuse the existing local bootstrap registry path and schema lineage; do not add a competing local authority.
- Keep migration/update cleanup beyond the initial association flow out of scope for Story 1.7.

### Library / Framework Requirements

- Stay on the current Rust dependency set in `Cargo.toml` unless implementation proves a new crate is required.
- Continue using `serde` / `serde_json` for machine-readable CLI output and registry snapshots.
- Keep the current lightweight CLI command parsing style unless there is a compelling reason to change it.
- No network or live SpacetimeDB dependency should be introduced for this story's tests.

### File Structure Requirements

- Likely files to edit:
  - `src/application/init.rs`
  - `src/domain/repository.rs`
  - `src/domain/workspace.rs`
  - `src/domain/init.rs`
  - `src/domain/registry.rs`
  - `src/main.rs`
- Likely docs to update:
  - `README.md`
  - `docs/api-contracts.md`
  - `docs/data-models.md`
  - `docs/development-guide.md`
- A small new domain/helper module is acceptable if it sharpens shared-project identity or association semantics cleanly.

### Testing Requirements

- Add deterministic automated coverage for:
  - registering an additional worktree/workspace to an existing project under the documented canonical Git identity rule without minting a second project
  - idempotent re-attach of an already attached workspace
  - keeping unrelated repositories as distinct projects
  - explicit failure when association is ambiguous or cannot be determined safely
  - `inspect` showing one project with multiple linked workspaces in stable order
- Keep tests hermetic and filesystem-local. Do not depend on live SpacetimeDB or external Git commands.
- Reuse the existing registry lock/write test style where possible so concurrency/durability guarantees are not regressed.

### Previous Story Intelligence

- Story 1.3 hardened the bootstrap registry with atomic replacement, locking, strict Git marker validation, and explicit provenance fields. Build on that instead of inventing new persistence behavior.
- Story 1.4 already made registry inspection read-only, machine-readable, and grouped by shared project identity. Story 1.6 should preserve that clarity while making the grouping meaningful for real multi-workspace projects.
- Story 1.5 intentionally kept active-context resolution explicit and conflict-driven. Do not weaken that by silently preferring one overlapping workspace over another.

### Git Intelligence Summary

- Git is still non-informative in this repository state because the full tree shows as untracked, so rely on the live source and story artifacts rather than commit history.

### Latest Technical Information

- No external web research is required for Story 1.6. The relevant technical surface is the current local Rust/Serde/bootstrap-registry implementation already in this repository.

### Project Context Reference

- No `project-context.md` was found in the repository.
- Use `docs/project-overview.md`, `docs/provider_cli_runtime_architecture.md`, `docs/data-models.md`, and `docs/api-contracts.md` as the current brownfield references.

### References

- [Source: _bmad-output/planning-artifacts/epics.md#Story 1.6: Add Additional Workspaces or Worktrees to an Existing Project]
- [Source: _bmad-output/planning-artifacts/prd.md#Repository & Workspace Lifecycle]
- [Source: _bmad-output/planning-artifacts/architecture.md#Project Context Analysis]
- [Source: _bmad-output/planning-artifacts/architecture.md#Requirements to Structure Mapping]
- [Source: docs/project-overview.md#ADR-006: Project and Workspace Tracking Is Required]
- [Source: docs/provider_cli_runtime_architecture.md#Project Tracking Model]
- [Source: docs/provider_cli_runtime_architecture.md#Resolution algorithm]
- [Source: docs/data-models.md#Current Implemented Models]
- [Source: docs/api-contracts.md#CLI Commands]
- [Source: src/application/init.rs]
- [Source: src/domain/repository.rs]
- [Source: src/domain/workspace.rs]
- [Source: src/domain/registry.rs]
- [Source: src/main.rs]

## Dev Agent Record

### Agent Model Used

GPT-5 Codex

### Debug Log References

- Missing BMAD validator task noted during story generation: `_bmad/core/tasks/validate-workflow.xml` is absent in this install.

### Completion Notes List

- Implemented the bootstrap-era Git project identity rule as the normalized shared Git common-directory path and persisted it on repository records.
- Updated `init` to reuse an existing project for matching worktrees, keeping unrelated Git projects separate and preserving distinct workspace identities.
- Added a new `attach` CLI path for attach-only request mode so unrelated targets fail explicitly with separate-initialization guidance.
- Added deterministic tests for worktree attachment, idempotent re-attach, unrelated-project separation, and ambiguous legacy duplicate detection.
- Updated operator docs and data-model references for `attach`, `project_identity`, and `project_identity_kind`.
- Added end-to-end read-path coverage proving that an attached worktree is surfaced correctly through both `inspect` and `resolve`.

### File List

- _bmad-output/implementation-artifacts/1-6-add-additional-workspaces-or-worktrees-to-an-existing-project.md
- _bmad-output/implementation-artifacts/sprint-status.yaml
- README.md
- docs/api-contracts.md
- docs/data-models.md
- docs/development-guide.md
- src/application/init.rs
- src/application/mod.rs
- src/domain/mod.rs
- src/domain/repository.rs
- src/main.rs

### Change Log

- 2026-03-07: Created Story 1.6 from sprint context, current source analysis, and prior Epic 1 implementation learnings.
- 2026-03-07: Implemented canonical Git common-directory matching, attach-only workspace registration, multi-workspace bootstrap identity persistence, and matching docs/tests.
- 2026-03-07: Closed the 1.6 review follow-up by adding end-to-end `attach -> inspect` and `attach -> resolve` coverage.

## Senior Developer Review (AI)

### Reviewer

GPT-5 Codex

### Date

2026-03-07

### Outcome

Approved after follow-up test hardening.

### Notes

- Added end-to-end read-path tests proving that an attached worktree is surfaced correctly through `inspect` and `resolve`.
- Kept the legacy pre-1.6 registry identity-drift risk out of Story 1.6 because it is a migration/update concern.
- Carry that deferred migration case into Story 1.7: if the original pre-1.6 checkout path is gone, legacy registry hydration can still reconstruct divergent project identities and split one logical project into duplicates.
