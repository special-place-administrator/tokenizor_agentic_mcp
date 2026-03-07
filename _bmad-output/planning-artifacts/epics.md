---
stepsCompleted:
  - step-01-validate-prerequisites
  - step-02-design-epics
  - step-03-create-stories
  - step-04-final-validation
inputDocuments:
  - _bmad-output/planning-artifacts/prd.md
  - _bmad-output/planning-artifacts/architecture.md
  - _bmad-output/planning-artifacts/product-brief-tokenizor_agentic_mcp-2026-03-07.md
  - docs/source-tree-analysis.md
  - docs/provider_cli_integration_research.md
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

- Build forward from the existing Rust 2024 Cargo and `rmcp` scaffold rather than re-bootstrap the repository from scratch.
- Keep a layered boundary across `protocol`, `application`, `domain`, `storage`, `indexing`, `parsing`, and `observability`, with domain logic remaining testable without MCP or database runtime dependencies.
- Use SpacetimeDB as the authoritative control plane for repositories, workspaces, index runs, checkpoints, leases, idempotency, health, repair, file metadata, and symbol metadata.
- Use a local byte-exact content-addressed store for raw file bytes and byte-sensitive derived artifacts; raw bytes must be written exactly as read with no newline normalization or decode/re-encode.
- Require byte/span verification against stored raw bytes before trusted retrieval is served.
- Treat quarantine as a first-class outcome for suspect files, symbols, parse outputs, blobs, or retrieval spans; quarantine state must be inspectable and repairable.
- Mutating operations such as indexing, repair, checkpoint, and invalidation must use canonical request hashing with deterministic idempotency-key replay behavior.
- Long-running mutating operations must return durable run identifiers, expose progress, cancellation, and checkpoint visibility, and preserve explicit terminal states.
- Startup must perform compatibility checks and recovery sweeps before new mutating work begins; shutdown must never be treated as a safe persistence boundary.
- Preserve CLI/operator commands such as `init`, `doctor`, `migrate`, and `run` as first-class baseline product surfaces alongside MCP.
- Baseline MCP support must include tools and a minimal useful resource set, with prompts designed in from the start where they materially improve adoption.
- Retrieval-related query results must expose explicit trust and integrity state where relevant instead of collapsing trust-bearing outcomes into generic success/failure.
- Baseline delivery must include at least one concrete retrieval-adoption mechanism in a primary workflow, not just generic MCP availability.
- Tokenizor remains authoritative for active project/workspace resolution and related operational truth; provider integrations are consumers and must not become the source of truth for project, workspace, retrieval, or operational state.
- Brownfield sequencing should reflect current implementation maturity: storage and health scaffolding already exist, while the indexing engine, parsing stack, run lifecycle, and retrieval surfaces still need substantial implementation depth.
- Epic sequencing should prioritize the adoption-critical baseline providers and surfaces identified in research: Codex and Claude first, with MCP as the universal minimum and provider-specific adapters layered on top later.
- The product must make retrieval the default path for repository outline, symbol search, text search, verified source retrieval, and codebase exploration before brute-force file rereads.

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
Users can start, monitor, checkpoint, cancel, and safely retry indexing work with durable run identities and deterministic mutation behavior.
**FRs covered:** FR8, FR9, FR10, FR11, FR12, FR13, FR14, FR15, FR16, FR36

### Epic 3: Trusted Code Discovery and Verified Retrieval
Users can access Tokenizor's core search, outline, and verified retrieval value through the baseline retrieval surface used by AI workflows, instead of brute-force file exploration.
**FRs covered:** FR17, FR18, FR19, FR20, FR21, FR22, FR23, FR24, FR25, FR26, FR27, FR28, FR29

### Epic 4: Recovery, Repair, and Operational Confidence
Users can recover interrupted work, diagnose unhealthy or suspect state, and repair the system without losing trust in the indexed repository.
**FRs covered:** FR30, FR31, FR32, FR34, FR35, FR37

### Epic 5: Retrieval-First Workflow Integration and Adoption
Users can make Tokenizor the first retrieval path in real AI coding workflows through adoption mechanisms, early-session reachability, observable usage behavior, authority-safe integrations, and supporting guidance.
**FRs covered:** FR40, FR41, FR42, FR43, FR44, FR45, FR46, FR47, FR48, FR49

**Epic 5 story ordering priority:**
- concrete retrieval-adoption mechanism in a primary workflow
- early-session reachability and retrieval-first behavior
- usage observation and instrumentation
- authority-safe integration behavior
- documentation, migration guidance, and prompt or resource breadth

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

**Given** readiness checks fail
**When** I run Tokenizor
**Then** Tokenizor refuses to start serving MCP
**And** it returns explicit and actionable failure output instead of partial startup

### Story 1.3: Initialize a Repository as a Durable Project Workspace

As a power user,
I want to initialize a local repository into Tokenizor,
So that the project and current workspace become durable identities across sessions.

**FRs implemented:** FR1, FR2, FR33, FR38

**Acceptance Criteria:**

**Given** I am inside a supported local repository or provide an explicit repository path
**When** I run the initialization flow
**Then** Tokenizor creates or reuses a durable project identity and registers the current workspace
**And** the result remains stable across later sessions

**Given** the same repository is initialized again with equivalent inputs
**When** the operation is replayed
**Then** Tokenizor behaves idempotently
**And** it does not create duplicate project or workspace records

### Story 1.4: Inspect Known Projects and Workspaces

As an operator,
I want to inspect registered projects and workspaces,
So that I can understand what Tokenizor currently knows and manage it safely.

**FRs implemented:** FR5, FR39

**Acceptance Criteria:**

**Given** one or more projects and workspaces are registered
**When** I request the current registry view
**Then** Tokenizor lists known projects and associated workspaces in scriptable output
**And** the output is clear enough for advanced local maintenance

**Given** no projects are registered
**When** I inspect the registry
**Then** Tokenizor returns an explicit empty-state response
**And** it does not imply hidden or partial state

### Story 1.5: Resolve the Active Workspace from Context or Override

As an AI coding workflow,
I want Tokenizor to resolve the active project and workspace from the current directory or an explicit override,
So that retrieval and operations target the correct repository context without manual rediscovery.

**FRs implemented:** FR4

**Acceptance Criteria:**

**Given** the current working directory belongs to a registered workspace
**When** active context resolution is requested
**Then** Tokenizor returns the matching project and workspace as authoritative context
**And** provider clients do not become the source of truth for that resolution

**Given** an explicit override is provided
**When** resolution is requested
**Then** Tokenizor uses the override deterministically
**And** it reports an error if the override is unknown or conflicts with registered state

### Story 1.6: Add Additional Workspaces or Worktrees to an Existing Project

As an operator,
I want to associate additional workspaces or worktrees with an existing project,
So that Tokenizor preserves one durable project identity across related working copies.

**FRs implemented:** FR3

**Acceptance Criteria:**

**Given** a project already exists in Tokenizor
**When** I register an additional workspace or worktree for that project
**Then** Tokenizor links it to the existing project identity
**And** it does not create an unrelated duplicate project

**Given** multiple workspaces are associated with one project
**When** I inspect the registry
**Then** all linked workspaces are shown clearly
**And** their relationship to the shared project is explicit

### Story 1.7: Migrate or Update Workspace State Safely

As an operator,
I want to migrate or update workspace state when lifecycle changes occur,
So that Tokenizor remains accurate without corrupting durable context.

**FRs implemented:** FR6

**Acceptance Criteria:**

**Given** a registered workspace path or local lifecycle state has changed
**When** I run the migration or update flow
**Then** Tokenizor updates durable state safely
**And** preserves continuity for later sessions where possible

**Given** a requested migration cannot be completed safely
**When** the operation fails
**Then** Tokenizor reports the failure explicitly
**And** it does not silently corrupt project or workspace state

## Epic 2: Durable Indexing and Run Control

Users can start, monitor, checkpoint, cancel, and safely retry indexing work with durable run identities and deterministic mutation behavior.

### Story 2.1: Start an Indexed Run with Durable Run Identity

As a power user,
I want to start an indexing run for a repository or workspace and receive a durable run ID,
So that I can track and manage indexing work over time.

**FRs implemented:** FR8, FR16

**Acceptance Criteria:**

**Given** a registered repository or workspace is ready for indexing
**When** I start an indexing operation
**Then** Tokenizor creates a durable run record, assigns an explicit initial lifecycle state, and returns its run ID
**And** the run is associated with the correct project and workspace context plus durable ownership or lease semantics where applicable

**Given** the indexing request is retried with the same idempotent inputs
**When** Tokenizor processes the request
**Then** it returns the stored result for the same effective request
**And** it does not create a duplicate run

### Story 2.2: Execute Indexing for the Initial Quality-Focus Language Set

As a power user,
I want Tokenizor to execute indexing for the initial quality-focus language set (`Rust`, `Python`, `JavaScript / TypeScript`, and `Go`),
So that the first trusted retrieval slice is implementable at high quality.

**FRs implemented:** FR9

**Acceptance Criteria:**

**Given** a repository contains eligible `Rust`, `Python`, `JavaScript / TypeScript`, or `Go` files
**When** an indexing run executes
**Then** Tokenizor discovers and processes eligible files under bounded concurrency
**And** the run records explicit per-file processing progress within the correct run and repository context

**Given** a repository also contains files outside the initial quality-focus language set
**When** language eligibility is evaluated for the run
**Then** Tokenizor marks only `Rust`, `Python`, `JavaScript / TypeScript`, and `Go` files as in scope for this story
**And** it does not claim indexing support for other languages during this execution slice

**Given** some files fail parsing or extraction during the run
**When** processing continues
**Then** the affected files are isolated safely
**And** the full run is not treated as globally poisoned by a single file failure

### Story 2.3: Persist File-Level Indexing Outputs and Symbol/File Metadata for the Initial Quality-Focus Language Set

As a power user,
I want Tokenizor to persist file-level indexing outputs and symbol/file metadata for the initial quality-focus language set,
So that the first bounded indexing slice produces durable, inspectable indexing state instead of only transient run activity.

**FRs implemented:** FR9

**Acceptance Criteria:**

**Given** an indexing run successfully processes eligible `Rust`, `Python`, `JavaScript / TypeScript`, or `Go` files
**When** file-level indexing results are committed
**Then** Tokenizor persists durable file records plus symbol and file metadata for those processed files
**And** the persisted outputs are linked to the correct repository, workspace, and run context

**Given** a processed file has no extractable symbols or produces suspect metadata during persistence
**When** commit-time validation runs
**Then** Tokenizor records an explicit file-level outcome such as empty-symbol, failed, or quarantined
**And** it does not silently claim trusted symbol coverage for that file

**Given** the repository contains files outside the initial quality-focus language set
**When** persistence for this story completes
**Then** Tokenizor persists usable indexing outputs only for the in-scope initial quality-focus slice
**And** it does not represent out-of-scope languages as supported persisted outputs for this story

### Story 2.4: Extend Indexing Through a Repeatable Broader-Language Onboarding Pattern

As a power user,
I want Tokenizor to extend indexing through a repeatable broader-language onboarding pattern,
So that additional languages can be added as implementation-sized follow-on slices instead of one oversized parity story.

**FRs implemented:** FR9

**Acceptance Criteria:**

**Given** one explicitly named broader-language slice outside the initial quality-focus set is onboarded through the new pattern
**When** an indexing run executes against a repository containing that slice
**Then** Tokenizor discovers, processes, and persists usable file-level indexing outputs plus symbol/file metadata for the onboarded slice
**And** the onboarded slice uses the same run, commit, and inspection contracts as the initial quality-focus stories without redesigning the overall indexing lifecycle

**Given** files in the onboarded broader-language slice fail parsing, extraction, or commit-time validation
**When** processing continues
**Then** Tokenizor records explicit per-file failure outcomes for those files
**And** the onboarding pattern reuses the shared failure-isolation behavior rather than requiring one-off handling for that language

**Given** a repository also contains broader baseline languages that have not yet been onboarded through the pattern
**When** indexing completes
**Then** Tokenizor reports only the explicitly onboarded slice as supported for that run
**And** it preserves an inspectable not-yet-supported outcome for the remaining broader-language files instead of implying full baseline parity coverage

### Story 2.5: Inspect Run Status and Health

As an operator,
I want to inspect indexing run status and health,
So that I can understand whether active or recent indexing work is healthy, degraded, or needs intervention.

**FRs implemented:** FR12, FR36

**Acceptance Criteria:**

**Given** an indexing run is active or recently completed
**When** I request run status
**Then** Tokenizor returns the run lifecycle state plus the current health classification for that run
**And** the response distinguishes active, completed, cancelled, interrupted, degraded, and unhealthy conditions rather than collapsing them into a generic status

**Given** a run is interrupted, degraded, or unhealthy
**When** I inspect run status
**Then** Tokenizor reports that condition explicitly
**And** it exposes enough state for an operator to determine whether cancellation, repair, or later recovery work is required

### Story 2.6: Observe Live or Near-Live Indexing Progress

As an operator,
I want live or near-live progress visibility for active indexing runs,
So that I can tell whether work is advancing without waiting for terminal completion.

**FRs implemented:** FR13

**Acceptance Criteria:**

**Given** an indexing run is active
**When** I request run progress
**Then** Tokenizor returns the current phase plus concrete progress fields such as processed work, remaining work, or last completed checkpoint
**And** the reported progress state is no more than 1 second behind the actual run state under normal operation

**Given** a run is no longer active
**When** I request run progress
**Then** Tokenizor returns the last durable progress snapshot or terminal outcome explicitly
**And** it does not present completed, cancelled, or failed work as if it were still live

### Story 2.7: Cancel an Active Indexing Run Safely

As an operator,
I want to cancel an active indexing run,
So that I can stop or restart work without leaving ambiguous run state behind.

**FRs implemented:** FR14

**Acceptance Criteria:**

**Given** an indexing run is active
**When** I request cancellation
**Then** Tokenizor transitions the run into an explicit cancelled terminal state
**And** cancellation is visible through later run inspection

**Given** a run is already terminal
**When** cancellation is requested
**Then** Tokenizor responds deterministically
**And** it does not create contradictory run state

### Story 2.8: Checkpoint Long-Running Indexing Work

As an operator,
I want to checkpoint indexing progress during long-running work,
So that interrupted runs can later resume from durable progress.

**FRs implemented:** FR15

**Acceptance Criteria:**

**Given** a long-running indexing run is in progress
**When** a checkpoint is created
**Then** Tokenizor persists checkpoint state durably before reporting success
**And** the checkpoint is associated with the correct run identity

**Given** no valid active run exists
**When** checkpoint creation is requested
**Then** Tokenizor returns an explicit failure
**And** it does not create orphan checkpoint state

### Story 2.9: Re-index Managed Repository or Workspace State Deterministically

As an operator,
I want to re-index managed repository or workspace state deterministically,
So that Tokenizor can refresh indexed state after source changes without ambiguous run behavior.

**FRs implemented:** FR10, FR16

**Acceptance Criteria:**

**Given** an indexed repository or workspace has changed
**When** I trigger re-indexing
**Then** Tokenizor starts a new managed run against the correct target
**And** prior state remains inspectable until replacement policy is applied

**Given** the re-index request is replayed with the same effective inputs
**When** Tokenizor processes the request
**Then** it behaves idempotently
**And** it does not create conflicting managed refresh work

### Story 2.10: Invalidate Indexed State So It Is No Longer Trusted

As an operator,
I want to invalidate indexed state that should no longer be trusted,
So that retrieval flows cannot silently use stale or unsafe repository state.

**FRs implemented:** FR11

**Acceptance Criteria:**

**Given** I request invalidation for a repository or workspace
**When** the invalidation is processed
**Then** Tokenizor marks the indexed state as invalid for trusted use
**And** later retrieval flows do not silently treat invalidated state as healthy

**Given** invalidated state exists
**When** I inspect repository or run status
**Then** Tokenizor reports that trust-impacting condition explicitly
**And** the system preserves a clear path toward re-index or repair

### Story 2.11: Reject Conflicting Idempotent Replays

As an operator,
I want conflicting replays of idempotent indexing mutations to fail deterministically,
So that retries cannot silently mutate state under a reused idempotency identity.

**FRs implemented:** FR16

**Acceptance Criteria:**

**Given** a mutating indexing-related request has already been recorded with an idempotency key
**When** the same key is replayed with different effective inputs
**Then** Tokenizor rejects the replay deterministically
**And** it preserves the original request record and outcome

**Given** the same key is replayed with the same effective inputs
**When** Tokenizor processes the request
**Then** it returns the stored outcome
**And** it does not execute a second conflicting mutation

## Epic 3: Trusted Code Discovery and Verified Retrieval

Users can access Tokenizor's core search, outline, and verified retrieval value through the baseline retrieval surface used by AI workflows, instead of brute-force file exploration.

### Story 3.1: Search Indexed Repositories by Text

As an AI coding user,
I want to search indexed repositories by text,
So that I can find relevant code locations without brute-force file rereads.

**FRs implemented:** FR17, FR23

**Acceptance Criteria:**

**Given** a repository has indexed searchable content
**When** I perform a text search
**Then** Tokenizor returns matching results scoped to the correct repository or workspace context
**And** the response is fast enough to support normal coding workflow use

**Given** no matches exist
**When** I perform a text search
**Then** Tokenizor returns an explicit empty result
**And** it does not imply stale or hidden matches

### Story 3.2: Search Indexed Repositories by Symbol

As an AI coding user,
I want to search indexed repositories by symbol,
So that I can navigate to relevant code structures quickly.

**FRs implemented:** FR18, FR23

**Acceptance Criteria:**

**Given** symbol metadata exists for indexed files
**When** I search by symbol
**Then** Tokenizor returns matching symbol results for the correct project or workspace context
**And** symbol results include enough metadata to support further retrieval or navigation

**Given** symbol extraction is incomplete or unavailable for some files
**When** I search by symbol
**Then** Tokenizor returns the best valid results available
**And** it does not overstate coverage for missing symbol data

### Story 3.3: Retrieve File and Repository Outlines

As an AI coding user,
I want to retrieve structural outlines for files and repositories,
So that I can understand code organization quickly before reading raw files.

**FRs implemented:** FR19, FR20, FR23

**Acceptance Criteria:**

**Given** indexed file and repository structure metadata exists
**When** I request a file outline or repository outline
**Then** Tokenizor returns the requested structural view for the active context
**And** the response distinguishes missing outline data from valid empty structure

**Given** a requested file or repository is not known to the active context
**When** I request an outline
**Then** Tokenizor returns an explicit failure or not-found result
**And** it does not silently fall back to unrelated scope

### Story 3.4: Expose the Full Baseline Retrieval Slice Through MCP

As an AI coding workflow,
I want Tokenizor's core search, outline, and verified retrieval capabilities exposed through the baseline MCP retrieval surface,
So that trusted repository discovery and grounded code retrieval are usable from the primary AI-facing entrypoint.

**FRs implemented:** FR24

**Acceptance Criteria:**

**Given** Tokenizor is running through its baseline operator entrypoint
**When** an MCP client connects
**Then** baseline MCP tools for search, outline, verified symbol retrieval, and batched retrieval are available
**And** those tools resolve against Tokenizor's authoritative project and workspace context

**Given** the MCP client invokes retrieval tools without a valid active context
**When** the request is processed
**Then** Tokenizor returns an explicit actionable failure
**And** it does not let the client silently redefine repository truth

### Story 3.5: Retrieve Verified Source for a Symbol or Code Slice

As an AI coding user,
I want to retrieve source for a symbol or code slice from indexed content,
So that I can rely on returned code as trustworthy retrieval rather than guessed output.

**FRs implemented:** FR21, FR25, FR28, FR29

**Acceptance Criteria:**

**Given** indexed symbol or code-slice metadata points to stored raw bytes
**When** I request source retrieval
**Then** Tokenizor verifies the requested span against byte-exact stored content before serving trusted output
**And** the result includes explicit modeled trust or integrity outcomes where relevant rather than generic protocol errors

**Given** the requested source passes verification
**When** retrieval completes
**Then** Tokenizor returns the verified source slice
**And** it preserves exact raw source fidelity

### Story 3.6: Block or Quarantine Suspect Retrieval

As an AI coding user,
I want suspect retrieval to fail explicitly instead of being served as trustworthy,
So that integrity problems do not silently poison my coding workflow.

**FRs implemented:** FR26, FR27, FR29

**Acceptance Criteria:**

**Given** retrieval verification fails because of stale spans, corrupted metadata, or byte mismatch
**When** source retrieval is requested
**Then** Tokenizor blocks, quarantines, or marks the result suspect explicitly
**And** it does not serve the result as trusted code

**Given** a retrieval result is blocked or quarantined
**When** the result is returned
**Then** Tokenizor exposes actionable trust or integrity state
**And** the response makes repair or re-index implications understandable

### Story 3.7: Retrieve Multiple Symbols or Code Slices in One Workflow

As an AI coding workflow,
I want to retrieve multiple symbols or code slices in one request path,
So that I can gather grounded code context efficiently.

**FRs implemented:** FR22

**Acceptance Criteria:**

**Given** multiple valid retrieval targets exist in the active context
**When** I request batched retrieval
**Then** Tokenizor returns each result with independent trust or integrity state where relevant
**And** one failed item does not silently invalidate unrelated successful items

**Given** some requested items are missing or suspect
**When** batched retrieval completes
**Then** Tokenizor reports mixed outcomes explicitly
**And** it preserves determinism about which items were trusted, blocked, or absent

## Epic 4: Recovery, Repair, and Operational Confidence

Users can recover interrupted work, diagnose unhealthy or suspect state, and repair the system without losing trust in the indexed repository.

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

### Story 4.3: Trigger Deterministic Repair for Suspect or Incomplete State

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

### Story 4.4: Inspect Repository Health and Repair-Required Conditions

As an operator,
I want to inspect repository health and repair-required conditions,
So that I can decide whether retrieval is safe or intervention is needed.

**FRs implemented:** FR35

**Acceptance Criteria:**

**Given** a repository has indexed operational state
**When** I inspect repository health
**Then** Tokenizor reports health, suspect conditions, and repair-required indicators explicitly
**And** the result distinguishes healthy, degraded, interrupted, quarantined, and invalid states

**Given** no health-impacting issues exist
**When** I inspect repository health
**Then** Tokenizor reports an explicit healthy state
**And** it does not rely on silence to imply safety

### Story 4.5: Preserve Operational History for Runs, Repairs, and Integrity Events

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

### Story 4.6: Classify Action-Required States and Signal the Next Safe Action

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

Users can make Tokenizor the first retrieval path in real AI coding workflows through adoption mechanisms, early-session reachability, observable usage behavior, authority-safe integrations, and supporting guidance.

### Story 5.1: Implement a Codex `AGENTS.md` Retrieval-First Bootstrap

As a power user of Codex,
I want a repository-local `AGENTS.md` bootstrap that directs Codex to use Tokenizor MCP retrieval before broad repository exploration,
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

**FRs implemented:** FR41

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

### Story 5.7: Provide Migration Guidance from jcodemunch-mcp

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
