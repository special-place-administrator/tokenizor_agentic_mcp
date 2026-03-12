# Structure

This repo is organized around a Rust runtime in `src/` plus black-box tests in `tests/`.

Top-level layout:
- `Cargo.toml`: crate manifest, runtime dependencies, tree-sitter grammars, RMCP transport, and Tokio runtime settings.
- `Cargo.lock`: locked dependency graph.
- `src/lib.rs`: public crate surface; re-exports `ApplicationContext`, `ServerConfig`, `ControlPlaneBackend`, `TokenizorError`, and `TokenizorServer`.
- `src/main.rs`: CLI/runtime entry point and readiness gate.
- `tests/`: integration and conformance coverage for indexing, retrieval, grammar loading, and MCP hardening.
- `docs/`: human-written product and architecture notes; useful context, but not the runtime authority.
- `spacetime/`: early SpacetimeDB module area; currently only `spacetime/tokenizor/README.md`.
- `.planning/codebase/`: generated codebase-map docs.
- `.codex/`, `.claude/`, `.gemini/`, `.opencode/`, `.serena/`: workflow or agent scaffolding, not core runtime code.
- `target/`: Cargo build output.

`src/` ownership map:
- `src/application/`: orchestration layer and service-style use cases.
- `src/domain/`: serializable domain models, statuses, reports, and retrieval envelopes.
- `src/indexing/`: deterministic indexing pipeline internals.
- `src/parsing/`: tree-sitter parsing and symbol extraction.
- `src/protocol/`: MCP protocol adapter.
- `src/storage/`: byte store, control-plane abstraction, and persistence.
- `src/config.rs`: environment-driven configuration.
- `src/error.rs`: shared error type and systemic/non-systemic classification.
- `src/observability.rs`: tracing bootstrap.

`src/application/` is the runtime coordination hot zone:
- `src/application/mod.rs`: `ApplicationContext` facade and cross-service wiring.
- `src/application/run_manager.rs`: run lifecycle, cancellation, checkpointing, resume, startup recovery, idempotency, and run reporting.
- `src/application/search.rs`: request gating and all retrieval/query behavior.
- `src/application/init.rs`: workspace/repository registration and active-context resolution.
- `src/application/deployment.rs`: deployment/bootstrap readiness checks.
- `src/application/health.rs`: aggregated health reporting.

`src/domain/` is noun-centric and should stay transport-agnostic:
- `src/domain/index.rs`: indexing records and enums such as `IndexRunMode`, `IndexRunStatus`, `FileRecord`, `Checkpoint`, `DiscoveryManifest`, and `RunStatusReport`.
- `src/domain/retrieval.rs`: retrieval DTOs such as `ResultEnvelope`, `SearchResultItem`, `FileOutlineResponse`, `RepoOutlineResponse`, and verified source responses.
- `src/domain/health.rs`: `HealthReport`, `DeploymentReport`, and readiness semantics.
- `src/domain/repository.rs`: repository status plus invalidation/quarantine fields.
- `src/domain/init.rs`, `src/domain/context.rs`, `src/domain/workspace.rs`: initialization and workspace state.
- `src/domain/idempotency.rs`: idempotency record/status definitions.

`src/storage/` is split between exact bytes and control metadata:
- `src/storage/blob.rs`: `BlobStore` trait and `StoredBlob`.
- `src/storage/local_cas.rs`: current byte-exact CAS implementation.
- `src/storage/control_plane.rs`: `ControlPlane` trait plus `InMemory`, `LocalRegistry`, and partial `SpacetimeDb` backends.
- `src/storage/registry_persistence.rs`: file-backed registry durability, locking, integrity checks, and atomic replace.
- `src/storage/sha256.rs`: shared hashing helpers.

`src/indexing/` owns mutation mechanics:
- `src/indexing/discovery.rs`: repo walk and supported-language filtering.
- `src/indexing/pipeline.rs`: async orchestration, progress, checkpoint tracker, circuit breaker, resume state.
- `src/indexing/commit.rs`: mapping parse outcomes into persisted file outcomes after CAS writes.

`src/parsing/` owns language-specific extraction:
- `src/parsing/mod.rs`: parse orchestration and panic containment.
- `src/parsing/languages/rust.rs`
- `src/parsing/languages/python.rs`
- `src/parsing/languages/javascript.rs`
- `src/parsing/languages/typescript.rs`
- `src/parsing/languages/go.rs`
- `src/parsing/languages/java.rs`

`src/protocol/` is currently a single-file adapter:
- `src/protocol/mcp.rs`: all current tool definitions, resource listing, URI parsing, and MCP error conversion.
- If the MCP surface grows further, this file is the first candidate to split by concern.

Test layout is capability-oriented rather than unit-test-only:
- `tests/indexing_integration.rs`: end-to-end mutation and recovery scenarios.
- `tests/retrieval_integration.rs`: end-to-end retrieval and request-gating behavior.
- `tests/retrieval_conformance.rs`: enum/result-shape coverage and protocol contracts.
- `tests/epic4_hardening.rs`: MCP server handler and hardening regressions.
- `tests/tree_sitter_grammars.rs`: grammar loading smoke tests.

Naming patterns already in use:
- `mod.rs` files are layer entry points.
- Service files are named by use case, not framework role: `run_manager.rs`, `search.rs`, `deployment.rs`, `init.rs`.
- Domain files are mostly noun-based and serialize cleanly across storage/protocol boundaries.
- Persistence helpers use `save_*`, `find_*`, `get_*`, `update_*`, and `cancel_*`.
- Recovery paths use explicit verbs like `resume_*`, `checkpoint_*`, `startup_*`, and `invalidate_*`.
- Integration tests are grouped by behavior area, not by source file.

Important files to open first when working in this repo:
- `src/main.rs`
- `src/lib.rs`
- `src/application/mod.rs`
- `src/application/run_manager.rs`
- `src/application/search.rs`
- `src/storage/control_plane.rs`
- `src/storage/registry_persistence.rs`
- `src/indexing/pipeline.rs`
- `src/domain/index.rs`
- `src/protocol/mcp.rs`

Future work should probably land here:
- New MCP tools/resources/prompts: `src/protocol/`
- New runtime workflows such as repair/audit/orchestration: `src/application/`
- New shared records or statuses: `src/domain/`
- SpacetimeDB-backed authoritative persistence: `src/storage/control_plane.rs` and likely new siblings under `src/storage/`, with related assets under `spacetime/`
- CAS hardening or integrity checks: `src/storage/local_cas.rs` and `src/indexing/commit.rs`
- New language onboarding: `src/parsing/languages/` plus the extension/support mapping in `src/domain/index.rs`
- More black-box regressions: `tests/indexing_integration.rs`, `tests/retrieval_integration.rs`, and `tests/epic4_hardening.rs`
