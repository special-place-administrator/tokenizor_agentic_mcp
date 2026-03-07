# Architecture

Status: First pass  
Date: 2026-03-06

## Purpose

`tokenizor_agentic_mcp` is a Rust-native, coding-first MCP server for repository indexing, retrieval, orchestration, and repair.

It is not a carbon copy of `jcodemunch-mcp`. The design should optimize for:
- speed
- robustness
- idempotency
- deterministic behavior
- self-healing and self-recovery
- byte-exact source retrieval
- strong edge-case handling for real codebases

## Design Position

The system should use a hybrid architecture:
- Rust application and MCP surface
- SpacetimeDB as the authoritative control plane
- local byte-exact content-addressed storage for raw file bytes and large artifacts
- tree-sitter parsing and extraction in Rust

This split is deliberate.

SpacetimeDB is a strong fit for durable operational state, subscriptions, reducers, and scheduled recovery work. It is not the best default place for every raw source blob. Raw bytes need exact handling because symbol spans and later retrieval must remain correct on every platform, including Windows.

## Goals

Primary goals:
- index local folders and Git repositories reliably
- support incremental and full indexing
- provide stable symbol identities
- provide verified source retrieval by byte span
- recover from crashes and partial failures
- prevent duplicate mutation side effects via idempotency
- expose enough operational state for debugging and repair

Secondary goals:
- live progress visibility
- strong observability
- extensible memory model for future agent workflows
- eventual semantic recall support without distorting the core design

## Non-Goals

Not first-phase goals:
- exact parity with Python MCPs
- storing all repository bytes inside the database
- embedding-first retrieval
- hiding failure states behind blind retries

## High-Level Architecture

Core subsystems:
- `protocol`
  - MCP tool/resource/prompt surface
  - request validation
  - response shaping
- `application`
  - orchestration services
  - use cases
  - job lifecycle
- `domain`
  - repository, file, symbol, run, checkpoint, lease, idempotency, health models
  - core invariants
- `storage`
  - SpacetimeDB integration
  - local content-addressed blob store
  - snapshots and integrity verification
- `indexing`
  - discovery
  - filtering
  - hashing
  - parsing
  - extraction
  - validation
  - commit pipeline
- `parsing`
  - tree-sitter language bindings
  - language-specific extraction
- `observability`
  - logging
  - metrics
  - traces
  - health reports

## Data Ownership

### SpacetimeDB: authoritative control plane

SpacetimeDB should own structured operational state:
- repository registration
- index runs
- checkpoints
- leases
- health events
- repair jobs
- idempotency records
- file metadata
- symbol metadata
- operational history

This gives us:
- transactional mutation boundaries
- resumable runs
- clearer concurrency control
- durable replay after failure
- subscriptions for live run status and health

### Local CAS: authoritative raw content plane

Raw file bytes should live in a local content-addressed store.

Why:
- symbol retrieval depends on exact bytes
- newline or encoding normalization can corrupt spans
- large blobs are better handled outside the DB
- integrity verification is simpler with direct hash-addressed blobs

Suggested layout:

```text
.tokenizor/
  blobs/
    sha256/
      ab/
        cd/
          <fullhash>
  temp/
  quarantine/
  derived/
```

Raw content rules:
- store bytes exactly as read
- never normalize line endings
- never decode and re-encode before persistence
- compute spans against exact stored bytes
- write via temp file then atomic rename

## Stable IDs and Retrieval

Stable symbol ID format:

```text
{file_path}::{qualified_name}#{kind}
```

Symbol ID stability comes from:
- path
- qualified name
- symbol kind

Source retrieval does not come from the symbol ID alone. It comes from:
- blob hash
- byte start
- byte length
- verification hash

This distinction is critical. A stable ID can still point to bad bytes if storage is sloppy. This project must prevent that class of error by design.

## Core Domain Model

### Repository

Tracks a logical codebase.

Key fields:
- `repo_id`
- `kind` local or git
- `root_uri`
- `default_branch`
- `last_known_revision`
- `status`

### IndexRun

Tracks one full, incremental, repair, or verification pass.

Key fields:
- `run_id`
- `repo_id`
- `mode`
- `status`
- `requested_at`
- `started_at`
- `finished_at`
- `idempotency_key`
- `request_hash`
- `checkpoint_cursor`
- `error_summary`

### FileRecord

Tracks indexed file metadata.

Key fields:
- `repo_id`
- `path`
- `content_hash`
- `blob_id`
- `language`
- `size_bytes`
- `deleted`
- `last_indexed_run_id`

### SymbolRecord

Tracks extracted symbol metadata.

Key fields:
- `symbol_id`
- `repo_id`
- `file_path`
- `qualified_name`
- `kind`
- `language`
- `span_start_byte`
- `span_len_bytes`
- `content_hash`
- `signature`
- `summary`

### Lease

Tracks ownership of active background work.

Key fields:
- `lease_id`
- `run_id`
- `worker_id`
- `expires_at`
- `state`

### Checkpoint

Supports resumable work.

Key fields:
- `run_id`
- `cursor`
- `files_processed`
- `symbols_written`
- `created_at`

### IdempotencyRecord

Prevents duplicate mutation side effects.

Key fields:
- `operation`
- `idempotency_key`
- `request_hash`
- `status`
- `result_ref`
- `created_at`
- `expires_at`

### HealthEvent

Captures system degradation and repairs.

Key fields:
- `component`
- `severity`
- `message`
- `details`
- `occurred_at`

## Indexing Pipeline

The indexing pipeline should be event-driven and checkpointed.

Proposed flow:
1. Discover candidate files.
2. Apply ignore and security filtering.
3. Read exact raw bytes.
4. Compute content hash.
5. Persist raw bytes into CAS if absent.
6. Detect language.
7. Parse with tree-sitter.
8. Extract symbols and spans.
9. Verify spans against exact bytes.
10. Commit metadata transactionally.
11. Emit progress and checkpoint updates.

Properties:
- resumable
- replayable
- bounded
- backpressured
- incremental when possible
- safe under crash or partial failure

## Idempotency

Mutating operations must be idempotent.

Applies to:
- `index_folder`
- `index_repository`
- `repair_index`
- `checkpoint_now`
- future mutation tools

Model:
1. Client sends `idempotency_key`.
2. Server canonicalizes request args and computes `request_hash`.
3. First execution stores a pending idempotency record.
4. Success stores a durable result reference.
5. Retry with same key and same hash returns stored result.
6. Retry with same key and different hash fails deterministically.

This is required because MCP clients may retry after timeout, cancellation, or transport interruption.

## Recovery and Self-Healing

Self-healing should mean explicit deterministic repair.

Mechanisms:
- startup sweep for stale leases and temp files
- resume from latest checkpoint
- quarantine malformed parses or invalid spans
- scheduled repair tasks
- periodic integrity verification
- health scoring for repos and runs
- explicit repair tools

Failure handling policy:
- process crash
  - recover active runs from DB state and checkpoints
- parser failure on one file
  - quarantine file, continue run, emit health event
- missing blob
  - mark records degraded, schedule repair
- bad span verification
  - quarantine symbol rows, reparse affected file
- lease expiry
  - stop stale worker from committing, resume safely elsewhere

## Concurrency Model

Use Tokio with structured concurrency.

Recommended execution pools:
- discovery workers
- read/hash workers
- parser workers
- commit workers

Recommended control primitives:
- cancellation token per run
- bounded channels between stages
- task tracker for shutdown

Rules:
- CPU-heavy parsing must not block MCP responsiveness
- logically conflicting commits should serialize cleanly
- long-running work should return durable run IDs
- progress should be available through structured state, not logs alone

## MCP Surface

This project should support all three major MCP surfaces over time:
- tools
- resources
- prompts

### Initial tools

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

### Useful resources

- repository outline
- repository health
- run status
- symbol metadata
- failure diagnostics

### Useful prompts

- codebase audit
- architecture map
- broken-symbol diagnosis
- index failure triage

## Memory Strategy

The project should eventually support multiple memory layers.

### Authoritative memory

Use SpacetimeDB for:
- architecture decisions
- run history
- checkpoints
- idempotency records
- repair history
- tool outcomes

### Code memory

Use structured metadata for:
- files
- symbols
- outlines
- hashes
- repository status

### Semantic memory

Optional later layer:
- embeddings for fuzzy retrieval over docs, notes, conversations, and maybe chunks

Current position:
- SpacetimeDB is not a dedicated vector database
- do not contort phase one around ANN requirements
- add semantic retrieval later if it proves useful

## Observability

This system should be debuggable in production-like conditions.

Required:
- structured logging
- counters and timing metrics
- traces for run lifecycle and indexing stages
- health summaries
- explicit degraded-state reporting

Observability questions should be answerable quickly:
- what is running
- what failed
- what was retried
- what was quarantined
- what can be resumed

## Security and Safety

Needed from the start:
- path traversal protection
- symlink policy
- binary detection
- secret and unsafe path exclusion
- repository root confinement
- bounded resource usage

Safety principle:
- if integrity is uncertain, degrade safely and report it
- never fabricate confidence about retrieved code slices

## Recommended Near-Term Milestones

### M1 Foundation

- clean crate/module layout
- error model
- config model
- health tool
- logging and tracing bootstrap
- storage traits for DB and CAS

### M2 Durable control plane

- SpacetimeDB boundary crate or integration module
- repository and run domain types
- idempotency model
- checkpoint model
- lease model

### M3 Byte-exact storage and indexing skeleton

- local CAS implementation
- discovery and filtering pipeline
- hashing
- run orchestration
- checkpoint writes

### M4 Parsing and retrieval

- tree-sitter integration
- language support starting with Rust, Python, TS/JS
- stable symbol IDs
- verified `get_symbol`

### M5 Repair and live operations

- repair workflows
- health resources
- progress subscriptions
- integrity sweeps

## Current Strategic Decision

Adopt SpacetimeDB as the control plane, not the universal storage layer.

That gives us:
- strong operational state
- better recovery semantics
- a durable memory model for the project
- room for future agentic workflows

And it avoids:
- blob misuse
- byte corruption risk
- turning a strong coordination store into a poor raw-content store

## Final Recommendation

Build Tokenizor as a Rust-first MCP platform with:
- SpacetimeDB for durable structured state
- local byte-exact CAS for raw content
- tree-sitter extraction with verification before commit
- checkpointed idempotent jobs
- explicit recovery and repair tooling

That is the right base for a genuinely better coding MCP.
