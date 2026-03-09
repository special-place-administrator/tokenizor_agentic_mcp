# Stack

- Primary implementation language: Rust 2024 edition from `Cargo.toml`.
- Package shape: single Cargo crate with library exports in `src/lib.rs` and the executable entrypoint in `src/main.rs`.
- Runtime model: async Tokio runtime via `tokio` with multi-threaded execution and `tokio-util` cancellation support.
- MCP framework: `rmcp` 1.1.0 with stdio transport, wired in `src/main.rs` and implemented in `src/protocol/mcp.rs`.
- Parsing stack: `tree-sitter` plus language grammars for Rust, Python, JavaScript, TypeScript, Go, and Java from `src/parsing/mod.rs` and `src/parsing/languages/`.
- Serialization and schemas: `serde`, `serde_json`, and `schemars`, used heavily in `src/domain/index.rs`, `src/domain/retrieval.rs`, and protocol responses.
- Error boundary: `thiserror` for domain/application errors in `src/error.rs`, with `anyhow` used at the CLI boundary in `src/main.rs`.
- Observability: `tracing` and `tracing-subscriber` in `src/observability.rs`; log filtering comes from standard tracing env configuration and defaults to `info`.
- Filesystem traversal: `ignore::WalkBuilder` in `src/indexing/discovery.rs`.
- Local persistence safety: `fs2::FileExt` lock usage in `src/storage/registry_persistence.rs`.
- Bounded parallelism: indexing concurrency is capped from CPU count in `src/indexing/pipeline.rs`.

## Crate Layout

- `src/application/` is the orchestration layer for startup, bootstrap/init flows, indexing lifecycle, retrieval, and health reporting.
- `src/domain/` holds durable domain types for runs, repositories, health, retrieval, idempotency, migration, and workspace identity.
- `src/storage/` holds the storage abstractions and implementations: blob CAS, control-plane adapters, registry persistence, and hashing helpers.
- `src/indexing/` holds file discovery, parsing commit, and pipeline execution.
- `src/parsing/` holds tree-sitter language dispatch and symbol extraction logic.
- `src/protocol/` exposes the MCP server surface.
- `tests/` contains integration, conformance, grammar, and hardening coverage.

## Key Modules

- `src/application/mod.rs` builds `ApplicationContext`, wires storage backends, and runs the startup recovery sweep.
- `src/application/run_manager.rs` owns run lifecycle, idempotency handling, checkpoints, cooperative cancellation, and startup cleanup/recovery.
- `src/application/search.rs` implements verified retrieval and search result gating on top of persisted metadata plus CAS-backed file bytes.
- `src/application/init.rs` owns `init`, `attach`, `migrate`, `inspect`, and `resolve` style bootstrap flows and registry schema migration.
- `src/storage/local_cas.rs` implements byte-exact local CAS rooted at `TOKENIZOR_BLOB_ROOT` / `.tokenizor`.
- `src/storage/registry_persistence.rs` persists registry/control-plane data to `control-plane/project-workspace-registry.json` under the blob root.
- `src/storage/control_plane.rs` defines the `ControlPlane` trait and the `InMemory`, `LocalRegistry`, and `SpacetimeDb` backend adapters.
- `src/protocol/mcp.rs` is the tool/resource surface for indexing, retrieval, run inspection, cancellation, checkpoints, and invalidation.

## Storage Choices

- Default blob root is `.tokenizor`, configured in `src/config.rs`.
- CAS layout is created by `src/storage/local_cas.rs` under `blobs/sha256/`, `temp/`, `quarantine/`, and `derived/`.
- Repository/workspace registry and current local control-plane persistence live at `control-plane/project-workspace-registry.json`, assembled in `src/application/mod.rs`.
- Raw blob ids are SHA-256 addressed and sharded by digest prefix in `src/storage/local_cas.rs`.
- Registry persistence uses atomic replace and lock files in `src/storage/registry_persistence.rs`.
- The architecture direction still treats SpacetimeDB as the intended authoritative control plane, while the local registry remains the currently wired durable path.

## Runtime And Operator Commands

- Standard build entry is `cargo build` because the repo is a single Cargo package.
- Core test command is `cargo test`, documented in `README.md` and backed by `tests/`.
- MCP server startup is `cargo run -- run`.
- Readiness/deployment diagnostics are `cargo run -- doctor`.
- Bootstrap/operator flows are `cargo run -- init`, `cargo run -- attach`, `cargo run -- migrate`, `cargo run -- inspect`, and `cargo run -- resolve`.

## Config Surface

- `src/config.rs` loads `TOKENIZOR_BLOB_ROOT`.
- `src/config.rs` loads `TOKENIZOR_CONTROL_PLANE_BACKEND`.
- `src/config.rs` loads `TOKENIZOR_SPACETIMEDB_CLI`.
- `src/config.rs` loads `TOKENIZOR_SPACETIMEDB_ENDPOINT`.
- `src/config.rs` loads `TOKENIZOR_SPACETIMEDB_DATABASE`.
- `src/config.rs` loads `TOKENIZOR_SPACETIMEDB_MODULE_PATH`.
- `src/config.rs` loads `TOKENIZOR_SPACETIMEDB_SCHEMA_VERSION`.
- `src/config.rs` loads `TOKENIZOR_REQUIRE_READY_CONTROL_PLANE`.

## Test And Fixture Coverage

- Integration coverage lives in `tests/indexing_integration.rs`, `tests/retrieval_integration.rs`, and `tests/epic4_hardening.rs`.
- Retrieval contract coverage lives in `tests/retrieval_conformance.rs`.
- Grammar sanity checks live in `tests/tree_sitter_grammars.rs`.
- Backward-compat registry fixtures live in `tests/fixtures/epic1-registry.json` and `tests/fixtures/epic2-registry.json`.
