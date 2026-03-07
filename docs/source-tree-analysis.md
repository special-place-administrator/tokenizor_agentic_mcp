# tokenizor_agentic_mcp - Source Tree Analysis

**Date:** 2026-03-07

## Overview

This repository is a single-part Rust project with a small but intentional source layout. The current implementation is still a foundation slice, so the source tree primarily shows architectural boundaries, bootstrap commands, storage/control-plane scaffolding, and project-direction documentation rather than a full indexing engine.

## Complete Directory Structure

```text
tokenizor_agentic_mcp/
├── Cargo.toml                  # Rust package manifest and dependency set
├── Cargo.lock
├── README.md                   # Current implementation status and commands
├── AGENTS.md                   # Local AI-agent rules and architectural direction
├── docs/                       # Primary source for product intent and target-state direction
│   ├── architecture.md
│   ├── tokenizor_project_direction.md
│   ├── provider_cli_runtime_architecture.md
│   ├── provider_cli_integration_research.md
│   ├── project-overview.md
│   ├── source-tree-analysis.md
│   ├── development-guide.md
│   ├── api-contracts.md
│   ├── data-models.md
│   ├── index.md
│   ├── project-parts.json
│   └── project-scan-report.json
├── src/                        # Rust implementation root
│   ├── main.rs                 # CLI entrypoint: run, doctor, init
│   ├── lib.rs                  # Crate module exports
│   ├── config.rs               # Environment-driven configuration
│   ├── error.rs                # Shared error model
│   ├── observability.rs        # Tracing/logging bootstrap
│   ├── application/            # Application services
│   ├── domain/                 # Core domain models
│   ├── storage/                # CAS and control-plane boundaries
│   ├── protocol/               # MCP transport surface
│   ├── indexing/               # Indexing subsystem placeholder
│   └── parsing/                # Parsing subsystem placeholder
├── spacetime/
│   └── tokenizor/              # Expected local SpacetimeDB module path
├── _bmad/                      # BMAD workflows, agents, and templates
├── _bmad-output/               # BMAD-generated planning/implementation artifacts
└── target/                     # Cargo build output
```

## Critical Directories

### `src/`

The implementation core. The current structure already mirrors the intended layered architecture.

**Purpose:** Hold the Rust application, domain, storage, protocol, and observability layers.  
**Contains:** Entry points, config, errors, domain structs, CAS implementation, SpacetimeDB boundary, MCP server scaffold.  
**Entry Points:** `src/main.rs`, `src/lib.rs`

### `src/application/`

Thin orchestration services for health and deployment/bootstrap flows.

**Purpose:** Encapsulate use-case style application services.  
**Contains:** Deployment and health reporting services.

### `src/domain/`

Current domain-model slice for repository, run, idempotency, and health concepts.

**Purpose:** Define structured system state and invariants already present in the scaffold.  
**Contains:** `Repository`, `IndexRun`, `Checkpoint`, `IdempotencyRecord`, `HealthReport`, deployment/health types.

### `src/storage/`

The most concrete subsystem in the current codebase.

**Purpose:** Own byte-exact local CAS behavior and the control-plane abstraction.  
**Contains:** CAS blob trait, SHA-256 utilities, local CAS implementation, in-memory control plane, SpacetimeDB deployment/reachability boundary.  
**Integration:** Connects domain types to durable storage strategy.

### `src/protocol/`

Current MCP-facing transport layer.

**Purpose:** Expose Tokenizor capabilities over stdio MCP.  
**Contains:** Server type and `health` tool wiring.  
**Integration:** Bridges `ApplicationContext` into the MCP surface.

### `docs/`

The authoritative source for intended product state and probable architecture.

**Purpose:** Hold project direction, architecture, provider research, and now BMAD brownfield documentation.  
**Contains:** Product-direction docs plus generated overview and supporting analysis.  
**Integration:** This repo should be interpreted as docs-led during BMAD planning.

### `spacetime/tokenizor/`

Local control-plane module scaffold.

**Purpose:** Reserve the expected SpacetimeDB schema/module path.  
**Contains:** Current README scaffold only.  
**Integration:** Used by deployment readiness checks and future schema/module deployment.

### `_bmad/`

Local BMAD installation and workflow definitions.

**Purpose:** Planning and workflow engine assets.  
**Contains:** Skills, tasks, templates, workflow manifests, and module configs.

## Entry Points

- **Main CLI Entry:** `src/main.rs`
- **Library Entry:** `src/lib.rs`
- **Primary command paths:** `run`, `doctor`, `init`
- **Primary external transport:** stdio MCP via `rmcp`

## File Organization Patterns

- Rust code is organized by architectural responsibility rather than by feature slice.
- Domain and storage boundaries are already explicit even though implementation depth is still limited.
- `docs/` carries more product-definition weight than the current code, which is unusual but intentional for this repository state.
- `_bmad/` and `_bmad-output/` indicate the repo is being formalized through BMAD rather than purely ad hoc planning.

## Key File Types

### Rust source

- **Pattern:** `src/**/*.rs`
- **Purpose:** Application, domain, storage, protocol, and runtime scaffolding
- **Examples:** `src/main.rs`, `src/storage/local_cas.rs`, `src/protocol/mcp.rs`

### Project direction docs

- **Pattern:** `docs/*.md`
- **Purpose:** Product direction, architecture, research, and brownfield project documentation
- **Examples:** `docs/architecture.md`, `docs/tokenizor_project_direction.md`

### BMAD workflow assets

- **Pattern:** `_bmad/**`
- **Purpose:** Planning workflows, templates, tasks, and agent definitions
- **Examples:** `_bmad/core/tasks/workflow.xml`, `_bmad/bmm/workflows/document-project/workflow.yaml`

### Control-plane scaffold

- **Pattern:** `spacetime/**`
- **Purpose:** Future SpacetimeDB module/schema location
- **Examples:** `spacetime/tokenizor/README.md`

## Configuration Files

- `Cargo.toml`: Rust package manifest and dependency set
- `Cargo.lock`: locked dependency graph
- `AGENTS.md`: repository-level AI-agent guidance and architectural rules
- `_bmad/bmm/config.yaml`: BMAD planning output paths and project-knowledge mapping

## Notes for Development

- The source tree shows a sound architectural starting point, but not a full retrieval engine yet.
- The most mature implementation area today is byte-exact storage and deployment/health scaffolding.
- The most authoritative system design still lives in the `docs/` directory.

---

_Generated using BMAD Method `document-project` workflow_
