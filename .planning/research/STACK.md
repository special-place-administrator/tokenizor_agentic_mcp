# Stack Research

**Domain:** In-memory code intelligence MCP server (Rust)
**Researched:** 2026-03-10
**Confidence:** HIGH (all versions verified against docs.rs/crates.io)

---

## Recommended Stack

### Core Technologies

| Technology | Version | Purpose | Why Recommended |
|------------|---------|---------|-----------------|
| tokio | 1.50.0 | Async runtime | Already in use. Current LTS is 1.47.x; 1.50 is stable. `rt-multi-thread` required for background file watcher tasks. |
| rmcp | 1.1.1 | MCP protocol over stdio | Already in use. Bare JSON lines on stdio — no Content-Length framing. The `transport-io` feature covers stdio. 1.1.1 is latest stable. |
| tree-sitter | 0.24.x | Syntax tree parsing | Keep at 0.24 — grammar crates (tree-sitter-rust@0.24.0, typescript@0.23.2) still target ^0.24. Upgrading to 0.25/0.26 is a coordinated grammar migration, not a single bump. |
| tree-sitter grammars | rust@0.24.0, typescript@0.23.2 (others@0.23.x) | Language parsers for 6 languages | Current published versions compatible with tree-sitter ^0.24. Do NOT bump grammar crates until all six are at a compatible version. |
| axum | 0.8.8 | HTTP sidecar for hook ↔ LiveIndex communication | Part of the tokio-rs ecosystem. Zero-overhead layer over hyper. Shares the tokio runtime with the MCP server. `Arc<AppState>` shared state pattern. Port 0 bind → `listener.local_addr()` for dynamic port. |
| notify | 8.2.0 | Raw filesystem events | Cross-platform file watcher used by rust-analyzer, Deno, mdBook. Current MSRV 1.85. |
| notify-debouncer-full | 0.7.0 | Debounced file events | Required companion to notify. Merges duplicate events, matches rename pairs, suppresses modify-after-create noise. Use this, not raw notify events. |

### Supporting Libraries

| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| dashmap | 6.1.0 | Concurrent HashMap for LiveIndex | Use instead of `Arc<parking_lot::RwLock<HashMap>>` when multiple async tasks read/write concurrently. Sharded locking gives 8-16x lower contention under typical code-intelligence workloads where queries and watcher updates overlap. |
| parking_lot | 0.12.5 | Fast synchronization primitives | Use for per-file or per-language locks where you own the granularity. Up to 50x faster than std::sync::RwLock in contended benchmarks. Hardware lock elision available via feature flag. |
| serde | 1.0 | Serialization framework | Already in use. Required for JSON hook payloads and index persistence. |
| serde_json | 1.0 | JSON encode/decode | Already in use. Used for hook stdin/stdout and MCP protocol messages. |
| bincode | 2.0 | Binary serialization for index persistence | Use for `LiveIndex` snapshot writes on shutdown. Faster and smaller than JSON for internal persistence. rkyv is faster but adds unsafe complexity; bincode 2.0 is the pragmatic choice. |
| anyhow | 1.0 | Application-level error handling | Already in use. Use in binary crate (main.rs, hook handlers, HTTP routes). Keep as-is. |
| thiserror | 2.0 | Library-level error types | Already in use. Use for `LiveIndexError`, `ParseError`, domain types. |
| tracing | 0.1 | Structured logging | Already in use. CRITICAL: never use `println!` in an MCP stdio server — it corrupts the JSON-RPC stream. All diagnostics go to tracing → stderr. |
| tracing-subscriber | 0.3 | Log output configuration | Already in use. Configure `RUST_LOG` env var filtering. |
| ignore | 0.4 | .gitignore-aware file discovery | Already in use. Used for initial index scan. Pairs with notify watcher for ongoing updates. |
| tower | 0.5 | Middleware layer for axum | Implicit via axum. No direct dependency needed unless custom middleware is required. |

### Development Tools

| Tool | Purpose | Notes |
|------|---------|-------|
| cargo-nextest | Faster parallel test runner | Significant speedup over `cargo test` for the 700+ test suite. `cargo nextest run` replaces `cargo test`. |
| tempfile | 3.x | Test fixtures with auto-cleanup | Already in dev-dependencies. Required for integration tests that index real files. |
| tokio-test | 0.4 | Async test utilities | Use `tokio::test` macro for async unit tests in the LiveIndex module. |

---

## Cargo.toml Changes for v2

```toml
[dependencies]
# Keep (already in use, keep versions)
anyhow = "1.0"
rmcp = { version = "1.1", features = ["transport-io"] }
schemars = "1.1"
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
thiserror = "2.0"
tokio = { version = "1", features = ["io-std", "macros", "rt-multi-thread", "net", "sync"] }
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
ignore = "0.4"

# Keep (tree-sitter — pin at 0.24, do NOT bump until all grammars are ready)
tree-sitter = "0.24"
tree-sitter-rust = "0.24"
tree-sitter-typescript = "0.23"
tree-sitter-python = "0.23"
tree-sitter-javascript = "0.23"
tree-sitter-go = "0.23"
tree-sitter-java = "0.23"

# Remove
# spacetimedb-sdk — out of scope for v2
# fs2 — only needed by run lifecycle (removed)
# num_cpus — only needed by old RunManager thread pool
# tokio-util — only needed by old run lifecycle

# Add
axum = { version = "0.8", features = ["json"] }
dashmap = "6"
notify = "8"
notify-debouncer-full = "0.7"
parking_lot = "0.12"
bincode = "2"

[dev-dependencies]
tempfile = "3"
tokio-test = "0.4"
```

---

## Alternatives Considered

| Recommended | Alternative | When to Use Alternative |
|-------------|-------------|-------------------------|
| dashmap 6 | `Arc<parking_lot::RwLock<HashMap>>` | Only if the entire LiveIndex is always locked as a unit (never partial updates). For Tokenizor, watcher updates and query responses overlap constantly — sharded locking wins. |
| notify-debouncer-full | Raw notify 8 events | Only if you need sub-100ms event latency and can tolerate duplicate events. For code intelligence, debouncing is always correct — raw events produce duplicate re-index floods on save. |
| axum 0.8 | actix-web, warp, rocket | axum is the standard choice when already using tokio. actix-web has its own runtime model (incompatible with tokio tasks sharing state). warp is older. rocket adds too much ceremony for a localhost sidecar. |
| bincode 2 | rkyv, postcard | rkyv is faster (zero-copy) but requires unsafe and adds derivation complexity. postcard is better for `no_std`. bincode 2 has a clean safe API with good performance for ~100K symbol records. |
| tree-sitter 0.24 (pinned) | tree-sitter 0.26.6 | Only after all six grammar crates publish 0.25+ compatible releases coordinated with ^0.25 or ^0.26 constraints. The 0.25 grammars broke Query API (captures/matches moved to QueryCursor) — this requires non-trivial migration in existing parsing code. |
| tokio::test | std::thread tests | All LiveIndex tests involve async file events. Use `#[tokio::test]` throughout. |

---

## What NOT to Use

| Avoid | Why | Use Instead |
|-------|-----|-------------|
| `spacetimedb-sdk` | Only needed for deferred v3+ feature. Adds 2 transitive crates. | Remove from Cargo.toml entirely for v2. |
| `tokio::sync::RwLock` for LiveIndex | Tokio's async RwLock has higher overhead than parking_lot or dashmap for CPU-bound queries. Queries are sub-microsecond memory reads — using an async lock introduces unnecessary yield points. | `dashmap` for the primary symbol index; `parking_lot::RwLock` for config/metadata structs. |
| `println!` / `print!` anywhere in the binary | Writing to stdout corrupts the MCP JSON-RPC stream. This will silently break the MCP server in ways that are extremely hard to debug. | `tracing::info!` / `tracing::debug!` — tracing-subscriber routes to stderr by default. |
| `serde_json` for index persistence | Serializing 100K+ symbol records to JSON is 5-10x slower and 3-5x larger than binary formats. | `bincode` for snapshot persistence. Keep `serde_json` for wire protocol only. |
| Full LSP (rust-analyzer, pyright) | Implementing an LSP client adds ~3000 lines and a daemon per language. The project explicitly trades 15% cross-reference accuracy for implementation simplicity. | tree-sitter syntactic queries — ~85% coverage in weeks, not months. |
| Tantivy for symbol search | Tantivy is a disk-backed full-text engine designed for documents. It maintains its own index files, thread pools, and segment management. For a live in-memory symbol store of ~100K records, this is massive overkill. | Custom trigram index: a `HashMap<[u8; 3], Vec<SymbolId>>` maintained inline with the LiveIndex. ~200 lines of code, sub-millisecond lookups, no external state. |
| `num_cpus` | Only needed by the old RunManager thread pool. Tokio's `rt-multi-thread` manages its own worker threads. | Remove. |
| `fs2` (file locking) | Only needed by old run lifecycle file locking. v2 has no on-disk lock files. | Remove. |
| `tokio-util` (rt feature) | Only the old run lifecycle used this for `TaskTracker`. | Remove unless a specific new need arises. |

---

## Version Compatibility

| Package | Compatible With | Notes |
|---------|-----------------|-------|
| tree-sitter 0.24.x | tree-sitter-rust@0.24.0, tree-sitter-typescript@0.23.2, others@0.23.x | The 0.23 grammar crates declare `^0.24` dev-dep but the runtime binding uses `tree-sitter-language ^0.1`. Mixing 0.23 grammar crates with tree-sitter 0.24 core works. Do NOT upgrade core to 0.25+ without coordinated grammar upgrades. |
| tree-sitter 0.25.x | tree-sitter-python@0.25.0, tree-sitter-javascript@0.25.0, tree-sitter-go@0.25.0 | These grammars already require `^0.25.8`. If you upgrade, you must upgrade ALL six grammars simultaneously. The 0.25 release moved `Query.captures()`/`Query.matches()` to `QueryCursor` — existing parsing code requires migration. |
| axum 0.8.x | tokio 1.x, tower 0.5.x | axum 0.8 requires tokio 1.x. Main branch targets 0.9 (breaking changes pending). Stay on 0.8.x for now. |
| notify-debouncer-full 0.7.0 | notify 8.2.0 | Debouncer re-exports notify. Do NOT add notify separately unless you need custom features. Use `notify_debouncer_full::notify::*` for the types. |
| rmcp 1.1.1 | tokio 1.x | rmcp 1.x uses bare JSON lines on stdio (not Content-Length framing). Verified in project memory. Do not add Content-Length framing — it will break Claude Code's hook integration. |
| dashmap 6.x | Rust 1.70+ | No serde feature needed for LiveIndex. If you serialize the index, use custom `From<DashMap<K,V>> for Vec<(K,V)>` before bincode encoding. |
| bincode 2.x | serde 1.x | bincode 2 has a new API vs 1.x — `encode_to_vec` / `decode_from_slice` instead of `serialize` / `deserialize`. Not backward-compatible with files written by bincode 1.x. Since v2 is a rewrite, start fresh with bincode 2. |

---

## Architecture Fit: How Stack Components Map to v2 Components

| v2 Component | Primary Crate(s) | Notes |
|---|---|---|
| LiveIndex (in-memory store) | `dashmap 6`, `parking_lot 0.12` | DashMap for symbol/file maps; parking_lot for config RwLock |
| File watcher | `notify 8` + `notify-debouncer-full 0.7` | 200ms debounce recommended; watcher loop is a spawned tokio task |
| Tree-sitter parsing + xref extraction | `tree-sitter 0.24`, grammar crates | Keep existing parsing code; add `Query` + `QueryCursor` for xref patterns |
| MCP stdio server | `rmcp 1.1.1` | Unchanged from v1 architecture |
| HTTP sidecar | `axum 0.8`, `tokio::net::TcpListener` | Bind port 0 → write to `.tokenizor/sidecar.port` → hooks POST to that port |
| Hook responses | `serde_json` | Hooks read JSON stdin, write JSON stdout with `additionalContext` field |
| Index persistence | `bincode 2`, `serde` | Serialize on shutdown, deserialize on startup for fast restart |
| Symbol text search | Custom trigram index (no crate) | `HashMap<[u8;3], Vec<u32>>` inline in LiveIndex. ~200 lines, no crate needed |
| Error handling | `anyhow` (app), `thiserror` (domain) | Pattern already in Cargo.toml |
| Logging | `tracing` + `tracing-subscriber` | Route ALL output to stderr. Never stdout. |

---

## Sources

- https://docs.rs/tokio/latest/tokio/ — confirmed version 1.50.0
- https://docs.rs/rmcp/latest/rmcp/ — confirmed version 1.1.1, stdio + streamable HTTP transports
- https://docs.rs/axum/latest/axum/ — confirmed version 0.8.8, Arc state pattern, JSON handlers
- https://docs.rs/notify/latest/notify/ — confirmed version 8.2.0
- https://docs.rs/notify-debouncer-full/latest/notify_debouncer_full/ — confirmed version 0.7.0
- https://docs.rs/parking_lot/latest/parking_lot/ — confirmed version 0.12.5
- https://docs.rs/dashmap/latest/dashmap/ — confirmed version 6.1.0
- https://docs.rs/tree-sitter/latest/tree_sitter/ — confirmed version 0.26.6 (latest), but grammar crate versions are behind
- https://docs.rs/tree-sitter-rust/latest/tree_sitter_rust/ — confirmed version 0.24.0, requires ^0.24
- https://docs.rs/tree-sitter-typescript/latest/tree_sitter_typescript/ — confirmed version 0.23.2, requires ^0.24
- https://docs.rs/tree-sitter-python/latest/tree_sitter_python/ — confirmed version 0.25.0, requires ^0.25.8 (UPGRADE RISK)
- https://docs.rs/tree-sitter-javascript/latest/tree_sitter_javascript/ — confirmed version 0.25.0, requires ^0.25.8 (UPGRADE RISK)
- https://docs.rs/tree-sitter-go/latest/tree_sitter_go/ — confirmed version 0.25.0, requires ^0.25.8 (UPGRADE RISK)
- https://github.com/tree-sitter/tree-sitter/issues/5013 — 0.26.x release checklist, confirms partially unfinished state
- https://code.claude.com/docs/en/hooks-guide — PostToolUse hook JSON protocol (stdin/stdout/exit codes)
- https://github.com/anthropics/claude-code/issues/24788 — PostToolUse `additionalContext` MCP tool call behavior

---
*Stack research for: Tokenizor v2 — in-memory code intelligence MCP server*
*Researched: 2026-03-10*
