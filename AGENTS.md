# AGENTS.md

This repository is `tokenizor_agentic_mcp`.

It is a Rust-native, coding-first MCP project for code indexing, retrieval, and recovery.

## Mission

Build a world-class MCP for code indexing, retrieval, orchestration, and recovery.

Primary qualities:
- speed
- robustness
- idempotency
- deterministic behavior
- self-healing and self-recovery
- strong edge-case handling
- coding-first ergonomics

## Core Architecture Direction

Use a hybrid architecture:
- Rust MCP server for the protocol surface
- SpacetimeDB as the authoritative control plane
- local byte-exact content-addressed blob storage for raw file bytes and large derived artifacts
- tree-sitter-based parsing and symbol extraction in Rust

SpacetimeDB is for:
- repositories
- index runs
- checkpoints
- leases
- health
- repair actions
- idempotency records
- symbol and file metadata
- operational history
- live progress and subscriptions

Do not force every raw source blob into SpacetimeDB by default.

Reason:
- raw file handling must be byte exact
- symbol spans depend on exact bytes
- large blobs are better handled by a local CAS

## Product Principles

- Coding-first beats generic document-first behavior.
- Determinism beats convenience.
- Explicit recovery beats hidden retry magic.
- Corruption should be quarantined, not silently served.
- Long-running operations must be resumable.
- Mutating operations must support idempotency.
- Shutdown is not a safe persistence boundary.

## Storage Principles

Use SpacetimeDB as the control plane, not the universal storage substrate.

Recommended split:
- SpacetimeDB:
  - repo metadata
  - file metadata
  - symbol metadata
  - index runs
  - job state
  - idempotency keys
  - checkpoints
  - health events
  - repair history
- Local CAS:
  - raw file bytes
  - large derived artifacts
  - anything where exact bytes matter for later retrieval

Raw file rules:
- write bytes exactly as read
- never normalize line endings
- never decode and re-encode for storage
- verify source slices against stored hashes

## Idempotency Rules

Mutating tools must accept an `idempotency_key` when appropriate.

Required behavior:
- normalize request arguments into a canonical hash
- first execution stores `idempotency_key + request_hash + status`
- replay with same key and same hash returns the stored result
- replay with same key and different hash fails deterministically

Likely idempotent tools:
- `index_folder`
- `index_repository`
- `repair_index`
- `checkpoint_now`
- future write or annotation tools

## Recovery Rules

Self-healing means deterministic repair paths.

The system should support:
- startup sweeps for stale leases and temp files
- checkpoint replay for interrupted runs
- quarantine of bad parses or bad spans
- scheduled repair jobs
- integrity verification
- explicit health and repair tools

Failure should degrade safely:
- process crashes should be resumable
- parser failures should isolate a file, not poison a run
- bad symbol spans should never be served silently

## MCP Surface

This project should eventually support:
- tools
- resources
- prompts

Do not design for tools only.

Likely foundation tools:
- `health`
- `index_folder`
- `index_repository`
- `get_index_run`
- `cancel_index_run`
- `checkpoint_now`
- `repair_index`
- `search_symbols`
- `search_text`
- `get_file_outline`
- `get_symbol`
- `get_symbols`
- `get_repo_outline`
- `invalidate_cache`

Likely useful resources:
- repository outline
- repository health
- run status
- symbol metadata views

Likely useful prompts:
- codebase audit
- architecture map
- failure triage
- index repair diagnosis

## Memory Strategy

Project memory should be layered:
- authoritative memory:
  - architecture decisions
  - run history
  - checkpoints
  - health and repair history
- code memory:
  - file metadata
  - symbol metadata
  - outlines
  - hashes
- semantic memory:
  - optional embeddings for fuzzy recall over docs, notes, and conversations

SpacetimeDB is not a purpose-built vector database.

Use it confidently for authoritative and structured memory.
If semantic search becomes important:
- start simple
- embeddings may be stored there for small-scale use
- add a dedicated ANN/vector sidecar only if scale or latency requires it

## Current Known Context

As of 2026-03-06:
- this repo was freshly created and bootstrapped as a Rust project
- there is an `rmcp`-based stdio server scaffold
- an earlier Python prototype found a real Windows byte-offset bug caused by newline translation during raw cache writes
- that bug is a design warning: byte-exact storage is non-negotiable

## Implementation Guidance

- Prefer clean module boundaries:
  - `protocol`
  - `application`
  - `domain`
  - `storage`
  - `indexing`
  - `parsing`
  - `observability`
- Keep domain logic testable without MCP or database runtime dependencies.
- Prefer bounded concurrency and structured shutdown.
- Long-running operations should return durable run ids when appropriate.
- Use Rust everywhere possible.
- If Python tooling is ever needed, use `uv`, not `pip`.

## Working Style

- Be pragmatic, direct, and engineering-focused.
- Avoid unnecessary boilerplate.
- Prefer implementing over theorizing once direction is clear.
- Preserve backward compatibility only when it serves the product.
- This project is ours now; optimize for the best end state, not legacy imitation.

## Tooling Preference

When Tokenizor MCP is available, prefer its tools for repository and code inspection before falling back to direct file reads.

Use Tokenizor first for:
- symbol discovery
- text/code search
- file outlines
- repository outlines
- targeted symbol/source retrieval
- inspection of implementation code under `src/`, `tests/`, and similar code-bearing directories

Preferred tools:
- `search_text`
- `search_symbols`
- `get_file_outline`
- `get_repo_outline`
- `get_symbol`
- `get_symbols`

Default rule:
- use Tokenizor to narrow and target code inspection first
- use direct file reads only when exact full-file source or surrounding context is still required after tool-based narrowing

Direct file reads are still appropriate for:
- exact document text in `docs/` or planning artifacts when literal wording matters
- configuration files where exact raw contents are the point of inspection

Do not default to broad raw file reads for source-code inspection when Tokenizor can answer the question more directly.
