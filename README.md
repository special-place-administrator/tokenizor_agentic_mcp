# tokenizor_agentic_mcp

Tokenizor is a Rust-native, coding-first MCP project for code indexing, retrieval, orchestration, and recovery.

The product direction is not "just another MCP server." The goal is a trusted local retrieval engine for AI coding workflows:
- byte-exact raw content storage
- deterministic indexing and retrieval
- explicit recovery and repair
- durable operational state
- workflow integration that makes retrieval-first behavior practical

## Quick Start

Prerequisites: [Rust toolchain](https://rustup.rs) (edition 2024).

```bash
# One command — builds binary, installs SpacetimeDB CLI if missing,
# starts the local runtime, publishes the module, verifies readiness,
# and prints MCP config for your client.
bash scripts/setup.sh
```

After setup, add the printed config to your MCP client (Claude Code, Claude Desktop, Cursor, etc.).

For a hands-off experience on reboot, use the launcher wrapper as your MCP command:

```json
{
  "mcpServers": {
    "tokenizor": {
      "command": "bash",
      "args": ["/path/to/tokenizor_agentic_mcp/scripts/tokenizor-mcp.sh"]
    }
  }
}
```

The wrapper auto-starts SpacetimeDB if it's not running, then launches the MCP server.

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

Epics 1–4 are complete. The codebase is a working indexing, retrieval, recovery, and repair engine with full lifecycle management, 18 MCP tools, and a live SpacetimeDB control plane.

Implemented:
- layered Rust crate structure across `application`, `domain`, `storage`, `protocol`, `indexing`, `parsing`, and `observability`
- guarded CLI entrypoints for `run`, `doctor`, `init`, `attach`, `migrate`, `inspect`, and `resolve`
- 18 MCP tools covering indexing, retrieval, recovery, repair, health inspection, and operational history
- MCP resources: `tokenizor://runs/{run_id}/status` for live progress observation
- deployment-aware startup/readiness checks
- local byte-exact CAS foundation with atomic writes and writeability checks
- durable local bootstrap registry for repository/workspace identity with atomic file writes and advisory locking
- canonical Git common-directory matching for worktree attachment under one project identity
- explicit `migrate` flow for legacy registry identity upgrades and stale path reconciliation
- read-only registry inspection and active-context resolution
- structured JSON operator output for initialization, migration, inspection, and workspace resolution flows
- full indexing pipeline: gitignore-aware discovery, bounded concurrent processing, CAS storage, registry persistence
- tree-sitter-based parsing and symbol extraction for 6 languages (Rust, Python, JavaScript, TypeScript, Go, Java) with 10 additional language variants registered
- run lifecycle management: Queued → Running → Succeeded/Failed/Aborted/Cancelled/Interrupted
- cooperative cancellation via CancellationToken with safe pipeline shutdown
- checkpointing with configurable frequency and resume-from-cursor support
- re-indexing with idempotency-backed replay and stale record detection
- repository invalidation for untrusted indexed state
- idempotent operation replay with conflict detection (5-case decision tree)
- circuit breaker for consecutive file failures (default N=5)
- run health classification and live progress tracking with phase observation
- text search with blob integrity verification and quarantine exclusion
- symbol search with coverage transparency (no blob I/O required)
- file and repository outlines with quarantine visibility
- verified source retrieval with byte-exact fidelity (blob integrity + byte-range extraction + UTF-8 validation)
- suspect retrieval blocking with actionable NextAction guidance
- batched multi-target retrieval with per-item independence and mixed symbol/code-slice targets
- shared trust contract: ResultEnvelope, RetrievalOutcome, TrustLevel, Provenance, RequestGateError, NextAction
- universal request gate (check_request_gate) reused across all retrieval operations
- SpacetimeDB-backed mutable control plane (real SDK integration, schema, migration safety)
- ControlPlane trait with 20+ methods across run lifecycle, checkpointing, file records, repair, and operational history
- startup sweep for stale leases and interrupted state recovery
- resume from durable checkpoints with discovery manifest validation
- deterministic repair for suspect, quarantined, or incomplete indexed state (repository/run/file scopes)
- repository health inspection with structured action classification
- operational history with unified event model covering run transitions, checkpoints, repairs, integrity changes, and startup sweeps
- action classification: ActionCondition (10 variants), NextAction vocabulary (Resume, Reindex, Repair, Wait, ResolveContext, Migrate)

Not implemented yet:
- MCP prompts
- provider-native adoption layers and workflow integration (Epic 5)

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

**Automated (recommended):**

```bash
bash scripts/setup.sh        # one-time: build, SpacetimeDB setup, verify
bash scripts/tokenizor-mcp.sh # start MCP server (auto-starts SpacetimeDB)
```

**Manual workflow:**

```bash
spacetime start               # start SpacetimeDB runtime
spacetime publish tokenizor --module-path spacetime/tokenizor --server local -y
cargo run -- doctor           # verify readiness
cargo run -- init .           # register a repository
cargo run -- run              # start MCP server
```

**Scripts:**
- `scripts/setup.sh` — one-time full setup (build, SpacetimeDB, module publish, verify)
- `scripts/tokenizor-mcp.sh` — MCP launcher wrapper (ensures SpacetimeDB is running, then serves)
- `scripts/ensure-runtime.sh` — lightweight SpacetimeDB startup check

## MCP Surface

Implemented MCP tools (18):
- `health` — deployment readiness check
- `index_folder` — launch background indexing run for a repository path
- `get_index_run` — inspect run status, health, progress, and structured action classification
- `list_index_runs` — list runs by repository or status
- `cancel_index_run` — cooperatively cancel an active run
- `reindex_repository` — re-index with idempotency and stale detection
- `invalidate_indexed_state` — mark indexed state as untrusted
- `checkpoint_now` — trigger checkpoint for an active run
- `resume_index_run` — resume an interrupted run from its last durable checkpoint
- `repair_index` — trigger deterministic repair for suspect, stale, quarantined, or incomplete state
- `inspect_repository_health` — repository health report with structured action classification
- `get_operational_history` — time-ordered operational events (run transitions, repairs, integrity changes)
- `search_text` — full-text search across indexed files with blob integrity verification
- `search_symbols` — search indexed symbol records with coverage transparency
- `get_file_outline` — file-level symbol outline with quarantine visibility
- `get_repo_outline` — repository-level structural overview
- `get_symbol` — verified source retrieval for a single symbol or code slice
- `get_symbols` — batched retrieval for multiple symbols or code slices in one request

Implemented MCP resources:
- `tokenizor://runs/{run_id}/status` — live run status and progress

Planned (future):
- MCP prompts: architecture map, codebase audit, failure triage, repair diagnosis
- additional MCP resources: repository health, symbol metadata views
- workflow integration and adoption layers (Epic 5)

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

## Build Order

Completed:
1. ~~finish durable local foundation and operator flows~~ (Epic 1)
2. ~~implement durable indexing runs and run lifecycle control~~ (Epic 2)
3. ~~add trusted code discovery and verified retrieval~~ (Epic 3)
4. ~~recovery, repair, and operational confidence~~ (Epic 4)

Next:
5. retrieval-first workflow integration and adoption (Epic 5)

## Development Notes

- Rust edition: 2024
- async runtime: Tokio
- MCP SDK: `rmcp` 1.1.0
- parsing: `tree-sitter` 0.24 with language-specific grammars
- serialization: `serde`, `serde_json`, `schemars`
- error handling: `thiserror` 2.0 (domain), `anyhow` (CLI boundary only)
- observability: `tracing`, `tracing-subscriber`
- SpacetimeDB SDK: `spacetimedb-sdk` 2.0.3
- test suite: 747 tests (unit + integration + conformance + grammar)

```bash
cargo test          # run all tests
cargo run -- doctor # verify deployment readiness
cargo run -- run    # start MCP stdio server
```

This repository is optimized for the best end state, not legacy imitation.
