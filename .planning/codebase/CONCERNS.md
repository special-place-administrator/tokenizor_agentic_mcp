# Concerns

This document calls out the main technical debt, likely regression zones, recovery gaps, and operational unknowns in the current codebase.

## Highest-Risk Gaps

- `src/storage/control_plane.rs`: `ControlPlaneBackend::SpacetimeDb` is selectable, but the `SpacetimeControlPlane` only wires health and deployment probes. Real CRUD methods like `find_run`, `save_run`, `save_checkpoint`, `save_repository`, and `save_idempotency_record` return a pending-operation error. Any deployment that flips to Spacetime will clear some health checks and still fail on first real mutation or lookup.
- `src/storage/control_plane.rs` and `src/config.rs`: this is a product-direction mismatch, not just missing code. The repo mission says SpacetimeDB should be authoritative, but the runtime default is still `LocalRegistry`, and the Spacetime path is not feature-complete.
- `src/application/run_manager.rs` and `src/storage/registry_persistence.rs`: active-run exclusion is check-then-save, not an atomic repo-level lease. `start_run` and `reindex_repository` check in-memory state and persisted runs before calling `save_run`, while registry persistence only upserts by `run_id`. Two processes can race and create concurrent queued runs for the same repo.
- `src/application/search.rs`, `src/indexing/pipeline.rs`, and `src/indexing/commit.rs`: a run can finish with status `Succeeded` even when files failed CAS persistence or later blob reads are skipped. Search then reports `Verified` repo-level results while silently omitting damaged files. "Succeeded" currently means "pipeline reached the end" more than "the repo is fully queryable."
- `src/application/search.rs`: `search_text_ungated` silently skips files on blob read failure, blob hash mismatch, or non-UTF-8 content and still returns `Empty` or `Success` with `TrustLevel::Verified`. Empty results can therefore mean "no match" or "coverage was degraded."
- `src/application/search.rs`: `search_symbols_ungated` at least tracks coverage counters, but the top-level trust still stays `Verified` even when files are skipped or failed. That can overstate confidence to downstream agents.
- `src/parsing/mod.rs` and `src/application/search.rs`: parsing uses `String::from_utf8_lossy(bytes)` before tree-sitter, but retrieval later blocks when symbol or code-slice bytes are non-UTF-8. That creates a fidelity gap where metadata may be produced from lossy-decoded content that the retrieval layer refuses to serve.
- `src/application/run_manager.rs`, `src/application/search.rs`, and `src/domain/retrieval.rs`: `NextAction::Repair` is emitted often, but there is no dedicated `repair_index` MCP tool. `repair` exists only as a mode string on `index_folder`, and repo-wide search does not show meaningful mode-specific execution logic for `repair`, `verify`, or `incremental`.

## Recovery And Persistence Fragility

- `src/storage/registry_persistence.rs`: `verify_integrity` only rejects `schema_version == 0` when data exists. It does not validate cross-record integrity between runs, checkpoints, discovery manifests, idempotency records, repositories, and workspaces. Corruption is likely to surface late and feature-by-feature.
- `src/application/init.rs` and `src/storage/registry_persistence.rs`: there are two different registry persistence stacks, including duplicated Windows `MoveFileExW` atomic-replace code and different lock strategies. That is risky in exactly the platform-sensitive area where the project already knows byte-level correctness matters.
- `src/application/init.rs`: the bootstrap registry lock is a custom sentinel-file loop with a 5 second timeout and stale-file heuristics. `src/storage/registry_persistence.rs` uses `fs2` exclusive file locking instead. Divergent locking semantics increase the chance of one path fixing a Windows edge case while the other keeps it.
- `src/application/run_manager.rs`: periodic checkpointing is hard-coded to every 100 processed files in `spawn_pipeline_for_run_with_resume`. Large repos can still lose a meaningful chunk of progress after a crash, especially if durable record persistence is slow.
- `src/application/run_manager.rs`: resumed runs restore `files_processed` and `symbols_extracted` but reset `files_failed` to `0` in `PipelineResumeState`. Health and progress reporting after resume can under-report prior failures.
- `src/application/run_manager.rs`: startup sweep only reconciles persisted `Queued` and `Running` runs plus owned temp artifacts. Other bad states, such as orphaned idempotency history or semantically corrupt manifests that still deserialize, are pushed to later code paths instead of getting a first-class repair pass.

## API, Security, And Operational Unknowns

- `src/protocol/mcp.rs`: write-oriented tools like `index_folder`, `resume_index_run`, and `reindex_repository` do not use the stricter `required_non_empty_string_param` helper used by read/search tools. Empty-string validation is inconsistent, and `repo_root` is described as absolute but not enforced as absolute.
- `src/protocol/mcp.rs` and `src/application/mod.rs`: `repo_id` and `repo_root` are fully caller-supplied and not bound to each other. A client can accidentally reuse a `repo_id` for a different path or point an existing `repo_id` at the wrong tree, which risks mixed history and confusing retrieval behavior.
- `src/protocol/mcp.rs` and `src/indexing/discovery.rs`: the server trusts the caller's chosen `repo_root` and recursively walks readable files from there. That is acceptable for a trusted local tool, but it becomes a real filesystem exposure boundary if this server is ever bridged remotely or used in a multi-tenant setting.
- `src/protocol/mcp.rs`: the MCP surface only exposes tools plus a narrow run-status resource view. There is no prompt surface, and repository-health / repository-outline resources described in the project direction are not implemented yet.
- `src/application/run_manager.rs`: `list_recent_run_ids` swallows control-plane errors with `unwrap_or_default`, so MCP resource listing can degrade to "no resources" instead of surfacing storage failure clearly.

## Regression Pressure

- `src/application/run_manager.rs`: this file is a major hotspot with lifecycle orchestration, idempotency, reindexing, resume, cancellation, startup recovery, reporting, and a large embedded test block in one place. It is the most likely future regression zone.
- `src/application/search.rs`: this file mixes request gating, repo/file/symbol retrieval, batch APIs, trust modeling, and many edge-case tests. It is doing too many jobs, which makes trust-model changes especially risky.
- `src/application/init.rs`: initialization, registry migration, path identity resolution, workspace attachment, file locking, and Windows atomic writes all live together. Any migration or path-resolution change has a wide blast radius.
- `src/protocol/mcp.rs`: the public API currently over-promises relative to implementation in a few places, especially around mode semantics and future MCP surface shape. That is manageable now, but it will become a support burden if clients start depending on documented behavior that is only partially real.
