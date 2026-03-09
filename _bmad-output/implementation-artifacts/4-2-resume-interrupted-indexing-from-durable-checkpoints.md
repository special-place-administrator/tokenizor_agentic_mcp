# Story 4.2: Resume Interrupted Indexing from Durable Checkpoints

Status: done

<!-- Note: Validation is optional. Run validate-create-story for quality check before dev-story. -->

## Story

As an operator,
I want interrupted indexing to resume from durable checkpoints when possible,
so that long-running work does not always restart from zero.

**FRs implemented:** FR30

- **FR30**: Users can resume interrupted indexing work without losing all prior progress when recovery is possible.

## Acceptance Criteria

1. **Given** an interrupted indexing run has a valid checkpoint and compatible source state **When** recovery is initiated **Then** Tokenizor resumes the run from durable checkpoint state **And** the resumed run remains inspectable as managed operational work
2. **Given** checkpoint-based recovery is not possible **When** recovery is attempted **Then** Tokenizor returns an explicit recovery outcome **And** it points to deterministic re-index as the safe fallback

## Tasks / Subtasks

### Phase 1: Define the resume contract and safety boundary

- [x] Task 1.1: Introduce an explicit interrupted-run recovery contract instead of treating resume as an implied side effect (AC: 1, 2)
  - [x] 1.1.1: Define the minimum resume eligibility checks on top of the current codebase: interrupted run status, latest checkpoint present, checkpoint cursor present, no conflicting active run for the repo, and no health/invalidation state that makes trusting the partial run unsafe
  - [x] 1.1.2: Add the smallest backward-compatible run or recovery metadata needed so status inspection can distinguish interrupted, resumed, and resume-rejected outcomes without breaking existing registry files
  - [x] 1.1.3: Keep the recovery result explicit and structured; do not rely on ad hoc free-text strings alone when deciding whether the next safe action is resume, repair, wait, or re-index

- [x] Task 1.2: Choose an explicit recovery entrypoint that fits the current product surface without inventing speculative infrastructure (AC: 1, 2)
  - [x] 1.2.1: Implement resume first at the application and run-manager layer, then expose it through a narrow operator or MCP-facing path only if the surface can remain explicit and non-blocking
  - [x] 1.2.2: Do not overload normal fresh indexing startup with silent implicit resume behavior that would hide recovery decisions from the operator
  - [x] 1.2.3: Do not introduce speculative SpacetimeDB write paths, a fake lease subsystem, or a premature broad `repair_index` subsystem in Story 4.2

### Phase 2: Make checkpoint state durably resumable

- [x] Task 2.1: Close the current durability gap between checkpoint state and persisted partial indexing outputs (AC: 1, 2)
  - [x] 2.1.1: Ensure any file records or equivalent resume-critical metadata for files at or before the checkpoint cursor are durably persisted before a checkpoint is treated as safe to resume from
  - [x] 2.1.2: Preserve the existing registry locking and atomic write discipline; resume durability must still go through `RegistryPersistence`, not through in-memory-only state
  - [x] 2.1.3: Keep checkpoint replay idempotent so repeated resume attempts do not duplicate persisted file records, corrupt counters, or advance the cursor past non-durable work

- [x] Task 2.2: Add the recovery read-paths needed to inspect interrupted work deterministically (AC: 1, 2)
  - [x] 2.2.1: Reuse and extend existing registry queries such as `find_run`, `get_latest_checkpoint`, `get_file_records`, and repository status lookup rather than creating a second persistence source of truth
  - [x] 2.2.2: Validate compatibility against the deterministic sorted discovery order already used by the pipeline; recovery must not depend on filesystem enumeration luck
  - [x] 2.2.3: Keep all newly persisted fields backward-compatible with existing Epic 1 and Epic 2 registry files using `Option<T>` plus `#[serde(default)]` where needed

### Phase 3: Resume execution and preserve inspectability

- [x] Task 3.1: Resume interrupted work from the durable checkpoint boundary instead of restarting from zero (AC: 1)
  - [x] 3.1.1: Rediscover files using the existing `ignore`-based walker and deterministic lowercase forward-slash sort, then skip only the files at or before the recovered checkpoint cursor
  - [x] 3.1.2: Seed resumed progress from the recovered checkpoint counts and cursor so run inspection reflects continued progress rather than a fake fresh run
  - [x] 3.1.3: Preserve one-active-run-per-repo enforcement and keep the resumed work inspectable through the existing run status surfaces while it is active and after it completes

- [x] Task 3.2: Return explicit fallback outcomes when resume is unsafe or impossible (AC: 2)
  - [x] 3.2.1: Detect at minimum: missing checkpoint, empty cursor, checkpoint cursor missing from the rediscovered file set, missing durable partial outputs, and conflicting repo or run state
  - [x] 3.2.2: Return an explicit recovery outcome that points to deterministic re-index as the safe fallback rather than hiding the reason inside a generic error
  - [x] 3.2.3: Tighten interrupted-run inspection so `get_index_run` and related status paths surface actionable next-step guidance that matches Epic 4 recovery vocabulary instead of vague prose

### Phase 4: Verification

- [x] Task 4.1: Add focused unit tests for resume eligibility, checkpoint durability, and idempotent replay behavior (AC: 1, 2)
  - [x] 4.1.1: Verify interrupted runs with valid checkpoints and durable prior outputs resume from the saved cursor without reprocessing already durable files
  - [x] 4.1.2: Verify interrupted runs with invalid or incompatible checkpoint state do not resume and instead return explicit re-index guidance
  - [x] 4.1.3: Verify repeated resume attempts are deterministic and do not duplicate persisted file records or drift progress counters

- [x] Task 4.2: Add integration coverage for operator-visible recovery behavior (AC: 1, 2)
  - [x] 4.2.1: Verify startup-swept interrupted runs remain inspectable and can later be resumed through the chosen recovery entrypoint
  - [x] 4.2.2: Verify the resumed run remains visible as managed operational work through `get_index_run` while active and after completion
  - [x] 4.2.3: Verify the fallback path surfaces explicit re-index guidance when recovery cannot proceed safely

- [x] Task 4.3: Add a basic latency sanity check for the resume eligibility and inspection path (AC: 1, 2)
  - [x] 4.3.1: Include at least one fixture-backed assertion that resume inspection and eligibility determination stay within a reasonable bound for the Epic 4 recovery flow

### Review Follow-ups (AI)

- [x] [AI-Review][MEDIUM] Stale `Queued` runs are still not swept by `startup_sweep()`. Deferred as explicit Epic 4 debt: Story 4.2 resume remains limited to `Interrupted` runs and rejects conflicting active `Queued`/`Running` repo state instead of treating `Queued` as resumable. [src/application/run_manager.rs]
  - **Resolved**: Follow-up implemented in `startup_sweep()` — stale `Queued` with durable evidence → `Interrupted`; without → `Aborted`.
#### Deferred debt: incremental durability performance (future story)
- [ ] [AI-Review][MEDIUM] Per-file registry writes create O(n) full read-modify-write cycles for large repositories. Each `persist_durable_file_record()` rewrites the entire registry JSON. Consider a sidecar journal or batched flush strategy. [src/application/run_manager.rs]
- [ ] [AI-Review][LOW] File records are persisted twice per file during resume runs — once via `durable_record_callback` and again in the final batch `save_file_records`. The final batch is idempotent but redundant when the callback is active. [src/application/run_manager.rs]

#### Known design tradeoffs (acceptable)
- [x] [AI-Review][MEDIUM] Discovery TOCTOU between `resume_run()` eligibility check and `pipeline.execute()` — both call `discover_files()` independently. Pipeline self-heals on missing cursor; inherent in the two-phase design. Accepted as-is.
- [x] [AI-Review][MEDIUM] Progress counters double-initialized during resume (`with_resume_state` then `process_discovered`). Harmless; counters converge before any meaningful progress query. Accepted as-is.

#### Optional cleanup
- [ ] [AI-Review][LOW] `action_required_message()` uses string comparison against `STALE_QUEUED_ABORTED_STARTUP_SWEEP_SUMMARY` to distinguish startup-aborted from circuit-breaker-aborted runs. A typed field would be more robust. Not a blocker. [src/application/run_manager.rs]
- [ ] [AI-Review][LOW] `transition_to_running` hardcodes `Interrupted` as the only recovery-eligible terminal state. Add an explanatory comment next time this code is touched. [src/storage/registry_persistence.rs]

**Note**: `RecoveryStateKind::Resumed` persisting on completed runs is intentional audit/provenance behavior — not an issue.

## Dev Notes

### What Already Exists

`ApplicationContext::from_config()` already runs `RunManager::startup_sweep()` during startup. Story 4.1 extended that sweep so persisted `Running` runs transition to `Interrupted`, startup findings flow into readiness, and interrupted runs remain visible through `inspect_run()` and `list_runs_with_health()`.

The checkpointing path already exists:

- `RunManager::checkpoint_run()` reads live progress and persists a `Checkpoint`
- `RegistryPersistence::save_checkpoint()` stores checkpoint records and updates `run.checkpoint_cursor`
- `IndexingPipeline` already tracks contiguous completion with `CheckpointTracker`
- `discovery::discover_files()` already produces deterministic sorted relative paths that are suitable for replay-safe resume logic

There is also already a non-blocking MCP inspection surface:

- `get_index_run` returns `RunStatusReport`
- `checkpoint_now` persists checkpoint state for active runs
- `reindex_repository` exists as a deterministic fresh-run fallback, not as a resume path

### Current Gap to Close

The current implementation can mark a run interrupted and can persist checkpoints, but it **cannot actually resume work yet**. More importantly, the current durability boundary is not sufficient for safe resume:

- periodic checkpoints persist cursor and counts during the run
- file records are currently flushed to the registry only after the pipeline finishes
- the checkpoint tracker marks files complete after CAS commit, not after registry-side partial output durability

That means Story 4.2 must not simply skip files at or before the checkpoint cursor unless the story first makes the already-completed portion durably recoverable. A checkpoint that outruns durable partial metadata would cause silent data loss on resume.

### Resume Guardrails

1. Do not skip files based only on `checkpoint_cursor` unless the already-completed portion is durably recoverable.
2. Reuse deterministic discovery ordering and `CheckpointTracker`; do not invent unordered resume heuristics.
3. Keep `RunManager` as the long-lived owner of background lifecycle and maintain one active run per repository.
4. Keep MCP tool handlers non-blocking. Resume should return promptly with a managed run reference, not block on full recovery completion.
5. If recovery is not safe, fail explicitly and point to deterministic re-index. Never silently fall back to a fresh run while pretending resume happened.

### Recovery Design Guidance

- Prefer extending the existing run lifecycle instead of building a second recovery orchestration stack.
- A minimal viable design is an explicit application or run-manager resume entrypoint that:
  - loads the interrupted run and latest checkpoint
  - validates compatibility against current repo state
  - seeds resumed execution from the durable checkpoint boundary
  - keeps the run inspectable through the existing status APIs
- If same-run resume is used, preserve inspectability and auditability with backward-compatible metadata rather than inventing a parallel run identity just to represent recovery.
- If a typed next-action or recovery-outcome model is introduced, align it deliberately with the existing retrieval-side `NextAction` vocabulary instead of adding one-off strings that Story 4.6 will later have to clean up.

### Existing Code to Reuse

| Function / Type | Location | Why it matters for 4.2 |
|---|---|---|
| `ApplicationContext::from_config()` | `src/application/mod.rs` | Startup already transitions stale running work to interrupted state |
| `RunManager::startup_sweep()` | `src/application/run_manager.rs` | Existing interrupted-run creation point |
| `RunManager::checkpoint_run()` | `src/application/run_manager.rs` | Current checkpoint persistence and cursor handling |
| `RunManager::inspect_run()` / `build_run_report()` | `src/application/run_manager.rs` | Existing run inspection path that must surface recovery state |
| `RunManager::launch_run()` / `reindex_repository()` | `src/application/run_manager.rs` | Existing background-run orchestration and deterministic fallback run creation |
| `RegistryPersistence::save_checkpoint()` / `get_latest_checkpoint()` | `src/storage/registry_persistence.rs` | Existing durable checkpoint storage |
| `RegistryPersistence::save_file_records()` / `get_file_records()` | `src/storage/registry_persistence.rs` | Existing partial-output persistence boundary that resume must make safe |
| `IndexingPipeline::execute()` / `process_discovered()` | `src/indexing/pipeline.rs` | Current pipeline orchestration, checkpoint callback wiring, and deterministic file ordering |
| `CheckpointTracker` | `src/indexing/pipeline.rs` | Existing contiguous-high-water cursor model |
| `discover_files()` | `src/indexing/discovery.rs` | Existing gitignore-aware deterministic discovery path |
| `commit_file_result()` | `src/indexing/commit.rs` | Current durable CAS commit boundary for per-file results |
| `get_index_run` / `checkpoint_now` / `reindex_repository` | `src/protocol/mcp.rs` | Current MCP surfaces that recovery must stay compatible with |

### Previous Story Intelligence

Story 4.1 already established the recovery baseline that Story 4.2 must build on rather than replace:

- startup recovery is centralized in `ApplicationContext::from_config()`
- interrupted runs are already surfaced as unhealthy and action-required
- startup recovery findings already participate in readiness instead of being log-only noise
- Epic 4 hardening checkpoints are complete, so Story 4.2 can assume the Epic 4 Definition of Done, agent-selection policy, MCP hardening tests, and registry benchmark evidence already exist

Important carry-forward learning from Story 4.1:

- do not invent speculative infrastructure that the repo does not yet own
- keep recovery logic grounded in the real registry and CAS persistence paths
- pair every state mutation with a read-side proof that the operator can observe the changed state

### Git Intelligence

Recent git history shows the repo has just completed Epic 4 hardening and the Epic 3 retrieval baseline. The most relevant pattern is that recovery and trust work is being added incrementally on top of the existing scaffold, not through large subsystem rewrites:

- `4123ee4` - Formalize Epic 4.0 hardening
- `225a778` - docs: update README to reflect Epic 3 completion
- `a06e67a` - feat: implement Epic 3 - trusted code discovery and verified retrieval
- `4eb6f9a` - docs: complete Epic 3 preparation tasks

Story 4.2 should follow that same pattern: extend the current run manager, registry persistence, and pipeline paths rather than replacing them.

### Library / Framework Requirements

- Keep the story on the repo-pinned versions already in `Cargo.toml` unless the implementation proves a dependency upgrade is strictly necessary:
  - `rmcp = 1.1.0`
  - `tokio = 1.48`
  - `tokio-util = 0.7`
  - `ignore = 0.4`
  - `fs2 = 0.4`
- Official current docs confirm the existing technology choices still fit this story:
  - `tokio_util::sync::CancellationToken` remains the right cooperative cancellation primitive for background resume work
  - `ignore::WalkBuilder` remains the right gitignore-aware filesystem walker to preserve deterministic discovery before resume filtering
  - `rmcp` still fits the current `#[tool]`-driven MCP surface; if Story 4.2 exposes recovery over MCP, keep it within the existing tool model
- Do not introduce a custom cancellation primitive, raw filesystem recursion, or a second MCP server pattern for this story.

### Testing Requirements

- Test both sides of recovery:
  - mutation side: interrupted run plus checkpoint becomes resumed or explicitly rejected
  - read side: `get_index_run` and related inspection surfaces show the correct resumed or fallback-required state
- Add coverage for repeated resume attempts and restart-safety, not just a single happy-path run
- Add at least one test proving already-durable partial outputs are not duplicated or corrupted by resume
- Keep recovery proofs local-first and registry-backed; do not treat in-memory-only test state as evidence of durability

### Epic 4 Definition of Done (mandatory)

- Expected test delta: Add focused unit and integration coverage for resume eligibility, durable partial-output persistence, resumed progress reporting, and explicit re-index fallback outcomes
- Build/test evidence: Record the exact `cargo test` command(s) and pass/fail summary before requesting review
- Acceptance-criteria traceability:
  - AC1 -> interrupted-run recovery contract, durable checkpoint boundary, resumed execution path, and run inspection updates
  - AC2 -> explicit recovery outcome for unsafe resume, deterministic re-index guidance, and protocol or operator-facing status coverage
- Trust-boundary traceability: Cite the architecture and `project-context.md` rules for deterministic recovery, checkpoint durability, explicit action-required classification, and shared next-safe-action guidance
- State-transition evidence: Prove both sides for every transition:
  - `Interrupted -> Running/active resumed work`
  - `durable partial progress -> resumed skip boundary`
  - `resume rejected -> explicit fallback guidance and unchanged unsafe state`

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

#### Story 4.2-Specific Verification
- [x] Confirm resume never skips work that was not durably recoverable
- [x] Confirm deterministic discovery ordering is preserved during resume eligibility checks and resumed execution
- [x] Confirm interrupted runs remain inspectable before, during, and after resume
- [x] Confirm repeated resume attempts are idempotent or explicitly rejected without corrupting persisted state
- [x] Confirm unsafe resume paths point to deterministic re-index rather than silently starting a fresh run
- [x] Confirm no speculative SpacetimeDB write path or unrelated repair subsystem was introduced

### Project Structure Notes

| File | Why it is in scope |
|---|---|
| `src/application/mod.rs` | Application entrypoints for launch or resume orchestration |
| `src/application/run_manager.rs` | Run lifecycle, checkpointing, interrupted-run inspection, and resume orchestration |
| `src/domain/index.rs` | Run, checkpoint, and recovery metadata types |
| `src/indexing/pipeline.rs` | Checkpoint tracker, deterministic processing order, and resumed execution path |
| `src/indexing/discovery.rs` | Deterministic repo file discovery used by checkpoint replay |
| `src/indexing/commit.rs` | Durable CAS commit boundary for per-file work |
| `src/storage/registry_persistence.rs` | Checkpoint and partial-output durability |
| `src/protocol/mcp.rs` | Existing run inspection and checkpoint tool surfaces |
| `tests/indexing_integration.rs` | End-to-end recovery and operator-visible verification |

**Alignment notes**

- Stay inside the current `application` / `domain` / `indexing` / `storage` layering.
- If resume needs new helper extraction, keep it bounded and motivated by the current code rather than by the target architecture tree alone.
- Do not let Story 4.2 balloon into Story 4.3 repair orchestration or Story 4.5 operational-history architecture.

### References

- [Source: _bmad-output/planning-artifacts/epics.md#Story 4.2]
- [Source: _bmad-output/planning-artifacts/epics.md#Epic 4.0 Hardening Checkpoint]
- [Source: _bmad-output/planning-artifacts/prd.md#Journey 2: Primary User Recovery Path - The System Recovers Without Losing Trust]
- [Source: _bmad-output/planning-artifacts/prd.md#Functional Requirements]
- [Source: _bmad-output/planning-artifacts/prd.md#Non-Functional Requirements]
- [Source: _bmad-output/planning-artifacts/architecture.md#Project Context Analysis]
- [Source: _bmad-output/planning-artifacts/architecture.md#API / Protocol & Communication]
- [Source: _bmad-output/planning-artifacts/architecture.md#Infrastructure / Deployment Safety]
- [Source: _bmad-output/project-context.md#Epic 2 Persistence Architecture]
- [Source: _bmad-output/project-context.md#Checkpointing & Resume]
- [Source: _bmad-output/project-context.md#MCP Server & Run Management]
- [Source: _bmad-output/project-context.md#Epic 4 Recovery Architecture]
- [Source: _bmad-output/project-context.md#Agent Selection]
- [Source: _bmad-output/implementation-artifacts/4-1-sweep-stale-leases-and-interrupted-state-on-startup.md]
- [Source: Cargo.toml]
- [Source: src/application/mod.rs]
- [Source: src/application/run_manager.rs]
- [Source: src/domain/index.rs]
- [Source: src/domain/retrieval.rs]
- [Source: src/indexing/pipeline.rs]
- [Source: src/indexing/discovery.rs]
- [Source: src/indexing/commit.rs]
- [Source: src/storage/registry_persistence.rs]
- [Source: src/protocol/mcp.rs]
- [Source: tests/indexing_integration.rs]
- [Source: docs.rs tokio-util `CancellationToken` documentation]
- [Source: docs.rs `ignore::WalkBuilder` documentation]
- [Source: docs.rs `rmcp` crate documentation]

## Dev Agent Record

### Agent Model Used

Claude Opus 4.6 (default primary implementer for Epic 4 per `project-context.md`; record any exception before implementation starts)

### Debug Log References

- `cargo test --no-run`
- `cargo test resume_ -- --nocapture`
- `cargo test`
- `cargo fmt`

### Completion Notes List

- 2026-03-08: Story created from Epic 4 planning artifacts, PRD recovery requirements, current architecture and project-context rules, Story 4.1 learnings, and the current run-manager / pipeline / registry implementation state
- 2026-03-08: Captured the central resume blind spot: checkpoints currently persist cursor and counts before partial file metadata is durably available for replay-safe skip behavior
- 2026-03-08: Scoped Story 4.2 to safe interrupted-run recovery, explicit fallback guidance, and status-surface updates without pulling in the later broad repair or operational-history stories
- 2026-03-08: Implemented durable per-file registry upserts ahead of checkpoint advancement, same-run resume orchestration, explicit recovery metadata and outcomes, and an MCP `resume_index_run` entrypoint
- 2026-03-08: Added unit and integration coverage for successful resume, rejected resume with explicit `reindex` guidance, repeated resume attempts with `wait` guidance, and a latency sanity bound for resume eligibility
- 2026-03-08: Left stale `Queued` startup sweep behavior as explicit Epic 4 debt; Story 4.2 resume only trusts `Interrupted` runs and rejects conflicting queued/running repo state instead of inferring resumability
- 2026-03-08: Verification results: `cargo test --no-run` passed, `cargo test resume_ -- --nocapture` passed (6 targeted recovery tests), and `cargo test` passed (474 unit/integration tests, plus 3 main-binary tests, 2 MCP hardening tests with 1 ignored benchmark, 71 indexing integration tests, 38 retrieval conformance tests, 34 retrieval integration tests, and 6 grammar tests)

### File List

- `_bmad-output/implementation-artifacts/4-2-resume-interrupted-indexing-from-durable-checkpoints.md`
- `src/application/mod.rs`
- `src/application/run_manager.rs`
- `src/domain/index.rs`
- `src/domain/mod.rs`
- `src/domain/retrieval.rs`
- `src/indexing/pipeline.rs`
- `src/protocol/mcp.rs`
- `src/storage/registry_persistence.rs`
- `tests/indexing_integration.rs`
- `tests/retrieval_conformance.rs`
