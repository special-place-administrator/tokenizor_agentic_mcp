# tokenizor_agentic_mcp - Interface and API Contracts

**Date:** 2026-03-07

## Contract Framing

This project does not currently expose an HTTP or REST API. The implemented external interfaces today are:

- local CLI commands
- stdio MCP tool surface

For planning purposes, those current interfaces should be distinguished from the broader target-state MCP surface described in the architecture documents.

## Current Implemented Interfaces

### CLI Commands

#### `cargo run -- run`

- **Purpose:** Start the Tokenizor MCP server over stdio
- **Inputs:** environment configuration
- **Output:** long-running stdio MCP session
- **Behavior:** initializes tracing, loads config, initializes local storage, checks runtime readiness, and serves the MCP transport

#### `cargo run -- doctor`

- **Purpose:** Report deployment readiness
- **Inputs:** environment configuration
- **Output:** JSON deployment report to stdout
- **Failure behavior:** exits non-zero if required checks are not ready

#### `cargo run -- init`

- **Purpose:** Bootstrap local storage, register or reuse a durable repository/workspace identity in local Tokenizor bootstrap state, and report remaining blockers
- **Inputs:** environment configuration plus an optional explicit repository/folder path argument
- **Output:** JSON initialization report to stdout
- **Failure behavior:** exits non-zero if required actions remain and now also blocks on matching legacy bootstrap state that must be reconciled explicitly via `migrate`
- **Matching rule:** for Git repositories, the bootstrap-era project identity is the normalized shared Git common-directory path; matching worktrees reuse the existing project identity

#### `cargo run -- attach`

- **Purpose:** Attach a Git workspace/worktree to an already registered project when the canonical Git project identity matches exactly one existing project
- **Inputs:** environment configuration plus an optional explicit workspace/worktree path argument
- **Output:** JSON initialization report to stdout
- **Failure behavior:** exits non-zero when the target does not match an existing project or matches more than one project; the error tells the operator when separate initialization is required and now also blocks on matching legacy bootstrap state that must be reconciled via `migrate`

#### `cargo run -- migrate`

- **Purpose:** Reconcile legacy bootstrap registry state explicitly, including canonical project-identity upgrades and safe workspace/root path updates
- **Inputs:** environment configuration plus either no path arguments or an explicit `<from-path> <to-path>` pair
- **Output:** JSON migration report to stdout
- **Failure behavior:** exits non-zero when unresolved records remain or when the path arguments are invalid; the JSON payload still reports actionable guidance for unresolved records

#### `cargo run -- inspect`

- **Purpose:** Inspect the current local Tokenizor bootstrap registry for known projects and workspaces
- **Inputs:** environment configuration
- **Output:** JSON registry inspection report to stdout
- **Failure behavior:** exits non-zero only if the local registry cannot be read or deserialized

#### `cargo run -- resolve`

- **Purpose:** Resolve the active repository/workspace context from the current directory or an explicit directory override
- **Inputs:** environment configuration plus an optional explicit directory path argument
- **Output:** JSON active-context report to stdout
- **Failure behavior:** exits non-zero if the requested path is unknown or conflicts with registered workspace state

### MCP Tool Surface

#### Tool: `health`

- **Transport:** stdio MCP
- **Purpose:** report runtime health for the MCP server, control plane, and local CAS
- **Current payload behavior:** returns a serialized health report as text content
- **Backed by:** `ApplicationContext` health service and storage/control-plane health checks

## Current Request/Response Model

### Health Report Shape

The current `health` tool returns a JSON-serialized structure conceptually shaped like:

- `checked_at_unix_ms`
- `service`
- `overall_status`
- `components`

Component entries include:

- `name`
- `status`
- `detail`
- `observed_at_unix_ms`

### Initialization Report Shape

The current `init` command returns a JSON-serialized structure conceptually shaped like:

- `initialized_at_unix_ms`
- `target_path`
- `registry_path`
- `repository`
- `workspace`
- `deployment`

Repository/workspace registration entries include:

- `action` (`created` or `reused`)
- `record`

Repository records now also include:

- `project_identity`
- `project_identity_kind`

The nested deployment payload reuses the existing deployment readiness report shape.

The `registry_path` points to a local bootstrap registry owned by Tokenizor for this scaffold slice; it is not the final authoritative SpacetimeDB control-plane model.

`attach` returns this same report shape.

### Migration Report Shape

The current `migrate` command returns a JSON-serialized structure shaped like:

- `migrated_at_unix_ms`
- `registry_path`
- `request`
- `changed`
- `summary`
- `migrated`
- `updated`
- `unchanged`
- `unresolved`

The nested `request` payload includes:

- `mode` (`scan` or `update`)
- `source_path`
- `target_path`

The nested `summary` payload includes counts for:

- `migrated`
- `updated`
- `unchanged`
- `unresolved`

Migration record entries include:

- `entity_kind` (`repository` or `workspace`)
- `entity_id`
- `previous_path`
- `current_path`
- `detail`

Unresolved issue entries include:

- `entity_kind`
- `entity_id`
- `path`
- `detail`
- `guidance`
- `candidate_paths`

### Registry Inspection Report Shape

The current `inspect` command returns a JSON-serialized structure conceptually shaped like:

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

Project entries include:

- `repository`
- `workspaces`

This output is read-only and intentionally independent from deployment readiness so operators can inspect local bootstrap state even when SpacetimeDB is unavailable.

### Active Context Report Shape

The current `resolve` command returns a JSON-serialized structure conceptually shaped like:

- `resolved_at_unix_ms`
- `requested_path`
- `resolution_mode`
- `registry_path`
- `registry_kind`
- `authority_mode`
- `control_plane_backend`
- `repository`
- `workspace`

This output is read-only and sourced from Tokenizor's local bootstrap registry rather than provider-owned context.

## Current Gaps

The following planned interfaces are not yet implemented:

- `index_folder`
- `index_repository`
- `get_index_run`
- `cancel_index_run`
- `checkpoint_now`
- `repair_index`
- `search_symbols`
- `search_text`
- `get_file_outline`
- `get_symbol`
- `get_symbols`
- `get_repo_outline`
- `invalidate_cache`

## Target-State Contract Direction

The architecture docs describe a richer MCP surface over time:

- tools for indexing, querying, health, checkpointing, and repair
- resources for repository outline, health, run status, and symbol metadata
- prompts for codebase audit, failure triage, and architecture mapping

These should be treated as target-state contract direction, not current implementation.

## Reliability Expectations for Future Contracts

Based on the project direction docs, future mutating contracts should support:

- idempotency keys where appropriate
- deterministic request hashing
- durable run identifiers
- resumable state transitions
- explicit degraded or repairable failure reporting

## Notes

- The current codebase exposes an MCP contract, but only one tool is implemented.
- The absence of HTTP endpoints is not a defect in the current project shape; MCP is the intended universal transport surface.

---

_Generated using BMAD Method `document-project` workflow_
