# tokenizor_agentic_mcp

Tokenizor is a Rust-native, coding-first MCP project for code indexing, retrieval, orchestration, and recovery.

The product direction is not "just another MCP server." The goal is a trusted local retrieval engine for AI coding workflows:
- byte-exact raw content storage
- deterministic indexing and retrieval
- explicit recovery and repair
- durable operational state
- workflow integration that makes retrieval-first behavior practical

## What Tokenizor Is

Tokenizor is being built as a Rust-native successor to the older `jcodemunch-mcp` style of code-intelligence tooling, but with a stricter architecture and stronger correctness guarantees.

Core design position:
- Rust owns the engine and protocol surface
- SpacetimeDB is the authoritative control plane
- a local byte-exact CAS owns raw file bytes and other byte-sensitive artifacts
- tree-sitter-based parsing and symbol extraction are the planned indexing core

This split exists for a reason:
- operational state wants durable structured storage
- raw source retrieval wants exact bytes
- those are not the same storage problem

## Current Status

The repository is in a real foundation phase, not a blank scaffold and not a finished product.

Implemented now:
- layered Rust crate structure across `application`, `domain`, `storage`, `protocol`, `indexing`, `parsing`, and `observability`
- guarded CLI entrypoints for `run`, `doctor`, `init`, `attach`, `migrate`, `inspect`, and `resolve`
- MCP stdio server scaffold with a working `health` tool
- deployment-aware startup/readiness checks
- local byte-exact CAS foundation with atomic writes and writeability checks
- durable local bootstrap registry for repository/workspace identity
- canonical Git common-directory matching for worktree attachment under one project identity
- explicit `migrate` flow for legacy registry identity upgrades and stale path reconciliation
- read-only registry inspection and active-context resolution
- structured JSON operator output for initialization, migration, inspection, and workspace resolution flows

Not implemented yet:
- real SpacetimeDB-backed project/workspace persistence
- full indexing pipeline and durable index runs
- search, outlines, symbol extraction, and verified source retrieval
- repair workflows, checkpoints, and idempotency-backed mutating run orchestration
- MCP resources/prompts beyond the initial direction
- provider-native adoption layers beyond the current planning baseline

## Architecture

Tokenizor uses a hybrid model:

- `protocol`
  - MCP and CLI entrypoints
- `application`
  - orchestration, readiness, and use-case logic
- `domain`
  - correctness-critical types and invariants
- `storage`
  - SpacetimeDB boundary plus local CAS
- `indexing`
  - discovery, hashing, commit pipeline, recovery
- `parsing`
  - tree-sitter integration and extraction
- `observability`
  - logs, health, and diagnostics

Authoritative data split:

- SpacetimeDB control plane
  - repositories
  - workspaces
  - index runs
  - checkpoints
  - leases
  - health
  - repair actions
  - idempotency
  - metadata and operational history

- Local CAS
  - raw file bytes
  - byte-sensitive derived artifacts
  - any content where newline normalization or re-encoding would break correctness

## Product Principles

- Determinism beats convenience.
- Explicit recovery beats hidden retry magic.
- Corruption should be quarantined, not silently served.
- Shutdown is not a safe persistence boundary.
- Mutating operations should be idempotent.
- Retrieval must be verified before it is trusted.
- Provider clients are consumers, not the source of truth.

## CLI Surface

Current CLI commands:

### `cargo run -- doctor`

Evaluates deployment/runtime readiness and exits non-zero when required blockers exist.

Current readiness coverage includes:
- configuration validity
- local CAS/storage readiness
- SpacetimeDB runtime/service reachability
- configured database/module/schema prerequisites
- CLI/operator remediation guidance where relevant

### `cargo run -- run`

Starts the MCP stdio server only if required readiness checks pass.

Current behavior:
- refuses to serve MCP from an unsafe or degraded startup state
- prints actionable blocker information when startup is blocked
- uses `http://127.0.0.1:3007` as the default local SpacetimeDB endpoint unless overridden

### `cargo run -- init`

Initializes the current repository or an explicit path into Tokenizor's local bootstrap registry.

Current behavior:
- creates or reuses durable local repository/workspace identity
- for Git repositories, uses the normalized shared Git common-directory path as the bootstrap-era project identity
- attaches matching worktrees to the existing project instead of minting duplicate projects
- returns machine-readable JSON
- remains explicit about deployment blockers instead of hiding them
- blocks on matching legacy bootstrap state until `migrate` reconciles it explicitly

Usage:

```powershell
cargo run -- init
cargo run -- init .
cargo run -- init C:\path\to\repo
```

### `cargo run -- attach`

Attaches the current Git workspace/worktree or an explicit path to an already registered project when the canonical Git project identity matches exactly one existing project.

Current behavior:
- returns the same machine-readable JSON report shape as `init`
- preserves a distinct workspace identity for the attached working copy
- fails explicitly with separate-initialization guidance when the target does not match an existing project
- fails explicitly when multiple registered projects match the same canonical Git identity
- fails explicitly when matching legacy bootstrap state must be reconciled through `migrate` before safe registration can continue

Usage:

```powershell
cargo run -- attach
cargo run -- attach C:\path\to\workspace
```

### `cargo run -- migrate`

Reconciles legacy bootstrap registry state explicitly instead of relying on hidden read-time upgrades.

Current behavior:
- scans the local registry for legacy project identity drift and stale workspace paths
- upgrades pre-1.6 Git registrations only when canonical identity can be proven safely from local evidence
- reports `migrated`, `updated`, `unchanged`, and `unresolved` records as machine-readable JSON
- exits non-zero when unresolved records remain so operators can review and act explicitly
- supports an explicit path update mode for moved workspaces or renamed local roots

Usage:

```powershell
cargo run -- migrate
cargo run -- migrate C:\old\workspace C:\current\workspace
```

### `cargo run -- inspect`

Prints the current local bootstrap registry view as JSON.

Current behavior:
- read-only
- groups workspaces under registered project/repository identity
- exposes explicit empty-state and provenance fields
- remains usable even if SpacetimeDB readiness is blocked

### `cargo run -- resolve`

Resolves active repository/workspace context from the current directory or an explicit directory path.

Current behavior:
- returns Tokenizor-owned context as JSON
- fails explicitly for unknown or conflicting workspace state
- does not let external callers silently redefine repository truth

Usage:

```powershell
cargo run -- resolve
cargo run -- resolve C:\path\to\workspace
```

## Environment

Supported configuration variables:

- `TOKENIZOR_BLOB_ROOT`
- `TOKENIZOR_CONTROL_PLANE_BACKEND`
- `TOKENIZOR_SPACETIMEDB_CLI`
- `TOKENIZOR_SPACETIMEDB_ENDPOINT`
- `TOKENIZOR_SPACETIMEDB_DATABASE`
- `TOKENIZOR_SPACETIMEDB_MODULE_PATH`
- `TOKENIZOR_SPACETIMEDB_SCHEMA_VERSION`
- `TOKENIZOR_REQUIRE_READY_CONTROL_PLANE`

Important defaults:

- default endpoint: `http://127.0.0.1:3007`
- default product path expects a SpacetimeDB-backed control plane
- `TOKENIZOR_CONTROL_PLANE_BACKEND=in_memory` is for tests and scaffolding, not the intended production path

## Running Locally

Basic local workflow:

```powershell
cargo run -- doctor
cargo run -- init .
cargo run -- attach C:\path\to\worktree
cargo run -- migrate
cargo run -- inspect
cargo run -- resolve
cargo run -- run
```

At the moment, `run` will only succeed when the configured SpacetimeDB runtime/service is actually reachable and compatible.

## MCP Direction

Tokenizor is being designed for:
- tools
- resources
- prompts

Likely foundation MCP tools:
- `health`
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

Planned useful resources:
- repository outline
- repository health
- run status
- symbol metadata views

Planned useful prompts:
- architecture map
- codebase audit
- failure triage
- repair diagnosis

## Documentation

Important repo docs:

- `docs/project-overview.md`
- `docs/architecture.md`
- `docs/tokenizor_project_direction.md`
- `docs/provider_cli_runtime_architecture.md`
- `docs/provider_cli_integration_research.md`
- `docs/data-models.md`
- `docs/api-contracts.md`
- `docs/development-guide.md`

The root `README.md` should be read as a project-facing summary of current state and direction.
The `docs/` folder contains the deeper architecture and planning baseline.

## Near-Term Build Order

Near-term implementation focus:

1. finish durable local foundation and operator flows
2. wire real control-plane persistence through SpacetimeDB
3. implement durable indexing runs and run lifecycle control
4. add search, outlines, symbol extraction, and verified retrieval
5. add recovery, repair, and richer MCP surfaces
6. layer in workflow adoption paths after retrieval is trustworthy enough to matter

## Development Notes

- Rust edition: 2024
- async runtime: Tokio
- MCP SDK: `rmcp`
- serialization: `serde`, `serde_json`, `schemars`
- observability: `tracing`, `tracing-subscriber`

This repository is optimized for the best end state, not legacy imitation.
