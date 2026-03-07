# Story 1.5: Resolve the Active Workspace from Context or Override

Status: done

<!-- Note: Validation is optional. Run validate-create-story for quality check before dev-story. -->

## Story

As an AI coding workflow,
I want Tokenizor to resolve the active project and workspace from the current directory or an explicit override,
so that retrieval and operations target the correct repository context without manual rediscovery.

## Acceptance Criteria

1. Given the current working directory belongs to a registered workspace
   When active context resolution is requested
   Then Tokenizor returns the matching project and workspace as authoritative context
   And provider clients do not become the source of truth for that resolution

2. Given an explicit override is provided
   When resolution is requested
   Then Tokenizor uses the override deterministically
   And it reports an error if the override is unknown or conflicts with registered state

## Tasks / Subtasks

- [x] Add a machine-readable active-context resolution model. (AC: 1, 2)
  - [x] Represent whether resolution came from the current directory or an explicit override.
  - [x] Include the matched repository/workspace plus local bootstrap registry provenance.
  - [x] Keep conflict and unknown-context states as explicit errors rather than silent fallback.

- [x] Implement active workspace resolution against the current local registry. (AC: 1, 2)
  - [x] Reuse the Story 1.3/1.4 local bootstrap registry reader rather than introducing provider-owned context.
  - [x] Match the requested directory against registered workspaces deterministically.
  - [x] Fail explicitly when no registered workspace matches or multiple registered workspaces match the same requested path.

- [x] Wire a dedicated CLI entrypoint for active-context resolution. (AC: 1, 2)
  - [x] Add a new command that resolves from the current directory by default.
  - [x] Allow an optional explicit path override for deterministic resolution.
  - [x] Return machine-readable JSON on success and a non-zero error on unknown/conflicting overrides.

- [x] Cover the resolution flow with deterministic tests and update operator docs. (AC: 1, 2)
  - [x] Add unit coverage for current-directory resolution, explicit override resolution, unknown override failure, and conflicting workspace failure.
  - [x] Update CLI/docs references so the active-context command and override semantics are discoverable.

## Dev Notes

### Story Requirements

- Scope this story to read-only active workspace resolution only.
- Keep Tokenizor authoritative for context resolution by reading the local bootstrap registry introduced in Story 1.3.
- Do not expand into provider bindings, indexing, or workspace mutation flows yet.
- Treat conflicting matches as errors, not heuristics, for this slice.

### Current Implementation Baseline

- `src/main.rs` currently exposes `run`, `doctor`, `init`, and `inspect`.
- Story 1.4 already exposes a read-only machine-readable registry view.
- Story 1.3 already hardened registry loading, canonical path handling, and strict Git-root detection.
- No active-context resolution contract exists yet in the domain or CLI layers.

### Developer Context

- Story 1.3 introduced the local bootstrap registry and explicit provenance that it is not yet the final SpacetimeDB control plane.
- Story 1.4 exposed that registry read-only through a scriptable `inspect` CLI command.
- Story 1.5 should build on the same registry reader so active context comes from Tokenizor-owned state, not from provider clients or ad hoc path guessing.

### Technical Requirements

- Resolve from `std::env::current_dir()` when no override is supplied.
- Support an optional explicit path override for deterministic resolution.
- Treat a requested directory as matching a workspace when it is the workspace root or a descendant of it.
- Error when zero or multiple registered workspaces match the requested path.
- Return a machine-readable JSON report on success.

### Architecture Compliance

- Preserve the layered flow `main.rs` -> `application` -> `domain/storage`.
- Keep provider clients out of the authority path for context resolution.
- Reuse the existing local bootstrap registry path; do not add another source of truth.
- Keep this story read-only and separate from bootstrap/readiness mutation behavior.

### Library / Framework Requirements

- Stay on the current dependency set in `Cargo.toml`.
- Continue using `serde` / `serde_json` for machine-readable CLI output.
- Do not add a CLI parsing framework for this command.

### File Structure Requirements

- Prefer edits in:
  - `src/main.rs`
  - `src/application/mod.rs`
  - `src/application/init.rs`
  - `src/domain/mod.rs`
- Minimal new domain files are acceptable if they sharpen the active-context contract:
  - `src/domain/context.rs`

### Testing Requirements

- Add deterministic unit coverage for:
  - current-directory resolution
  - explicit override resolution
  - unknown override failure
  - conflicting workspace failure
- Keep tests hermetic and local; no live SpacetimeDB runtime.

### Previous Story Intelligence

- Story 1.4 kept registry inspection read-only and independent from SpacetimeDB readiness.
- Story 1.3 already established canonical path normalization and strict `.git` validation; reuse that work instead of duplicating path-resolution logic.
- The local bootstrap registry remains transitional, but it is still the authoritative source for this slice's active workspace resolution.

### Git Intelligence Summary

- Git remains non-informative in this repo state because the full tree shows as untracked.
- Use the live Story 1.3 and Story 1.4 implementations as the continuity reference.

### Latest Technical Information

- The current CLI surface is `cargo run -- run`, `cargo run -- doctor`, `cargo run -- init`, and `cargo run -- inspect`.
- The current registry model already carries explicit local-bootstrap provenance that should flow through the active-context result.

### Project Context Reference

- No `project-context.md` was found in the repository.
- Use `docs/index.md`, `docs/development-guide.md`, `docs/data-models.md`, and `docs/api-contracts.md` as the current brownfield references.

### References

- [Source: _bmad-output/planning-artifacts/epics.md#Story 1.5: Resolve the Active Workspace from Context or Override]
- [Source: _bmad-output/planning-artifacts/prd.md#FR4]
- [Source: _bmad-output/planning-artifacts/architecture.md#Requirements to Structure Mapping]
- [Source: docs/project-overview.md#ADR-006: Project and Workspace Tracking Is Required]
- [Source: docs/api-contracts.md#CLI Commands]
- [Source: src/main.rs]
- [Source: src/application/mod.rs]
- [Source: src/application/init.rs]

## Dev Agent Record

### Agent Model Used

GPT-5 Codex

### Debug Log References

- Missing BMAD validator task noted during story generation: `_bmad/core/tasks/validate-workflow.xml` is absent in this install.

### Completion Notes List

- Added a read-only `resolve` CLI command that returns the active repository/workspace context from the current directory or an explicit directory override.
- Introduced `ActiveWorkspaceContext` and `ContextResolutionMode` so resolution stays machine-readable and explicit about authority provenance.
- Reused the local bootstrap registry reader for active context resolution and made unknown or overlapping workspace matches fail explicitly instead of falling back heuristically.
- Added deterministic tests for current-directory request shaping, explicit override success, unknown override failure, and conflicting workspace failure.
- Verified with `cargo fmt`, `cargo test`, and `cargo run -- resolve` on 2026-03-07.
- Tightened Story 1.5 after review so nonexistent explicit overrides now return deterministic Tokenizor resolution failures, the default current-directory path has end-to-end automated coverage, and conflict errors include workspace root paths for operator cleanup.
- Deferred one architecture follow-up for a later story: future non-CLI callers should pass context explicitly instead of relying on process `current_dir()`.

### File List

- _bmad-output/implementation-artifacts/1-5-resolve-the-active-workspace-from-context-or-override.md
- _bmad-output/implementation-artifacts/sprint-status.yaml
- README.md
- docs/api-contracts.md
- docs/data-models.md
- docs/development-guide.md
- src/application/init.rs
- src/application/mod.rs
- src/domain/context.rs
- src/domain/mod.rs
- src/main.rs

### Change Log

- 2026-03-07: Created Story 1.5 from sprint context and Stories 1.3/1.4 implementation learnings.
- 2026-03-07: Implemented active workspace resolution from current-directory or explicit-path context, added a dedicated `resolve` CLI command, and updated docs/tests for the new read-only authority path.
- 2026-03-07: Resolved Story 1.5 review findings around deterministic unknown-override failures, cwd-based end-to-end coverage, and conflict error ergonomics; recorded the later non-CLI context-source follow-up.

## Senior Developer Review (AI)

### Reviewer

GPT-5 Codex

### Date

2026-03-07

### Outcome

Approved after follow-up fixes.

### Notes

- Normalized nonexistent explicit overrides into deterministic Tokenizor resolution failures instead of raw OS I/O noise.
- Added end-to-end automated coverage for `resolve_active_context(None)` with a changed current directory.
- Expanded conflicting-workspace errors to include root paths as well as workspace ids.
- Deferred one architectural follow-up: when a non-CLI caller is added, it should pass context explicitly instead of relying on process `current_dir()`.
