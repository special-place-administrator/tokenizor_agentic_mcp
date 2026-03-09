---
stepsCompleted:
  - step-01-validate-prerequisites
  - step-02-design-epics
  - step-03-create-stories
inputDocuments:
  - _bmad-output/planning-artifacts/prd.md
  - _bmad-output/planning-artifacts/architecture.md
---

# tokenizor_agentic_mcp - Epic Breakdown

## Overview

This document provides the complete epic and story breakdown for tokenizor_agentic_mcp, decomposing the requirements from the PRD, UX Design if it exists, and Architecture requirements into implementable stories.

## Requirements Inventory

### Functional Requirements

FR1: Users can initialize Tokenizor for a local repository or folder they want to use in AI coding workflows.
FR2: Users can register and manage projects and workspaces as durable Tokenizor identities across sessions.
FR3: Users can associate multiple workspaces or worktrees with the same underlying project where applicable.
FR4: Users and AI coding workflows can have Tokenizor resolve the active project/workspace from current context or explicit override when a session begins.
FR5: Users can inspect which projects and workspaces are currently known to Tokenizor.
FR6: Users can update or migrate Tokenizor project/workspace state when lifecycle changes require it.
FR7: Users can validate local Tokenizor setup and dependency health before relying on the system in normal work.
FR8: Users can start indexing for a repository or folder and receive a durable run identity for that work.
FR9: Users can index supported repositories across the baseline language coverage set.
FR10: Users can re-index previously indexed repositories or workspaces when source state changes.
FR11: Users can invalidate indexed state for a repository or workspace when they need a clean rebuild.
FR12: Users can inspect the current status and progress of an indexing run.
FR13: Users and AI coding clients can observe live or near-live run progress and health state for active indexing work.
FR14: Users can cancel an active indexing run when they need to stop or restart work.
FR15: Users can checkpoint long-running indexing work so interrupted progress can be resumed or recovered later.
FR16: Users can retry supported mutating operations with deterministic idempotent behavior, including rejection of conflicting replays where the same idempotency identity is reused with different effective inputs.
FR17: Users can search indexed repositories by text content.
FR18: Users can search indexed repositories by symbol.
FR19: Users can retrieve a structural outline for a file.
FR20: Users can retrieve a structural outline for a repository.
FR21: Users can retrieve source for a symbol or equivalent code slice from indexed content.
FR22: Users can retrieve multiple symbols or code slices in one workflow when needed.
FR23: Users can discover code using supported languages without having to manually re-explore the repository from scratch each session.
FR24: AI coding clients can consume Tokenizor retrieval capabilities through baseline MCP integration.
FR25: Users can rely on Tokenizor to verify source retrieval before trusted code is served.
FR26: The system can refuse to serve suspect or unverified retrieval as trustworthy output.
FR27: Users can see when retrieval has failed verification and understand that the result is blocked, quarantined, or marked suspect.
FR28: Users can rely on Tokenizor to preserve exact raw source fidelity for retrieval-sensitive content.
FR29: Users can distinguish between trusted retrieval results and results that require repair or re-index before use.
FR30: Users can resume interrupted indexing work without losing all prior progress when recovery is possible.
FR31: Users can trigger deterministic repair or re-index flows when indexed state becomes stale, suspect, or incomplete.
FR32: Users can inspect repair-related state, including whether a repository, run, or retrieval problem requires action.
FR33: Users can continue using durable project/workspace context across sessions without repeatedly rebuilding the same repository understanding.
FR34: The system can preserve operational history for runs, checkpoints, repairs, and integrity-related failures so users can understand what happened.
FR35: Users can inspect repository health within Tokenizor.
FR36: Users can inspect run health and status for active or recent work.
FR37: Users can inspect whether operational state indicates stale, interrupted, or suspect conditions.
FR38: Users can perform operator lifecycle actions needed to initialize, validate, migrate, run, and maintain the product in local use.
FR39: Advanced users acting as their own operators can maintain Tokenizor without relying on hidden or implicit system behavior.
FR40: Users can connect Tokenizor to primary AI coding CLI workflows they already use.
FR41: AI coding workflows can access Tokenizor early enough in a session to influence repository exploration behavior.
FR42: Users can rely on at least one primary workflow in which Tokenizor is used before broad brute-force repository exploration.
FR43: Users can observe whether Tokenizor retrieval capabilities are being used in active workflows.
FR44: Integration surfaces can improve retrieval-first behavior without becoming the source of truth for project, workspace, retrieval, or operational state.
FR45: AI coding clients can access minimal baseline Tokenizor resources such as repository outline, repository health, and run status.
FR46: Users can access guidance that explains how to use Tokenizor in primary AI coding workflows.
FR47: Users can access operational guidance for indexing, recovery, repair, troubleshooting, and trust-boundary behavior.
FR48: Users migrating from `jcodemunch-mcp` can access parity and migration guidance for adopting Tokenizor.
FR49: AI coding workflows can use a curated prompt surface where it materially improves adoption or retrieval usage, without making prompts the primary product surface.

### NonFunctional Requirements

NFR1: `search_text` on a warm local index should meet p50 <= 150 ms and p95 <= 500 ms on representative medium-to-large repositories.
NFR2: `search_symbols` on a warm local index should meet p50 <= 100 ms and p95 <= 300 ms.
NFR3: `get_file_outline` on a warm local index should meet p50 <= 120 ms and p95 <= 350 ms.
NFR4: Verified retrieval / `get_symbol` on a warm local index should meet p50 <= 150 ms and p95 <= 400 ms.
NFR5: Run-status and progress visibility should meet request latency p50 <= 100 ms, p95 <= 250 ms, and active progress freshness <= 1 second under normal operation.
NFR6: Unverified or suspect retrieval must never be silently served as trusted output, with a target of 100% explicit safe-fail behavior.
NFR7: Interrupted indexing should recover successfully in at least 95% of supported interruption cases when valid checkpoints exist and source data remains available, with a deterministic re-index fallback when recovery is impossible.
NFR8: Startup must perform stale lease, interrupted-run, and temporary-state recovery sweeps before new mutating work proceeds.
NFR9: Run, checkpoint, repair, and health state must be durably recorded before those transitions are reported as successful.
NFR10: Parse or retrieval integrity failures should quarantine affected artifacts, files, or runs rather than poisoning broader system state.
NFR11: Raw source bytes and byte-sensitive derived artifacts remain local by default, with no implicit remote export, sync, or telemetry.
NFR12: Any remote sync, export, or telemetry must be explicit and opt-in.
NFR13: Provider clients are consumers of Tokenizor capabilities and must not silently persist or redefine project/workspace, retrieval, or operational state.
NFR14: Operational mutations and integrity-significant events must be diagnosable through audit-friendly history.
NFR15: Local control-plane and related runtime surfaces should default to local-only exposure unless explicitly configured otherwise.
NFR16: Logs, diagnostics, and telemetry must avoid dumping raw source content by default unless explicitly requested for troubleshooting.
NFR17: The baseline release must credibly support medium-to-large repositories, at least tens of thousands of source files in aggregate indexed state, repeated use across multiple projects/workspaces, concurrent retrieval while indexing is active, and one active indexing workflow per project with overlapping read/retrieval activity across projects.
NFR18: Tokenizor must be usable from primary AI coding CLI workflows without fragile per-session manual reconfiguration.
NFR19: Bootstrap and dependency problems must be diagnosable through `doctor`.
NFR20: Integration failures must fail clearly and safely rather than degrading into misleading partial trust.
NFR21: Tools, resources, and prompts must degrade safely when a client only partially supports MCP surfaces.
NFR22: At least one primary workflow must make retrieval-first behavior materially more likely in practice.
NFR23: Integration surfaces must not weaken trust boundaries or authoritative Tokenizor state ownership.
NFR24: CLI output, diagnostics, and documentation must be clear, readable, and scriptable.
NFR25: Operator-facing messages must be actionable and understandable to advanced end users, not only system developers.
NFR26: Error messages must distinguish trust or integrity failure, recovery-required state, dependency or bootstrap failure, and integration or configuration failure.
NFR27: Indexing and repair work should use bounded concurrency and should not make normal local development workflows unusable under expected baseline usage.

### Additional Requirements

- Build forward from the existing Rust 2024 Cargo scaffold with `tokio` and `rmcp`; do not re-bootstrap the repository from scratch.
- Keep clear layer boundaries across `protocol`, `application`, `domain`, `storage`, `indexing`, `parsing`, and `observability`, with engine logic remaining transport-agnostic and testable outside MCP and runtime concerns.
- Use SpacetimeDB as the authoritative control plane for projects, repositories, workspaces, index runs, checkpoints, leases, idempotency, health, quarantine, and metadata; treat the `spacetime` CLI as a bootstrap and management surface rather than the architectural dependency itself.
- Use a local byte-exact CAS for raw source bytes and other byte-sensitive artifacts; raw content must be written exactly as read with no newline normalization or decode/re-encode.
- Use tree-sitter-based parsing and symbol extraction in Rust for supported languages.
- Require byte/span verification against stored raw bytes before trusted retrieval is served, and model blocked, suspect, quarantined, repair-required, and incompatible states explicitly rather than collapsing them into generic success or failure results.
- Treat quarantine as a first-class, inspectable, and repairable state for suspect files, symbols, spans, parse outputs, and metadata.
- Mutating operations such as indexing, checkpointing, repair, and invalidation must use explicit semantics plus idempotency handling, including deterministic rejection of conflicting replays.
- Long-running mutating operations must return durable run identifiers and expose status inspection, cancellation, checkpoint visibility, and durable terminal states.
- Startup must perform readiness, compatibility, and recovery sweeps before accepting new mutating work, and shutdown must never be treated as a safe persistence boundary.
- Preserve CLI and operator surfaces such as `init`, `doctor`, `migrate`, and `run` as first-class baseline product surfaces alongside MCP tools, resources, and prompts where they materially improve usage.
- Keep runtime and control-plane exposure local-first and loopback-bound by default; any non-local exposure must be explicit and opt-in.
- Keep provider integrations as consumers of Tokenizor authority; they must not redefine project/workspace or retrieval truth and must not bypass application or protocol boundaries to reach storage directly.
- Epic 4 planning must move mutable run durability from the interim local registry JSON path to the SpacetimeDB-backed control plane while keeping raw bytes in CAS and handling compatibility with prior local state explicitly.

### FR Coverage Map

FR1: Epic 1 - initialize a local repository or folder for AI coding workflows
FR2: Epic 1 - register durable project and workspace identities
FR3: Epic 1 - associate multiple workspaces or worktrees with one project
FR4: Epic 1 - resolve the active project or workspace from context or explicit override
FR5: Epic 1 - inspect known projects and workspaces
FR6: Epic 1 - update or migrate project and workspace state
FR7: Epic 1 - validate local setup and dependency health
FR8: Epic 2 - start indexing with a durable run identity
FR9: Epic 2 - index supported repositories across baseline languages
FR10: Epic 2 - re-index when source state changes
FR11: Epic 2 - invalidate indexed state for a clean rebuild
FR12: Epic 2 - inspect indexing run status and progress
FR13: Epic 2 - expose live or near-live run progress and health for active indexing
FR14: Epic 2 - cancel an active indexing run
FR15: Epic 2 - checkpoint long-running indexing work
FR16: Epic 2 - enforce deterministic idempotent mutation replay and conflicting replay rejection
FR17: Epic 3 - search indexed repositories by text
FR18: Epic 3 - search indexed repositories by symbol
FR19: Epic 3 - retrieve structural outlines for files
FR20: Epic 3 - retrieve structural outlines for repositories
FR21: Epic 3 - retrieve verified symbol or source slices from indexed content
FR22: Epic 3 - retrieve multiple symbols or code slices in one workflow
FR23: Epic 3 - reduce repeated manual repository re-exploration through supported language discovery
FR24: Epic 3 - expose baseline retrieval capabilities through MCP integration
FR25: Epic 3 - verify source retrieval before trusted code is served
FR26: Epic 3 - refuse suspect or unverified retrieval as trustworthy output
FR27: Epic 3 - show blocked, quarantined, or suspect retrieval failures explicitly
FR28: Epic 3 - preserve exact raw source fidelity for retrieval-sensitive content
FR29: Epic 3 - distinguish trusted retrieval from repair-required results
FR30: Epic 4 - resume interrupted indexing work
FR31: Epic 4 - trigger deterministic repair or re-index flows
FR32: Epic 4 - inspect repair-related state and required actions
FR33: Epic 1 - preserve durable project and workspace context across sessions
FR34: Epic 4 - preserve operational history for runs, checkpoints, repairs, and integrity failures
FR35: Epic 4 - inspect repository health
FR36: Epic 2 - inspect run health and status for active or recent work
FR37: Epic 4 - inspect stale, interrupted, or suspect operational conditions
FR38: Epic 1 - perform operator lifecycle actions such as init, validate, migrate, run, and maintain
FR39: Epic 1 - maintain Tokenizor without hidden or implicit system behavior
FR40: Epic 5 - connect Tokenizor to primary AI coding CLI workflows
FR41: Epic 5 - make Tokenizor reachable early enough in a session to influence exploration behavior
FR42: Epic 5 - provide at least one primary workflow where Tokenizor is used before brute-force exploration
FR43: Epic 5 - observe retrieval capability usage in active workflows
FR44: Epic 5 - improve retrieval-first behavior without surrendering Tokenizor authority
FR45: Epic 5 - expose baseline MCP resources such as repository outline, repository health, and run status
FR46: Epic 5 - provide workflow guidance for primary AI coding workflows
FR47: Epic 5 - provide operational guidance for indexing, recovery, repair, troubleshooting, and trust boundaries
FR48: Epic 5 - provide parity and migration guidance from `jcodemunch-mcp`
FR49: Epic 5 - provide a curated prompt surface where it materially improves adoption or retrieval usage

## Epic List

### Epic 1: Reliable Local Setup and Workspace Identity
Users can initialize Tokenizor, validate the local environment, register projects and workspaces, and carry stable repository context across sessions and worktrees.
**FRs covered:** FR1, FR2, FR3, FR4, FR5, FR6, FR7, FR33, FR38, FR39

### Epic 2: Durable Indexing and Run Control
Users can start, monitor, checkpoint, cancel, re-run, and safely retry indexing work with durable run identities and deterministic mutation behavior.
**FRs covered:** FR8, FR9, FR10, FR11, FR12, FR13, FR14, FR15, FR16, FR36

### Epic 3: Trusted Code Discovery and Verified Retrieval
Users can search, outline, and retrieve verified code from indexed repositories through the core retrieval surface that AI workflows depend on.
**FRs covered:** FR17, FR18, FR19, FR20, FR21, FR22, FR23, FR24, FR25, FR26, FR27, FR28, FR29

### Epic 4: Recovery, Repair, and Operational Confidence
Users can recover interrupted work, inspect unhealthy or suspect state, trigger deterministic repair, and understand operational history well enough to restore trust safely.
**FRs covered:** FR30, FR31, FR32, FR34, FR35, FR37

### Epic 5: Retrieval-First Workflow Integration and Adoption
Users can connect Tokenizor to real AI coding workflows, make retrieval available early in a session, observe usage, preserve authority-safe integration boundaries, and access workflow and migration guidance.
**FRs covered:** FR40, FR41, FR42, FR43, FR44, FR45, FR46, FR47, FR48, FR49

<!-- Repeat for each epic in epics_list (N = 1, 2, 3...) -->

## Epic 1: Reliable Local Setup and Workspace Identity

Users can initialize Tokenizor, validate the local environment, register projects and workspaces, and carry stable repository context across sessions and worktrees.

### Story 1.1: Validate Local Runtime Readiness

As an operator,
I want to validate Tokenizor's local runtime and dependency readiness,
So that I can trust the environment before starting or initializing the system.

**FRs implemented:** FR7, FR38

**Acceptance Criteria:**

**Given** Tokenizor is installed on a local machine
**When** I run the readiness check
**Then** Tokenizor reports whether runtime, service, schema, and storage prerequisites are healthy
**And** failures are categorized clearly as bootstrap, dependency, configuration, compatibility, or storage issues

**Given** a prerequisite is missing or unhealthy
**When** readiness validation completes
**Then** Tokenizor returns actionable remediation guidance
**And** it does not create or mutate project or workspace state

### Story 1.2: Start Tokenizor Through a Guarded Run Entrypoint

As an operator,
I want `run` to start Tokenizor only when prerequisites are healthy,
So that MCP is not served from an unsafe or degraded local state.

**FRs implemented:** FR38

**Acceptance Criteria:**

**Given** local runtime, schema, and storage prerequisites are healthy
**When** I run the Tokenizor operator entrypoint
**Then** Tokenizor starts serving MCP through the baseline operator entrypoint
**And** startup records clear healthy-state feedback

**Given** startup checks detect incompatibility, recovery-required state, or missing dependencies
**When** I run the operator entrypoint
**Then** Tokenizor blocks startup with an explicit reason and next-safe-action guidance
**And** it does not silently serve MCP from a degraded state

### Story 1.3: Initialize a Repository or Folder into Tokenizor

As a power user,
I want to initialize a local repository or folder into Tokenizor,
So that it becomes durable project and workspace state I can reuse across sessions.

**FRs implemented:** FR1, FR2, FR33

**Acceptance Criteria:**

**Given** a healthy local environment and a repository or folder path
**When** I run `init` for that path
**Then** Tokenizor creates or registers the project and active workspace binding in authoritative state
**And** the resulting identity is available to later sessions and workflows

**Given** the same repository or folder is initialized again with equivalent inputs
**When** `init` is retried
**Then** Tokenizor returns a deterministic existing-or-idempotent outcome
**And** it does not create conflicting duplicate project or workspace records

### Story 1.4: Inspect Registered Projects and Workspaces

As an operator,
I want to inspect the projects and workspaces known to Tokenizor,
So that I can understand and maintain the current authoritative state explicitly.

**FRs implemented:** FR5, FR39

**Acceptance Criteria:**

**Given** one or more projects or workspaces are registered
**When** I inspect known state
**Then** Tokenizor lists the registered projects and workspaces with stable identifiers, roots, and relevant status information
**And** the output is readable, actionable, and scriptable

**Given** no projects or workspaces are registered
**When** I inspect known state
**Then** Tokenizor returns an explicit empty result
**And** it does not imply state exists through silence or hidden defaults

### Story 1.5: Resolve the Active Project or Workspace for a Session

As an AI coding workflow,
I want Tokenizor to resolve the active project or workspace from current context or explicit override,
So that repository-aware sessions attach to the correct scope without guesswork.

**FRs implemented:** FR4, FR33

**Acceptance Criteria:**

**Given** the current working context maps to a registered workspace
**When** active resolution is requested without an override
**Then** Tokenizor returns the matching project and workspace identity
**And** the result preserves durable continuity across later sessions in the same context

**Given** an explicit override is supplied
**When** active resolution is requested
**Then** Tokenizor resolves the requested registered target deterministically
**And** it fails clearly if the override is unknown, ambiguous, or incompatible with the current context

### Story 1.6: Attach Additional Workspaces or Worktrees to an Existing Project

As a power user,
I want to associate multiple workspaces or worktrees with the same underlying project,
So that Tokenizor preserves one project identity across related local working copies.

**FRs implemented:** FR3

**Acceptance Criteria:**

**Given** an existing project is already registered
**When** I attach another compatible workspace or worktree to that project
**Then** Tokenizor preserves one underlying project identity with distinct workspace bindings
**And** later inspection and resolution surfaces reflect the association correctly

**Given** a workspace attachment conflicts with an existing binding or does not safely match the target project
**When** I attempt the association
**Then** Tokenizor rejects the request explicitly
**And** it does not silently rewrite or merge authoritative state

### Story 1.7: Update or Migrate Project and Workspace State Safely

As an operator,
I want to update or migrate project and workspace state when lifecycle changes occur,
So that durable identity remains correct without hidden mutation or drift.

**FRs implemented:** FR2, FR6, FR38, FR39

**Acceptance Criteria:**

**Given** registered project or workspace state needs an explicit update or migration
**When** I run the supported update or migrate flow
**Then** Tokenizor applies the requested change safely and records the resulting state transition
**And** the updated state remains inspectable through normal project and workspace views

**Given** the requested change is incompatible, ambiguous, or unsafe
**When** the update or migration is attempted
**Then** Tokenizor fails clearly with actionable guidance
**And** it does not silently rewrite authoritative project or workspace state

## Epic 2: Durable Indexing and Run Control

Users can start, monitor, checkpoint, cancel, re-run, and safely retry indexing work with durable run identities and deterministic mutation behavior.

### Story 2.1: Start Indexing with a Durable Run Identity

As a power user,
I want to start indexing for a repository or folder and receive a durable run identity,
So that I can track and manage indexing as real operational work.

**FRs implemented:** FR8

**Acceptance Criteria:**

**Given** a registered repository or folder and healthy runtime prerequisites
**When** I start indexing
**Then** Tokenizor creates a durable indexing run record and returns its stable run identity
**And** the run enters an explicit lifecycle state rather than being treated as fire-and-forget work

**Given** indexing cannot begin because prerequisites, compatibility checks, or active mutation rules fail
**When** I request indexing
**Then** Tokenizor rejects the request with a clear reason
**And** it does not create misleading partial run state

### Story 2.2: Index Supported Repositories Across the Baseline Language Coverage Set

As a power user,
I want Tokenizor to index supported repositories across the baseline language coverage set,
So that later retrieval workflows can rely on broad repository coverage instead of a narrow language subset.

**FRs implemented:** FR9

**Acceptance Criteria:**

**Given** a repository containing supported baseline languages
**When** an indexing run executes
**Then** Tokenizor discovers, filters, and records indexable files across the supported language coverage set
**And** unsupported or skipped files are handled explicitly without poisoning the run

**Given** indexing encounters per-file parse or extraction issues during a supported run
**When** the run continues
**Then** Tokenizor isolates affected files or artifacts safely
**And** the overall run remains inspectable rather than collapsing into silent corruption

### Story 2.3: Inspect Run Status, Health, and Live Progress

As an operator,
I want to inspect indexing run status, health, and live progress,
So that I can tell what active or recent indexing work is doing without guessing.

**FRs implemented:** FR12, FR13, FR36

**Acceptance Criteria:**

**Given** an indexing run exists
**When** I inspect run status
**Then** Tokenizor reports the run's current lifecycle state, health, and durable identifiers
**And** the response distinguishes active, completed, cancelled, failed, and blocked conditions explicitly

**Given** an indexing run is active
**When** I inspect progress
**Then** Tokenizor returns near-live progress information for that run
**And** the progress view remains available without requiring log scraping or hidden internal knowledge

### Story 2.4: Cancel an Active Indexing Run Safely

As an operator,
I want to cancel an active indexing run,
So that I can stop or restart work without leaving ambiguous operational state behind.

**FRs implemented:** FR14

**Acceptance Criteria:**

**Given** an indexing run is active and eligible for cancellation
**When** I request cancellation
**Then** Tokenizor records the cancellation request and transitions the run to a durable terminal cancellation state
**And** new inspection requests show that the run was cancelled explicitly

**Given** a run is already terminal or not cancellable
**When** I request cancellation
**Then** Tokenizor returns a clear non-cancellable outcome
**And** it does not silently mutate historical run state

### Story 2.5: Checkpoint Long-Running Indexing Work

As an operator,
I want to checkpoint long-running indexing work,
So that interrupted runs can preserve meaningful progress for later recovery.

**FRs implemented:** FR15

**Acceptance Criteria:**

**Given** an indexing run is active and has progressed far enough to checkpoint
**When** a checkpoint is created
**Then** Tokenizor durably records checkpoint state associated with the run
**And** later recovery logic can inspect that checkpoint without reconstructing it from logs

**Given** checkpoint creation is requested when the run is not eligible or durable state cannot be committed
**When** checkpointing is attempted
**Then** Tokenizor fails explicitly
**And** it does not report a checkpoint as successful before durability is achieved

### Story 2.6: Retry Mutating Operations with Deterministic Idempotency

As an operator,
I want supported indexing mutations to behave deterministically when retried,
So that repeated requests do not create conflicting or ambiguous operational state.

**FRs implemented:** FR16

**Acceptance Criteria:**

**Given** a supported mutating request is replayed with the same idempotency identity and equivalent effective input
**When** Tokenizor receives the replay
**Then** it returns the stored or equivalent prior outcome deterministically
**And** it does not create duplicate operational state

**Given** a request reuses an existing idempotency identity with materially different effective input
**When** Tokenizor receives the replay
**Then** it rejects the request as a conflicting replay
**And** the response makes the conflict explicit rather than silently reinterpreting the request

### Story 2.7: Re-Index Previously Indexed State After Source Changes

As a power user,
I want to re-index a previously indexed repository or workspace after source changes,
So that Tokenizor can refresh durable indexed state without making me rebuild everything manually.

**FRs implemented:** FR10

**Acceptance Criteria:**

**Given** a repository or workspace already has indexed state
**When** I request re-indexing after source changes
**Then** Tokenizor starts a new indexing run scoped to refreshing that known target
**And** the resulting run remains inspectable through normal run-status surfaces

**Given** the requested target is unknown, incompatible, or blocked by current mutation rules
**When** I request re-indexing
**Then** Tokenizor fails clearly with actionable guidance
**And** it does not silently create orphaned refresh state

### Story 2.8: Invalidate Indexed State for a Clean Rebuild

As an operator,
I want to invalidate indexed state for a repository or workspace,
So that I can force a clean rebuild when prior indexed data should no longer be trusted.

**FRs implemented:** FR11

**Acceptance Criteria:**

**Given** a repository or workspace has existing indexed state
**When** I request invalidation
**Then** Tokenizor marks the relevant indexed state as invalid for trusted use
**And** later status or retrieval-adjacent surfaces can distinguish that invalidated condition explicitly

**Given** invalidation is requested for an unknown target or during an unsafe conflicting mutation state
**When** the request is processed
**Then** Tokenizor rejects the request clearly
**And** it does not partially clear or silently corrupt authoritative state

## Epic 3: Trusted Code Discovery and Verified Retrieval

Users can search, outline, and retrieve verified code from indexed repositories through the core retrieval surface that AI workflows depend on.

### Story 3.1: Search Indexed Repositories by Text

As a power user,
I want to search indexed repositories by text content,
So that I can locate relevant code and artifacts without brute-force file rereads.

**FRs implemented:** FR17, FR23

**Acceptance Criteria:**

**Given** a repository has indexed searchable text state
**When** I run a text search query
**Then** Tokenizor returns matching results scoped to the requested repository or workspace
**And** the response remains explicit about unsupported, blocked, or invalid search conditions

**Given** the target repository is unindexed, invalidated, or otherwise not safely queryable
**When** I run a text search query
**Then** Tokenizor returns a clear non-trusted or unavailable outcome
**And** it does not pretend that an empty result means the repository was safely searched

### Story 3.2: Search Indexed Repositories by Symbol

As a power user,
I want to search indexed repositories by symbol,
So that I can navigate code structure through semantic lookup rather than raw filename guessing.

**FRs implemented:** FR18, FR23

**Acceptance Criteria:**

**Given** a repository has indexed symbol metadata
**When** I run a symbol search query
**Then** Tokenizor returns matching symbols with stable identifying context such as file path, kind, and location metadata
**And** the response remains scoped to authoritative indexed state

**Given** symbol metadata is partial, stale, or unavailable for the requested target
**When** I run a symbol search query
**Then** Tokenizor reports that trust or availability limitation explicitly
**And** it does not silently overstate symbol coverage

### Story 3.3: Retrieve a Structural Outline for a File

As an AI coding workflow,
I want to retrieve a structural outline for a file,
So that I can understand file shape quickly before deeper retrieval.

**FRs implemented:** FR19, FR23

**Acceptance Criteria:**

**Given** a file exists in authoritative indexed state
**When** I request its structural outline
**Then** Tokenizor returns the file outline with the indexed structural elements available for that file
**And** the response identifies the target file deterministically

**Given** the file is unindexed, invalidated, or affected by integrity issues
**When** I request its outline
**Then** Tokenizor returns an explicit blocked, degraded, or unavailable outcome
**And** it does not serve the outline as trusted if the underlying indexed state is not safe

### Story 3.4: Retrieve a Structural Outline for a Repository

As an AI coding workflow,
I want to retrieve a structural outline for a repository,
So that I can orient to codebase shape without manually re-exploring the tree from scratch.

**FRs implemented:** FR20, FR23

**Acceptance Criteria:**

**Given** a repository has authoritative indexed state
**When** I request its repository outline
**Then** Tokenizor returns the repository structure using the indexed outline surface
**And** the response is suitable for early-session codebase orientation

**Given** repository outline data is incomplete, invalidated, or blocked by trust conditions
**When** I request the outline
**Then** Tokenizor returns an explicit trust or availability state
**And** it does not quietly degrade into misleading partial trust

### Story 3.5: Retrieve Verified Source for a Symbol or Code Slice

As an AI coding workflow,
I want to retrieve verified source for a symbol or equivalent code slice,
So that I can rely on Tokenizor as a trustworthy source-serving layer.

**FRs implemented:** FR21, FR25, FR26, FR27, FR28

**Acceptance Criteria:**

**Given** indexed symbol metadata and raw CAS bytes are both available and span verification succeeds
**When** I request source for a symbol or equivalent code slice
**Then** Tokenizor returns the requested source as trusted retrieval
**And** the response indicates the verification-backed trust outcome explicitly

**Given** span verification fails or the underlying raw bytes cannot safely support the requested slice
**When** I request source retrieval
**Then** Tokenizor returns an explicit blocked, suspect, quarantined, or repair-required outcome
**And** it never serves the failed retrieval as trusted source

**Given** trusted retrieval is served from stored source content
**When** Tokenizor slices and returns the requested code span
**Then** the response is derived from byte-exact CAS-backed raw bytes without newline normalization or decode-reencode mutation
**And** the trusted result preserves exact raw source fidelity for the requested slice

### Story 3.6: Retrieve Multiple Symbols or Code Slices in One Request Flow

As an AI coding workflow,
I want to retrieve multiple symbols or code slices in one request flow,
So that I can gather related code context efficiently without repeating single-item retrieval overhead.

**FRs implemented:** FR22

**Acceptance Criteria:**

**Given** multiple requested symbols or code slices are resolvable from indexed state
**When** I request batched retrieval
**Then** Tokenizor returns a structured multi-result response for the requested set
**And** each item preserves its own trust or integrity outcome rather than collapsing the batch into one undifferentiated status

**Given** one or more requested items cannot be served as trusted retrieval
**When** batched retrieval is processed
**Then** Tokenizor reports item-level blocked, suspect, quarantined, or unavailable outcomes explicitly
**And** successfully verified items remain distinguishable from failed ones

### Story 3.7: Distinguish Trusted Retrieval from Repair-Required Results

As a power user,
I want retrieval responses to distinguish trusted results from repair-required or suspect results,
So that I can decide whether to proceed, retry later, or trigger repair.

**FRs implemented:** FR29

**Acceptance Criteria:**

**Given** a retrieval-adjacent query or source request completes
**When** Tokenizor returns the response
**Then** the result model exposes explicit trust or integrity state rather than a generic success boolean
**And** callers can tell whether the result is trusted, degraded, blocked, quarantined, or repair-required

**Given** the repository or file state requires operator intervention before trusted retrieval is possible
**When** Tokenizor returns the result
**Then** the response surfaces that action-required condition explicitly
**And** it does not hide recovery needs behind vague error text

### Story 3.8: Expose Baseline Retrieval Capabilities Through MCP

As an AI coding workflow,
I want Tokenizor's core search, outline, and retrieval capabilities available through MCP,
So that AI clients can use indexed discovery instead of repeated raw repository exploration.

**FRs implemented:** FR24

**Acceptance Criteria:**

**Given** Tokenizor is connected to an MCP-capable client
**When** the client invokes baseline retrieval tools
**Then** Tokenizor exposes text search, symbol search, file outline, repository outline, and verified source retrieval through MCP
**And** those MCP results preserve the same trust and integrity semantics as the underlying application layer

**Given** the requested retrieval cannot be served safely
**When** the MCP tool call completes
**Then** the response preserves explicit blocked, suspect, quarantined, or repair-required semantics as appropriate
**And** the protocol surface does not collapse trust-bearing domain outcomes into misleading generic success

## Epic 4: Recovery, Repair, and Operational Confidence

Users can recover interrupted work, inspect unhealthy or suspect state, trigger deterministic repair, and understand operational history well enough to restore trust safely.

### Story 4.1: Sweep Stale Leases and Interrupted State on Startup

As an operator,
I want Tokenizor to sweep stale leases, interrupted runs, and temporary recovery state on startup,
So that new mutating work does not begin on top of ambiguous operational state.

**FRs implemented:** FR37

**Acceptance Criteria:**

**Given** prior runs or leases were left stale by interruption or shutdown
**When** Tokenizor starts
**Then** it performs a startup recovery sweep before allowing new mutating operations
**And** it records the detected stale or interrupted conditions explicitly

**Given** startup detects unrecoverable or incompatible operational state
**When** the sweep completes
**Then** Tokenizor blocks unsafe mutation paths
**And** it reports actionable recovery or migration guidance

### Story 4.2: Resume Interrupted Indexing from Durable Checkpoints

As an operator,
I want interrupted indexing to resume from durable checkpoints when possible,
So that long-running work does not always restart from zero.

**FRs implemented:** FR30

**Acceptance Criteria:**

**Given** an interrupted indexing run has a valid checkpoint and compatible source state
**When** recovery is initiated
**Then** Tokenizor resumes the run from durable checkpoint state
**And** the resumed run remains inspectable as managed operational work

**Given** checkpoint-based recovery is not possible
**When** recovery is attempted
**Then** Tokenizor returns an explicit recovery outcome
**And** it points to deterministic re-index as the safe fallback

### Story 4.3: Move Mutable Run Durability to the SpacetimeDB Control Plane

As an operator,
I want runs, checkpoints, per-run durable file metadata, idempotency records, and recovery metadata to persist through the authoritative control plane,
So that recovery and operational state are durable, scalable, and aligned with the intended architecture.

**FRs implemented:** FR15, FR30, FR34

**Acceptance Criteria:**

**Given** Tokenizor creates or mutates indexing run state
**When** durable operational metadata is written
**Then** the write goes through the SpacetimeDB-backed control plane rather than direct interim local-registry mutation
**And** the resulting state remains inspectable through existing run APIs

**Given** an interrupted run is eligible for recovery
**When** resume occurs
**Then** Tokenizor uses persisted checkpoint and recovery metadata from the control-plane-backed durability path
**And** it does not depend on unsafe implicit reconstruction of mutable state from ad hoc local files

### Story 4.4: Trigger Deterministic Repair for Suspect or Incomplete State

As an operator,
I want to trigger deterministic repair flows for suspect, stale, or incomplete indexed state,
So that I can restore trusted retrieval without guessing which action is safe.

**FRs implemented:** FR31

**Acceptance Criteria:**

**Given** repository, run, or retrieval state is marked stale, suspect, quarantined, or incomplete
**When** I trigger repair
**Then** Tokenizor executes a deterministic repair path scoped to the affected state
**And** the repair action is recorded in operational history

**Given** repair cannot safely restore trust
**When** repair completes or fails
**Then** Tokenizor reports that explicit outcome
**And** it does not silently mark the state healthy

### Story 4.5: Inspect Repository Health and Repair-Required Conditions

As an operator,
I want to inspect repository health and repair-required conditions,
So that I can decide whether retrieval is safe or intervention is needed.

**FRs implemented:** FR32, FR35

**Acceptance Criteria:**

**Given** a repository has indexed operational state
**When** I inspect repository health
**Then** Tokenizor reports health, suspect conditions, and repair-required indicators explicitly
**And** the result distinguishes healthy, degraded, interrupted, quarantined, and invalid states

**Given** no health-impacting issues exist
**When** I inspect repository health
**Then** Tokenizor reports an explicit healthy state
**And** it does not rely on silence to imply safety

### Story 4.6: Preserve Operational History for Runs, Repairs, and Integrity Events

As an operator,
I want operational history preserved for runs, checkpoints, repairs, and integrity-related failures,
So that I can understand what happened and why the current state should or should not be trusted.

**FRs implemented:** FR34

**Acceptance Criteria:**

**Given** a run transition, checkpoint event, repair action, or integrity-significant failure occurs
**When** the event is recorded
**Then** Tokenizor persists audit-friendly operational history before reporting the transition as successful
**And** later inspection can reconstruct the relevant sequence of events

**Given** an operator is diagnosing a trust or recovery issue
**When** operational history is inspected
**Then** Tokenizor exposes enough structured detail to explain the current state
**And** it avoids leaking raw source content by default

### Story 4.7: Classify Action-Required States and Signal the Next Safe Action

As an operator,
I want stale, interrupted, suspect, repair-required, and degraded states classified explicitly with next-safe-action guidance,
So that I can respond correctly without mistaking action-required conditions for normal health.

**FRs implemented:** FR32, FR37

**Acceptance Criteria:**

**Given** a run or repository enters a stale, interrupted, suspect, repair-required, or degraded condition
**When** I inspect that state through operator or MCP-facing status surfaces
**Then** Tokenizor classifies the action-required state explicitly
**And** it distinguishes action-required conditions from normal healthy or terminal-complete states

**Given** a classified state requires intervention
**When** Tokenizor reports it
**Then** the response maps the condition to next-safe-action categories such as resume, repair, re-index, or migrate
**And** it does not hide the need for intervention behind generic error wording

## Epic 5: Retrieval-First Workflow Integration and Adoption

Users can connect Tokenizor to real AI coding workflows, make retrieval available early in a session, observe usage, preserve authority-safe integration boundaries, and access workflow and migration guidance.

### Story 5.1: Implement a Codex `AGENTS.md` Retrieval-First Bootstrap

As a power user of Codex,
I want a repository-local `AGENTS.md` bootstrap that directs Codex to use Tokenizor retrieval before broad repository exploration,
So that the baseline product proves retrieval-first behavior through a concrete Codex surface that can be verified in session traces.

**FRs implemented:** FR40, FR42

**Acceptance Criteria:**

**Given** Tokenizor MCP is configured for Codex and the repository contains the supported Tokenizor `AGENTS.md` bootstrap instructions
**When** the first repository-oriented task in a Codex session is processed
**Then** Codex can discover explicit instructions to use Tokenizor retrieval surfaces before broad raw file rereads or recursive repository listing
**And** the session emits at least one Tokenizor MCP retrieval call such as `get_repo_outline`, `get_file_outline`, `search_symbols`, `search_text`, or `get_symbol` before any assistant-directed broad repository reread or recursive file enumeration

**Given** the `AGENTS.md` bootstrap is present but Tokenizor MCP is unavailable or not attached
**When** Codex encounters the repository bootstrap instructions
**Then** the workflow reports explicit Tokenizor unavailability or setup failure
**And** it does not claim retrieval-first behavior was achieved for that session

### Story 5.2: Make Tokenizor Reachable at Session Start or First Repository Prompt

As an AI coding workflow,
I want Tokenizor reachable at session start or on the first repository-oriented prompt,
So that repository discovery and retrieval can influence exploration before broad file rereads begin.

**FRs implemented:** FR40, FR41

**Acceptance Criteria:**

**Given** a supported Codex workflow has Tokenizor MCP configured
**When** a Codex session starts in a repository or the first repository-oriented prompt is submitted
**Then** Tokenizor tools and resources are reachable in that same session without per-session manual reconfiguration
**And** Codex can invoke the configured Tokenizor integration path before asking the operator to reattach or reconfigure Tokenizor for that session

**Given** required repository, project, or workspace context is missing at session start
**When** Tokenizor reachability is attempted at session start or on the first repository-oriented prompt
**Then** Tokenizor returns an explicit missing-context failure that identifies the missing binding or active-context requirement
**And** it does not fabricate repository scope or silently fall back to unrelated context

### Story 5.3: Observe Retrieval Usage Behavior Locally in Active Workflows

As a product operator,
I want to observe whether Tokenizor retrieval capabilities are actually being used in active workflows through local-first signals,
So that I can tell whether retrieval-first behavior is happening in practice without requiring remote telemetry.

**FRs implemented:** FR43

**Acceptance Criteria:**

**Given** Tokenizor-enabled workflows are active
**When** usage observation is inspected
**Then** Tokenizor provides structured local signals showing whether retrieval capabilities were invoked in the workflow
**And** the signals are sufficient to distinguish meaningful usage from mere installation

**Given** observation data is unavailable or incomplete
**When** usage is inspected
**Then** Tokenizor reports that limitation explicitly
**And** it does not overclaim adoption success

### Story 5.4: Preserve Authority-Safe Integration Boundaries

As a system operator,
I want provider integrations to remain consumers of Tokenizor truth rather than owners of project, workspace, or retrieval state,
So that workflow adoption does not weaken trust boundaries.

**FRs implemented:** FR44

**Acceptance Criteria:**

**Given** a provider-facing integration invokes Tokenizor capabilities
**When** project, workspace, or retrieval context is resolved
**Then** Tokenizor remains the authoritative source of truth
**And** the integration cannot silently redefine that state

**Given** a client surface only partially supports the desired integration model
**When** Tokenizor adapts to that surface
**Then** the integration degrades safely
**And** trust, authority, and verification rules remain intact

### Story 5.5: Expose Baseline MCP Resources for Workflow Context

As an AI coding workflow,
I want baseline MCP resources such as repository outline, repository health, and run status,
So that workflow context can be consumed through more than tools alone.

**FRs implemented:** FR45

**Acceptance Criteria:**

**Given** Tokenizor is connected to an MCP-capable client
**When** the client requests baseline resources
**Then** Tokenizor exposes repository outline, repository health, and run status resources
**And** the resources reflect authoritative Tokenizor state

**Given** a client only partially supports MCP resource features
**When** resource access is attempted
**Then** Tokenizor degrades safely
**And** tool-based or other supported access paths remain consistent with the same underlying truth

### Story 5.6: Provide Workflow and Operational Guidance

As a power user,
I want guidance for primary AI coding workflows plus operational recovery and trust-boundary behavior,
So that I can adopt Tokenizor correctly and maintain it confidently.

**FRs implemented:** FR46, FR47

**Acceptance Criteria:**

**Given** I am adopting Tokenizor in a supported workflow
**When** I access guidance
**Then** Tokenizor provides practical instructions for workflow usage, indexing, recovery, repair, and trust boundaries
**And** the guidance matches the actual baseline product behavior

**Given** operator guidance changes as capabilities mature
**When** documentation is updated
**Then** it remains aligned with current supported behavior
**And** it avoids promising unsupported workflow magic

### Story 5.7: Provide Migration Guidance from `jcodemunch-mcp`

As a migrating user,
I want parity and migration guidance from `jcodemunch-mcp`,
So that I can adopt Tokenizor with realistic expectations about what is equivalent, improved, or intentionally different.

**FRs implemented:** FR48

**Acceptance Criteria:**

**Given** I am familiar with `jcodemunch-mcp`
**When** I review migration guidance
**Then** Tokenizor explains baseline parity, differences, and adoption expectations clearly
**And** it does not present itself as a vague clone without defined behavior changes

**Given** some parity areas are incomplete or staged
**When** migration guidance is presented
**Then** the limitations are explicit
**And** users are not misled about current support

### Story 5.8: Offer a Curated Prompt Surface Where It Improves Adoption

As an AI coding workflow,
I want a curated prompt surface where it materially improves retrieval usage,
So that prompts can help reinforce retrieval-first behavior without becoming the primary product surface.

**FRs implemented:** FR49

**Acceptance Criteria:**

**Given** a client supports MCP prompts or equivalent prompt surfacing
**When** Tokenizor exposes curated prompts
**Then** those prompts reinforce retrieval-first usage behavior
**And** they remain secondary to tools and resources rather than replacing them

**Given** a client does not support the prompt surface
**When** Tokenizor is used in that environment
**Then** the integration still functions through supported surfaces
**And** prompt absence does not break baseline workflow behavior
