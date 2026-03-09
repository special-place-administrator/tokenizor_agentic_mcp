# Integrations

- Primary interface is MCP over stdio, started from `src/main.rs` using `rmcp` transport I/O.
- The concrete server surface lives in `src/protocol/mcp.rs` under `TokenizorServer`.
- Implemented tool handlers in `src/protocol/mcp.rs` cover health, indexing, run inspection, cancellation, checkpointing, repository invalidation, search, and verified symbol/file retrieval.
- Implemented MCP resources currently expose recent run status documents as `tokenizor://runs/{run_id}/status`, generated in `src/protocol/mcp.rs`.
- There is no prompt interface implemented yet; searches for prompt handlers return no code hits, so prompts remain planned rather than wired.

## External Services

- SpacetimeDB is the only explicit external service integration today.
- SpacetimeDB settings are defined in `src/config.rs` with defaults for CLI path, endpoint `http://127.0.0.1:3007`, database `tokenizor`, module path `spacetime/tokenizor`, and schema version `2`.
- `src/storage/control_plane.rs` performs readiness checks against the SpacetimeDB CLI binary, HTTP endpoint reachability, configured database name, module path existence, and schema-version compatibility.
- `spacetime/tokenizor/README.md` shows the SpacetimeDB module path is currently a scaffold; schema/module deployment is not yet wired into the Rust runtime.
- The `SpacetimeControlPlane` type exists in `src/storage/control_plane.rs`, but mutable run-state persistence is still intentionally incomplete there and the local registry path remains the practical durable implementation.

## Control-Plane Boundaries

- `src/storage/control_plane.rs` defines the `ControlPlane` trait as the abstraction for repositories, runs, checkpoints, file metadata, discovery manifests, and idempotency records.
- `build_control_plane` in `src/storage/control_plane.rs` selects among `InMemory`, `LocalRegistry`, and `SpacetimeDb` backends from `TOKENIZOR_CONTROL_PLANE_BACKEND`.
- `InMemoryControlPlane` is scaffolding/test-only behavior.
- `RegistryBackedControlPlane` delegates to `src/storage/registry_persistence.rs` and is the currently wired durable local control plane.
- `SpacetimeControlPlane` is mainly a deployment/readiness integration point right now, not the fully authoritative mutable store yet.

## Local Storage And Retrieval Boundaries

- `src/application/mod.rs` roots local state under `TOKENIZOR_BLOB_ROOT`, defaulting to `.tokenizor`.
- `src/storage/local_cas.rs` keeps raw bytes in a local CAS under `blobs/sha256/`, plus `temp/`, `quarantine/`, and `derived/`.
- `src/storage/registry_persistence.rs` writes control-plane and bootstrap metadata to `control-plane/project-workspace-registry.json` under the same root.
- `src/application/search.rs` joins registry metadata and CAS blobs for trusted retrieval, rather than serving metadata-only matches.
- `src/storage/local_cas.rs` validates blob ids as SHA-256 digests and preserves byte-exact content, including CRLF and NUL cases covered by tests.

## Env And Operator Integration Points

- All product-specific runtime configuration is loaded from environment in `src/config.rs`.
- `TOKENIZOR_REQUIRE_READY_CONTROL_PLANE` gates whether degraded control-plane readiness blocks serving.
- `src/main.rs` exposes operator entrypoints through `doctor`, `run`, `init`, `attach`, `migrate`, `inspect`, and `resolve`.
- `README.md` documents those commands as the local workflow expected by operators.
- `src/observability.rs` also integrates with the standard tracing environment filter, defaulting to `info` if no env filter is present.

## API And Protocol Notes

- MCP tool arguments are plain JSON objects parsed in `src/protocol/mcp.rs`.
- Batch retrieval supports explicit symbol/code-slice targets in `src/protocol/mcp.rs`, which is the closest thing to an API payload contract in the current codebase.
- Resource reads return JSON text for run status reports from `src/protocol/mcp.rs`.
- There is no HTTP API server, webhook receiver, OAuth flow, API key store, or browser-session auth layer in the current repo.
- The only network-style dependency today is outbound reachability probing of the configured SpacetimeDB HTTP endpoint.

## Identity And Idempotency Integration

- Repository/workspace identity is derived and reconciled in `src/application/init.rs`.
- Run idempotency records are persisted through `src/domain/idempotency.rs`, `src/application/run_manager.rs`, `src/storage/control_plane.rs`, and `src/storage/registry_persistence.rs`.
- Current idempotency key spaces include index, reindex, and invalidate flows in `src/application/run_manager.rs`.
- Checkpoints and recovery state are integrated through `src/application/run_manager.rs` plus `src/storage/registry_persistence.rs`.

## Practical Status

- Current production-facing integration story is local-first: stdio MCP + local CAS + local registry persistence.
- Current future-facing integration story is hybrid: keep raw bytes local and move authoritative control-plane state into SpacetimeDB once the mutable backend is finished.
