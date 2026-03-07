# Story 1.4: Inspect Known Projects and Workspaces

Status: done

<!-- Note: Validation is optional. Run validate-create-story for quality check before dev-story. -->

## Story

As an operator,
I want to inspect registered projects and workspaces,
so that I can understand what Tokenizor currently knows and manage it safely.

## Acceptance Criteria

1. Given one or more projects and workspaces are registered
   When I request the current registry view
   Then Tokenizor lists known projects and associated workspaces in scriptable output
   And the output is clear enough for advanced local maintenance

2. Given no projects are registered
   When I inspect the registry
   Then Tokenizor returns an explicit empty-state response
   And it does not imply hidden or partial state

## Tasks / Subtasks

- [x] Add a machine-readable registry inspection model for the current scaffold. (AC: 1, 2)
  - [x] Represent the local bootstrap registry provenance explicitly in the inspection output.
  - [x] Group associated workspaces under their registered repository/project identity.
  - [x] Include explicit empty-state fields so operators can distinguish no registrations from partial or failed loading.

- [x] Implement a read-only registry inspection workflow in the application layer. (AC: 1, 2)
  - [x] Reuse the Story 1.3 local bootstrap registry reader rather than inventing a second persistence path.
  - [x] Keep inspection read-only and independent from deployment/bootstrap mutations.
  - [x] Return a deterministic ordering suitable for scriptable operator use.

- [x] Wire a dedicated CLI entrypoint for registry inspection. (AC: 1, 2)
  - [x] Add a new command that prints the current registry view as JSON.
  - [x] Keep the empty-state contract explicit and non-erroring when no registrations exist.
  - [x] Avoid coupling this inspection command to control-plane readiness blockers.

- [x] Cover the inspection flow with deterministic tests and update operator docs. (AC: 1, 2)
  - [x] Add unit coverage for populated registry grouping, empty-state output, and stable ordering.
  - [x] Update CLI/docs references so operators can discover and use the new inspection command.

## Dev Notes

### Story Requirements

- Scope this story to inspecting known projects/workspaces only.
- Keep the source of truth for this slice aligned with Story 1.3's local bootstrap registry.
- Do not expand into active workspace resolution, mutation, migration, or deletion flows yet.
- The output must stay scriptable and explicit about whether the registry is empty.

### Current Implementation Baseline

- `src/main.rs` currently exposes `run`, `doctor`, and `init`, but no registry inspection command.
- Story 1.3 introduced the local bootstrap registry under `.tokenizor/control-plane/project-workspace-registry.json`.
- `src/application/init.rs` already owns deterministic registry loading, locking, and snapshot provenance.
- The current docs still describe the CLI as `run`, `doctor`, and `init` only.

### Developer Context

- Story 1.1 established operator-readable readiness reporting.
- Story 1.2 aligned guarded startup with deployment readiness and kept MCP health narrower on purpose.
- Story 1.3 introduced the local bootstrap registry and explicitly marked it as scaffold state that later yields to SpacetimeDB-backed authoritative persistence.
- Story 1.4 should expose that current registry state cleanly without mutating it or pretending it is already the final control plane.

### Technical Requirements

- Prefer a dedicated read-only inspection command over overloading `doctor` or `init`.
- Reuse the existing registry schema and provenance metadata from Story 1.3.
- Preserve deterministic ordering in the JSON output.
- The empty state should be a successful response with clear counts and an empty projects collection.
- Do not gate inspection on SpacetimeDB reachability or CLI presence; this is local maintenance visibility.

### Architecture Compliance

- Preserve the layered flow `main.rs` -> `application` -> `domain/storage`.
- Keep registry inspection in the application/domain layers rather than ad hoc JSON formatting in `main.rs`.
- Treat the local bootstrap registry as transitional/local state, not the final authoritative control plane.
- Avoid introducing a separate registry file or a second project/workspace persistence model.

### Library / Framework Requirements

- Stay on the current dependency set in `Cargo.toml`.
- Continue using `serde` / `serde_json` for machine-readable CLI output.
- Do not add a CLI framework just for this command.

### File Structure Requirements

- Prefer edits in:
  - `src/main.rs`
  - `src/application/mod.rs`
  - `src/application/init.rs`
  - `src/domain/mod.rs`
- Minimal new domain files are acceptable if they sharpen the registry inspection contract:
  - `src/domain/registry.rs`

### Testing Requirements

- Add deterministic unit coverage for:
  - populated registry inspection grouped by repository
  - explicit empty-state output when no registrations exist
  - stable ordering of repositories/workspaces in the inspection output
- Keep tests hermetic and local; no live SpacetimeDB runtime.

### Previous Story Intelligence

- Story 1.3 hardened the local bootstrap registry with atomic replace writes, a cross-process lock, schema provenance metadata, and strict Git root detection.
- Any 1.4 inspection flow should reuse that registry reader and schema rather than duplicating registry parsing logic elsewhere.
- The `init` operator contract already prints JSON and exits non-zero only for remaining deployment blockers; 1.4 should stay read-only and not inherit those blockers.

### Git Intelligence Summary

- Git remains non-informative in this repo state because the full tree shows as untracked.
- Use the live Story 1.3 implementation and tests as the immediate continuity reference.

### Latest Technical Information

- The current CLI surface is still `cargo run -- run`, `cargo run -- doctor`, and `cargo run -- init`.
- The local bootstrap registry now carries explicit provenance fields identifying it as local bootstrap state rather than final authoritative SpacetimeDB persistence.

### Project Context Reference

- No `project-context.md` was found in the repository.
- Use `docs/index.md`, `docs/development-guide.md`, `docs/data-models.md`, and `docs/api-contracts.md` as the current brownfield references for the implemented scaffold.

### References

- [Source: _bmad-output/planning-artifacts/epics.md#Story 1.4: Inspect Known Projects and Workspaces]
- [Source: _bmad-output/planning-artifacts/prd.md#FR5]
- [Source: _bmad-output/planning-artifacts/implementation-readiness-report-2026-03-07.md#FR39]
- [Source: _bmad-output/planning-artifacts/architecture.md#Requirements to Structure Mapping]
- [Source: docs/api-contracts.md#CLI Commands]
- [Source: docs/data-models.md#Current Implemented Models]
- [Source: README.md#Commands]
- [Source: src/main.rs]
- [Source: src/application/mod.rs]
- [Source: src/application/init.rs]

## Dev Agent Record

### Agent Model Used

GPT-5 Codex

### Debug Log References

- Missing BMAD validator task noted during story generation: `_bmad/core/tasks/validate-workflow.xml` is absent in this install.

### Completion Notes List

- Added a read-only `inspect` CLI command that prints the current local bootstrap registry view as JSON without depending on SpacetimeDB readiness.
- Introduced `RegistryView` / `RegisteredProject` domain types so registry inspection stays machine-readable and explicit about provenance, empty state, and orphaned workspaces.
- Reused the Story 1.3 registry reader in `src/application/init.rs` and kept inspection separate from bootstrap mutations.
- Added deterministic tests for empty-state inspection, grouped/stable ordering, and explicit orphan workspace surfacing.
- Verified with `cargo fmt`, `cargo test`, and `cargo run -- inspect` on 2026-03-07.

### File List

- _bmad-output/implementation-artifacts/1-4-inspect-known-projects-and-workspaces.md
- _bmad-output/implementation-artifacts/sprint-status.yaml
- README.md
- docs/api-contracts.md
- docs/data-models.md
- docs/development-guide.md
- src/application/init.rs
- src/application/mod.rs
- src/domain/mod.rs
- src/domain/registry.rs
- src/main.rs

### Change Log

- 2026-03-07: Created Story 1.4 from sprint context and Story 1.3 implementation learnings.
- 2026-03-07: Implemented read-only registry inspection with a dedicated `inspect` CLI command, grouped registry view output, empty-state reporting, and matching documentation/test coverage.

## Senior Developer Review (AI)

### Reviewer

Claude Opus 4.6

### Date

2026-03-07

### Outcome

Approve

### Summary

- Verified Story 1.4 implementation against acceptance criteria and changed source files.
- The `inspect` command follows the established CLI pattern cleanly: `main.rs` delegates to `ApplicationContext::inspect_registry()`, which delegates to `InitializationService::inspect_registry()`.
- `build_registry_view()` correctly groups workspaces under their parent repository using a BTreeMap for deterministic ordering, and explicitly surfaces orphan workspaces.
- The `RegistryView` domain type includes explicit provenance fields (`registry_kind`, `authority_mode`, `control_plane_backend`), computed summary fields (`empty`, `project_count`, `workspace_count`, `orphan_workspace_count`), and derives Serialize/Deserialize for machine-readable JSON output.
- Read-only behavior confirmed: no lock acquisition, no mutation, no bootstrap/deployment dependency. Inspection is independent from SpacetimeDB readiness, as required.
- All 59 tests pass (56 library + 3 binary). Four Story 1.4-specific tests cover: empty state, grouped/stable ordering, orphan workspace surfacing, and end-to-end attached worktree inspection.

### Findings

- No blocking issues found.
- Code is clean, well-structured, and follows established patterns from Stories 1.1-1.3.
- Test coverage is thorough for the inspection contract.

### Deferred Follow-ups

- None. Story 1.4 is a self-contained read-only inspection surface with no deferred architectural decisions.
