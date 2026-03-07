---
stepsCompleted:
  - 1
  - 2
  - 3
  - 4
  - 5
  - 6
  - 7
  - 8
inputDocuments:
  - _bmad-output/planning-artifacts/prd.md
  - _bmad-output/planning-artifacts/product-brief-tokenizor_agentic_mcp-2026-03-07.md
  - docs/project-overview.md
  - docs/tokenizor_project_direction.md
  - docs/architecture.md
  - docs/provider_cli_runtime_architecture.md
  - docs/provider_cli_integration_research.md
  - docs/source-tree-analysis.md
  - docs/data-models.md
  - docs/api-contracts.md
  - docs/development-guide.md
  - docs/index.md
  - Cargo.toml
workflowType: 'architecture'
project_name: 'tokenizor_agentic_mcp'
user_name: 'Sir'
date: '2026-03-07'
lastStep: 8
status: 'complete'
completedAt: '2026-03-07'
---

# Architecture Decision Document

_This document builds collaboratively through step-by-step discovery. Sections are appended as we work through each architectural decision together._

## Project Context Analysis

### Requirements Overview

**Functional Requirements:**
The PRD defines 49 functional requirements across eight architectural capability areas. The largest requirement clusters are repository/workspace lifecycle, indexing/run management, code discovery/retrieval, trust/verification, recovery/repair, operational visibility, workflow integration and retrieval adoption, and MCP resources/prompts/guidance.

Architecturally, these requirements create two separate baseline product obligations:
- dependable retrieval correctness
- workflow adoption and behavior change in real AI coding sessions

This means the system is not a simple indexing service. It needs:
- durable project and workspace identity with worktree-aware resolution
- long-running, resumable indexing runs with checkpoints, cancellation, and idempotent mutation handling
- verified retrieval paths for symbols, outlines, text search, and repository discovery
- explicit operator-facing health, repair, and maintenance surfaces
- MCP tools and resources as real baseline product surfaces, with prompts designed in from the start where they materially improve usage
- explicit adoption architecture as a baseline concern, including at least one workflow-level mechanism that makes retrieval-first behavior more likely in practice

**Non-Functional Requirements:**
The NFR set is unusually architecture-shaping for an early product. The most important constraints are:
- trusted performance targets for search, outline, retrieval, and run-status operations
- mandatory safe-fail behavior for unverified or suspect retrieval
- resumable recovery with deterministic fallback to re-index when recovery is impossible
- local-first security and privacy, with raw source bytes and byte-sensitive artifacts remaining local by default
- scalability to medium-to-large repositories and repeated multi-project local use
- bounded-concurrency operation that preserves local usability during indexing and repair
- integration quality that tolerates partial MCP support without weakening trust boundaries

**Scale & Complexity:**
This is a high-complexity backend/platform architecture rather than a small MCP utility.

- Primary domain: developer tooling / local retrieval infrastructure for AI coding workflows
- Complexity level: high
- The minimum credible architecture spans multiple major components across protocol, orchestration, identity, storage, indexing, retrieval, integration, and observability.
- The system already shows strong pressure toward a split-plane architecture:
  - control plane
  - raw-content plane
  - retrieval/integration plane

### Technical Constraints & Dependencies

The strongest constraints and dependencies currently visible are:
- Rust-first implementation with current scaffold dependencies including `tokio`, `rmcp`, `serde`, `schemars`, `tracing`, `anyhow`, and `thiserror`
- SpacetimeDB as the authoritative control plane for repositories, runs, checkpoints, leases, idempotency, health, repair, and metadata
- local byte-exact CAS as the authoritative raw-content plane for source bytes and byte-sensitive artifacts
- tree-sitter-based parsing and extraction in Rust as the intended parsing strategy
- MCP tools and resources are baseline protocol surfaces; prompts are designed in from the start and supported where they materially improve usage
- project/workspace identity owned by Tokenizor rather than provider clients
- shutdown is not a safe persistence boundary, and must be treated as a system-wide correctness rule for run transitions, checkpoint durability, lease recovery, and startup repair behavior
- the architecture shows real pressure toward a long-lived local runtime because of warm state, resumable indexing, and multi-provider reuse, but that remains architectural pressure rather than a settled packaging decision
- docs-led dual-state planning: PRD and direction docs define target state, codebase defines implementation maturity, and architecture decisions must be validated against both

### Cross-Cutting Concerns Identified

The following concerns will affect most major components:
- byte-exact storage and span verification
- idempotent mutation handling and canonical request hashing
- resumable indexing and deterministic recovery
- quarantine as a first-class response to suspect parses, spans, blobs, or retrieval artifacts
- explicit degraded-state behavior rather than silent fallback
- project/workspace resolution across repositories and worktrees
- operational health visibility and repairability
- provider-agnostic trust boundaries
- retrieval-first workflow adoption instrumentation and steering
- local-first security/privacy and repository-root confinement
- bounded concurrency and structured shutdown
- avoiding reliance on shutdown as a persistence or correctness boundary

## Foundation Bootstrap Evaluation

### Primary Technology Domain

Rust-native backend / local runtime / MCP server foundation, based on project requirements analysis.

This is not primarily a web-app, mobile-app, or generic full-stack starter problem. The project is a coding-first local infrastructure product with:
- Rust-first implementation
- MCP protocol surface
- SpacetimeDB control-plane integration
- local byte-exact CAS
- planned tree-sitter parsing and extraction
- likely long-lived runtime pressure, without freezing runtime packaging too early

### Starter Options Considered

**Option 1: Keep the existing custom Cargo + RMCP scaffold**
- Current repo is already bootstrapped as a Rust 2024 package with a working module layout and MCP health surface.
- This aligns with the project’s docs-led brownfield reality.
- It avoids destructive re-bootstrap churn in a repository where architecture direction already exists.

**Option 2: Fresh `cargo init` minimal Rust starter**
- Official Cargo still supports `cargo init` with binary-target default behavior and Rust 2024 edition by default.
- This is the right lowest-level bootstrap mechanism if the project ever needs to be recreated from scratch.
- However, by itself it does not provide MCP architecture, control-plane boundaries, or retrieval-specific structure.

**Option 3: `cargo-generate` template-based bootstrap**
- `cargo-generate` remains a maintained Rust template generator.
- It is useful when the team has a reusable internal template worth stamping out repeatedly.
- For Tokenizor, this is less suitable as the primary foundation because the project is already bootstrapped and its architecture is specialized enough that a generic template adds little value unless Tokenizor later publishes its own internal template.

**RMCP fit check**
- The official Rust MCP SDK `rmcp` is current and suitable as the MCP foundation.
- It now supports tools, resources, prompts, stdio transport, child-process transport, and streamable HTTP transport.
- That makes it appropriate as the protocol substrate for Tokenizor, but it is an SDK choice rather than a project starter template.
- Architecturally, this means the foundation should not accidentally hard-lock Tokenizor to stdio-only transport even if stdio remains the baseline first surface.

### Selected Foundation: Existing Custom Cargo Scaffold

**Rationale for Selection:**
The existing repository bootstrap is the best foundation for this architecture run because:
- the project is already a brownfield scaffold, not a blank slate
- the docs define a specialized architecture that generic starters do not encode
- the current Cargo/Rust/RMCP base is sufficient and aligned with the intended direction
- preserving continuity is more valuable than replacing the foundation with a generic template tool
- the right next work is architecture and subsystem evolution, not foundation replacement

**Initialization Command:**

Recommended for this repo: keep the existing scaffold; do not regenerate it.

If the foundation ever must be recreated from scratch:

```bash
cargo init --bin --name tokenizor_agentic_mcp .
```

**Existing Foundation Selections:**

**Language & Runtime:**
- Rust 2024 package foundation
- Tokio async runtime
- `rmcp` as the MCP protocol SDK

**UI / Styling:**
- Not applicable for the core product foundation at this stage

**Build Tooling:**
- Native Cargo build/test workflow
- standard Rust dependency and feature management through `Cargo.toml`

**Testing Framework:**
- standard Rust test harness via `cargo test`
- no external framework lock-in imposed by the foundation

**Code Organization:**
- layered module structure already present across `application`, `domain`, `storage`, `protocol`, `indexing`, `parsing`, and `observability`
- supports engine-first evolution better than a generic template would

**Development Experience:**
- simple local bootstrap
- direct CLI development flow
- low ceremony for evolving architecture incrementally
- compatible with later daemon/runtime separation if justified

**Control-Plane Dependency Note:**
- the architectural dependency is the separately managed local SpacetimeDB runtime/service, not the `spacetime` CLI itself
- official local development currently uses the `spacetime` CLI as the management/bootstrap surface to install, manage, and start the local runtime, including standalone startup via `spacetime start`

**Note:** For Tokenizor, foundation selection is effectively complete already. The first implementation stories should build on the current scaffold rather than replace it.

## Core Architectural Decisions

### Data Architecture

**Control-Plane Model: Hybrid authoritative state plus append-only operational history**

Tokenizor will use a hybrid SpacetimeDB data model with:
- canonical current-state records for projects, repositories, workspaces, files, symbols, runs, checkpoints, leases, idempotency, health, and quarantine/integrity status
- append-only operational history for run transitions, repair actions, recovery actions, verification failures, and auditability

This model is selected because the product needs both fast current-state resolution and durable operational memory for trust, recovery, and diagnosis.

**Control-Plane Dependency Model**
- Tokenizor depends on a separately managed local SpacetimeDB runtime/service on the current stable public release line
- the `spacetime` CLI is a bootstrap/management surface, not the architectural dependency
- startup, health checks, and migration logic target the runtime/service rather than mere CLI presence

**Raw-Content Model**
- SpacetimeDB stores metadata and operational truth
- local CAS stores raw bytes and byte-sensitive artifacts
- raw source content is not duplicated into SpacetimeDB by default

This boundary is required to preserve byte exactness, safe retrieval verification, and proper separation between control-plane truth and raw-content storage.

**Validation Model**
- protocol/schema validation at ingress
- domain invariants enforced in Rust domain/application layers
- byte/span verification before trusted retrieval is served

This layered validation model ensures that malformed requests, invalid state transitions, and suspect retrieval spans are caught at the correct layer rather than blurred together.

**Migration Model**
- startup performs compatibility and version checks before mutating work begins
- incompatible schema/runtime state must fail clearly and block unsafe mutation until migration or repair completes
- migration steps must be idempotent and restart-safe

This is required because shutdown is not a safe persistence boundary and recovery must not depend on partially completed upgrade flows.

**Cache Model**
- local CAS serves as the durable raw-content cache/store
- narrow bounded in-process caches may be added later for hot derived lookups
- no general-purpose distributed cache is part of the baseline architecture

This keeps baseline behavior deterministic and reduces invalidation complexity while the core engine and recovery model mature.

**Architectural Rule**
- shutdown is not a safe persistence boundary for any control-plane transition or cache mutation that affects trust or recovery

### Trust / Security Model

**Authority Model**
- Tokenizor is authoritative for stored project/workspace identity and active project/workspace resolution
- Tokenizor is authoritative for run state, retrieval verification outcomes, health, repair, and idempotency state
- provider clients are consumers and invokers, not authorities
- provider clients must not silently redefine repository root, workspace binding, or retrieval truth

This authority model preserves a single system of record for correctness-critical state and prevents provider-specific surfaces from fragmenting operational truth.

**Exposure Model**
- local-first runtime and control-plane exposure only by default
- local control-plane/runtime surfaces should bind to loopback/local-only by default
- stdio MCP remains the baseline first surface
- local IPC is allowed for daemon/runtime evolution
- any non-local exposure must be explicit, opt-in, and treated as outside the baseline trust model

This model supports local developer workflows without accidentally turning Tokenizor into an implicitly networked service.

**Retrieval Trust Model**
- trusted retrieval requires byte/span verification against stored raw bytes
- verification failure must produce explicit blocked, suspect, or quarantined state rather than ambiguous partial success
- failed verification must never silently fall through to trusted serve behavior

This is the core trust contract of the product.

**Mutation Safety Model**
- mutating operations must be rejectable when compatibility, health, or integrity preconditions are not satisfied
- mutating operations require explicit operation semantics and idempotency handling where applicable
- idempotent operations must reject conflicting replays deterministically

This protects the system from corruption-by-retry, mutation during degraded state, and unsafe control-plane transitions.

**Integrity / Quarantine Model**
- suspect files, symbols, retrieval spans, parse outputs, or metadata may enter explicit quarantine/integrity state
- quarantine state must be inspectable and repairable, not hidden
- quarantine transitions and repair outcomes must be visible in operational history

This makes integrity failures diagnosable and prevents silent contamination of trusted outputs.

**Operational Access Model**
- baseline architecture assumes local advanced-user or operator usage
- baseline architecture should not assume trusted multi-user shared-host access
- if later expanded to shared or multi-user environments, explicit authn/authz design will be required rather than inferred from local assumptions

This keeps the baseline honest about what is and is not secured by current assumptions.

**Logging / Privacy Model**
- raw source content must not be dumped by default in logs, diagnostics, or telemetry
- diagnostics should favor hashes, identifiers, offsets, and metadata over raw source excerpts by default
- any source-content emission for troubleshooting must be explicit and user-directed
- integrity-significant events should remain audit-visible without leaking code content by default

This preserves local privacy expectations without sacrificing operational diagnosability.

**Architectural Rule**
- trust boundaries must degrade safely under partial client support; a weaker client surface must not weaken verification, authority, or integrity rules

### API / Protocol & Communication

**Public Protocol Model**
- MCP is the primary AI-facing protocol surface
- tools are required in the baseline
- resources are baseline-supported, and a minimal useful set should ship in the baseline
- prompts are designed in from the start and shipped where they materially improve usage

This preserves MCP as the universal integration surface while keeping the product broader than “tools only.”

**Tooling Model**
- read/query results should return explicit trust and integrity state where relevant
- long-running mutating tools should return durable operation or run references rather than pretending synchronous completion
- tool contracts should distinguish synchronous completion from accepted/background work
- tool contracts should make idempotency expectations explicit where applicable

This keeps the protocol honest about work duration, trust semantics, and retry behavior.

**Transport Model**
- stdio MCP is the baseline first transport
- architecture remains transport-extensible rather than stdio-locked
- any later Streamable HTTP support must preserve the local-first trust model by default
- architecture should remain transport-extensible without requiring transport-specific domain logic
- internal local IPC is allowed between a thin MCP shim and a longer-lived runtime if that boundary becomes justified

This supports present-day interoperability without freezing future runtime evolution.

**Error / Result Semantics**
- protocol errors are reserved for true invocation, transport, or contract failures
- domain-level states such as suspect, quarantined, repair-required, blocked, degraded, or incompatible should be represented explicitly in result models where possible
- integrity failure, compatibility failure, and repair-required state must remain distinguishable rather than collapsed into generic failures

This preserves diagnostic clarity and prevents domain trust states from being hidden behind transport-level error handling.

**Progress / Cancellation Model**
- long-running mutating operations should expose durable run IDs, status inspection, cancellation, and checkpoint visibility
- protocol shape should remain compatible with richer MCP progress/task patterns later
- cancellation must be explicit and observable; cancelled runs should transition into durable terminal state rather than vanishing

This aligns protocol behavior with resumability and operator visibility expectations.

**CLI / Operator Boundary**
- CLI remains the operator, bootstrap, and admin surface (`init`, `doctor`, `migrate`, `run`, and later integration/install/status flows)
- CLI is the authoritative bootstrap and maintenance surface for runtime readiness and migration safety, not just a convenience wrapper
- MCP is not the only public surface; CLI/operator commands are first-class baseline product surfaces for operability

This preserves a clean split between AI-facing retrieval/use flows and operator-facing lifecycle management.

**Internal Communication Model**
- engine and domain logic remain transport-agnostic
- if a daemon/runtime split emerges, local IPC is the internal communication boundary
- provider adapters should communicate only through supported application/protocol boundaries, never directly against storage layers

This prevents provider-specific integration concerns from bypassing trust, validation, or operational invariants.

**Architectural Rule**
- public protocol design must preserve future compatibility with richer MCP lifecycle and capability evolution without requiring a rewrite of core domain or application layers

### Infrastructure / Runtime & Deployment

**Runtime Topology**
- baseline architecture is local-first and single-user by default
- SpacetimeDB remains a separate runtime/service process architecturally
- Tokenizor may still own installation, bootstrap, readiness checks, and local lifecycle management for that runtime as part of the product experience
- current baseline execution model is CLI plus stdio MCP process with separately managed local SpacetimeDB runtime/service
- architecture explicitly allows evolution toward a longer-lived local Tokenizor runtime if justified by warm-state, resumability, and multi-provider pressure
- no hard commitment yet to daemon-only packaging in the baseline

This preserves the distinction between architectural dependency and integrated operator experience.

**Deployment Model**
- baseline supports direct local installation and operation first
- containerized SpacetimeDB/runtime deployment is a valid secondary path for advanced local or self-hosted use
- remote or network deployment is outside the baseline trust model unless explicitly configured

This keeps the baseline product centered on serious local developer workflows.

**Bootstrap / Readiness Model**
- `doctor` diagnoses readiness and incompatibility conditions
- `init` bootstraps required local foundations
- `migrate` advances compatibility safely
- `run` must either verify that required dependencies are ready or fail clearly before serving MCP
- readiness covers runtime/service reachability, schema/module compatibility, local CAS/storage readiness, and recovery-required state before accepting new mutating work

This makes runtime bring-up an explicit trust and operability gate rather than a best-effort convenience step.

**Migration / Deployment Safety**
- no mutating work begins until runtime, storage, and compatibility checks pass
- startup recovery sweep must run before new mutating work is accepted
- stale leases, temp artifacts, and checkpoints must be reconciled deterministically during bring-up
- deployment/bootstrap flows must be idempotent and restart-safe

This makes crash recovery and deployment safety part of normal system behavior rather than exceptional repair work.

**Runtime Boundary Model**
- baseline implementation may remain process-local
- architecture should isolate application/runtime services so later daemonization does not require domain-layer redesign
- if a daemon/runtime split is adopted later, MCP transport should become a thin ingress layer over application/runtime services
- provider adapters target supported application/protocol boundaries, not process internals

This keeps future packaging evolution from forcing architectural churn in core logic.

**Configuration Model**
- environment and config-file driven local configuration
- explicit versioning for runtime compatibility, schema compatibility, and local storage layout compatibility
- no silent auto-migration past incompatible state without explicit operator-visible handling

This keeps compatibility changes explicit, auditable, and safe.

**Observability / Deployment Model**
- health and readiness surfaces are baseline requirements
- deployment and bring-up behavior must make degraded, blocked, incompatible, or recovery-required states explicit to operators
- operator-visible state should distinguish:
  - runtime absent
  - runtime unreachable
  - runtime version incompatible
  - schema/module incompatible
  - storage/layout incompatible
  - recovery required
  - integrity/quarantine issues present

This gives operators enough clarity to recover safely without guesswork.

**Architectural Rule**
- infrastructure design must preserve the local-first trust model even when later packaging evolves toward daemon, service, or container deployment

## Implementation Patterns & Consistency Rules

### Pattern Categories Defined

**Critical Conflict Points Identified:**
6 major areas where AI agents could make incompatible choices:
- naming across Rust, MCP, control-plane, and CAS boundaries
- responsibility boundaries between architectural layers
- trust/integrity result modeling
- operational event and history conventions
- mutation/recovery/idempotency process rules
- provider integration boundary enforcement

### Naming Patterns

**Control-Plane and Storage Naming Conventions:**
- SpacetimeDB current-state tables use plural `snake_case` names
  - examples: `projects`, `repositories`, `workspaces`, `index_runs`, `checkpoints`, `health_events`
- fields use `snake_case`
  - examples: `project_id`, `workspace_id`, `request_hash`, `last_seen_at_unix_ms`
- foreign keys and references use explicit `_id` suffixes
- byte-oriented fields use explicit byte semantics in the name
  - examples: `span_start_byte`, `span_len_bytes`
- hashes use lowercase hex strings
- CAS paths and shard keys use lowercase hexadecimal only

**MCP Naming Conventions:**
- tool names use `snake_case` verb-noun or get/query action style
  - examples: `index_folder`, `index_repository`, `get_index_run`, `search_symbols`, `repair_index`
- resource identifiers use stable noun-oriented naming
  - examples: `repository_outline`, `repository_health`, `run_status`
- prompt identifiers use `snake_case`
- protocol field names use `snake_case` for consistency with Rust and control-plane metadata

**Rust Code Naming Conventions:**
- modules, files, functions, and local variables use `snake_case`
- structs, enums, and traits use `PascalCase`
- enum variants use `PascalCase`
- constants use `SCREAMING_SNAKE_CASE`
- types representing domain concepts should be singular nouns
  - examples: `Project`, `Repository`, `IndexRun`, `QuarantineRecord`
- boundary and transport DTO types should use explicit suffixes where applicable
  - examples: `IndexRepositoryRequest`, `GetIndexRunResponse`, `RunStatus`, `ProjectRef`
- filenames should reflect primary responsibility, not arbitrary utility naming
  - examples: `project_registry.rs`, `runtime_health.rs`, `local_cas.rs`

### Structure Patterns

**Layering and Responsibility Rules:**
- `domain` defines core entities, value types, invariants, and state-machine semantics
- `application` orchestrates use cases, policies, and workflow coordination
- `storage` implements persistence boundaries for control-plane and CAS concerns
- `protocol` adapts external surfaces such as MCP and CLI-facing contracts
- `indexing` owns discovery, hashing, pipeline coordination, and commit preparation
- `parsing` owns tree-sitter bindings, extraction, and parse-specific translation
- `observability` owns logging, tracing, metrics, and health-report composition
- provider-specific integration code must not live in `domain` or `storage`

**Project Organization Rules:**
- new logic must be placed in the most specific architectural layer that owns the concern
- no catch-all `utils` module at the root architecture level
- shared helpers are allowed only when their ownership is explicit and bounded
- tests for domain and implementation details are co-located where practical
- cross-module and behavior-level integration tests belong under `tests/`
- fixtures belong under dedicated fixture paths, not inline ad hoc literals across the codebase

### Format Patterns

**Result and Response Format Rules:**
- trust-bearing query results must expose explicit trust/integrity state where relevant
- domain states such as `trusted`, `suspect`, `quarantined`, `repair_required`, `blocked`, or `incompatible` must not be collapsed into generic success/failure booleans
- trust/integrity state vocabulary must be centralized and reused across layers rather than re-created as ad hoc strings
- protocol errors are reserved for invocation, transport, or contract failures
- long-running mutation responses must return durable run/operation references instead of pretending synchronous completion

**Time, Identity, and Metadata Rules:**
- machine-facing timestamps use explicit `*_unix_ms` fields unless a different boundary is justified
- identifiers are opaque and stable within their boundary; do not derive behavior from string parsing unless explicitly designed
- raw hashes, request hashes, and blob identifiers must be preserved exactly as canonical lowercase values
- metadata should prefer hashes, IDs, offsets, and structured status over free-form text when representing operational truth

### Communication Patterns

**Operational Event Naming Rules:**
- operational event names use lowercase dot-separated naming
  - examples: `run.started`, `run.cancelled`, `repair.completed`, `retrieval.quarantined`
- event payload fields use `snake_case`
- event names describe completed or observed facts, not vague intentions

**Logging and Diagnostics Rules:**
- logs must be structured first, prose second
- component, operation, subject ID, and outcome should be explicit where possible
- diagnostics should prefer identifiers, hashes, offsets, and compatibility state over raw content excerpts
- source-content emission for debugging must be explicit and operator-directed

### Process Patterns

**Mutation and Recovery Rules:**
- validate before mutating
- verify before serving trusted retrieval
- reconcile recovery-required state before accepting new mutating work
- quarantine instead of silently degrading trust-bearing artifacts
- cancellation must produce durable terminal state
- idempotent mutation paths must reject conflicting replays deterministically

**Concurrency and Lifecycle Rules:**
- bounded concurrency is required for indexing and repair flows
- background work must have explicit ownership, lease semantics, or equivalent runtime accountability
- shutdown is never treated as a safe persistence boundary
- startup recovery sweep is mandatory before new mutating work begins

**Provider Integration Rules:**
- provider adapters communicate only through supported application/protocol boundaries
- provider integrations must not directly mutate storage layers
- weaker client capability must not weaken authority, integrity, or verification rules

### Enforcement Guidelines

**All AI Agents MUST:**
- preserve architectural layer boundaries and avoid bypassing domain/application invariants
- use explicit trust/integrity states in retrieval-related results
- follow the canonical naming conventions for Rust, MCP, control-plane fields, and operational events
- route provider-facing behavior through supported protocol/application surfaces only
- treat quarantine, compatibility failure, and recovery-required state as explicit modeled outcomes
- keep mutating operations compatible with idempotency, recovery, and startup-safety rules

**Pattern Enforcement:**
- code review should reject layer-boundary leaks and silent trust-state collapse
- tests should verify explicit trust, cancellation, recovery, and idempotency behavior
- architecture/pattern violations should be corrected in the owning layer rather than patched around downstream
- new conventions should be added centrally to this architecture document before agents rely on them

### Pattern Examples

**Good Examples:**
- `search_symbols` returns results plus an explicit retrieval/trust state when integrity matters
- `get_index_run` exposes stable run status rather than inferring from logs
- `Project` lives in `domain`, orchestration in `application`, persistence in `storage`
- a retrieval span verification failure becomes `quarantined` or `blocked`, not silent fallback
- a cancelled indexing run transitions to a durable terminal state

**Anti-Patterns:**
- provider adapter writes directly to CAS or control-plane storage
- generic `success: false` for a repair-required or quarantined domain outcome
- new code added to an unowned `utils` bucket to avoid deciding responsibility
- inconsistent naming such as `project_id`, `projectId`, and `ProjectID` for the same concept across boundaries
- inconsistent naming such as `workspace_id`, `workspaceId`, and `WorkspaceID` for the same concept across boundaries
- inconsistent naming such as `run_id`, `runId`, and `RunID` for the same concept across boundaries
- background work that leaves no durable ownership, lease, checkpoint, or terminal state

## Project Structure & Boundaries

### Intended Evolved Project Structure

The following tree describes the intended evolved target structure for Tokenizor after the architecture is implemented. It is not a claim about the current on-disk file inventory.

```text
tokenizor_agentic_mcp/
├── Cargo.toml
├── Cargo.lock
├── README.md
├── AGENTS.md
├── .gitignore
├── .env.example
├── docs/
│   ├── index.md
│   ├── architecture.md
│   ├── tokenizor_project_direction.md
│   ├── provider_cli_runtime_architecture.md
│   ├── provider_cli_integration_research.md
│   ├── project-overview.md
│   ├── source-tree-analysis.md
│   ├── data-models.md
│   ├── api-contracts.md
│   └── development-guide.md
├── spacetime/
│   └── tokenizor/
│       ├── README.md
│       ├── schema/
│       │   ├── projects/
│       │   ├── repositories/
│       │   ├── workspaces/
│       │   ├── index_runs/
│       │   ├── checkpoints/
│       │   ├── leases/
│       │   ├── files/
│       │   ├── symbols/
│       │   ├── health_events/
│       │   ├── quarantine_records/
│       │   └── idempotency_records/
│       └── migrations/
│           └── README.md
├── src/
│   ├── main.rs
│   ├── lib.rs
│   ├── config.rs
│   ├── error.rs
│   ├── application/
│   │   ├── mod.rs
│   │   ├── services/
│   │   │   ├── mod.rs
│   │   │   ├── project_registry.rs
│   │   │   ├── workspace_resolution.rs
│   │   │   ├── runtime_health.rs
│   │   │   ├── recovery_sweep.rs
│   │   │   ├── quarantine_service.rs
│   │   │   └── idempotency_service.rs
│   │   ├── queries/
│   │   │   ├── mod.rs
│   │   │   ├── search_symbols.rs
│   │   │   ├── search_text.rs
│   │   │   ├── get_file_outline.rs
│   │   │   ├── get_repo_outline.rs
│   │   │   ├── get_symbol.rs
│   │   │   └── get_symbols.rs
│   │   ├── commands/
│   │   │   ├── mod.rs
│   │   │   ├── index_folder.rs
│   │   │   ├── index_repository.rs
│   │   │   ├── cancel_index_run.rs
│   │   │   ├── checkpoint_now.rs
│   │   │   ├── repair_index.rs
│   │   │   └── invalidate_cache.rs
│   │   └── policy/
│   │       ├── mod.rs
│   │       ├── retrieval_trust.rs
│   │       ├── mutation_gate.rs
│   │       └── provider_capability.rs
│   ├── domain/
│   │   ├── mod.rs
│   │   ├── project.rs
│   │   ├── repository.rs
│   │   ├── workspace.rs
│   │   ├── index.rs
│   │   ├── symbol.rs
│   │   ├── file_record.rs
│   │   ├── checkpoint.rs
│   │   ├── lease.rs
│   │   ├── idempotency.rs
│   │   ├── health.rs
│   │   ├── quarantine.rs
│   │   └── trust_state.rs
│   ├── storage/
│   │   ├── mod.rs
│   │   ├── blob.rs
│   │   ├── sha256.rs
│   │   ├── local_cas.rs
│   │   ├── cas/
│   │   │   ├── mod.rs
│   │   │   ├── blob_store.rs
│   │   │   ├── quarantine_store.rs
│   │   │   └── temp_store.rs
│   │   └── control_plane/
│   │       ├── mod.rs
│   │       ├── client.rs
│   │       ├── health.rs
│   │       ├── projects.rs
│   │       ├── repositories.rs
│   │       ├── workspaces.rs
│   │       ├── index_runs.rs
│   │       ├── checkpoints.rs
│   │       ├── leases.rs
│   │       ├── files.rs
│   │       ├── symbols.rs
│   │       ├── quarantine.rs
│   │       └── idempotency.rs
│   ├── indexing/
│   │   ├── mod.rs
│   │   ├── coordinator.rs
│   │   ├── discovery.rs
│   │   ├── filtering.rs
│   │   ├── hashing.rs
│   │   ├── language_detection.rs
│   │   ├── checkpointing.rs
│   │   ├── commit.rs
│   │   ├── recovery.rs
│   │   └── verification.rs
│   ├── parsing/
│   │   ├── mod.rs
│   │   ├── tree_sitter.rs
│   │   ├── extraction.rs
│   │   ├── spans.rs
│   │   └── languages/
│   │       ├── mod.rs
│   │       ├── rust.rs
│   │       ├── python.rs
│   │       ├── typescript.rs
│   │       └── go.rs
│   ├── protocol/
│   │   ├── mod.rs
│   │   ├── mcp/
│   │   │   ├── mod.rs
│   │   │   ├── server.rs
│   │   │   ├── tools/
│   │   │   │   ├── mod.rs
│   │   │   │   ├── health.rs
│   │   │   │   ├── index_folder.rs
│   │   │   │   ├── index_repository.rs
│   │   │   │   ├── get_index_run.rs
│   │   │   │   ├── cancel_index_run.rs
│   │   │   │   ├── checkpoint_now.rs
│   │   │   │   ├── repair_index.rs
│   │   │   │   ├── search_symbols.rs
│   │   │   │   ├── search_text.rs
│   │   │   │   ├── get_file_outline.rs
│   │   │   │   ├── get_repo_outline.rs
│   │   │   │   ├── get_symbol.rs
│   │   │   │   ├── get_symbols.rs
│   │   │   │   └── invalidate_cache.rs
│   │   │   ├── resources/
│   │   │   │   ├── mod.rs
│   │   │   │   ├── repository_outline.rs
│   │   │   │   ├── repository_health.rs
│   │   │   │   └── run_status.rs
│   │   │   └── prompts/
│   │   │       ├── mod.rs
│   │   │       ├── architecture_map.rs
│   │   │       ├── codebase_audit.rs
│   │   │       └── repair_diagnosis.rs
│   │   └── cli/
│   │       ├── mod.rs
│   │       ├── doctor.rs
│   │       ├── init.rs
│   │       ├── migrate.rs
│   │       ├── run.rs
│   │       └── status.rs
│   ├── integration/
│   │   ├── mod.rs
│   │   ├── providers/
│   │   │   ├── mod.rs
│   │   │   └── shared/
│   │   │       ├── capability_matrix.rs
│   │   │       └── project_binding.rs
│   │   └── assets/
│   │       ├── mod.rs
│   │       └── installer.rs
│   ├── runtime/
│   │   ├── mod.rs
│   │   ├── server.rs
│   │   ├── session.rs
│   │   ├── supervisor.rs
│   │   └── ipc.rs
│   └── observability/
│       ├── mod.rs
│       ├── logging.rs
│       ├── metrics.rs
│       ├── tracing.rs
│       └── health.rs
├── templates/
│   ├── providers/
│   │   └── shared/
│   │       └── project.toml
│   └── prompts/
│       └── README.md
├── tests/
│   ├── integration/
│   │   ├── index_runs.rs
│   │   ├── retrieval_trust.rs
│   │   ├── repair_flow.rs
│   │   └── project_resolution.rs
│   ├── protocol/
│   │   ├── mcp_tools.rs
│   │   └── mcp_resources.rs
│   ├── storage/
│   │   ├── local_cas.rs
│   │   └── control_plane.rs
│   └── fixtures/
│       ├── repos/
│       └── blobs/
└── .github/
    └── workflows/
        └── ci.yml
```

**Structure notes:**
- `src/runtime/` is conditional and exists only if/when a longer-lived runtime split is adopted.
- provider-specific subtrees under `integration/providers/` are expansion paths, not a baseline commitment to multiple simultaneous adapters.
- schema and migration assets under `spacetime/tokenizor/` are intentionally format-agnostic at this stage.

### Architectural Boundaries

**API Boundaries:**
- `protocol/mcp` is the AI-facing boundary for MCP tools, resources, and prompts
- `protocol/cli` is the operator-facing boundary for bootstrap, diagnostics, migration, and runtime launch
- protocol layers translate external requests into `application` commands/queries and never talk directly to raw storage primitives

**Component Boundaries:**
- `domain` owns correctness-critical types and invariants
- `application` owns orchestration, trust gates, mutation gates, and use-case coordination
- `storage` owns persistence implementations only
- `indexing` and `parsing` are execution subsystems, not external protocol layers
- `integration` owns provider-specific install/binding behavior and is isolated from core storage internals
- `runtime/` is optional and only introduced if a longer-lived local runtime boundary is adopted

**Service Boundaries:**
- project/workspace resolution lives in `application/services`
- query use cases live in `application/queries`
- mutating workflows live in `application/commands`
- trust, mutation, and provider-capability rules live in `application/policy`

**Data Boundaries:**
- SpacetimeDB control-plane access is isolated under `storage/control_plane`
- CAS access is isolated under `storage/cas` and `storage/local_cas.rs`
- raw bytes never bypass CAS abstractions
- provider integration code never bypasses application boundaries to access storage directly

### Requirements to Structure Mapping

**FR Category Mapping:**
- Repository/workspace lifecycle → `domain/project.rs`, `domain/workspace.rs`, `application/services/project_registry.rs`, `application/services/workspace_resolution.rs`, `storage/control_plane/projects.rs`, `storage/control_plane/workspaces.rs`
- Indexing and run management → `domain/index.rs`, `domain/checkpoint.rs`, `domain/lease.rs`, `application/commands/`, `indexing/`, `storage/control_plane/index_runs.rs`, `storage/control_plane/checkpoints.rs`, `storage/control_plane/leases.rs`
- Code discovery and retrieval → `application/queries/`, `indexing/verification.rs`, `parsing/`, `storage/control_plane/files.rs`, `storage/control_plane/symbols.rs`, `protocol/mcp/tools/`
- Trust, verification, and safe failure → `domain/trust_state.rs`, `domain/quarantine.rs`, `application/policy/retrieval_trust.rs`, `application/services/quarantine_service.rs`, `storage/control_plane/quarantine.rs`
- Recovery, repair, and continuity → `application/commands/repair_index.rs`, `application/services/recovery_sweep.rs`, `indexing/recovery.rs`, `observability/`, `storage/control_plane/health.rs`
- Workflow integration and adoption → `protocol/mcp/resources/`, `protocol/mcp/prompts/`, `integration/`, `templates/`

**Cross-Cutting Concerns:**
- idempotency → `domain/idempotency.rs`, `application/services/idempotency_service.rs`, `storage/control_plane/idempotency.rs`
- health/reporting → `domain/health.rs`, `application/services/runtime_health.rs`, `protocol/mcp/tools/health.rs`, `protocol/mcp/resources/repository_health.rs`
- compatibility/readiness → `protocol/cli/doctor.rs`, `protocol/cli/migrate.rs`, `application/services/runtime_health.rs`, `runtime/supervisor.rs` if/when runtime split exists

### Integration Points

**Internal Communication:**
- `protocol/*` → `application/*`
- `application/*` → `domain/*` and `storage/*`
- `indexing/*` ↔ `parsing/*` through typed extraction/verification interfaces
- `integration/*` → `application/services` and `protocol` only
- future `runtime/*` ↔ `protocol/mcp` through internal runtime/application boundaries if that split is adopted

**External Integrations:**
- SpacetimeDB runtime/service via `storage/control_plane`
- local filesystem/CAS via `storage/cas`
- provider clients through MCP and installation/binding assets under `integration/` and `templates/`

**Data Flow:**
- ingest/index path: protocol/CLI command → application command → indexing discovery/hash/parse/verify → storage commit → operational history
- retrieval path: MCP tool/resource → application query → control-plane lookup + CAS verification → explicit trust-state result
- repair path: operator/MCP repair trigger → application command → quarantine/recovery logic → updated control-plane state + history

### File Organization Patterns

**Configuration Files:**
- root-level Cargo/build/workflow files remain at repo root
- runtime compatibility and environment settings remain centralized in config/env handling
- SpacetimeDB schema and migration assets remain under `spacetime/tokenizor/`

**Source Organization:**
- `src/lib.rs` contains the reusable engine/application core
- `src/main.rs` stays thin as the executable/bootstrap entrypoint
- source is organized by architectural responsibility, not generic feature buckets
- new files should be added under the owning layer rather than into unbounded helpers
- provider-specific code is isolated from engine/storage layers

**Test Organization:**
- high-level integration behavior under `tests/integration/`
- protocol contract tests under `tests/protocol/`
- storage tests under `tests/storage/`
- fixtures under `tests/fixtures/`

**Asset Organization:**
- provider-facing generated/static assets under `templates/`
- no mixing of runtime state with repo-managed template assets

### Development Workflow Integration

**Development Server Structure:**
- current development entry remains CLI-driven through `protocol/cli/run.rs`
- any future runtime split should wrap the same application services without domain redesign

**Build Process Structure:**
- Cargo remains the build/test/package entrypoint
- CI should validate core library, protocol contracts, and integration behavior separately

**Deployment Structure:**
- local-first deployment uses the same source structure for CLI, MCP, storage, and control-plane readiness flows
- later runtime/service or container packaging should layer on top of the same application/storage boundaries rather than restructuring the codebase

## Architecture Validation Results

### Coherence Validation

**Decision Compatibility:**
The architecture decisions are mutually compatible and reinforce the same system shape:
- Rust-first engine, `rmcp`, local CAS, and SpacetimeDB control-plane responsibilities align cleanly
- trust, quarantine, idempotency, and recovery rules are consistent across protocol, storage, and runtime decisions
- the architecture remains transport-extensible and runtime-extensible without forcing premature daemonization
- SpacetimeDB is consistently treated as a separate runtime/service dependency, while the `spacetime` CLI is treated as a management/bootstrap surface

**Pattern Consistency:**
Implementation patterns support the architectural decisions well:
- naming conventions align across Rust, MCP, control-plane, and storage boundaries
- trust/integrity vocabulary is centralized and explicit
- protocol/domain/result semantics are clearly separated
- provider integration rules consistently prevent boundary violations

**Structure Alignment:**
The intended evolved structure supports the chosen architecture:
- ownership boundaries are explicit between `domain`, `application`, `storage`, `protocol`, `indexing`, and `parsing`
- optional `runtime/` and provider-integration expansion paths are present without being hard-frozen baseline commitments
- requirements map cleanly onto physical modules and directories

### Requirements Coverage Validation

**Functional Requirements Coverage:**
All major FR categories are architecturally supported:
- repository/workspace lifecycle
- indexing and run management
- code discovery and retrieval
- trust, verification, and safe failure
- recovery, repair, and operational continuity
- operational visibility and maintenance
- workflow integration and retrieval adoption
- MCP tools/resources/prompts direction

**Non-Functional Requirements Coverage:**
The architecture directly addresses the key NFRs:
- trusted retrieval and safe-fail semantics
- resumable recovery and deterministic repair
- local-first privacy and bounded exposure
- bounded concurrency and operator-visible health/readiness
- explicit compatibility, migration, and startup recovery gating
- baseline operability through CLI and MCP surfaces

### Implementation Readiness Validation

**Decision Completeness:**
Critical architectural decisions are documented with enough specificity to guide implementation consistently:
- data architecture
- trust/security model
- API/protocol and result semantics
- infrastructure/runtime/deployment model

**Structure Completeness:**
The intended evolved project structure is concrete enough to guide implementation while preserving conditional areas that are not yet hard decisions.

**Pattern Completeness:**
The implementation-pattern section addresses the main multi-agent conflict risks:
- naming drift
- boundary leakage
- trust-state inconsistency
- mutation/recovery inconsistency
- provider-boundary violations

### Gap Analysis Results

**Critical Gaps for the baseline architecture:**
- none identified

**Important Deferred Areas:**
- exact provider-specific physical packaging beyond baseline adoption path
- exact timing and packaging shape of any long-lived runtime split
- exact SpacetimeDB schema asset format and migration artifact conventions

**Nice-to-Have Future Enhancements:**
- richer prompt surface once usage evidence justifies it
- expanded provider integration trees after baseline adoption proof
- deeper shared-host or multi-user security design if the deployment model expands

### Validation Issues Addressed

The major issues identified during architecture creation were resolved:
- SpacetimeDB runtime/service dependency was separated clearly from CLI management surface
- daemon/runtime pressure was preserved without freezing daemonization as baseline law
- provider integration structure was generalized to avoid premature multi-provider commitment
- trust, quarantine, and compatibility states were made explicit rather than implicit
- dual-state brownfield framing was preserved in both decisions and project structure

### Architecture Completeness Checklist

**✅ Requirements Analysis**
- [x] Project context thoroughly analyzed
- [x] Scale and complexity assessed
- [x] Technical constraints identified
- [x] Cross-cutting concerns mapped

**✅ Architectural Decisions**
- [x] Critical decisions documented
- [x] Technology stack and runtime dependencies specified
- [x] Integration patterns defined
- [x] operability, trust, and recovery considerations addressed

**✅ Implementation Patterns**
- [x] Naming conventions established
- [x] Structure patterns defined
- [x] Communication patterns specified
- [x] Process patterns documented

**✅ Project Structure**
- [x] Intended evolved structure defined
- [x] Component boundaries established
- [x] Integration points mapped
- [x] Requirements-to-structure mapping completed

### Architecture Readiness Assessment

**Overall Status:** READY FOR BASELINE IMPLEMENTATION

**Confidence Level:** High for baseline architecture decisions

**Key Strengths:**
- strong trust/recovery model
- clear authority boundaries
- explicit local-first runtime and deployment posture
- clean separation between control-plane metadata and raw-content storage
- implementation patterns that should reduce cross-agent drift significantly

**Areas for Future Enhancement:**
- provider-specific packaging refinement after adoption evidence
- optional runtime split if warm-state and orchestration pressure justify it
- expanded deployment security model if the product grows beyond local-first assumptions

### Implementation Handoff

**AI Agent Guidelines:**
- follow architectural boundaries exactly as written
- preserve explicit trust/integrity result semantics
- do not bypass application boundaries from provider-facing or protocol-facing code
- treat compatibility, quarantine, and recovery-required states as modeled outcomes, not incidental errors

**First Implementation Priority:**
Build forward from the current scaffold rather than replacing it, starting with control-plane persistence, project/workspace identity, indexing/run lifecycle, and verified retrieval foundations in service of the full baseline product floor, including eventual `jcodemunch-mcp` parity and retrieval-adoption requirements.
