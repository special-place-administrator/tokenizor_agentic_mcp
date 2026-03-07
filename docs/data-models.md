# tokenizor_agentic_mcp - Data and Domain Models

**Date:** 2026-03-07

## Model Framing

This repository currently contains domain and storage models rather than application business data models in the usual CRUD sense. The important distinction is:

- **current implemented models:** Rust domain structs already present in code
- **target-state models:** broader operational entities described in the architecture docs

## Current Implemented Models

### Repository

Tracks one logical repository registration.

**Fields**

- `repo_id`
- `kind`
- `root_uri`
- `project_identity`
- `project_identity_kind`
- `default_branch`
- `last_known_revision`
- `status`

`project_identity_kind` currently distinguishes the bootstrap-era matching rule:

- `git_common_dir`: normalized shared Git common-directory path for Git repositories/worktrees
- `local_root_path`: normalized local root path for non-Git folders
- `legacy_root_uri`: fallback value for older registry entries that predate the explicit identity field

### Workspace

Tracks one durable workspace/worktree registration bound to a repository identity.

**Fields**

- `workspace_id`
- `repo_id`
- `root_uri`
- `status`

### IndexRun

Tracks a single indexing or repair run.

**Fields**

- `run_id`
- `repo_id`
- `mode`
- `status`
- `requested_at_unix_ms`
- `started_at_unix_ms`
- `finished_at_unix_ms`
- `idempotency_key`
- `request_hash`
- `checkpoint_cursor`
- `error_summary`

### Checkpoint

Supports resumable progress tracking.

**Fields**

- `run_id`
- `cursor`
- `files_processed`
- `symbols_written`
- `created_at_unix_ms`

### IdempotencyRecord

Tracks idempotent mutation state.

**Fields**

- `operation`
- `idempotency_key`
- `request_hash`
- `status`
- `result_ref`
- `created_at_unix_ms`
- `expires_at_unix_ms`

### Health and Deployment Types

Current health/reporting model family:

- `HealthStatus`
- `ComponentHealth`
- `ServiceIdentity`
- `HealthReport`
- `DeploymentReport`

These are used to represent readiness and degraded state in both MCP and CLI reporting.

### StoredBlob

Represents CAS write outcomes.

**Fields**

- `blob_id`
- `byte_len`
- `was_created`

### InitializationReport

Represents the current `init` and `attach` command outcome.

**Fields**

- `initialized_at_unix_ms`
- `target_path`
- `registry_path`
- `repository`
- `workspace`
- `deployment`

### MigrationReport

Represents the current `migrate` command outcome for explicit bootstrap-registry reconciliation.

**Fields**

- `migrated_at_unix_ms`
- `registry_path`
- `request`
- `changed`
- `summary`
- `migrated`
- `updated`
- `unchanged`
- `unresolved`

`request` captures whether the operator ran a scan-only migration or an explicit `<from-path> <to-path>` update request.

`migrated`, `updated`, and `unchanged` hold machine-readable per-entity records for repositories and workspaces.

`unresolved` holds deterministic, operator-actionable failure or review items rather than silently rewriting state.

### RegistryView

Represents the current `inspect` command outcome for the local bootstrap registry.

**Fields**

- `inspected_at_unix_ms`
- `registry_path`
- `registry_kind`
- `authority_mode`
- `control_plane_backend`
- `empty`
- `project_count`
- `workspace_count`
- `orphan_workspace_count`
- `projects`
- `orphan_workspaces`

### ActiveWorkspaceContext

Represents the current `resolve` command outcome.

**Fields**

- `resolved_at_unix_ms`
- `requested_path`
- `resolution_mode`
- `registry_path`
- `registry_kind`
- `authority_mode`
- `control_plane_backend`
- `repository`
- `workspace`

## Current Enum Families

Important current enums include:

- `RepositoryKind`
- `RepositoryStatus`
- `WorkspaceStatus`
- `IndexRunMode`
- `IndexRunStatus`
- `IdempotencyStatus`
- `HealthStatus`

These already encode part of the system lifecycle semantics the full product will build on.

## Target-State Domain Model Direction

The architecture documents indicate the intended model set should expand to include or mature:

- `Repository`
- `IndexRun`
- `FileRecord`
- `SymbolRecord`
- `Lease`
- `Checkpoint`
- `IdempotencyRecord`
- `HealthEvent`
- project/workspace tracking entities
- provider binding metadata

## Ownership Split

### SpacetimeDB-owned structured state

Target-state authoritative control-plane data should include:

- repositories
- runs
- checkpoints
- leases
- idempotency records
- health events
- repair history
- file metadata
- symbol metadata

### Local CAS-owned raw content state

Raw file bytes and large derived artifacts should remain outside the control plane.

This split matters because:

- exact bytes are required for verified retrieval
- spans depend on byte-level correctness
- raw blob storage and structured operational state have different durability needs

## Model Maturity Assessment

Current model maturity is strongest around:

- run lifecycle scaffolding
- health/reporting
- storage outcome representation
- idempotency and checkpoint concepts

Missing or incomplete target-state model areas include:

- file metadata records
- symbol metadata records
- lease ownership
- repair-event history
- project/workspace identity model
- provider binding model

---

_Generated using BMAD Method `document-project` workflow_
