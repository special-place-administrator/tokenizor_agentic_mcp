# tokenizor_agentic_mcp - Development Guide

> Historical note: this BMAD-generated scan predates the current install, runtime, and release operator flow.
> For current commands use [README.md](../README.md), [docs/release-process.md](release-process.md), and `python execution/release_ops.py guide`.

**Date:** 2026-03-07

## Development Posture

This repository should currently be treated as a Rust-native foundation slice. Development work should preserve the existing architectural boundaries while moving the engine toward practical indexing and retrieval usefulness.

## Prerequisites

- Rust toolchain with `cargo`
- A Windows shell environment or equivalent supported local shell
- Optional but expected for full control-plane readiness: `spacetimedb` CLI
- A reachable SpacetimeDB endpoint if running with the default control-plane backend; the default local endpoint is `http://127.0.0.1:3007`

## Current Verification Status

The existing test suite was run during this brownfield scan.

- `cargo test`
- Result: `47` library tests and `3` main tests passed

Current test coverage is foundation-oriented and focuses on:

- SHA-256 utility correctness
- SpacetimeDB endpoint authority parsing
- byte-exact CAS behavior and idempotent blob reuse

## Key Commands

### Run tests

```powershell
cargo test
```

### Check runtime/deployment readiness

```powershell
cargo run -- doctor
```

This reports:

- SpacetimeDB CLI presence
- endpoint reachability
- configured database/module path
- CAS directory readiness

### Initialize local storage/bootstrap prerequisites

```powershell
cargo run -- init
```

Or initialize an explicit repository/folder:

```powershell
cargo run -- init C:\path\to\repo
```

For Git repositories and worktrees, the bootstrap-era project identity rule is the normalized shared Git common-directory path. Matching worktrees therefore attach to one existing project identity instead of creating duplicate projects.

If `init` finds matching legacy bootstrap state that predates the canonical identity fields, it now fails explicitly and tells you to run `migrate` first rather than silently minting a duplicate project.

### Attach an additional workspace/worktree to an existing project

```powershell
cargo run -- attach
```

Or attach an explicit workspace/worktree path:

```powershell
cargo run -- attach C:\path\to\worktree
```

This prints the same JSON report shape as `init`. It exits non-zero when the target does not match exactly one existing project and tells you to use separate initialization when needed.

If the target matches legacy bootstrap state that is missing canonical identity fields, `attach` now fails explicitly and tells you to run `migrate` first instead of silently creating a duplicate project.

### Reconcile legacy bootstrap state or update a moved workspace path

```powershell
cargo run -- migrate
```

Or provide an explicit old and new path mapping:

```powershell
cargo run -- migrate C:\old\workspace C:\current\workspace
```

This prints a machine-readable JSON report that separates `migrated`, `updated`, `unchanged`, and `unresolved` records. It exits non-zero when unresolved items remain so the operator can review and act explicitly.

### Inspect the local registry

```powershell
cargo run -- inspect
```

This prints the current local bootstrap registry view as JSON and returns an explicit empty-state response when no projects or workspaces are registered.

### Resolve the active workspace context

```powershell
cargo run -- resolve
```

Or resolve from an explicit directory override:

```powershell
cargo run -- resolve C:\path\to\repo\subdir
```

This prints the active repository/workspace context as JSON and exits non-zero when the requested path is unknown or conflicts with registered workspace state.

### Start the MCP server

```powershell
cargo run -- run
```

## Environment Configuration

The current scaffold recognizes these environment variables:

- `TOKENIZOR_BLOB_ROOT`
- `TOKENIZOR_CONTROL_PLANE_BACKEND`
- `TOKENIZOR_SPACETIMEDB_CLI`
- `TOKENIZOR_SPACETIMEDB_ENDPOINT` (defaults to `http://127.0.0.1:3007`)
- `TOKENIZOR_SPACETIMEDB_DATABASE`
- `TOKENIZOR_SPACETIMEDB_MODULE_PATH`
- `TOKENIZOR_SPACETIMEDB_SCHEMA_VERSION`
- `TOKENIZOR_REQUIRE_READY_CONTROL_PLANE`

## Current Development Workflow

1. update code inside the established module boundaries
2. run `cargo test`
3. run `cargo run -- doctor` when control-plane assumptions changed
4. use `cargo run -- init` to bootstrap local storage and register the current or explicit repository/workspace in Tokenizor's local bootstrap registry
5. use `cargo run -- attach <path>` when you want to add another Git workspace/worktree to an existing registered project
6. use `cargo run -- migrate` to reconcile legacy identity drift or explicitly update a moved workspace/root path before further mutation
7. use `cargo run -- inspect` to audit what the local bootstrap registry currently knows
8. use `cargo run -- resolve` to resolve the active repository/workspace context from the current directory or a directory override
9. use `cargo run -- run` to exercise the MCP server

## Current Architecture Constraints

- Treat the current codebase as implementation scaffolding, not finished product scope
- Preserve byte-exact storage behavior
- Avoid conflating control-plane metadata with raw-content storage
- Keep domain logic testable and decoupled from transport/runtime specifics
- Avoid adapter-first work before engine parity and retrieval usefulness improve

## Expected Near-Term Development Areas

- durable SpacetimeDB persistence rather than readiness checks only
- richer project/workspace registry and run lifecycle creation
- checkpoint persistence and idempotency handling
- indexing discovery/hashing pipeline
- tree-sitter parsing and symbol extraction
- verified retrieval paths
- project/workspace tracking
- later refinement of runtime/daemon boundaries

## Practical Guidance

- Use `docs/architecture.md` and `docs/tokenizor_project_direction.md` as the main target-state references
- Use the Rust codebase as the source of truth for what is actually implemented now
- Treat `docs/index.md` as the BMAD brownfield entry point after this scan completes

---

_Generated using BMAD Method `document-project` workflow_
