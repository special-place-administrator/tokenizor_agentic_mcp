# Story 4.6: Preserve Operational History for Runs, Repairs, and Integrity Events

Status: in-progress

<!-- Note: Validation is optional. Run validate-create-story for quality check before dev-story. -->

## Story

As an operator,
I want operational history preserved for runs, checkpoints, repairs, and integrity-related failures,
so that I can understand what happened and why the current state should or should not be trusted.

**FRs implemented:** FR34

- **FR34**: The system can preserve operational history for runs, checkpoints, repairs, and integrity-related failures so users can understand what happened.

## Acceptance Criteria

1. **Given** a run transition, checkpoint event, repair action, or integrity-significant failure occurs **When** the event is recorded **Then** Tokenizor persists audit-friendly operational history before reporting the transition as successful **And** later inspection can reconstruct the relevant sequence of events
2. **Given** an operator is diagnosing a trust or recovery issue **When** operational history is inspected **Then** Tokenizor exposes enough structured detail to explain the current state **And** it avoids leaking raw source content by default

## Tasks / Subtasks

### Phase 1: Domain types for operational history

- [ ] Task 1.1: Define `OperationalEvent` and `OperationalEventKind` in `src/domain/index.rs` (AC: 1, 2)
  - [ ] 1.1.1: Define `OperationalEventKind` enum with variants:
    - `RunStarted { run_id: String, mode: IndexRunMode }` — run transitioned to Running
    - `RunCompleted { run_id: String, status: IndexRunStatus, files_processed: usize, error_summary: Option<String> }` — run reached a terminal state (Succeeded, Failed, Cancelled, Aborted)
    - `RunInterrupted { run_id: String, reason: String }` — run transitioned to Interrupted (startup sweep or signal)
    - `CheckpointCreated { run_id: String, cursor: String, files_committed: usize }` — checkpoint durably written
    - `RepairPerformed { scope: RepairScope, previous_status: RepositoryStatus, outcome: RepairOutcome, detail: String }` — repair action completed (replaces standalone RepairEvent recording path)
    - `RepositoryStatusChanged { previous: RepositoryStatus, current: RepositoryStatus, trigger: String }` — repository status transition (invalidation, quarantine, reindex-completion, repair-restoration)
    - `IntegrityEvent { run_id: Option<String>, relative_path: Option<String>, kind: IntegrityEventKind, detail: String }` — integrity-significant occurrence (quarantine, verification failure, suspect detection)
    - `StartupSweepCompleted { stale_runs_found: usize, actions_taken: Vec<String> }` — startup recovery sweep finished
  - [ ] 1.1.2: Define `IntegrityEventKind` enum with variants: `Quarantined`, `VerificationFailed`, `SuspectDetected`
  - [ ] 1.1.3: Define `OperationalEvent` struct with fields: `repo_id: String`, `kind: OperationalEventKind`, `timestamp_unix_ms: u64`
  - [ ] 1.1.4: Implement `OperationalEvent::event_name(&self) -> &'static str` method returning dot-separated names per architecture convention:
    - `RunStarted` → `"run.started"`
    - `RunCompleted` with Succeeded → `"run.succeeded"`, with Failed → `"run.failed"`, with Cancelled → `"run.cancelled"`, with Aborted → `"run.aborted"`
    - `RunInterrupted` → `"run.interrupted"`
    - `CheckpointCreated` → `"checkpoint.created"`
    - `RepairPerformed` → `"repair.completed"`
    - `RepositoryStatusChanged` → `"repository.status_changed"`
    - `IntegrityEvent` with Quarantined → `"integrity.quarantined"`, with VerificationFailed → `"integrity.verification_failed"`, with SuspectDetected → `"integrity.suspect_detected"`
    - `StartupSweepCompleted` → `"startup.sweep_completed"`
  - [ ] 1.1.5: All new types derive `Clone, Debug, Serialize, Deserialize, PartialEq, Eq` and follow existing domain type conventions; serde `rename_all = "snake_case"` on enums

- [ ] Task 1.2: Define `OperationalEventFilter` struct (AC: 2)
  - [ ] 1.2.1: Fields: `category: Option<String>` (filter by event_name prefix, e.g. `"run"`, `"repair"`, `"integrity"`), `since_unix_ms: Option<u64>` (events after this timestamp), `limit: Option<usize>` (max events returned, default 50)
  - [ ] 1.2.2: Derive `Clone, Debug, Default`

### Phase 2: ControlPlane trait extension for operational history

- [ ] Task 2.1: Add operational event methods to `ControlPlane` trait (AC: 1)
  - [ ] 2.1.1: Add `fn save_operational_event(&self, event: &OperationalEvent) -> Result<()>` to the `ControlPlane` trait
  - [ ] 2.1.2: Add `fn get_operational_events(&self, repo_id: &str, filter: &OperationalEventFilter) -> Result<Vec<OperationalEvent>>` to the `ControlPlane` trait

- [ ] Task 2.2: Implement on `InMemoryControlPlane` (AC: 1)
  - [ ] 2.2.1: Add `operational_events: Vec<OperationalEvent>` field to `InMemoryState`
  - [ ] 2.2.2: `save_operational_event`: push event to vec
  - [ ] 2.2.3: `get_operational_events`: filter by repo_id, apply category prefix filter on `event_name()`, apply `since_unix_ms` filter, sort by timestamp descending, apply limit

- [ ] Task 2.3: Implement on `RegistryBackedControlPlane` (AC: 1)
  - [ ] 2.3.1: Add `operational_events: Vec<OperationalEvent>` field to `RegistryData` with `#[serde(default)]` for backward compatibility
  - [ ] 2.3.2: `save_operational_event`: append to vec, persist to registry JSON
  - [ ] 2.3.3: `get_operational_events`: filter by repo_id, apply filters, sort, limit

- [ ] Task 2.4: Implement on `SpacetimeControlPlane` (AC: 1)
  - [ ] 2.4.1: `save_operational_event`: serialize event to JSON and write via SpacetimeDB reducer call (follow the pattern from existing `insert_run` / `update_run_status` SpacetimeDB writes)
  - [ ] 2.4.2: `get_operational_events`: query SpacetimeDB for events matching repo_id and filters
  - [ ] 2.4.3: If SpacetimeDB table does not yet have an `operational_events` schema, use the existing `pending_write_warning` pattern and persist to the registry fallback path — document as known technical debt for SpacetimeDB schema migration

- [ ] Task 2.5: Implement on `RunManagerPersistenceAdapter` (AC: 1)
  - [ ] 2.5.1: Delegate `save_operational_event` to inner control plane with `NotFound` suppression matching the pattern from 4.3 review fix M7
  - [ ] 2.5.2: Delegate `get_operational_events` to inner control plane

- [ ] Task 2.6: Refactor `save_repair_event` / `get_repair_events` as wrappers (AC: 1)
  - [ ] 2.6.1: Implement `save_repair_event` as: convert `RepairEvent` to `OperationalEvent` with `OperationalEventKind::RepairPerformed`, delegate to `save_operational_event`
  - [ ] 2.6.2: Implement `get_repair_events` as: call `get_operational_events` with category filter `"repair"`, convert back to `Vec<RepairEvent>`
  - [ ] 2.6.3: Keep `save_repair_event` / `get_repair_events` signatures on the trait for backward compatibility — callers like `repair_repository()` and `inspect_repository_health()` do not change
  - [ ] 2.6.4: Migrate InMemoryControlPlane's standalone `repair_events: Vec<RepairEvent>` to the unified `operational_events` vec
  - [ ] 2.6.5: Migrate RegistryBackedControlPlane's `repair_events` section to `operational_events` — keep `repair_events` deserialization with `#[serde(default)]` for backward-compatible reads from older registry files; new writes go through `operational_events`

- [ ] Task 2.7: Update test fakes in `src/application/deployment.rs` (AC: 1)
  - [ ] 2.7.1: Add `save_operational_event` and `get_operational_events` stubs to the ControlPlane fake used in deployment tests (pattern: `unreachable!("not used in deployment tests")` or minimal stub)

### Phase 3: Instrument state transitions in RunManager

- [ ] Task 3.1: Record run lifecycle events (AC: 1)
  - [ ] 3.1.1: In `start_indexing_run()` (or wherever the run transitions to `Running`): record `OperationalEventKind::RunStarted { run_id, mode }`
  - [ ] 3.1.2: In the indexing pipeline completion path (where run transitions to `Succeeded` / `Failed` / `Aborted`): record `OperationalEventKind::RunCompleted { run_id, status, files_processed, error_summary }`
  - [ ] 3.1.3: In `cancel_run()`: record `RunCompleted` with status `Cancelled`
  - [ ] 3.1.4: In `startup_sweep()` where runs transition to `Interrupted`: record `OperationalEventKind::RunInterrupted { run_id, reason }` for each stale run transitioned
  - [ ] 3.1.5: At the end of `startup_sweep()`: record `OperationalEventKind::StartupSweepCompleted { stale_runs_found, actions_taken }`

- [ ] Task 3.2: Record checkpoint events (AC: 1)
  - [ ] 3.2.1: In `checkpoint_run()` after successful checkpoint write: record `OperationalEventKind::CheckpointCreated { run_id, cursor, files_committed }`

- [ ] Task 3.3: Record repository status change events (AC: 1)
  - [ ] 3.3.1: In `invalidate_repository()`: record `OperationalEventKind::RepositoryStatusChanged { previous, current: Invalidated, trigger: reason }`
  - [ ] 3.3.2: In repair paths where repo status transitions (Degraded→Ready, Quarantined→Ready, etc.): record `RepositoryStatusChanged`
  - [ ] 3.3.3: In indexing completion where repo status transitions to Ready or Degraded: record `RepositoryStatusChanged`

- [ ] Task 3.4: Record integrity events (AC: 1, 2)
  - [ ] 3.4.1: Where files are quarantined during indexing or verification: record `OperationalEventKind::IntegrityEvent { kind: Quarantined, ... }` — **do NOT include raw source content in the event detail; use file path, blob_id, and reason only**
  - [ ] 3.4.2: Where verification failures occur in retrieval paths: record `IntegrityEvent { kind: VerificationFailed, ... }` — **detail includes blob_id mismatch info, NOT file content**
  - [ ] 3.4.3: Where suspect state is detected during health inspection or repair: record `IntegrityEvent { kind: SuspectDetected, ... }`

- [ ] Task 3.5: Ensure durability-before-acknowledgment rule (AC: 1)
  - [ ] 3.5.1: Every `save_operational_event` call must occur BEFORE the method returns success to the caller. Operational history write failures must propagate as errors (not swallowed) — matching the pattern from 4.4 review fix C3
  - [ ] 3.5.2: For background tasks (indexing pipeline): event recording happens in the task context before status is reported to the progress/completion path. If event recording fails in a background task context, log `warn!` but do not crash the pipeline

### Phase 4: ApplicationContext bridge

- [ ] Task 4.1: Add `get_operational_history` bridge on `ApplicationContext` (AC: 2)
  - [ ] 4.1.1: Signature: `pub fn get_operational_history(&self, repo_id: &str, filter: &OperationalEventFilter) -> Result<Vec<OperationalEvent>>`
  - [ ] 4.1.2: Delegate to `self.run_manager.get_operational_history(repo_id, filter)` — RunManager delegates to control plane

- [ ] Task 4.2: Add `get_operational_history` on `RunManager` (AC: 2)
  - [ ] 4.2.1: Delegate to `self.persistence.get_operational_events(repo_id, filter)`
  - [ ] 4.2.2: Validate that repo_id exists via `get_repository(repo_id)` before querying — return `NotFound` if missing

### Phase 5: MCP tool exposure

- [ ] Task 5.1: Add `get_operational_history` MCP tool to `src/protocol/mcp.rs` (AC: 2)
  - [ ] 5.1.1: Define tool with description: "Inspect operational history for a repository. Returns an audit-friendly event log of run transitions, checkpoint events, repair actions, and integrity events. Does not expose raw source content."
  - [ ] 5.1.2: Parameters: `repository_id: String` (required), `category: Option<String>` (optional, filter by event category: "run", "checkpoint", "repair", "integrity", "repository", "startup"), `since_unix_ms: Option<u64>` (optional, events after this timestamp), `limit: Option<u64>` (optional, max events, default 50, max 200)
  - [ ] 5.1.3: Build `OperationalEventFilter` from parameters, delegate to `self.application.get_operational_history(repo_id, &filter)`
  - [ ] 5.1.4: Serialize `Vec<OperationalEvent>` to JSON response — each event includes `event_name`, `kind`, `timestamp_unix_ms`, `repo_id`
  - [ ] 5.1.5: Error handling: `NotFound` → `invalid_params` ("Repository not found: {repo_id}"); internal failures → `server_error` with detail
  - [ ] 5.1.6: Privacy enforcement: before serializing, assert no event detail contains raw source content (this is enforced by design at recording time, but the MCP layer double-checks by not including any `content` or `source_bytes` fields in the response schema)

### Phase 6: Testing

- [ ] Task 6.1: Unit tests for operational event types (AC: 1, 2)
  - [ ] 6.1.1: `test_operational_event_names_follow_dot_convention` — each OperationalEventKind variant returns correct dot-separated name
  - [ ] 6.1.2: `test_operational_event_serialization_roundtrip` — serialize/deserialize each variant preserves all fields
  - [ ] 6.1.3: `test_operational_event_filter_by_category` — category filter correctly matches event_name prefixes
  - [ ] 6.1.4: `test_operational_event_filter_by_timestamp` — since_unix_ms filter excludes older events
  - [ ] 6.1.5: `test_operational_event_filter_limit` — limit caps result count

- [ ] Task 6.2: Unit tests for event recording at transition points (AC: 1)
  - [ ] 6.2.1: `test_run_started_records_event` — starting an indexing run records a `run.started` event
  - [ ] 6.2.2: `test_run_completed_records_event` — successful run completion records a `run.succeeded` event
  - [ ] 6.2.3: `test_run_failed_records_event` — failed run records a `run.failed` event
  - [ ] 6.2.4: `test_run_cancelled_records_event` — cancelled run records a `run.cancelled` event
  - [ ] 6.2.5: `test_run_interrupted_records_event` — startup sweep interruption records a `run.interrupted` event
  - [ ] 6.2.6: `test_checkpoint_created_records_event` — checkpoint creation records a `checkpoint.created` event
  - [ ] 6.2.7: `test_repair_records_operational_event` — repair records a `repair.completed` event through the unified path
  - [ ] 6.2.8: `test_invalidation_records_status_change_event` — invalidation records a `repository.status_changed` event
  - [ ] 6.2.9: `test_startup_sweep_records_event` — startup sweep records a `startup.sweep_completed` event
  - [ ] 6.2.10: `test_event_recorded_before_return` — event is persisted before the operation reports success (verify with fake that tracks save order)

- [ ] Task 6.3: Unit tests for backward compatibility (AC: 1)
  - [ ] 6.3.1: `test_save_repair_event_wrapper_creates_operational_event` — calling save_repair_event stores an OperationalEvent with RepairPerformed kind
  - [ ] 6.3.2: `test_get_repair_events_wrapper_filters_correctly` — calling get_repair_events returns only repair events from the unified store
  - [ ] 6.3.3: `test_health_report_recent_repairs_still_works` — RepositoryHealthReport.recent_repairs populated correctly through the new unified path

- [ ] Task 6.4: Unit tests for privacy (AC: 2)
  - [ ] 6.4.1: `test_integrity_event_does_not_contain_source_content` — IntegrityEvent detail uses blob_id and path, not raw bytes or source strings
  - [ ] 6.4.2: `test_event_serialization_excludes_raw_content` — serialized event JSON contains no field named `content`, `source`, or `bytes`

- [ ] Task 6.5: Integration tests for end-to-end operational history flows (AC: 1, 2)
  - [ ] 6.5.1: `test_operational_history_full_lifecycle` — create repo → index → checkpoint → complete → inspect history → events in correct chronological order with correct event_names
  - [ ] 6.5.2: `test_operational_history_repair_flow` — degrade repo → repair → inspect history → includes both repository.status_changed and repair.completed events
  - [ ] 6.5.3: `test_operational_history_category_filter` — record mix of events → filter by "run" → only run events returned
  - [ ] 6.5.4: `test_operational_history_timestamp_filter` — record events at different times → filter by since_unix_ms → only recent events returned
  - [ ] 6.5.5: `test_operational_history_mcp_tool_returns_structured_json` — call get_operational_history through MCP handler → JSON contains event_name, timestamp, structured kind fields
  - [ ] 6.5.6: `test_operational_history_not_found_repository` — query history for non-existent repo → NotFound error

## Dev Notes

### What Already Exists

**RepairEvent Infrastructure** (`src/domain/index.rs`, `src/storage/control_plane.rs`):
- `RepairEvent` struct: repo_id, scope, previous_status, outcome, detail, timestamp_unix_ms
- `ControlPlane::save_repair_event()` / `get_repair_events()` — implemented on all three backends
- `InMemoryControlPlane`: stores in `Vec<RepairEvent>` in `InMemoryState`
- `RegistryBackedControlPlane`: stores in `repair_events` section of registry JSON
- `SpacetimeControlPlane`: `save_repair_event` logs `warn!` (stub — Story 4.6 must wire this)
- `RunManagerPersistenceAdapter`: delegates with `NotFound` suppression

**Run State Transitions** (`src/application/run_manager.rs`):
- `start_indexing_run()` — creates run, transitions to Running via `transition_to_running()`
- Pipeline completion callback — transitions to Succeeded/Failed/Aborted via `update_run_status_with_finish()`
- `cancel_run()` — transitions to Cancelled via `update_run_status()`
- `startup_sweep()` — transitions stale Running → Interrupted via `update_run_status()`
- `resume_run()` — transitions Interrupted → Running via `transition_to_running()`
- **None of these currently record an operational event.** Story 4.6 instruments them.

**Checkpoint Events** (`src/application/run_manager.rs`):
- `checkpoint_run()` — creates checkpoint and persists via `write_checkpoint()` / `save_checkpoint()`
- **No operational event currently recorded at checkpoint creation.**

**Repository Status Changes**:
- `invalidate_repository()` on RunManager — transitions to Invalidated
- `reindex_repository()` — transitions to Pending (new run)
- Repair paths in `repair_repository_state()` — transition Degraded→Ready, Quarantined→Ready, etc.
- Indexing completion — transitions Pending→Ready or Pending→Degraded/Failed
- **No operational events recorded for any of these status changes.**

**RepositoryHealthReport** (`src/domain/health.rs`):
- `recent_repairs: Vec<RepairEvent>` — capped at 10 events
- Loaded via `control_plane.get_repair_events(repo_id)` then truncated
- **Must continue to work after the RepairEvent→OperationalEvent migration**

**Architecture Requirements** (from architecture.md):
- Control-plane model: "Hybrid authoritative state plus append-only operational history"
- Event naming: "operational event names use lowercase dot-separated naming" — e.g., `run.started`, `run.cancelled`, `repair.completed`, `retrieval.quarantined`
- Event payload fields use `snake_case`
- Event names describe completed or observed facts, not vague intentions
- Logging/Privacy: "raw source content must not be dumped by default in logs, diagnostics, or telemetry"
- "integrity-significant events should remain audit-visible without leaking code content by default"

**Current MCP Tools** (17 tools):
health, index_folder, get_index_run, list_index_runs, cancel_index_run, checkpoint_now, resume_index_run, reindex_repository, invalidate_indexed_state, search_text, search_symbols, get_file_outline, get_repo_outline, get_symbol, get_symbols, repair_index, inspect_repository_health

### What 4.6 Builds vs. What Already Exists

| Concern | Already exists | 4.6 adds |
|---------|---------------|----------|
| Event types | `RepairEvent` (repair-specific only) | `OperationalEvent` + `OperationalEventKind` covering all event categories |
| Event naming | None (RepairEvent has no event_name) | Dot-separated names per architecture: `run.started`, `repair.completed`, etc. |
| Run lifecycle recording | Status transitions happen silently | Events recorded at every run state transition |
| Checkpoint recording | Checkpoint created and persisted | Checkpoint event recorded in operational history |
| Repair recording | `save_repair_event()` stores RepairEvent | Refactored as wrapper → unified operational event |
| Repository status recording | Status transitions happen silently | Events recorded at every status transition |
| Integrity recording | Quarantine/verification failures logged but not persisted as events | Persisted as `IntegrityEvent` in operational history |
| Startup sweep recording | `StartupRecoveryReport` returned but not persisted | Sweep events persisted in operational history |
| Inspection surface | `get_repair_events()` returns repair events only | `get_operational_events()` with category/time/limit filters |
| MCP exposure | No history inspection tool | `get_operational_history` MCP tool |
| Privacy | Raw content in tracing logs only | Events use identifiers/hashes/paths, never raw source content |

### Design Decisions

**1. Unified `OperationalEvent` with `OperationalEventKind` enum replaces ad-hoc event types.**
A single event type with variant-specific payloads ensures consistent storage, serialization, filtering, and inspection. The enum is exhaustive — the compiler enforces that new event kinds get naming and serialization support.

**2. `event_name()` method derives dot-separated names from kind variants.**
The architecture requires lowercase dot-separated event names. Rather than storing the name as a freeform string (risking drift), derive it from the variant. The method is `&'static str` to avoid allocation.

**3. RepairEvent backward compatibility through wrapper methods.**
`save_repair_event` and `get_repair_events` remain on the `ControlPlane` trait with the same signatures. Internally, they convert to/from `OperationalEvent` with `RepairPerformed` kind. This avoids changing callers in `repair_repository()` or `inspect_repository_health()`.

**4. RegistryBackedControlPlane migrates storage in-place.**
New writes go to `operational_events`. Old `repair_events` data is read during `get_repair_events` (backward compat) and lazily migrated. `#[serde(default)]` on the new field handles registry files that predate 4.6.

**5. `save_operational_event` errors propagate, never swallow.**
Architecture rule: "Operational history writes must be durable before reporting the action as completed." Per 4.4 code review fix C3, event recording errors propagate. Exception: background pipeline tasks log `warn!` instead of crashing.

**6. Privacy by design — no raw source content in events.**
AC2 requires avoiding raw source content leaks. Events use file paths, blob_ids, status enums, and structured reasons. The `IntegrityEvent.detail` field is constrained to metadata descriptors (hash mismatch, path, reason), never raw source bytes.

**7. `get_operational_events` returns events in reverse chronological order (newest first).**
Operators diagnosing issues typically care about recent events. Returning newest-first with a configurable limit supports both "what just happened?" and "show me everything" use cases.

**8. Default limit of 50 events, maximum 200.**
Prevents unbounded result sets from repositories with long operational history. 50 events covers typical diagnostic scenarios; 200 covers deep investigation.

### Trust Boundary Rules (from architecture and project-context)

1. **Operational history writes must be durable before reporting the action as completed.** (project-context Epic 4 Rule 3) — enforce at every instrumented transition point
2. **Recovery paths must classify stale, interrupted, suspect, quarantined, degraded, and invalid states explicitly.** (project-context Epic 4 Rule 4) — OperationalEventKind captures each state distinctly
3. **Next-action guidance must stay consistent across recovery and retrieval surfaces.** (project-context Epic 4 Rule 5) — repair events reuse shared vocabulary
4. **Raw source content must not be dumped by default in diagnostics.** (architecture Logging/Privacy Model) — events use identifiers, never raw content
5. **Integrity-significant events should remain audit-visible without leaking code content.** (architecture Logging/Privacy Model) — IntegrityEvent stores blob_id and reason, not source bytes
6. **Event names describe completed or observed facts, not vague intentions.** (architecture Operational Event Naming Rules) — e.g., `run.succeeded` not `run.completing`

[Source: _bmad-output/planning-artifacts/architecture.md — Data Architecture, Logging/Privacy Model, Communication Patterns]
[Source: _bmad-output/project-context.md — Epic 4 Recovery Architecture Rules 1-5]
[Source: _bmad-output/project-context.md — ADR-5 Trait-first storage, ADR-7 Persistence correction]

### Previous Story Intelligence

**Story 4.5** (Inspect Repository Health and Repair-Required Conditions) — DONE:
- Added `RepositoryHealthReport`, `FileHealthSummary`, `RunHealthSummary`, `StatusContext` to `src/domain/health.rs`
- `inspect_repository_health()` on RunManager reads repo + run + file state + repair events
- `recent_repairs: Vec<RepairEvent>` in health report — capped at 10, loaded from `get_repair_events()`
- 22 new tests, total 684
- **Key for 4.6**: `get_repair_events()` is used by health inspection — the wrapper refactoring must preserve this call chain
- **Code review gap**: MCP wiring test is compile-time only (no runtime integration test for the MCP handler) — noted but not a 4.6 concern

**Story 4.4** (Trigger Deterministic Repair for Suspect or Incomplete State) — DONE:
- Added `RepairScope`, `RepairOutcome`, `RepairResult`, `RepairEvent` to `src/domain/index.rs`
- `repair_repository()` calls `save_repair_event()` before returning result
- Extended `ControlPlane` trait with `save_repair_event()` / `get_repair_events()`
- SpacetimeDB `save_repair_event` logs `warn!` — Story 4.6 should replace this with real operational event persistence
- 19 new tests
- **Code review fix C3**: `record_repair_event` now propagates errors instead of swallowing — pattern to follow for all event recording
- **Key for 4.6**: The `save_repair_event()` → `save_operational_event()` wrapper refactoring must not break the error propagation guarantee

**Story 4.3** (Move Mutable Run Durability to SpacetimeDB Control Plane) — DONE:
- Expanded `ControlPlane` trait with 20+ methods
- `RunManagerPersistenceAdapter` wraps control plane — Story 4.6 must add new methods to this adapter
- `NotFound` suppression pattern on adapter write methods — follow for `save_operational_event`
- **M1 deferred item**: `save_file_records` atomicity — same concern applies to `save_operational_event` batch scenarios (mitigate by recording events individually)

**Story 4.1** (Sweep Stale Leases on Startup) — DONE:
- `startup_sweep()` transitions Running → Interrupted and returns `StartupRecoveryReport`
- Report is returned to caller but NOT persisted as operational history
- **Key for 4.6**: instrument `startup_sweep()` to also record events in operational history

### Git Intelligence

Recent commits:
- `10e7907` — docs: update planning artifacts and add tooling configs
- `b84ce37` — feat(control-plane): land story 4.3 with review fixes

Pattern: each story builds on the previous one's foundation. Story 4.6 builds on 4.4's repair event infrastructure and 4.5's health inspection surface. It replaces the ad-hoc RepairEvent storage with a unified operational history system.

### Existing Code to Reuse

| Function / Type | Location | Why it matters for 4.6 |
|---|---|---|
| `RepairEvent` struct | `src/domain/index.rs` | Fields map to `OperationalEventKind::RepairPerformed` payload |
| `RepairScope`, `RepairOutcome` | `src/domain/index.rs` | Used inside RepairPerformed variant |
| `RepositoryStatus` enum | `src/domain/repository.rs` | Used in RepositoryStatusChanged and RepairPerformed |
| `IndexRunStatus`, `IndexRunMode` | `src/domain/index.rs` | Used in RunStarted, RunCompleted variants |
| `ControlPlane::save_repair_event()` | `src/storage/control_plane.rs` | Becomes wrapper for save_operational_event |
| `ControlPlane::get_repair_events()` | `src/storage/control_plane.rs` | Becomes wrapper for get_operational_events with category filter |
| `RunManagerPersistenceAdapter` | `src/storage/control_plane.rs` | Must add new operational event methods |
| `InMemoryControlPlane` / `InMemoryState` | `src/storage/control_plane.rs` | Primary in-memory storage for events |
| `RegistryBackedControlPlane` / `RegistryData` | `src/storage/registry_persistence.rs` | JSON persistence for events (backward-compatible) |
| `SpacetimeControlPlane` | `src/storage/control_plane.rs` | Currently stubs repair events — wire for operational events |
| `unix_timestamp_ms()` | `src/domain/health.rs` | Timestamp helper for event creation |
| `RunManager::start_indexing_run()` | `src/application/run_manager.rs` | Instrument: record RunStarted |
| `RunManager::cancel_run()` | `src/application/run_manager.rs` | Instrument: record RunCompleted(Cancelled) |
| `RunManager::startup_sweep()` | `src/application/run_manager.rs` | Instrument: record RunInterrupted + StartupSweepCompleted |
| `RunManager::checkpoint_run()` | `src/application/run_manager.rs` | Instrument: record CheckpointCreated |
| `RunManager::invalidate_repository()` | `src/application/run_manager.rs` | Instrument: record RepositoryStatusChanged |
| `RunManager::repair_repository()` | `src/application/run_manager.rs` | Already records via save_repair_event — backward-compat path |
| `ApplicationContext::repair_repository()` | `src/application/mod.rs` | Bridge pattern for new get_operational_history |
| `TokenizorServer` MCP tools | `src/protocol/mcp.rs` | Pattern for new get_operational_history tool |

### Library / Framework Requirements

- No new external dependencies required for Story 4.6
- Keep all existing dependency versions (`rmcp = 1.1.0`, `tokio = 1.48`, `serde = 1.0`, `schemars = 1.1`)
- SpacetimeDB SDK (`spacetimedb-sdk = 2.0.3`) — may need SpacetimeDB schema update for `operational_events` table if wiring real writes; otherwise use warning+fallback pattern

### Epic 4 Definition of Done (mandatory)

- Expected test delta: Add 5 unit tests for event types and naming, 10 unit tests for event recording at transition points, 3 unit tests for backward compatibility, 2 unit tests for privacy, and 6 integration tests for end-to-end history flows. **Total: ~26 new tests minimum.**
- Build/test evidence: [Record the exact `cargo test` command(s) and pass/fail summary]
- Acceptance-criteria traceability:
  - AC1 → `save_operational_event()` called at every run transition, checkpoint, repair, and integrity event; durability enforced before returning success; `get_operational_events()` reconstructs the event sequence
  - AC2 → `get_operational_history` MCP tool with category/time/limit filters; events use identifiers and metadata, never raw source content; `test_integrity_event_does_not_contain_source_content` validates privacy
- Trust-boundary traceability: Architecture Logging/Privacy Model (no raw content in diagnostics), Operational Event Naming Rules (dot-separated facts), Epic 4 Recovery Architecture rules 3-5 (durable-before-ack, classify explicitly, shared vocabulary)
- State-transition evidence:
  - Event recording side: every transition point records an event observable via `get_operational_events()` (verified by unit tests per transition)
  - Inspection side: MCP tool returns structured events matching recorded transitions (verified by integration test)
  - Backward compatibility side: `get_repair_events()` still returns repair events through the unified path (verified by compatibility tests)

### Self-Audit Checklist (mandatory before requesting review)

_Run this checklist after all tasks are complete. This is a blocking step — do not request review until every item is verified._

#### Generic Verification
- [ ] For every task marked `[x]`, cite the specific test that verifies it
- [ ] For every new error variant or branch, confirm a test exercises it
- [ ] For every computed value, trace it to where it surfaces (log, return value, persistence)
- [ ] For every test, verify the assertion can actually fail (no `assert!(true)`, no conditionals that always pass)

#### Epic 4 Recovery Verification
- [ ] The declared expected test delta was met or exceeded by the actual implementation
- [ ] Build/test evidence is recorded with the exact command and outcome summary
- [ ] Every acceptance criterion is traced to concrete implementation code and at least one concrete test
- [ ] Every trust-boundary or recovery-policy decision cites the exact architecture or `project-context.md` source
- [ ] Every state transition is tested from both sides: the mutation itself and the resulting retrieval/inspection behavior

#### Story 4.6-Specific Verification
- [ ] Confirm `OperationalEvent::event_name()` returns correct dot-separated names for all variants
- [ ] Confirm `save_operational_event()` is called at every instrumented transition point
- [ ] Confirm event recording happens BEFORE the operation reports success to the caller
- [ ] Confirm `save_repair_event()` wrapper correctly converts to/from OperationalEvent
- [ ] Confirm `get_repair_events()` wrapper correctly filters and converts from unified store
- [ ] Confirm `RepositoryHealthReport.recent_repairs` still works through the new path
- [ ] Confirm `get_operational_history` MCP tool returns structured JSON with all expected fields
- [ ] Confirm no event detail field contains raw source content, source bytes, or file content strings
- [ ] Confirm filtering by category, timestamp, and limit all work correctly
- [ ] Confirm events are returned in reverse chronological order (newest first)
- [ ] Confirm backward-compatible deserialization of registry files that predate Story 4.6

### Project Structure Notes

| File | Why it is in scope |
|---|---|
| `src/domain/index.rs` | Add `OperationalEvent`, `OperationalEventKind`, `IntegrityEventKind`, `OperationalEventFilter` types |
| `src/domain/mod.rs` | Add re-exports for new operational event types |
| `src/domain/health.rs` | No changes needed — `RepositoryHealthReport` uses `get_repair_events()` which now wraps the unified store |
| `src/storage/control_plane.rs` | Add `save_operational_event()`, `get_operational_events()` to trait; implement on all backends; refactor `save_repair_event`/`get_repair_events` as wrappers |
| `src/storage/registry_persistence.rs` | Add `operational_events` to `RegistryData`; backward-compat migration for `repair_events` |
| `src/application/run_manager.rs` | Instrument all state transitions with event recording; add `get_operational_history()` |
| `src/application/mod.rs` | Add `get_operational_history` bridge on `ApplicationContext` |
| `src/application/deployment.rs` | Update test fake with new ControlPlane methods |
| `src/protocol/mcp.rs` | Add `get_operational_history` MCP tool |
| `tests/indexing_integration.rs` | Add operational history integration tests |

**Alignment notes:**
- Stay inside the current `application` / `domain` / `storage` / `protocol` layering
- `OperationalEvent` and related types go in `src/domain/index.rs` alongside `RepairEvent` — they are the evolution of the same concept
- `save_operational_event()` on the ControlPlane trait follows the same trait-first pattern (ADR-5)
- `RunManagerPersistenceAdapter` must delegate new methods with `NotFound` suppression (4.3 pattern)
- MCP tool follows existing `inspect_repository_health` pattern for parameter parsing and error handling
- Indexing pipeline instrumentation modifies existing code paths minimally — add event recording calls adjacent to existing status transitions

### References

- [Source: _bmad-output/planning-artifacts/epics.md#Story 4.6]
- [Source: _bmad-output/planning-artifacts/architecture.md — Data Architecture (Control-Plane Model)]
- [Source: _bmad-output/planning-artifacts/architecture.md — Logging/Privacy Model]
- [Source: _bmad-output/planning-artifacts/architecture.md — Communication Patterns (Operational Event Naming Rules)]
- [Source: _bmad-output/planning-artifacts/architecture.md — Process Patterns (Mutation and Recovery Rules)]
- [Source: _bmad-output/planning-artifacts/architecture.md — Data Flow (ingest/repair paths include operational history)]
- [Source: _bmad-output/planning-artifacts/prd.md — FR34]
- [Source: _bmad-output/project-context.md — Epic 4 Recovery Architecture Rules 1-5]
- [Source: _bmad-output/project-context.md — ADR-5 Trait-first storage]
- [Source: _bmad-output/project-context.md — ADR-7 Bootstrap registry narrowed]
- [Source: _bmad-output/project-context.md — Agent Selection (Claude Opus 4.6 primary)]
- [Source: _bmad-output/implementation-artifacts/4-5-inspect-repository-health-and-repair-required-conditions.md]
- [Source: _bmad-output/implementation-artifacts/4-4-trigger-deterministic-repair-for-suspect-or-incomplete-state.md]
- [Source: _bmad-output/implementation-artifacts/4-3-move-mutable-run-durability-to-the-spacetimedb-control-plane.md]
- [Source: src/domain/index.rs — RepairEvent, RepairScope, RepairOutcome, IndexRunStatus, IndexRunMode]
- [Source: src/domain/repository.rs — RepositoryStatus]
- [Source: src/domain/health.rs — RepositoryHealthReport, unix_timestamp_ms()]
- [Source: src/domain/retrieval.rs — NextAction]
- [Source: src/storage/control_plane.rs — ControlPlane trait, all implementations]
- [Source: src/storage/registry_persistence.rs — RegistryData]
- [Source: src/application/run_manager.rs — RunManager state transition methods]
- [Source: src/application/mod.rs — ApplicationContext bridge pattern]
- [Source: src/application/deployment.rs — ControlPlane test fake]
- [Source: src/protocol/mcp.rs — TokenizorServer MCP tools]

## Dev Agent Record

### Agent Model Used

Claude Opus 4.6

### Debug Log References

### Completion Notes List

- Code review performed by Claude Opus 4.6 on 2026-03-09
- Fixed C2: Wrapper refactoring — save_repair_event/get_repair_events now delegate through save_operational_event/get_operational_events on all ControlPlane backends (InMemory, Registry, Spacetime). Removed standalone repair_events vec from InMemoryState. RegistryData keeps repair_events field for backward-compat deserialization.
- Fixed H2: Added RepositoryStatusChanged events at all repair status transitions (Degraded→Ready, Quarantined→Ready, Quarantined→Degraded) and indexing completion (Invalidated→Ready)
- Fixed H3: cancel_run() now propagates save_operational_event errors with `?` instead of `let _ =`
- Fixed H4: MCP get_operational_history default limit corrected from 100 to 50
- Fixed M1: Pipeline completion event recording uses `if let Err` with `warn!` instead of silent `let _ =`
- Fixed M2: cancel_run captures actual files_processed from PipelineProgress before deregistering
- Fixed M3: resume_run now records RunStarted event when transitioning Interrupted→Running
- Fixed M4: startup_sweep event recording uses `if let Err` with `warn!` instead of silent `let _ =`
- Fixed RunManager::record_repair_event to stop double-writing (save_repair_event now handles operational event internally)

### Review Follow-ups

- [ ] [AI-Review][HIGH] Task 3.4: IntegrityEvent instrumentation requires architectural change. Specific instrumentation points:
  - **Quarantine (3.4.1)**: `src/indexing/commit.rs:validate_for_commit()` is a pure function returning `PersistedFileOutcome::Quarantined` — no control plane access. Instrument at the call site in the pipeline's durable_record_callback or after `commit_file_result()`. The callback already has access to RunManager via `manager_for_records`.
  - **Verification failure (3.4.2)**: `src/application/search.rs:626` detects blob hash mismatch during `get_verified_symbol_source()`. This function does not hold a control plane reference — thread it through the search path or record at the MCP handler level after receiving a Suspect/Blocked result.
  - **Suspect detection (3.4.3)**: `repair_repository_state()` in `run_manager.rs` already has control plane access — add IntegrityEvent recording where quarantined files are identified during repair (line ~2378-2403).
  - **Design note**: Consider adding an `event_recorder: Option<Arc<dyn Fn(OperationalEvent)>>` callback to the pipeline, similar to `durable_record_callback`, to avoid threading the full control plane.
- [x] [AI-Review][LOW] OperationalEventKind serde tagging — current default external tagging preserved; changing requires migration of persisted data
- [x] [AI-Review][LOW] Cargo.toml spacetimedb-sdk bump documented in File List
- [x] [AI-Review][LOW] files_failed field on Checkpoint documented in File List; has #[serde(default)] for backward compat

### File List

- src/domain/index.rs — OperationalEvent, OperationalEventKind, IntegrityEventKind, OperationalEventFilter, RepairEvent wrapper methods
- src/domain/mod.rs — re-exports for new operational event types
- src/domain/health.rs — RepositoryHealthReport, FileHealthSummary, RunHealthSummary, StatusContext
- src/storage/control_plane.rs — ControlPlane trait with save/get operational event methods; wrapper refactoring on all backends
- src/storage/registry_persistence.rs — RegistryData with operational_events; wrapper save/get_repair_events
- src/application/run_manager.rs — Event instrumentation at all state transitions; repair, health inspection, get_operational_history
- src/application/mod.rs — ApplicationContext bridge for repair, health, operational history
- src/application/deployment.rs — Test fake updated with new ControlPlane methods
- src/protocol/mcp.rs — repair_index, inspect_repository_health, get_operational_history MCP tools
- tests/indexing_integration.rs — Integration tests for repair, health inspection, operational history
- Cargo.toml — spacetimedb-sdk version bump
