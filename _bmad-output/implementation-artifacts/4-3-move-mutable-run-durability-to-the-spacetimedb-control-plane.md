# Story 4.3: Move Mutable Run Durability to the SpacetimeDB Control Plane

Status: review

<!-- Note: Validation is optional. Run validate-create-story for quality check before dev-story. -->

## Story

As an operator,
I want runs, checkpoints, durable per-run file metadata, idempotency records, and typed recovery metadata to persist through the authoritative control plane,
so that recovery and operational state are durable, scalable, and aligned with the intended architecture.

**FRs implemented:** FR15, FR16, FR30, FR34

- **FR15**: Checkpoints persist through the authoritative control plane
- **FR16**: Idempotency records persist through the authoritative control plane
- **FR30**: Resume uses a persisted discovery manifest, not live rediscovery
- **FR34**: Operational state writes are durable before reporting success

## Acceptance Criteria

1. **Given** Tokenizor creates or mutates indexing run state **When** durable operational metadata is written **Then** the write goes through the SpacetimeDB-backed control plane rather than direct `RegistryPersistence` mutation **And** the resulting state remains inspectable through existing run APIs
2. **Given** a file is durably committed during indexing **When** the durable file-record boundary is advanced **Then** the system upserts per-run file metadata and any related checkpoint state without full registry-file rewrites **And** checkpoint advancement never outruns durable prior state
3. **Given** an interrupted run is eligible for recovery **When** resume occurs **Then** Tokenizor uses a persisted discovery manifest for that run to determine replay boundaries **And** it does not rely on a fresh filesystem rediscovery to infer checkpoint compatibility
4. **Given** older local-registry-backed run state exists **When** the corrected control-plane path is introduced **Then** migration or compatibility behavior is explicit and safe **And** unsafe mixed-state mutation is rejected clearly

## Tasks / Subtasks

### Phase 1: Expand the ControlPlane trait to cover mutable run operations

- [x] Task 1.1: Extend the `ControlPlane` trait with the mutable run operations that `RegistryPersistence` currently owns (AC: 1, 2)
  - [x] 1.1.1: Add query methods to `ControlPlane`: `find_run`, `find_runs_by_status`, `list_runs`, `get_runs_by_repo`, `get_latest_completed_run`, `get_repository`, `get_file_records`, `get_latest_checkpoint`, `find_idempotency_record`
  - [x] 1.1.2: Add mutation methods to `ControlPlane`: `save_run`, `update_run_status`, `transition_to_running`, `update_run_status_with_finish`, `cancel_run_if_active`, `save_file_records`, `save_checkpoint`, `save_repository`, `update_repository_status`, `save_idempotency_record`
  - [x] 1.1.3: Implement all new methods on `InMemoryControlPlane` with the same `Mutex<InMemoryState>` pattern, backed by the existing in-memory collections
  - [x] 1.1.4: Add backward-compatible default method implementations or explicit `pending_write_error()` stubs on `SpacetimeControlPlane` for methods not yet wired to SpacetimeDB, so the trait compiles immediately

- [x] Task 1.2: Introduce a `RegistryBackedControlPlane` adapter that delegates to `RegistryPersistence` (AC: 1, 4)
  - [x] 1.2.1: Create `RegistryBackedControlPlane` that wraps `RegistryPersistence` and implements the expanded `ControlPlane` trait by forwarding all calls to the existing registry methods
  - [x] 1.2.2: Wire `RegistryBackedControlPlane` as a third `ControlPlaneBackend` variant (e.g., `LocalRegistry`) so existing test and development flows can use the expanded trait without requiring SpacetimeDB
  - [x] 1.2.3: Verify that all existing `RegistryPersistence` tests pass through the `RegistryBackedControlPlane` adapter without behavioral change

### Phase 2: Move RunManager onto the ControlPlane boundary

- [x] Task 2.1: Refactor `RunManager` to depend on `Arc<dyn ControlPlane>` instead of `RegistryPersistence` (AC: 1, 2)
  - [x] 2.1.1: Replace the `persistence: RegistryPersistence` field on `RunManager` with `control_plane: Arc<dyn ControlPlane>` and update the constructor
  - [x] 2.1.2: Migrate all `self.persistence.*` calls in `RunManager` to equivalent `self.control_plane.*` calls, verifying each call compiles and passes existing tests
  - [x] 2.1.3: Update `ApplicationContext::from_config()` to pass the constructed control plane to `RunManager` instead of a raw `RegistryPersistence`
  - [x] 2.1.4: Ensure the `startup_sweep`, `resume_run`, `start_run`, `launch_run`, `reindex_repository`, `checkpoint_run`, `inspect_run`, `list_runs_with_health`, and `cancel_run` paths all work through the control plane without regression

- [x] Task 2.2: Update the durable file-record callback to use the control plane boundary (AC: 2)
  - [x] 2.2.1: Change `persist_durable_file_record` to call `control_plane.save_file_records()` instead of `persistence.save_file_records()`
  - [x] 2.2.2: Verify that checkpoint advancement still never outruns durable prior state through the new boundary

### Phase 3: Add a persisted discovery manifest for deterministic resume

- [x] Task 3.1: Persist a discovery manifest when an indexing run starts (AC: 3)
  - [x] 3.1.1: Define a `DiscoveryManifest` type that captures the sorted list of indexable file paths discovered at run start, along with the run_id and discovery timestamp
  - [x] 3.1.2: Add `save_discovery_manifest` and `get_discovery_manifest` to the `ControlPlane` trait
  - [x] 3.1.3: Persist the discovery manifest after file discovery completes in `IndexingPipeline::execute()` or during `RunManager::launch_run()`/`spawn_pipeline_for_run`
  - [x] 3.1.4: Implement on `InMemoryControlPlane` and `RegistryBackedControlPlane`

- [x] Task 3.2: Use the persisted discovery manifest during resume instead of live rediscovery (AC: 3)
  - [x] 3.2.1: In `resume_run()`, load the discovery manifest for the interrupted run instead of calling `discovery::discover_files()`
  - [x] 3.2.2: Validate the checkpoint cursor against the manifest's sorted path list rather than a fresh filesystem walk
  - [x] 3.2.3: Pass the manifest-derived file list to the pipeline resume state so the pipeline skips only manifest-validated files
  - [x] 3.2.4: Reject resume if the manifest is missing or corrupt, with explicit `Reindex` guidance

### Phase 4: Wire SpacetimeDB writes for mutable run state

- [x] Task 4.1: Create the SpacetimeDB schema for mutable run state (AC: 1, 2)
  - [x] 4.1.1: Define SpacetimeDB tables for `index_runs`, `checkpoints`, `file_records`, `idempotency_records`, `discovery_manifests`, and `repositories` under `spacetime/tokenizor/`
  - [x] 4.1.2: Ensure the schema supports the same query patterns used by `RunManager`: find by run_id, find by status, find by repo_id, latest checkpoint, latest completed run
  - [x] 4.1.3: Increment `SUPPORTED_SPACETIMEDB_SCHEMA_VERSION` to reflect the new tables

- [x] Task 4.2: Implement the expanded `ControlPlane` trait on `SpacetimeControlPlane` (AC: 1, 2)
  - [x] 4.2.1: Wire query methods to SpacetimeDB reads using the SpacetimeDB Rust SDK
  - [x] 4.2.2: Wire mutation methods to SpacetimeDB writes, replacing `pending_write_error()` stubs
  - [x] 4.2.3: Ensure `save_file_records` uses SpacetimeDB upsert semantics (by relative_path) rather than full-table replacement, eliminating the O(n) registry-rewrite bottleneck from Story 4.2

### Phase 5: Migration and compatibility

- [x] Task 5.1: Handle migration from local registry to SpacetimeDB (AC: 4)
  - [x] 5.1.1: On startup, detect whether mutable run state exists in the local registry but not in SpacetimeDB
  - [x] 5.1.2: Provide an explicit migration path that reads existing runs, checkpoints, and file records from the local registry and writes them to SpacetimeDB
  - [x] 5.1.3: Reject unsafe mixed-state mutation: if the control plane backend is SpacetimeDB but local registry still contains un-migrated mutable state, block new run creation with actionable migration guidance
  - [x] 5.1.4: Keep `RegistryPersistence` available for project/workspace bootstrap data; do not remove it entirely

### Phase 6: Verification

- [x] Task 6.1: Add unit tests for the expanded `ControlPlane` trait (AC: 1, 2, 3)
  - [x] 6.1.1: Verify `InMemoryControlPlane` passes all existing `RunManager` lifecycle tests when used as the backing control plane
  - [x] 6.1.2: Verify `RegistryBackedControlPlane` passes all existing `RegistryPersistence` round-trip and backward-compatibility tests
  - [x] 6.1.3: Verify discovery manifest persistence and retrieval roundtrips

- [x] Task 6.2: Add integration tests for resume with discovery manifest (AC: 3)
  - [x] 6.2.1: Verify resume uses the persisted manifest rather than live rediscovery
  - [x] 6.2.2: Verify resume rejects when the manifest is missing with explicit reindex guidance
  - [x] 6.2.3: Verify the manifest-based resume produces the same file processing result as a fresh run on the same file set

- [x] Task 6.3: Add integration tests for migration safety (AC: 4)
  - [x] 6.3.1: Verify startup detects un-migrated local registry state and blocks new mutations
  - [x] 6.3.2: Verify the migration path correctly transfers runs, checkpoints, and file records
  - [x] 6.3.3: Verify post-migration run creation goes through SpacetimeDB without local registry writes

- [x] Task 6.4: Add a basic latency sanity check for SpacetimeDB control plane writes (AC: 1, 2)
  - [x] 6.4.1: Verify that per-file durable record writes through SpacetimeDB avoid the per-write reconnect overhead and stay off the O(n) registry rewrite baseline from Story 4.2

## Dev Notes

### What Already Exists

**ControlPlane trait** (`src/storage/control_plane.rs`):
- Currently has 4 write methods: `upsert_repository`, `create_index_run`, `write_checkpoint`, `put_idempotency_record`
- 2 read methods: `health_check`, `deployment_checks`, `backend_name`
- `SpacetimeControlPlane` returns `pending_write_error()` for all 4 write methods
- `InMemoryControlPlane` implements all methods with `Mutex<InMemoryState>`

**RegistryPersistence** (`src/storage/registry_persistence.rs`):
- 20+ methods covering runs, checkpoints, file records, repositories, idempotency records
- All use `read_modify_write` with advisory file locking for atomic JSON rewrite
- Story 4.2 added per-file `save_file_records` with upsert semantics (BTreeMap merge by relative_path)
- Story 4.2 proved this creates O(n) full registry rewrites for large repos — the direct motivation for this story

**RunManager** (`src/application/run_manager.rs`):
- Currently holds `persistence: RegistryPersistence` directly
- All run lifecycle operations (start, launch, checkpoint, cancel, inspect, resume, startup_sweep) go through `self.persistence.*`
- Story 4.2 added `resume_run()` which validates against live `discover_files()` — AC3 replaces this with a manifest

**SpacetimeDB infrastructure**:
- Config: `SpacetimeDbConfig` with endpoint (`http://127.0.0.1:3007`), database (`tokenizor`), CLI path, module path, schema version
- Module scaffold: `spacetime/tokenizor/README.md` — placeholder only
- `SUPPORTED_SPACETIMEDB_SCHEMA_VERSION = 1` in `src/config.rs`
- `ControlPlaneBackend` enum: `InMemory` | `SpacetimeDb`

### Current Gap to Close

The architecture ADR (lines 398-415) explicitly states:
> "Starting in Epic 4, mutable operational state must move to the SpacetimeDB-backed control plane. `index_runs`, checkpoints, per-run durable file records, idempotency records, typed recovery metadata, and related operational history are no longer planned to remain on the local bootstrap registry JSON path."

The rationale is threefold:
1. PRD defines SpacetimeDB as the authoritative control plane
2. Story 4.2 proved per-file registry writes create avoidable persistence debt
3. Continuing on `RegistryPersistence` deepens the architecture mismatch

### Migration Strategy

**Phase 1-2** can be done without SpacetimeDB by expanding the `ControlPlane` trait and wrapping `RegistryPersistence` in an adapter. This lets `RunManager` code migrate to the trait boundary immediately, with tests passing through the adapter.

**Phase 3** (discovery manifest) is independent of the SpacetimeDB backend and can be implemented on `InMemoryControlPlane` and `RegistryBackedControlPlane` first.

**Phase 4** wires the actual SpacetimeDB SDK. This phase has the external dependency risk (SpacetimeDB Rust SDK, local runtime).

**Phase 5** handles coexistence of old registry data with the new control plane.

### Resume Design Change (AC3)

Story 4.2's `resume_run()` currently calls `discovery::discover_files()` to rediscover the filesystem and validate the checkpoint cursor. AC3 requires replacing this with a persisted discovery manifest:

1. When a run starts, the sorted indexable file list is persisted as a `DiscoveryManifest`
2. On resume, the manifest is loaded instead of re-walking the filesystem
3. The checkpoint cursor is validated against the manifest, not a fresh walk
4. This eliminates the TOCTOU race identified in the Story 4.2 review (M2 finding)

### Existing Code to Reuse

| Function / Type | Location | Why it matters for 4.3 |
|---|---|---|
| `ControlPlane` trait | `src/storage/control_plane.rs` | Expand this trait rather than creating a new abstraction |
| `InMemoryControlPlane` | `src/storage/control_plane.rs` | Must implement all new methods for test support |
| `SpacetimeControlPlane` | `src/storage/control_plane.rs` | Wire real SpacetimeDB writes here |
| `RegistryPersistence` | `src/storage/registry_persistence.rs` | Wrap in adapter; keep for bootstrap data |
| `RunManager` | `src/application/run_manager.rs` | Migrate from `persistence` to `control_plane` |
| `ApplicationContext::from_config()` | `src/application/mod.rs` | Wire the control plane into RunManager |
| `IndexingPipeline::execute()` | `src/indexing/pipeline.rs` | Persist discovery manifest here |
| `discovery::discover_files()` | `src/indexing/discovery.rs` | Manifest captures this output |
| `PipelineResumeState` | `src/indexing/pipeline.rs` | Resume from manifest instead of live walk |

### Previous Story Intelligence

Story 4.2 code review findings that directly motivate this story:

- **M1 (MEDIUM)**: Per-file registry writes create O(n) full rewrites — AC2 eliminates this with SpacetimeDB upserts
- **M2 (MEDIUM)**: Discovery TOCTOU between resume eligibility and pipeline — AC3 eliminates this with persisted manifest
- **L4 (LOW)**: File records persisted twice per file — SpacetimeDB upsert semantics make this naturally idempotent without the redundancy cost

### Git Intelligence

Recent git history shows incremental hardening pattern:
- `4123ee4` — Formalize Epic 4.0 hardening
- Story 4.1 and 4.2 built on existing registry persistence
- Story 4.3 is the first story that introduces a new external persistence dependency

### Library / Framework Requirements

- **SpacetimeDB Rust SDK**: Use the current stable public v2 line. Research the exact crate name and version before implementation starts. Do not pin a version until the SDK is confirmed compatible with the local SpacetimeDB runtime at `127.0.0.1:3007`.
- Keep all existing dependency versions (`rmcp = 1.1.0`, `tokio = 1.48`, `serde = 1.0`, `fs2 = 0.4`, `ignore = 0.4`)
- The SpacetimeDB module (schema) should live under `spacetime/tokenizor/` as the existing scaffold expects

### Testing Requirements

- All existing `RunManager` tests must pass when backed by `InMemoryControlPlane` (expanded)
- All existing `RegistryPersistence` tests must pass through `RegistryBackedControlPlane` adapter
- New discovery manifest tests for persist, retrieve, and resume-from-manifest
- Migration safety: startup blocks on un-migrated mutable state
- Latency: per-file writes through SpacetimeDB faster than registry baseline
- Keep recovery proofs local-first: `InMemoryControlPlane` tests are the primary verification surface; SpacetimeDB integration tests are supplementary

### Epic 4 Definition of Done (mandatory)

- Expected test delta: Add tests for expanded `ControlPlane` trait implementations (InMemory and RegistryBacked), discovery manifest roundtrip and resume, migration detection and blocking, and SpacetimeDB write latency baseline
- Build/test evidence: Record the exact `cargo test` command(s) and pass/fail summary before requesting review
- Acceptance-criteria traceability:
  - AC1 -> ControlPlane trait expansion, RunManager migration, SpacetimeDB write wiring
  - AC2 -> SpacetimeDB upsert for file records, checkpoint ordering integrity
  - AC3 -> DiscoveryManifest type, persist at run start, resume from manifest
  - AC4 -> Migration detection, mixed-state rejection, compatibility path
- Trust-boundary traceability: Cite architecture persistence correction ADR (lines 398-415), project-context ADR-7, and PRD SpacetimeDB requirements
- State-transition evidence: Prove both sides for every transition:
  - `RunManager` operations through `ControlPlane` produce same observable state as through `RegistryPersistence`
  - Resume from manifest produces same result as resume from live discovery
  - Migration detection blocks unsafe mutation and surfaces actionable guidance

### Self-Audit Checklist (mandatory before requesting review)

_Run this checklist after all tasks are complete. This is a blocking step - do not request review until every item is verified._

#### Generic Verification
- [x] For every task marked `[x]`, cite the specific test that verifies it
- [x] For every new error variant or branch, confirm a test exercises it
- [x] For every computed value, trace it to where it surfaces (log, return value, persistence)
- [x] For every test, verify the assertion can actually fail (no `assert!(true)`, no conditionals that always pass)

#### Epic 4 Recovery Verification
- [x] The declared expected test delta was met or exceeded by the actual implementation
- [x] Build/test evidence is recorded with the exact command and outcome summary
- [x] Every acceptance criterion is traced to concrete implementation code and at least one concrete test
- [x] Every trust-boundary or recovery-policy decision cites the exact architecture or `project-context.md` source
- [x] Every state transition is tested from both sides: the mutation itself and the resulting retrieval/inspection behavior

#### Story 4.3-Specific Verification
- [x] Confirm `RunManager` no longer holds a direct `RegistryPersistence` field for mutable run state
- [x] Confirm `InMemoryControlPlane` passes all existing `RunManager` lifecycle tests
- [x] Confirm discovery manifest is persisted at run start and used during resume
- [x] Confirm resume with manifest produces the same result as resume with live discovery for an identical file set
- [x] Confirm migration detection blocks new run creation when un-migrated registry state exists
- [x] Confirm per-file durable writes through SpacetimeDB do not require full-table replacement
- [x] Confirm `RegistryPersistence` is still used for project/workspace bootstrap data
- [x] Confirm no speculative schema features beyond the declared mutable-state tables were introduced

### Project Structure Notes

| File | Why it is in scope |
|---|---|
| `src/storage/control_plane.rs` | Expand `ControlPlane` trait, implement on all backends |
| `src/storage/registry_persistence.rs` | Wrap in adapter, keep for bootstrap data |
| `src/storage/mod.rs` | Re-export new types |
| `src/application/run_manager.rs` | Migrate from `RegistryPersistence` to `Arc<dyn ControlPlane>` |
| `src/application/mod.rs` | Wire control plane into RunManager and ApplicationContext |
| `src/domain/index.rs` | Add `DiscoveryManifest` type |
| `src/indexing/pipeline.rs` | Persist discovery manifest at run start |
| `src/protocol/mcp.rs` | No changes expected (passes through ApplicationContext) |
| `src/config.rs` | Add `LocalRegistry` backend variant, bump schema version |
| `spacetime/tokenizor/` | SpacetimeDB schema/module source |
| `tests/indexing_integration.rs` | Expand with manifest-based resume and migration tests |

**Alignment notes**

- Stay inside the current `application` / `domain` / `indexing` / `storage` layering.
- The `RegistryBackedControlPlane` adapter is an interim bridge, not a permanent fixture. It exists to let Phase 1-2 land without blocking on SpacetimeDB SDK integration.
- Do not attempt to migrate project/workspace bootstrap data to SpacetimeDB in this story. That is a separate future concern.

### References

- [Source: _bmad-output/planning-artifacts/epics.md#Story 4.3]
- [Source: _bmad-output/planning-artifacts/architecture.md#Persistence Correction ADR (lines 398-415)]
- [Source: _bmad-output/planning-artifacts/prd.md#Technical Success Criteria]
- [Source: _bmad-output/planning-artifacts/prd.md#Architectural Constraints]
- [Source: _bmad-output/project-context.md#ADR-7]
- [Source: _bmad-output/project-context.md#Epic 2 Persistence Architecture]
- [Source: _bmad-output/project-context.md#Epic 4 Recovery Architecture]
- [Source: _bmad-output/project-context.md#Agent Selection]
- [Source: _bmad-output/implementation-artifacts/4-2-resume-interrupted-indexing-from-durable-checkpoints.md]
- [Source: src/storage/control_plane.rs]
- [Source: src/storage/registry_persistence.rs]
- [Source: src/application/run_manager.rs]
- [Source: src/application/mod.rs]
- [Source: src/config.rs]
- [Source: src/indexing/pipeline.rs]
- [Source: spacetime/tokenizor/README.md]

## Dev Agent Record

### Agent Model Used

GPT-5 Codex (exception to the default Epic 4 primary-model policy because the user explicitly invoked `bmad-bmm-dev-story 4.3` and requested direct story execution in this session)

### Debug Log References

- `cargo check --workspace` (2026-03-09): pass
- `cargo test --test indexing_integration test_application_context_from_config_transitions_running_runs_before_runtime_ready` (2026-03-09): pass
- `cargo fmt` (2026-03-09): pass
- `cargo test --workspace` (2026-03-09): pass
- `jcodemunch index_folder --incremental` (2026-03-09): aborted by the user after the local symbol-index refresh stalled; implementation and test verification were completed without the refresh

### Completion Notes List

- 2026-03-08: Story created from Epic 4.3 AC definitions in epics.md, architecture persistence correction ADR, Story 4.2 code review findings (M1/M2/L4), current ControlPlane trait surface, RegistryPersistence method inventory, and SpacetimeDB infrastructure scaffold state
- 2026-03-08: Expanded `ControlPlane` across mutable run state, added `RegistryBackedControlPlane`, and moved `RunManager` onto the control-plane boundary with a compatibility adapter that mirrors durable run state into the local registry for retrieval and startup-recovery paths
- 2026-03-08: Added `DiscoveryManifest`, persisted it from the indexing pipeline, and changed resume to validate checkpoint compatibility against the persisted manifest instead of live rediscovery
- 2026-03-08: Updated startup/recovery and retrieval integration tests for manifest-backed resume behavior; full `cargo test --quiet` suite passed after the control-plane compatibility refactor
- 2026-03-09: Added the SpacetimeDB mutable-state module under `spacetime/tokenizor/` with tables and reducers for repositories, index runs, checkpoints, per-run file records, idempotency records, and discovery manifests, and generated the Rust SDK client bindings for the module
- 2026-03-09: Implemented `SdkSpacetimeStateStore` and wired `SpacetimeControlPlane` onto the SDK-backed read and write path, including per-file upsert semantics, checkpoint persistence, repository persistence, and authoritative run-state reads
- 2026-03-09: Added explicit migration and mixed-state safety behavior: local mutable registry state is detected during deployment checks, authoritative Spacetime run-state reads and writes are gated until migration completes, and the operator-facing `tokenizor_agentic_mcp migrate control-plane` path copies legacy mutable state into SpacetimeDB before clearing only the mutable registry sections
- 2026-03-09: Tightened `RunManagerPersistenceAdapter` so Spacetime-backed mutable state no longer silently dual-writes to the registry, while the in-memory backend still falls back to the registry on cold start and repository/bootstrap data continues to mirror to the local registry where required
- 2026-03-09: Added migration-safety and authority-boundary coverage in `src/storage/control_plane.rs`, adapter/bootstrap-mirror coverage in `src/application/run_manager.rs`, and fixed the startup-recovery integration regression so the full workspace test suite now passes
- 2026-03-09: Review follow-up fixes closed the BMAD code-review findings: only fully healthy succeeded reindex runs now clear repository invalidation, the migration remediation and CLI now expose `tokenizor_agentic_mcp migrate control-plane`, and `SdkSpacetimeStateStore` now reuses a cached SpacetimeDB mutation connection with regression tests covering reuse and reconnect-on-failure
- 2026-03-09: Verification results: `cargo check --workspace` passed; `cargo test --test indexing_integration test_application_context_from_config_transitions_running_runs_before_runtime_ready` passed after the adapter fallback fix; `cargo fmt` passed; `cargo test --workspace` passed (488 library tests, 3 main-binary tests, 2 MCP hardening tests with 1 ignored benchmark, 71 indexing integration tests, 38 retrieval conformance tests, 34 retrieval integration tests, and 6 grammar tests)

### File List

- `_bmad-output/implementation-artifacts/4-3-move-mutable-run-durability-to-the-spacetimedb-control-plane.md`
- `Cargo.toml`
- `Cargo.lock`
- `spacetime/tokenizor/Cargo.toml`
- `spacetime/tokenizor/src/lib.rs`
- `spacetime/tokenizor/generated/`
- `src/application/run_manager.rs`
- `src/application/deployment.rs`
- `src/application/mod.rs`
- `src/config.rs`
- `src/domain/health.rs`
- `src/domain/index.rs`
- `src/domain/mod.rs`
- `src/domain/retrieval.rs`
- `src/indexing/pipeline.rs`
- `src/main.rs`
- `src/protocol/mcp.rs`
- `src/storage/control_plane.rs`
- `src/storage/local_cas.rs`
- `src/storage/mod.rs`
- `src/storage/registry_persistence.rs`
- `src/storage/spacetime_store.rs`
- `tests/indexing_integration.rs`
- `tests/retrieval_conformance.rs`
