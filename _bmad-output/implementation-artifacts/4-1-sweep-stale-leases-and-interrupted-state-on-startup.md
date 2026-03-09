# Story 4.1: Sweep Stale Leases and Interrupted State on Startup

Status: done

<!-- Note: Validation is optional. Run validate-create-story for quality check before dev-story. -->

## Story

As an operator,
I want Tokenizor to sweep stale leases, interrupted runs, and temporary recovery state on startup,
so that new mutating work does not begin on top of ambiguous operational state.

**FRs implemented:** FR37

- **FR37**: Users can inspect whether operational state indicates stale, interrupted, or suspect conditions.

## Acceptance Criteria

1. **Given** prior runs or leases were left stale by interruption or shutdown **When** Tokenizor starts **Then** it performs a startup recovery sweep before allowing new mutating operations **And** it records the detected stale or interrupted conditions explicitly
2. **Given** startup detects unrecoverable or incompatible operational state **When** the sweep completes **Then** Tokenizor blocks unsafe mutation paths **And** it reports actionable recovery or migration guidance

## Tasks / Subtasks

### Phase 1: Define the startup recovery contract

- [x] Task 1.1: Replace the current bare `Vec<String>` startup-sweep result with a structured startup recovery report (AC: 1, 2)
  - [x] 1.1.1: Introduce a startup recovery result model that captures at minimum: transitioned run ids, cleaned temp artifacts, blocking findings, and operator remediation guidance
  - [x] 1.1.2: Keep the model local-first and compatible with the current bootstrap-registry persistence path; do not invent a speculative SpacetimeDB write path in Story 4.1
  - [x] 1.1.3: Make repeated startup sweeps idempotent so a second clean startup produces a stable no-op report instead of duplicate mutations

- [x] Task 1.2: Define the startup sweep scope for stale temporary state explicitly (AC: 1)
  - [x] 1.2.1: Treat the current Tokenizor-owned temp surfaces as the sweep scope: registry sibling `.<registry>.*.tmp` files and CAS temp blobs under `TOKENIZOR_BLOB_ROOT/temp`
  - [x] 1.2.2: Do not sweep arbitrary OS temp locations, repository files, or hypothetical future lease tables that do not exist in the current codebase
  - [x] 1.2.3: If "lease" handling is needed for this story, model it narrowly as a startup-owned recovery finding over current runtime state, not as a full new lease subsystem

### Phase 2: Expand startup recovery execution

- [x] Task 2.1: Extend `RunManager::startup_sweep()` to reconcile all currently knowable stale startup state (AC: 1, 2)
  - [x] 2.1.1: Preserve the existing persisted `Running -> Interrupted` transition and explicit stale-run error summary
  - [x] 2.1.2: Add deterministic cleanup for stale registry temp files and stale CAS temp blobs, recording what was removed
  - [x] 2.1.3: If cleanup cannot safely complete or startup finds incompatible recovery state, return explicit blocking findings instead of silently continuing

- [x] Task 2.2: Keep startup orchestration centralized in `ApplicationContext::from_config()` (AC: 1, 2)
  - [x] 2.2.1: Reuse the existing startup hook so `run`, `doctor`, `init`, `attach`, `migrate`, `inspect`, and `resolve` all observe the same reconciled startup state
  - [x] 2.2.2: Emit structured startup logs that summarize transitioned runs, cleaned temp artifacts, and blocking findings
  - [x] 2.2.3: Do not move recovery sweep later in the flow; it must happen before any new mutating work can start

### Phase 3: Make recovery-required startup state observable

- [x] Task 3.1: Surface startup recovery findings through the deployment-readiness path (AC: 2)
  - [x] 3.1.1: Extend `DeploymentService::report()`, `DeploymentReport`, and/or `ComponentHealth` as needed so recovery-required startup findings appear as explicit health checks with remediation
  - [x] 3.1.2: Ensure `ApplicationContext::ensure_runtime_ready()` treats blocking startup recovery findings the same way it treats other run-blocking readiness failures
  - [x] 3.1.3: Keep operator wording actionable and aligned with Epic 4 next-safe-action vocabulary such as `repair`, `reindex`, `migrate`, `wait`, and `resolve_context`

- [x] Task 3.2: Preserve inspection-side evidence after startup reconciliation (AC: 1, 2)
  - [x] 3.2.1: Confirm interrupted runs remain visible through `inspect_run()` and `list_runs_with_health()` as `RunHealth::Unhealthy` with action-required guidance
  - [x] 3.2.2: Ensure startup sweep findings are observable through durable fields or readiness/reporting output; do not rely on transient logs alone for the core operator signal

### Phase 4: Verification

- [x] Task 4.1: Add focused run-manager and storage tests for expanded startup sweep behavior (AC: 1)
  - [x] 4.1.1: Verify persisted running runs transition to `Interrupted` exactly once on startup
  - [x] 4.1.2: Verify non-running terminal and non-terminal runs are not mutated incorrectly by the sweep
  - [x] 4.1.3: Verify stale registry temp files are cleaned only when they match Tokenizor-owned startup sweep patterns
  - [x] 4.1.4: Verify stale CAS temp blobs under `.tokenizor/temp` are cleaned safely and idempotently

- [x] Task 4.2: Add application and integration coverage for readiness coupling and operator visibility (AC: 1, 2)
  - [x] 4.2.1: Verify `ApplicationContext::from_config()` performs startup reconciliation before the runtime becomes available
  - [x] 4.2.2: Verify blocking startup recovery findings make readiness fail and prevent the serve path from starting
  - [x] 4.2.3: Verify interrupted runs remain inspectable as unhealthy with actionable guidance after startup sweep completes

### Review Follow-ups (AI)

- [ ] [AI-Review][MEDIUM] Stale `Queued` runs are not swept by `startup_sweep()` — a crash between `save_run()` and `transition_to_running()` leaves a permanently blocking stale Queued run per repo. Track as a follow-up in Epic 4 (scope wider than Story 4.1). [src/application/run_manager.rs:197-285]

## Dev Notes

### What Already Exists

`ApplicationContext::from_config()` already calls `RunManager::startup_sweep()` during application construction. `RunManager::startup_sweep()` currently transitions persisted `Running` runs to `Interrupted` and records a stale-run summary. `inspect_run()`, `list_runs_with_health()`, `classify_run_health()`, and `action_required_message()` already surface interrupted runs as `RunHealth::Unhealthy` with action-required text. Story 4.1 must extend that existing startup path rather than re-invent startup orchestration elsewhere.

### Current Gap to Close

The current implementation only handles one stale-startup condition: persisted runs stuck in `Running`. It does **not** yet:

- reconcile Tokenizor-owned temporary recovery artifacts
- surface startup recovery findings through `DeploymentReport`
- block unsafe startup on explicit recovery-required findings
- provide an explicit, structured startup recovery report for operators

There is also no current lease subsystem in `src/` or `tests/`. Story 4.1 must not pretend one already exists. Keep the implementation grounded in the actual local bootstrap-registry and CAS temp-state that the codebase owns today.

### Startup Recovery Guardrails

1. Keep the sweep before new work starts. The current startup hook in `ApplicationContext::from_config()` is the correct orchestration point.
2. Sweep only Tokenizor-owned paths. Current concrete temp surfaces are:
   - registry temp siblings created by atomic registry writes in `src/storage/registry_persistence.rs`
   - CAS temp files in `.tokenizor/temp` created by `LocalCasBlobStore::store_bytes()` in `src/storage/local_cas.rs`
3. Do not scan or delete generic OS temp directories, repository working trees, or future/hypothetical lease locations.
4. Treat blocking startup recovery findings as readiness failures, not as warnings hidden in logs.
5. Keep startup recovery deterministic and idempotent. A second startup after successful reconciliation should not create new mutations or duplicate operator noise.

### Recovery and Readiness Design Guidance

- `DeploymentService::report()` currently aggregates control-plane deployment checks and blob-store health only. Story 4.1 likely needs to thread startup recovery findings into this path so `ready_for_run` can become false for recovery-required startup state.
- `HealthIssueCategory` currently covers `Bootstrap`, `Dependency`, `Configuration`, `Compatibility`, and `Storage`. If Story 4.1 adds a recovery-specific readiness signal, extend the health model deliberately rather than overloading an unrelated category.
- `guard_and_serve()` in `src/main.rs` already blocks startup when `ensure_runtime_ready()` returns an error. Reuse that path instead of creating a second startup block mechanism.

### Existing Code to Reuse

| Function / Type | Location | Why it matters for 4.1 |
|---|---|---|
| `ApplicationContext::from_config()` | `src/application/mod.rs` | Existing startup hook; keep recovery sweep centralized here |
| `RunManager::startup_sweep()` | `src/application/run_manager.rs` | Current persisted-run sweep foundation |
| `RunManager::inspect_run()` | `src/application/run_manager.rs` | Inspection-side evidence after startup reconciliation |
| `RunManager::list_runs_with_health()` | `src/application/run_manager.rs` | Bulk inspection surface that should reflect startup results |
| `classify_run_health()` / `action_required_message()` | `src/application/run_manager.rs` | Existing interrupted/unhealthy signaling |
| `DeploymentService::report()` | `src/application/deployment.rs` | Current readiness aggregation path that needs recovery-required startup findings |
| `DeploymentReport` / `ComponentHealth` | `src/domain/health.rs` | Readiness/reporting model that may need extension |
| `LocalCasBlobStore::temp_blob_path()` / `store_bytes()` | `src/storage/local_cas.rs` | Real CAS temp-file ownership and naming pattern |
| `save_registry_data()` | `src/storage/registry_persistence.rs` | Real registry temp-file naming pattern |

### Epic 4 Hardening Context

The Epic 4.0 hardening checkpoint is complete before this story starts. Relevant guardrails already in place:

- Epic 4 Definition of Done is embedded in the story template
- `project-context.md` now carries Epic 4 recovery rules and the agent-selection policy
- full-chain MCP `call_tool` hardening tests live in `tests/epic4_hardening.rs`
- registry read benchmark evidence is recorded in `_bmad-output/implementation-artifacts/epic3-registry-benchmark.md`
- read-only registry query extraction is already complete; retrieval/query code uses the `RegistryQuery` boundary while write-capable startup recovery can continue using `RegistryPersistence`

### Agent Selection Constraint

Per `project-context.md`, **Claude Opus 4.6** is the default primary implementer for Epic 4 execution. If a different primary model is used for Story 4.1, record the exception and rationale in this story file before implementation starts.

### Testing Requirements

- Preserve and extend the existing unit coverage around `startup_sweep()`
- Add explicit tests for temp-artifact cleanup and repeated-startup idempotence
- Add readiness-level tests proving that blocking startup findings stop runtime startup
- Keep mutation-side and inspection-side verification paired: a startup mutation is not "done" until a read path proves the changed state is observable

### Epic 4 Definition of Done (mandatory)

- Expected test delta: Add focused unit and integration coverage for startup sweep transitions, temp-artifact cleanup, repeated-startup idempotence, and readiness blocking/reporting
- Build/test evidence: Record exact `cargo test` command(s) and pass/fail summary before requesting review
- Acceptance-criteria traceability:
  - AC1 -> startup sweep execution path, structured recovery report, interrupted-run inspection, temp-artifact cleanup tests
  - AC2 -> readiness/reporting integration, blocking startup test, actionable operator guidance assertions
- Trust-boundary traceability: Cite the startup recovery, explicit outcome, durability, and next-safe-action rules from architecture and `project-context.md`
- State-transition evidence: Prove both sides for every transition:
  - persisted `Running -> Interrupted`
  - stale temp artifact present -> cleaned or explicitly blocked
  - blocking recovery finding present -> runtime readiness denied

### Self-Audit Checklist (mandatory before requesting review)

_Run this checklist after all tasks are complete. This is a blocking step - do not request review until every item is verified._

#### Generic Verification
- [x] For every task marked `[x]`, cite the specific test that verifies it — Tests cited in Completion Notes (AC1: 8 tests, AC2: 6 tests). All 618 tests pass via `cargo test`.
- [x] For every new error variant or branch, confirm a test exercises it — Blocking findings exercised by `test_startup_sweep_reports_blocking_findings_for_malformed_temp_artifacts`; run-transition errors by `test_startup_sweep_transitions_running_to_interrupted`; readiness blocking by `ensure_runtime_ready_blocks_on_startup_recovery_errors`.
- [x] For every computed value, trace it to where it surfaces (log, return value, persistence) — `StartupRecoveryReport` surfaces through structured `info!`/`warn!` logging in `ApplicationContext::from_config()`, through `readiness_checks()` → `DeploymentReport`, and through `ensure_runtime_ready()` → `guard_and_serve()`.
- [x] For every test, verify the assertion can actually fail (no `assert!(true)`, no conditionals that always pass) — All assertions use concrete `assert_eq!` on status enums, `assert!` on string content, and `expect_err` on error paths. No `assert!(true)` or tautological conditions found.

#### Epic 4 Recovery Verification
- [x] The declared expected test delta was met or exceeded by the actual implementation — Story declared: startup sweep transitions, temp-artifact cleanup, idempotence, readiness blocking/reporting. All implemented with 12+ dedicated tests across unit and integration.
- [x] Build/test evidence is recorded with the exact command and outcome summary — Completion Notes record: `cargo fmt`, focused test commands, and full `cargo test` pass with 468 library + 67 integration + all other suites.
- [x] Every acceptance criterion is traced to concrete implementation code and at least one concrete test — AC1 → `startup_sweep()` + 8 tests; AC2 → `readiness_checks()` + `merge_startup_recovery_checks()` + 6 tests. Traced in Completion Notes.
- [x] Every trust-boundary or recovery-policy decision cites the exact architecture or `project-context.md` source — Completion Notes cite `project-context.md` Epic 4 rules for deterministic startup recovery, explicit action-required classification, readiness-visible outcomes, and shared next-safe-action vocabulary.
- [x] Every state transition is tested from both sides: the mutation itself and the resulting retrieval/inspection behavior — `Running → Interrupted` tested by `test_startup_sweep_transitions_running_to_interrupted` (write) + `test_inspect_interrupted_run_returns_unhealthy` and `test_list_runs_after_startup_sweep_keeps_interrupted_runs_unhealthy` (read). Temp artifact present → cleaned tested by `test_startup_sweep_cleans_owned_temp_artifacts_and_is_idempotent`. Blocking finding → readiness denied tested by `test_application_context_startup_reconciliation_blocks_readiness_on_blocking_finding`.

#### Story 4.1-Specific Verification
- [x] Confirm startup reconciliation still happens before new mutating work is allowed to begin — `ApplicationContext::from_config()` calls `startup_sweep()` before returning `Ok(Self{...})`. Verified by `test_application_context_from_config_transitions_running_runs_before_runtime_ready`.
- [x] Confirm only Tokenizor-owned temp paths are swept — `is_owned_registry_temp_artifact_path` and `is_owned_temp_blob_path` filter candidates. Verified by `owned_temp_blob_path_matches_only_tokenizor_temp_pattern` and `test_owned_registry_temp_artifact_matches_only_registry_sibling_pattern`. Non-matching files preserved in `test_startup_sweep_cleans_owned_temp_artifacts_and_is_idempotent`.
- [x] Confirm repeated startup sweeps are idempotent — Second sweep in `test_startup_sweep_cleans_owned_temp_artifacts_and_is_idempotent` returns empty report.
- [x] Confirm blocking startup recovery findings surface through `DeploymentReport` / readiness rather than logs only — `deployment_report_includes_startup_recovery_warnings` and `ensure_runtime_ready_blocks_on_startup_recovery_errors` verify the readiness path; `test_application_context_startup_reconciliation_blocks_readiness_on_blocking_finding` verifies end-to-end.
- [x] Confirm interrupted runs remain inspectable as `RunHealth::Unhealthy` with action-required guidance after the sweep — `test_inspect_interrupted_run_returns_unhealthy` and `test_list_runs_after_startup_sweep_keeps_interrupted_runs_unhealthy` verify this.
- [x] Confirm no new speculative lease subsystem or remote control-plane write path was introduced — No new files, traits, or modules created. All changes extend existing `RunManager`, `RegistryPersistence`, `LocalCasBlobStore`, and `health.rs`. No SpacetimeDB write paths added.

_Checklist completed during code review on 2026-03-08 by Claude Opus 4.6 (adversarial reviewer)._

### Project Structure Notes

| File | Why it is in scope |
|---|---|
| `src/application/mod.rs` | Startup orchestration entrypoint |
| `src/application/run_manager.rs` | Existing startup sweep foundation and run inspection behavior |
| `src/application/deployment.rs` | Deployment/readiness reporting path |
| `src/domain/health.rs` | Health and readiness result model |
| `src/domain/index.rs` | Interrupted run status / inspection payloads |
| `src/storage/registry_persistence.rs` | Registry temp-file ownership and cleanup rules |
| `src/storage/local_cas.rs` | CAS temp-file ownership and cleanup rules |
| `tests/indexing_integration.rs` | End-to-end startup and inspection verification |
| `tests/epic4_hardening.rs` | Existing Epic 4 hardening context and integration-test style reference |

**Alignment notes**

- Stay within the current `application` / `domain` / `storage` layering
- Do not create a broad new subsystem when the current code only needs a startup-recovery expansion
- If a future `application/services/recovery_sweep.rs` shape is helpful, keep the extraction bounded and justified by the current implementation, not just the architecture target tree

### References

- [Source: _bmad-output/planning-artifacts/epics.md#Epic 4.0 Hardening Checkpoint]
- [Source: _bmad-output/planning-artifacts/epics.md#Story 4.1]
- [Source: _bmad-output/planning-artifacts/prd.md#Journey 2: Primary User Recovery Path - The System Recovers Without Losing Trust]
- [Source: _bmad-output/planning-artifacts/prd.md#Reliability]
- [Source: _bmad-output/planning-artifacts/architecture.md#Infrastructure, Deployment & Runtime]
- [Source: _bmad-output/planning-artifacts/architecture.md#Data Architecture]
- [Source: _bmad-output/planning-artifacts/architecture.md#Implementation Patterns]
- [Source: _bmad-output/project-context.md#Epic 4 Recovery Architecture]
- [Source: _bmad-output/project-context.md#Agent Selection]
- [Source: src/application/mod.rs]
- [Source: src/application/run_manager.rs]
- [Source: src/application/deployment.rs]
- [Source: src/domain/health.rs]
- [Source: src/storage/local_cas.rs]
- [Source: src/storage/registry_persistence.rs]
- [Source: tests/indexing_integration.rs]

## Dev Agent Record

### Agent Model Used

GPT-5 Codex

### Debug Log References

- `cargo fmt`
- `cargo test startup_sweep_ --lib`
- `cargo test startup_recovery --lib`
- `cargo test owned_ --lib`
- `cargo test list_runs_after_startup_sweep_keeps_interrupted_runs_unhealthy --test indexing_integration`
- `cargo test application_context_from_config_transitions_running_runs_before_runtime_ready --test indexing_integration`
- `cargo test application_context_startup_reconciliation --test indexing_integration`
- `cargo test`
- `jcodemunch index_folder --incremental` completed despite an MCP timeout symptom; `local/tokenizor_agentic_mcp` refreshed at `2026-03-08T20:02:51.386845`

### Completion Notes List

- 2026-03-08: Created Story 4.1 from Epic 4 planning artifacts, PRD recovery requirements, architecture startup-recovery rules, and current codebase state
- 2026-03-08: Captured the current partial implementation baseline (`ApplicationContext::from_config()` + `RunManager::startup_sweep()`) so implementation can extend it instead of re-creating startup recovery from scratch
- 2026-03-08: Scoped stale temporary-state cleanup to the concrete Tokenizor-owned temp paths that already exist in the registry and CAS code
- 2026-03-08: Epic 4 implementer exception recorded: GPT-5 Codex is executing Story 4.1 in the current user-directed session with explicit verification and bounded scope against the existing recovery architecture
- 2026-03-08: Replaced the bare startup-sweep `Vec<String>` with a structured `StartupRecoveryReport` that records transitioned run ids, cleaned Tokenizor-owned temp artifacts, blocking findings, and deduplicated operator guidance without introducing any speculative lease subsystem or remote control-plane write path
- 2026-03-08: Kept recovery execution centralized in `ApplicationContext::from_config()`, added structured startup summary logging, preserved the persisted `Running -> Interrupted` transition, and added deterministic cleanup for registry sibling temp files plus CAS temp blobs under `TOKENIZOR_BLOB_ROOT/temp`
- 2026-03-08: Surfaced startup recovery through deployment readiness by adding `HealthIssueCategory::Recovery`, merging startup recovery checks into `DeploymentReport`, and making blocking startup findings fail `ensure_runtime_ready()` with actionable `repair` / `reindex` / `migrate` / `wait` guidance
- 2026-03-08: AC1 verification: `test_startup_sweep_transitions_running_to_interrupted`, `test_startup_sweep_ignores_non_running_statuses`, `test_startup_sweep_cleans_owned_temp_artifacts_and_is_idempotent`, `owned_temp_blob_path_matches_only_tokenizor_temp_pattern`, `test_owned_registry_temp_artifact_matches_only_registry_sibling_pattern`, `test_application_context_from_config_transitions_running_runs_before_runtime_ready`, `test_inspect_interrupted_run_returns_unhealthy`, and `test_list_runs_after_startup_sweep_keeps_interrupted_runs_unhealthy`
- 2026-03-08: AC2 verification: `deployment_report_includes_startup_recovery_warnings`, `ensure_runtime_ready_blocks_on_startup_recovery_errors`, `test_startup_sweep_reports_blocking_findings_for_malformed_temp_artifacts`, `test_application_context_startup_reconciliation_blocks_readiness_on_blocking_finding`, `tests::guard_and_serve_stops_before_serving_when_readiness_fails`, and the full `cargo test` regression run all passed
- 2026-03-08: Build/test evidence: `cargo fmt`; focused recovery test commands above; `cargo test` passed with `468` library tests, `3` binary tests, `2` Epic 4 hardening tests passed (`1` ignored benchmark), `67` indexing integration tests, `38` retrieval conformance tests, `34` retrieval integration tests, and `6` tree-sitter grammar tests
- 2026-03-08: Trust-boundary traceability: implementation follows the `project-context.md` Epic 4 rules for deterministic startup recovery, explicit action-required classification, readiness-visible outcomes, and shared next-safe-action vocabulary while preserving the current local bootstrap registry + CAS split

### Change Log

- 2026-03-08: Story created and marked ready-for-dev
- 2026-03-08: Implemented structured startup recovery, bounded temp-artifact cleanup, recovery readiness signaling, and verification coverage; story advanced to review
- 2026-03-08: Code review (Claude Opus 4.6, adversarial): C1 self-audit checklist completed with evidence; M2 stricter registry temp validation fix applied and tested; M1 stale-Queued-run gap tracked as follow-up action item; story advanced to done

### File List

- `_bmad-output/implementation-artifacts/4-1-sweep-stale-leases-and-interrupted-state-on-startup.md`
- `_bmad-output/implementation-artifacts/sprint-status.yaml`
- `src/application/mod.rs`
- `src/application/run_manager.rs`
- `src/domain/health.rs`
- `src/storage/local_cas.rs`
- `src/storage/registry_persistence.rs`
- `tests/indexing_integration.rs`
