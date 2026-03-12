# tokenizor_agentic_mcp - Project Overview

> Historical note: this BMAD-generated snapshot predates the current shipped CLI and release/install workflow.
> Do not use it as the operator runbook. Use [README.md](../README.md), [docs/release-process.md](release-process.md), and `python execution/release_ops.py guide` for current operations.

**Date:** 2026-03-07
**Type:** Backend platform foundation
**Architecture:** Rust-native MCP server scaffold evolving toward a durable code-indexing runtime

## Executive Summary

`tokenizor_agentic_mcp` is an early-stage Rust foundation for a coding-first indexing, retrieval, orchestration, and recovery system. The current codebase implements bootstrap runtime behavior such as `run`, `doctor`, `init`, a health MCP tool, a local byte-exact CAS scaffold, and a SpacetimeDB control-plane boundary.

The authoritative product direction is currently defined in the `docs/` folder rather than in the code alone. Those documents describe the intended end state: a Rust-native code-intelligence engine with deterministic indexing, resumable runs, explicit recovery, a SpacetimeDB-backed control plane, local content-addressed blob storage, and later provider-specific adapters.

## Project Classification

- **Repository Type:** Monolith
- **Project Type(s):** Backend platform foundation in transition
- **Primary Language(s):** Rust
- **Architecture Pattern:** Layered Rust service scaffold with MCP transport, domain modules, storage boundary, and planned daemon/runtime split

## Planning Thesis

For BMAD purposes, Tokenizor should be planned as a docs-led, dual-state brownfield project: the codebase establishes current implementation maturity, while the `docs/` folder establishes the intended product and architectural direction that the implementation is meant to reach.

## BMAD Interpretation Rules

These sections define how this repository should be interpreted during BMAD planning. They exist to prevent the current scaffold from being mistaken for the full product, and to prevent target-state direction from being mistaken for implemented reality.

## Current State vs Target State

### Current State

- Bootstrap Rust project with one primary part rooted at the repository root
- Entry points at `src/main.rs` and `src/lib.rs`
- Module boundaries already established across `application`, `domain`, `storage`, `protocol`, `indexing`, `parsing`, and `observability`
- Stdio MCP server exposes a `health` tool
- SpacetimeDB module path scaffold exists under `spacetime/tokenizor/`

### Target State

- The target state described here includes both stable architectural commitments and still-refinable implementation directions; see Decision Confidence Levels below for the distinction.
- Durable Tokenizor runtime for indexing, retrieval, orchestration, and repair
- Byte-exact local CAS for raw file and large derived artifacts
- SpacetimeDB as authoritative control plane for repositories, runs, checkpoints, health, leases, and idempotency
- Tree-sitter-based parsing and symbol extraction in Rust
- MCP tools, resources, and prompts plus later provider-native adapters

## Technology Stack Summary

### Implemented Foundation Stack

| Category | Technology | Version/Status | Role in Current Codebase |
| --- | --- | --- | --- |
| Primary language | Rust | Edition 2024 | Main implementation language |
| Async/runtime | Tokio | `1.48` | Async runtime for MCP server execution |
| MCP transport | `rmcp` | `1.1.0` | Stdio MCP transport and tool routing |
| Serialization | Serde, `serde_json`, `schemars` | current dependency set | Config, reports, and MCP payload handling |
| Error model | `anyhow`, `thiserror` | current dependency set | Runtime and domain-specific error handling |
| Observability | `tracing`, `tracing-subscriber` | current dependency set | Structured logging and diagnostics |
| Raw content storage | Local CAS scaffold | implemented | Byte-exact blob persistence with atomic writes |
| Control plane | SpacetimeDB boundary | partial/scaffold | Health and deployment checks exist; persistence not wired yet |
| Fallback control plane | In-memory backend | dev/test only | Temporary non-production scaffold |

### Target Architecture Stack

| Category | Technology / Direction | Status | Architectural Intent |
| --- | --- | --- | --- |
| Control plane | SpacetimeDB | intended | Authoritative operational state, leases, checkpoints, health, and idempotency |
| Raw content plane | Local byte-exact CAS | intended and partially scaffolded | Exact file-byte storage for verified retrieval |
| Parsing | tree-sitter in Rust | planned | Language-aware parsing and symbol extraction |
| Runtime model | Long-lived local Tokenizor runtime | probable / target-state | Warm project state, resumable indexing, and shared runtime services if the current direction holds |
| MCP surface | Tools, resources, prompts | planned | Universal compatibility layer |
| Provider integration | MCP plus provider-native adapters | planned | Universal compatibility first, native higher-frequency integration where provider surfaces allow it |

## Architecture Decisions

### ADR-001: Rust-First Core Implementation

- **Decision:** Build the core engine and protocol surface in Rust.
- **Status:** Accepted
- **Rationale:** The project prioritizes speed, deterministic behavior, robustness, and byte correctness. The docs consistently position Rust as the primary implementation language.
- **Consequences:** Core logic should remain testable and native; provider-specific glue should not pull the system away from this center.

### ADR-002: SpacetimeDB Is the Control Plane, Not Universal Blob Storage

- **Decision:** Use SpacetimeDB for structured operational state only.
- **Status:** Accepted
- **Rationale:** Repositories, runs, checkpoints, leases, health events, repair actions, and idempotency records benefit from durable structured state. Raw source bytes do not.
- **Consequences:** Metadata and run state live in SpacetimeDB, while raw content remains outside it.

### ADR-003: Local CAS Owns Raw File Bytes

- **Decision:** Store raw repository content in a local byte-exact content-addressed store.
- **Status:** Accepted
- **Rationale:** Symbol spans and verified retrieval depend on exact bytes. The project explicitly treats Windows newline translation and re-encoding risks as design-level hazards.
- **Consequences:** Storage paths, hashing, atomic writes, quarantine, and later verification logic are core architectural concerns, not implementation details.

### ADR-004: Separate Current Scaffold From Intended Runtime

- **Decision:** Document the current Rust code as a foundation slice, not as the finished product architecture.
- **Status:** Accepted
- **Rationale:** The repository code currently proves direction and scaffolding, while the fuller product definition lives in `docs/architecture.md`, `docs/tokenizor_project_direction.md`, and related documents.
- **Consequences:** BMAD artifacts must describe both the current implemented state and the intended target-state architecture.

### ADR-005: MCP Is Mandatory but Not Sufficient

- **Decision:** Keep MCP as the universal surface, but design toward provider-native adapters.
- **Status:** Accepted direction
- **Rationale:** MCP is mandatory as the universal compatibility surface. Provider-native adapters are a strategic target, but the exact adapter/runtime shape is still being refined.
- **Consequences:** The product should not be framed as "just an MCP server"; internal boundaries should support later Codex, Claude Code, Gemini, Copilot, and Amazon Q adapters.

### ADR-006: Project and Workspace Tracking Is Required

- **Decision:** Tokenizor must maintain its own project and workspace identity model.
- **Status:** Accepted
- **Rationale:** Provider CLIs differ too much in how they represent repositories, worktrees, and local context. Tokenizor cannot delegate project identity to clients.
- **Consequences:** The system will need project registration, workspace resolution, worktree handling, and provider binding metadata.

### ADR-007: Engine First, MCP Second, Native Adapters Third

- **Decision:** Build the Rust indexing/retrieval engine to practical parity before investing heavily in provider-native integrations.
- **Status:** Accepted
- **Rationale:** A weak engine wrapped in native integrations is still a weak product. The retrieval engine must become genuinely useful first.
- **Consequences:** Early implementation focus stays on indexing, symbol extraction, search, verified retrieval, and durable runtime foundations.

### ADR-008: Verified Retrieval Before Serving Source

- **Decision:** Tokenizor must verify source spans against exact stored bytes before serving retrieved code or symbol slices.
- **Status:** Accepted
- **Rationale:** Byte-exact storage only matters if retrieval is verified at serve time. Bad spans, newline translation issues, stale metadata, or corrupted derived state must not be served as if they were trustworthy.
- **Consequences:** Retrieval paths must use blob hash, byte offsets, and verification together. If verification fails, the system must degrade safely through quarantine, repair, or explicit failure rather than silently returning suspect code.

## Architecture Pattern

The project currently uses a layered Rust service scaffold with explicit module boundaries:

- `protocol` for the MCP surface
- `application` for orchestration services
- `domain` for core models and invariants
- `storage` for CAS and control-plane boundaries
- `indexing` and `parsing` as planned execution subsystems
- `observability` for runtime diagnostics

The engine is the product; MCP, hooks, skills, prompts, plugins, and extensions are delivery surfaces.

The intended architecture goes further than the current scaffold:

- a probable long-lived local Tokenizor runtime
- a thin MCP transport layer
- SpacetimeDB as structured operational state
- local CAS as the byte-exact raw content plane
- later provider adapters for Codex, Claude Code, Gemini, Copilot, and Amazon Q

## Foundational Truths

The project direction documents reduce Tokenizor to a small set of non-negotiable truths:

1. Exact source retrieval must be byte correct.
2. Indexing and repair work must be resumable and idempotent.
3. Operational state must be durable, inspectable, and repairable.
4. Coding-agent workflows need a trusted retrieval engine more than another generic tool surface.
5. Integration surfaces may change by provider, but the engine's correctness model cannot.

## Architectural Consequences

These truths force several consequences:

- raw source bytes must live in a byte-exact local CAS
- structured operational state must live in a durable control plane
- indexing must be checkpointed and recoverable
- verification and quarantine must be first-class behaviors
- project and workspace identity must be owned by Tokenizor, not delegated to provider clients
- MCP is required for compatibility, but it is not the center of the system
- provider-native integrations are delivery optimizations, not substitutes for engine quality

## Current Implementation Evidence

The current codebase partially supports these consequences:

- local CAS scaffolding already exists
- SpacetimeDB boundary and readiness checks already exist
- domain types for runs, checkpoints, idempotency, and health already exist
- config/error/observability foundations already exist
- MCP health reporting already exists

What does not exist yet is the full retrieval engine:

- tree-sitter parsing
- symbol extraction
- verified symbol retrieval
- durable run orchestration
- repair workflows
- project/workspace tracking
- local runtime / daemon model
- provider-native adapter implementation

### Documentation Framing Rule

This project should be documented using a dual-state framing model:

- **Current implemented state:** what the Rust scaffold demonstrably supports today
- **Intended target state:** the system described in `docs/architecture.md`, `docs/tokenizor_project_direction.md`, and related direction documents

This framing is required because the codebase is still a foundation slice, while the product definition is already more ambitious and more complete than the implementation.

For BMAD planning purposes, `docs/` is the authoritative source for product intent, and the codebase is the authoritative source for implementation maturity.

### Decision Confidence Levels

Not all target-state architecture has the same certainty level.

#### Accepted Foundations

These are treated as stable architectural commitments:

- Rust-first core implementation
- SpacetimeDB as the control plane
- local byte-exact CAS for raw content
- dual-state documentation framing
- project/workspace identity owned by Tokenizor
- engine-first sequencing

#### Accepted Directions

These are strategically accepted, but still allow implementation refinement:

- MCP as the universal compatibility surface
- provider-native adapters as an important later delivery layer

#### Probable but Refinable Direction

These are current likely directions, not yet frozen implementation law:

- a long-lived local runtime / daemon model
- exact adapter packaging and installation shape per provider
- exact boundary between MCP shim, daemon, and provider-facing helper tooling

This distinction exists to keep BMAD planning rigorous: strong enough to guide implementation, but honest enough to avoid freezing still-evolving design choices too early.

### Planning Guardrails

To avoid common failure modes during BMAD planning, this project should be interpreted with the following guardrails:

- do not collapse the project into the current Rust scaffold
- do not describe target-state architecture as already implemented
- do not freeze probable runtime or daemon decisions too early
- do not prioritize provider-native integrations ahead of engine parity and retrieval usefulness
- do not delegate project, workspace, or retrieval ownership to provider clients
- do not blur accepted foundations, accepted directions, and probable directions

These guardrails exist to keep planning aligned with both reality and intent:

- the codebase shows current implementation maturity
- the docs define product direction
- BMAD artifacts must respect both without letting either distort the other

### Planning Sequence Implication

The architecture implies a dependency-ordered build sequence:

1. establish durable foundations
   - config, error model, observability, storage boundaries, and domain models
2. make the retrieval engine practically useful
   - discovery, hashing, parsing, extraction, search, and verified retrieval
3. refine durable runtime shape where justified by engine needs
   - checkpoints, resumability, orchestration, and possible daemon/runtime boundaries
4. expand delivery surfaces after engine value is proven
   - MCP completeness and usefulness first, then provider-native adapters where they improve real workflow usage

This sequence exists to keep implementation pressure aligned with product value. The system should earn deeper runtime and adapter complexity by proving retrieval usefulness first.

### Trust Boundary Rule

For planning purposes, external provider clients should be treated as consumers of Tokenizor capabilities, not as authorities over Tokenizor's authoritative system state.

Tokenizor must remain authoritative for:

- project and workspace identity
- index state and recovery state
- retrieval verification outcomes
- health, repair, and idempotency state

Provider integrations may influence how Tokenizor is invoked or surfaced, but they should not become the source of truth for core system correctness or operational state.

## Source of Truth

The most important direction-setting documents discovered so far are:

- `docs/architecture.md`
- `docs/tokenizor_project_direction.md`
- `docs/provider_cli_runtime_architecture.md`
- `docs/provider_cli_integration_research.md`
- `docs/README.md`

This overview is an initial brownfield classification checkpoint and will be expanded by later workflow steps.
This overview is intended to establish project framing and planning interpretation rules. Detailed technical design decisions should be expanded, refined, or revised in the formal BMAD architecture artifact rather than treated as permanently fixed here.

## Existing Documentation Baseline

The repository already contains meaningful project-intent documentation and minimal code-level documentation:

- `docs/architecture.md` - first-pass architecture document covering subsystem boundaries, data ownership, indexing pipeline, idempotency, recovery, concurrency, and MCP surface
- `docs/tokenizor_project_direction.md` - primary statement of purpose, product model, build order, and long-term direction
- `docs/provider_cli_runtime_architecture.md` - probable runtime architecture centered on a long-lived local daemon, MCP shim, and provider adapters
- `docs/provider_cli_integration_research.md` - verified provider-surface research covering Codex, Claude Code, Gemini CLI, Copilot CLI, and Amazon Q
- `docs/README.md` - small local index for the current documentation set
- `README.md` - current implementation status for the Rust scaffold
- `AGENTS.md` - local AI-agent working guidance and project rules

## Documentation Notes

- the codebase documents current scaffold behavior, while the project-direction docs describe the intended end state
- formal BMAD artifacts do not exist yet, so current documentation is useful context but not yet a structured BMAD planning set
- user guidance for this brownfield scan: treat the `docs/` folder, including `architecture.md`, as the primary source for intended product state and use the Rust code mainly to document implementation maturity
