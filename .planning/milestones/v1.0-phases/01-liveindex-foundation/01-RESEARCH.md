# Phase 1: LiveIndex Foundation - Research

**Researched:** 2026-03-10
**Domain:** Rust in-memory indexing, concurrent data structures, file discovery, circuit breaker pattern
**Confidence:** HIGH

## Summary

Phase 1 is a clean-slate rewrite that replaces the v1 RunManager/CAS/registry architecture (~20,000 lines) with a minimal in-memory HashMap-based LiveIndex. The codebase already contains all the building blocks: `src/parsing/` (battle-tested, kept as-is), `src/indexing/discovery.rs` (reusable file-walking logic), `src/observability.rs` (tracing setup), and `src/error.rs` (error taxonomy). The core work is writing three new modules (`src/live_index/`, `src/discovery/`, rewritten `main.rs`/`lib.rs`/`error.rs`) that wire these pieces together into a coherent, crash-proof foundation.

The two design choices left to Claude's discretion (DashMap vs `Arc<RwLock<HashMap>>`, and content storage policy for total-parse-failure files) are well-understood and have clear answers: use `Arc<RwLock<HashMap>>` (simpler, no extra dependency, write-once-at-startup then read-many fits perfectly), and store content bytes even for failed files (needed for Phase 3 retry on watcher events). The circuit breaker is implemented as two `AtomicUsize` counters plus an `AtomicBool` tripped flag — no external crate needed.

The only integration wrinkle is `src/parsing/mod.rs` importing `crate::storage::digest_hex`. Since the storage module is deleted in this phase, `digest_hex` must either be inlined into parsing or moved to a utility module. The cleanest path is to inline it: a SHA-256 hex digest is three lines with the `sha2` crate already in the ecosystem, or simply use a Blake3/xxHash — but checking Cargo.toml shows no hash crate currently. The simplest move is to compute `digest_hex` as a helper in the new `src/live_index/store.rs` and re-export it so `src/parsing/` can import from `crate::live_index`.

**Primary recommendation:** Use `Arc<RwLock<HashMap<String, IndexedFile>>>` for the LiveIndex store. Load synchronously at startup with Rayon parallelism (`par_iter` over discovered files), then wrap in `Arc` and hand to the async MCP server. Single write at boot, unlimited concurrent reads after — no sharded map complexity needed.

---

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions
- Clean-slate rewrite: delete all v1 modules except `src/parsing/` (978 lines, kept as-is), partial `src/domain/index.rs` (SymbolRecord, SymbolKind, LanguageId), `npm/`, and `tests/tree_sitter_grammars.rs`
- Deletion happens as the first task of Phase 1 (not a separate prep step)
- New module structure: `src/live_index/` (store, query), `src/discovery/` (file walking), plus rewritten `main.rs`, `lib.rs`, `error.rs`
- Kept domain types stripped to minimal derives — remove JsonSchema and serde annotations that only served registry/CAS. Add back what's needed when MCP tools land in Phase 2.
- Fresh test suite: delete all `tests/` except `tree_sitter_grammars.rs` and `retrieval_conformance.rs`. Write new tests alongside new code.
- Only index files with known language extensions (6 languages: Rust, Python, JS, TS, Go, Java)
- Non-code files (.json, .toml, .yaml, .md) are NOT stored in LiveIndex — they exist on disk only
- Discovery walks from .git root (auto-detected). Fallback: CWD if .git not found.
- .gitignore-respected filtering only (via `ignore::WalkBuilder`). No custom .tokenizorignore in v2.
- Block all MCP tool responses until full index load completes (guard pattern)
- `health` tool is the sole exception — always responds regardless of state
- All other tools return an error ("Index loading") until ready
- Circuit breaker trips (>20% parse failures): server marks degraded, stops indexing, refuses all non-health queries
- Phase 1 validates readiness via integration tests (Rust API: `is_ready()` method). MCP wiring comes in Phase 2.
- Partial parse (syntax errors, some symbols extracted): silent — file stored with extracted symbols, warning logged to stderr via tracing. No user-facing indication.
- Total parse failure: file stored with content bytes but empty symbol list. Counts toward circuit breaker threshold.
- Circuit breaker error message: one-line summary + first 3-5 failed file paths with reasons + suggested action. Not a full file list.
- Circuit breaker threshold configurable via `TOKENIZOR_CB_THRESHOLD` env var (defaults to 20%)
- Failed files auto-retry on next watcher event (Phase 3 implements retry; Phase 1 stores failure state)
- Per-file parse status (Parsed, PartialParse, Failed) stored as queryable field in LiveIndex
- Health stats include: file counts, symbol counts, parse status breakdown, total index load duration
- Logging: tracing + tracing-subscriber on stderr, ANSI disabled, env-filter defaulting to `info`. Simplified from v1 (no deployment report, no readiness gate complexity).

### Claude's Discretion
- Exact DashMap vs RwLock<HashMap> choice for concurrent map
- Internal data layout of LiveIndex entries (field ordering, auxiliary indexes)
- Whether total-parse-failure files store content or are excluded (leaning toward store content)
- Exact error types and error taxonomy in rewritten `error.rs`

### Deferred Ideas (OUT OF SCOPE)
- None — discussion stayed within phase scope.
</user_constraints>

---

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-----------------|
| LIDX-01 | All discovered source files loaded into in-memory HashMap on startup | `ignore::WalkBuilder` discovery + Rayon parallel load + `Arc<RwLock<HashMap>>` store |
| LIDX-02 | All tree-sitter extracted symbols stored with O(1) lookup by name, file, and ID | `HashMap<String, IndexedFile>` keyed by relative_path; per-file symbol Vec with linear scan (O(n) per file) is acceptable at Phase 1 scale; auxiliary name index deferred to Phase 2 |
| LIDX-03 | File content bytes stored in memory — zero disk I/O on read path | Store `Vec<u8>` bytes field in `IndexedFile`; read path queries map only |
| LIDX-04 | Concurrent access via shared ownership (Arc + concurrent map) — many readers, exclusive writer | `Arc<RwLock<HashMap>>` — write-once at boot, then read-only for query path |
| RELY-01 | Circuit breaker aborts indexing if >20% of files fail parsing | `AtomicUsize` failed_count + total_count; tripped `AtomicBool`; threshold from `TOKENIZOR_CB_THRESHOLD` env var |
| RELY-02 | Partial parse on syntax errors — keep previous symbols, log warning | `FileOutcome::PartialParse` already exists in `src/parsing/`; store extracted symbols, warn via `tracing::warn!` |
| RELY-04 | MCP server stdout purity — zero non-JSON output on stdout (CI gate) | tracing-subscriber initialized with `.with_writer(std::io::stderr)` — already done in v1 `observability.rs` |
</phase_requirements>

---

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| `std::sync::{Arc, RwLock}` | stdlib | Shared ownership + read-write guard for LiveIndex | Zero deps, write-once-at-startup then read-only is ideal RwLock fit |
| `std::collections::HashMap` | stdlib | Inner map: relative_path → IndexedFile | O(1) average lookup, no overhead |
| `ignore` | 0.4 (already in Cargo.toml) | .gitignore-aware recursive file walk | Used by ripgrep; already a dependency; handles .git/info/exclude and global gitignore |
| `rayon` | 1.x | Parallel file read + parse during startup load | Saturates CPU cores during I/O+parse batch; clean `par_iter` → `collect` pattern |
| `tracing` + `tracing-subscriber` | 0.1 / 0.3 (already in Cargo.toml) | Structured logging to stderr | Already in project; ANSI-disabled stderr keeps stdout pure |
| `thiserror` | 2.0 (already in Cargo.toml) | Domain error types in `error.rs` | Already in project; `#[derive(Error)]` with `#[error(...)]` messages |
| `anyhow` | 1.0 (already in Cargo.toml) | Error propagation in `main.rs` | Already in project; used at CLI boundary |

### Supporting
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| `tempfile` | 3 (already in dev-deps) | Integration test fixtures (temp directories) | All Phase 1 integration tests |
| `std::sync::atomic::{AtomicBool, AtomicUsize}` | stdlib | Lock-free circuit breaker state | During parallel index load phase |

### New Dependency Required
| Library | Version | Purpose | Why |
|---------|---------|---------|-----|
| `rayon` | 1.10 | Parallel startup indexing | Not yet in Cargo.toml; needed for LIDX-01 performance |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| `Arc<RwLock<HashMap>>` | `DashMap` (6.1.0) | DashMap reduces contention under concurrent writes via sharding, but Phase 1 has a single write at startup then reads only — the sharding overhead is pure cost, not benefit. Add DashMap only if Phase 3 (file watcher) benchmarks show contention. |
| Rayon parallel load | `tokio::spawn` per file | tokio spawn has overhead for CPU-bound work; Rayon's work-stealing thread pool is the correct tool for CPU-bound parallel batch |
| stdlib `find_git_root` (manual) | `git2` crate | git2 adds a large dependency for one function; manual upward `Path::parent()` traversal looking for `.git` directory is 10 lines and has no deps |

**Installation (new dependency only):**
```bash
cargo add rayon
```

---

## Architecture Patterns

### Recommended Project Structure (post-rewrite)
```
src/
├── main.rs              # Minimal: init tracing, call lib::run(), handle git-root detection
├── lib.rs               # Public API: LiveIndex type alias, run(), IndexState enum
├── error.rs             # TokenizorError enum (simplified from v1)
├── observability.rs     # init_tracing() — keep from v1 unchanged
├── domain/
│   └── mod.rs           # LanguageId, SymbolRecord, SymbolKind, FileOutcome (stripped from v1)
├── live_index/
│   ├── mod.rs           # pub use store::LiveIndex; pub use query::*;
│   ├── store.rs         # LiveIndex struct, IndexedFile struct, load_all(), is_ready()
│   └── query.rs         # Query methods: get_file(), iter_symbols(), health_stats()
├── discovery/
│   └── mod.rs           # discover_files(root: &Path) — adapted from v1 indexing/discovery.rs
└── parsing/
    ├── mod.rs           # KEPT from v1 — process_file() (update import for digest_hex)
    └── languages/       # KEPT from v1 — all 6 language extractors
tests/
├── tree_sitter_grammars.rs    # KEPT from v1
├── retrieval_conformance.rs   # KEPT from v1 (may need minor import updates)
└── live_index_integration.rs  # NEW — startup load, circuit breaker, readiness guard
```

### Pattern 1: Write-Once Arc<RwLock<HashMap>> LiveIndex
**What:** Build the entire HashMap synchronously during startup (using Rayon), then wrap in `Arc<RwLock<>>` and hand to the async MCP server. After startup, the RwLock is only ever read-locked.
**When to use:** Any scenario where the index is built once at startup and reads vastly outnumber writes. This is Phase 1. Phase 3 (file watcher) will add incremental writes, at which point per-file write locks are still fast because each write touches one HashMap entry.

```rust
// Source: Tokio shared state docs + std::sync::RwLock stdlib
pub struct LiveIndex {
    files: HashMap<String, IndexedFile>,       // relative_path → entry
    loaded_at: std::time::Instant,
    cb_state: CircuitBreakerState,
}

pub type SharedIndex = Arc<RwLock<LiveIndex>>;

impl LiveIndex {
    pub fn load(root: &Path) -> Result<SharedIndex> {
        let files = discover_files(root)?;
        let total = files.len();

        // Parallel parse with Rayon
        let results: Vec<(String, IndexedFile)> = files
            .par_iter()
            .map(|f| load_and_parse(f))
            .collect();

        let mut index = LiveIndex::new();
        for (path, entry) in results {
            index.ingest(path, entry);
            if index.cb_state.should_abort(total) {
                break;
            }
        }
        Ok(Arc::new(RwLock::new(index)))
    }

    pub fn is_ready(&self) -> bool {
        !self.cb_state.is_tripped()
    }
}
```

### Pattern 2: Per-file IndexedFile struct
**What:** Each HashMap entry holds all data for one file — content bytes, symbols, parse status, and metadata. No separate symbol index at Phase 1. Queries iterate `files.values()` when they need symbol search; this is O(n files) but acceptable since Phase 1 has no MCP tools (Phase 2 adds those).

```rust
// IndexedFile — the value type for the HashMap
pub struct IndexedFile {
    pub relative_path: String,
    pub language: LanguageId,
    pub content: Vec<u8>,               // LIDX-03: content in memory
    pub symbols: Vec<SymbolRecord>,     // LIDX-02: extracted symbols
    pub parse_status: ParseStatus,      // RELY-01/02: for circuit breaker + health stats
    pub byte_len: u64,
    pub content_hash: String,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ParseStatus {
    Parsed,
    PartialParse { warning: String },
    Failed { error: String },
}
```

### Pattern 3: Circuit Breaker as Inline State (no external crate)
**What:** Two `AtomicUsize` counters (total_seen, failed_count) + one `AtomicBool` (tripped). Check after each file ingest. If failed/total > threshold, set tripped=true and stop ingesting.
**Why no external crate:** The `circuitbreaker-rs` and similar crates are designed for request-gating retries with half-open states, not for a one-shot batch abort. The inline approach is simpler and avoids a dependency.

```rust
// Circuit breaker — all atomics, no locks
pub struct CircuitBreakerState {
    total: AtomicUsize,
    failed: AtomicUsize,
    tripped: AtomicBool,
    threshold: f64,     // read from TOKENIZOR_CB_THRESHOLD env var at construction
}

impl CircuitBreakerState {
    pub fn record_outcome(&self, failed: bool) {
        self.total.fetch_add(1, Ordering::Relaxed);
        if failed {
            self.failed.fetch_add(1, Ordering::Relaxed);
        }
    }

    pub fn should_abort(&self) -> bool {
        let total = self.total.load(Ordering::Relaxed);
        if total < 5 { return false; }  // don't trip on tiny repos
        let failed = self.failed.load(Ordering::Relaxed);
        let rate = failed as f64 / total as f64;
        if rate > self.threshold {
            self.tripped.store(true, Ordering::Release);
            true
        } else {
            false
        }
    }

    pub fn is_tripped(&self) -> bool {
        self.tripped.load(Ordering::Acquire)
    }
}
```

### Pattern 4: Git Root Detection (stdlib only, no git2)
**What:** Walk upward from CWD looking for a `.git` directory. Return the first ancestor that contains `.git`. Fall back to CWD if none found.

```rust
// No external crate needed — 10 lines
pub fn find_git_root() -> PathBuf {
    let mut current = std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."));
    loop {
        if current.join(".git").exists() {
            return current;
        }
        match current.parent() {
            Some(parent) => current = parent.to_path_buf(),
            None => return std::env::current_dir()
                .unwrap_or_else(|_| PathBuf::from(".")),
        }
    }
}
```

### Pattern 5: digest_hex Migration
**What:** `src/parsing/mod.rs` currently imports `crate::storage::digest_hex`. The `storage` module is deleted in this phase. Move `digest_hex` into `src/live_index/store.rs` (or a `src/util.rs`) and update the import in `src/parsing/mod.rs`.

The simplest approach: remove `digest_hex` from parsing entirely. `content_hash` is a v1 CAS concept. In v2, the LiveIndex does not deduplicate by hash — it stores one entry per path. The hash field can either be dropped from `FileProcessingResult` or computed lazily in the store. Dropping it from `FileProcessingResult` simplifies parsing and removes the hash dependency entirely.

**If hash is retained** (for Phase 3 change detection): add `sha2` or use `std::collections::hash_map::DefaultHasher` for a non-cryptographic fast hash.

### Anti-Patterns to Avoid
- **DashMap for write-once data:** DashMap's sharding only helps under concurrent writes. At Phase 1 the map is written once (batch load) then read-only. The shard overhead adds complexity for zero benefit.
- **tokio::spawn per file for parsing:** Tree-sitter parsing is CPU-bound. Spawning async tasks for CPU work steals tokio's I/O thread pool. Use Rayon for CPU-bound parallelism, then hand the completed Vec to the async runtime.
- **Holding RwLock write guard across await points:** The RwLock write guard is held during initial population only — this happens in sync code before the async server starts. No await-point contention.
- **Printing to stdout anywhere:** MCP protocol is JSON-RPC on stdout. Any `println!` anywhere in the codebase corrupts the protocol stream. All output must use `tracing::*` macros which route to stderr.
- **Panicking on malformed input:** `src/parsing/mod.rs` already uses `panic::catch_unwind` — preserve this. New code in `live_index/store.rs` must also never panic on bad file content.

---

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| .gitignore-aware file walking | Custom walker with manual .gitignore parsing | `ignore::WalkBuilder` | Handles nested .gitignore, .git/info/exclude, global gitignore, Windows path normalization, symlink cycles — already a dep |
| Parallel CPU-bound processing | tokio::spawn loop | `rayon::par_iter` | Work-stealing scheduler; designed for CPU-bound batch; no async overhead |
| Panic-safe parsing | Manual Result-wrapping of tree-sitter | `std::panic::catch_unwind` (already in `src/parsing/mod.rs`) | tree-sitter can panic on adversarial grammar input; existing code already handles this — keep it |

**Key insight:** The existing `discovery.rs` and `parsing/mod.rs` are well-written and already tested. The Phase 1 work is primarily wiring, not net-new algorithms.

---

## Common Pitfalls

### Pitfall 1: `storage::digest_hex` Import Breaks Compilation
**What goes wrong:** After deleting `src/storage/`, the `src/parsing/mod.rs` import `use crate::storage::digest_hex;` causes a compile error, blocking all other work.
**Why it happens:** Parsing module was coupled to the CAS storage layer via the hash function.
**How to avoid:** The first task after deletion must update `src/parsing/mod.rs`. Either drop `content_hash` from `FileProcessingResult` entirely (preferred for Phase 1 simplicity) or relocate `digest_hex` to `src/live_index/store.rs` and update the import.
**Warning signs:** `cargo check` fails immediately after deletion step.

### Pitfall 2: Writing to stdout (stdout purity — RELY-04)
**What goes wrong:** Any `println!`, `print!`, `eprintln!` that accidentally targets stdout, or a `tracing_subscriber` misconfiguration that writes to stdout instead of stderr.
**Why it happens:** Rust defaults `println!` to stdout. If tracing is not initialized before the MCP server starts, early log output can corrupt the JSON-RPC stream.
**How to avoid:** Call `init_tracing()` as the very first line of `main()`. Never use `println!` in any new code — all output via `tracing::info!`/`warn!`/`error!`. CI gate: pipe `tokenizor run` stdout through `jq` in integration test.
**Warning signs:** MCP client reports "unexpected token" on connection; `jq` pipe fails.

### Pitfall 3: Circuit Breaker Never Trips on Small Repos
**What goes wrong:** With only 3 files and 1 failure (33%), the circuit breaker trips. This is a false positive.
**Why it happens:** 1/3 = 33% > 20% threshold, but the sample is too small to be meaningful.
**How to avoid:** Add a minimum file threshold before tripping (e.g., don't evaluate the rate until at least 5 files have been processed). CONTEXT.md does not specify this; it is Claude's discretion to add.
**Warning signs:** Integration tests on tiny test repos trip the circuit breaker unexpectedly.

### Pitfall 4: RwLock Deadlock on Re-entrant Read
**What goes wrong:** A query method acquires a read guard, then calls another method that also tries to acquire a read guard on the same `RwLock`. On some implementations this deadlocks (especially on Windows with `std::sync::RwLock`).
**Why it happens:** `std::sync::RwLock` does not support re-entrant locking — calling `read()` while already holding a read guard from the same lock is undefined behavior on some platforms.
**How to avoid:** Never call any method that acquires the outer `RwLock` from inside another method that already holds the lock. Structure query methods to accept `&LiveIndex` (after the guard is taken by the caller) rather than taking `&SharedIndex`.
**Warning signs:** Integration tests hang or deadlock on Windows in query scenarios.

### Pitfall 5: `ignore::WalkBuilder` Skips Files Without a `.git` Directory
**What goes wrong:** In test fixtures created with `tempfile::tempdir()`, `.gitignore` files are not respected by `WalkBuilder` unless a `.git` directory also exists (the `ignore` crate requires actual git repo context to process `.gitignore` files, controlled by `require_git()`).
**Why it happens:** By default, `require_git()` is true — gitignore files only apply inside git repos.
**How to avoid:** In integration tests that need `.gitignore` behavior, either create a `.git` directory in the tempdir (as the existing `test_discover_files_respects_gitignore` test already does) or call `require_git(false)` on the builder.
**Warning signs:** Test expects filtered files but all files appear in results.

### Pitfall 6: Rayon and tokio Runtime Interaction
**What goes wrong:** Rayon's thread pool blocks during startup. If startup is called from within a tokio async context (e.g., inside `tokio::main`), the blocking Rayon work stalls the async runtime.
**Why it happens:** Rayon uses OS threads that block; running blocking work inside `async fn` without `tokio::task::spawn_blocking` starves the tokio scheduler.
**How to avoid:** Perform the entire `LiveIndex::load()` synchronously before the tokio MCP server starts, OR wrap it in `tokio::task::spawn_blocking`. The cleanest approach: load the index in `main()` before handing to the async server, similar to how v1's `ApplicationContext::from_config()` is sync.
**Warning signs:** MCP server appears to start but never responds; tokio reports stalled tasks.

---

## Code Examples

Verified patterns from existing codebase and stdlib documentation:

### Existing `discover_files` — Reuse Pattern
```rust
// Source: src/indexing/discovery.rs (v1, keep this logic in src/discovery/mod.rs)
pub fn discover_files(root: &Path) -> Result<Vec<DiscoveredFile>> {
    for entry in WalkBuilder::new(root).build() {
        // filters by extension via LanguageId::from_extension()
        // normalizes backslashes to forward slashes
        // respects .gitignore automatically
    }
}
// The v2 version: same logic, same signature. Just move the file.
```

### Tracing Init — Keep Unchanged
```rust
// Source: src/observability.rs (v1, keep verbatim)
pub fn init_tracing() -> Result<()> {
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .with_writer(std::io::stderr)  // CRITICAL: stderr, not stdout
        .with_ansi(false)
        .try_init()
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    Ok(())
}
```

### Parallel Load with Rayon
```rust
// Pattern: collect Vec of results in parallel then build map
use rayon::prelude::*;

let discovered = discover_files(&root)?;
let results: Vec<(String, IndexedFile)> = discovered
    .par_iter()
    .map(|df| {
        let bytes = std::fs::read(&df.absolute_path)
            .unwrap_or_default();
        let parse = parsing::process_file(&df.relative_path, &bytes, df.language.clone());
        let entry = IndexedFile::from_parse_result(parse, bytes);
        (df.relative_path.clone(), entry)
    })
    .collect();
// Then build HashMap from results (sequential, fast)
let map: HashMap<String, IndexedFile> = results.into_iter().collect();
```

### IndexReadiness Guard (Phase 1 version — no MCP tools yet)
```rust
// Rust API only in Phase 1. MCP wiring in Phase 2.
impl LiveIndex {
    pub fn is_ready(&self) -> bool {
        !self.cb_state.is_tripped()
    }

    pub fn index_state(&self) -> IndexState {
        match (self.loading, self.cb_state.is_tripped()) {
            (true, _) => IndexState::Loading,
            (false, true) => IndexState::CircuitBreakerTripped {
                summary: self.cb_state.summary(),
            },
            (false, false) => IndexState::Ready,
        }
    }
}

pub enum IndexState {
    Loading,
    Ready,
    CircuitBreakerTripped { summary: String },
}
```

### Minimal v2 main.rs Shape
```rust
// After rewrite: main.rs is ~30 lines
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    observability::init_tracing()?;
    let root = discovery::find_git_root();
    let index = live_index::LiveIndex::load(&root)?;   // sync, Rayon inside
    // Phase 2 adds: let server = McpServer::new(index); server.serve(stdio()).await;
    tracing::info!("LiveIndex ready: {} files", index.read().unwrap().file_count());
    Ok(())
}
```

---

## State of the Art

| Old Approach (v1) | New Approach (v2) | Impact |
|-------------------|-------------------|--------|
| CAS blob store + JSON registry on disk | In-memory HashMap, no disk on read path | Zero disk I/O on queries; instant startup for small repos |
| RunManager with run lifecycle, checkpoints | No runs — one load at startup, watcher events later | ~6,149 lines removed; no resume/repair complexity |
| `Arc<DashMap>` (mentioned in AD-1) | `Arc<RwLock<HashMap>>` (Phase 1 recommendation) | Simpler; DashMap appropriate if write contention appears in Phase 3 |
| `spacetimedb-sdk` dependency | Remove from Cargo.toml | Large transitive dep gone; faster compile |
| `schemars::JsonSchema` on domain types | Remove annotation until Phase 2 MCP wiring | Clean domain types; add back only what tools need |
| `num_cpus` dependency | Rayon manages thread pool internally | Remove `num_cpus` from Cargo.toml |

**Deprecated/outdated after Phase 1:**
- `fs2`: file locking (CAS store artifact) — remove
- `spacetimedb-sdk`: deferred to v3+ — remove
- `schemars`: only needed for MCP tool schemas — remove until Phase 2
- `num_cpus`: Rayon manages thread pool — remove
- `tokio-util`: may still be needed for cancellation token in Phase 3, check usage

---

## Open Questions

1. **Should `content_hash` stay in `FileProcessingResult`?**
   - What we know: v1 used it for CAS deduplication. v2 has no CAS. Phase 3 (file watcher) needs change detection but can use file mtime + size instead.
   - What's unclear: Whether hashing every file on load adds measurable overhead vs mtime-based change detection.
   - Recommendation: Drop `content_hash` from `FileProcessingResult` for Phase 1. Phase 3 can add a hash field to `IndexedFile` directly if mtime-only change detection proves insufficient.

2. **`retrieval_conformance.rs` — keep or drop?**
   - What we know: CONTEXT.md says keep it. But it imports v1 domain types (FileRecord, IndexRun, etc.) that are being deleted.
   - What's unclear: How much work is needed to update it for v2 types.
   - Recommendation: Keep the file but the planner should include a task to audit its imports and update or stub out sections that reference deleted types before the compilation gate.

3. **`tokio-util` — remove or keep?**
   - What we know: v1 uses `tokio_util::sync::CancellationToken` in the pipeline. The pipeline is deleted.
   - What's unclear: Whether Phase 2 or 3 will need `CancellationToken` for MCP request cancellation.
   - Recommendation: Remove from Phase 1 Cargo.toml. Re-add in Phase 3 if watcher needs it.

---

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | Rust built-in test harness (`cargo test`) |
| Config file | None (Cargo.toml `[dev-dependencies]`) |
| Quick run command | `cargo test --lib` |
| Full suite command | `cargo test` |

### Phase Requirements → Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| LIDX-01 | All source files loaded on startup | integration | `cargo test live_index_integration` | ❌ Wave 0 |
| LIDX-02 | Symbols stored with O(1) lookup by file | unit | `cargo test --lib live_index::store` | ❌ Wave 0 |
| LIDX-03 | Content bytes in memory, zero disk I/O on read | unit | `cargo test --lib live_index::store::test_content_in_memory` | ❌ Wave 0 |
| LIDX-04 | Arc<RwLock<_>> concurrent access | unit | `cargo test --lib live_index::store::test_concurrent_readers` | ❌ Wave 0 |
| RELY-01 | Circuit breaker trips at >20% failure | unit | `cargo test --lib live_index::store::test_circuit_breaker_trips` | ❌ Wave 0 |
| RELY-02 | Partial parse keeps symbols, logs warning | unit | `cargo test --lib live_index::store::test_partial_parse_retained` | ❌ Wave 0 |
| RELY-04 | stdout produces only JSON (CI gate) | integration | `cargo test live_index_integration::test_stdout_purity` | ❌ Wave 0 |

### Sampling Rate
- **Per task commit:** `cargo test --lib`
- **Per wave merge:** `cargo test`
- **Phase gate:** Full suite green before `/gsd:verify-work`

### Wave 0 Gaps
- [ ] `tests/live_index_integration.rs` — covers LIDX-01, RELY-04 (startup load from tempdir, stdout purity via subprocess spawn)
- [ ] `src/live_index/store.rs` inline tests — covers LIDX-02, LIDX-03, LIDX-04, RELY-01, RELY-02
- [ ] `src/discovery/mod.rs` tests — covers file walking behavior (can reuse logic from v1 `discovery.rs` tests verbatim)
- [ ] Rayon dev-dependency: `cargo add rayon --dev` OR as a regular dep if used in lib code

**Note on kept tests:** `tests/tree_sitter_grammars.rs` should pass without modification after the module restructure (it only imports from `src/parsing/`). `tests/retrieval_conformance.rs` needs an import audit — it likely references deleted types and will need updating before it compiles.

---

## Sources

### Primary (HIGH confidence)
- `src/indexing/discovery.rs` (v1 codebase) — WalkBuilder usage pattern, .gitignore test patterns
- `src/parsing/mod.rs` (v1 codebase) — `process_file` API, `FileProcessingResult` structure, adversarial input tests
- `src/observability.rs` (v1 codebase) — tracing init pattern (stderr, ANSI disabled)
- `src/error.rs` (v1 codebase) — error taxonomy and `thiserror` patterns
- `src/domain/index.rs` (v1 codebase) — `LanguageId`, `SymbolRecord`, `SymbolKind`, `FileOutcome` types
- [std::sync::RwLock docs](https://doc.rust-lang.org/std/sync/struct.RwLock.html) — RwLock API and re-entrancy warning
- [std::sync::atomic docs](https://doc.rust-lang.org/std/sync/atomic/) — AtomicBool, AtomicUsize, Ordering
- [ignore::WalkBuilder docs](https://docs.rs/ignore/latest/ignore/struct.WalkBuilder.html) — `require_git()` behavior, gitignore respecting

### Secondary (MEDIUM confidence)
- [DashMap 6.1.0 docs](https://docs.rs/dashmap/latest/dashmap/struct.DashMap.html) — deadlock warning, shard configuration, `try_get` pattern
- [Tokio shared state tutorial](https://tokio.rs/tokio/tutorial/shared-state) — `Arc<Mutex<_>>` pattern (generalized to RwLock)
- [Rayon data parallelism guide (2024)](https://www.shuttle.dev/blog/2024/04/11/using-rayon-rust) — `par_iter` → `collect` into HashMap pattern
- [rmcp docs](https://docs.rs/rmcp) — transport-io feature, stdio transport, server initialization

### Tertiary (LOW confidence)
- Community discussion on DashMap vs RwLock<HashMap> — forum posts corroborating "write-once → use RwLock" recommendation
- Circuit breaker crate survey — confirmed inline atomics are simpler than external crate for single-pass abort

---

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH — all libraries already in Cargo.toml or well-known stdlib types; rayon is the only new addition
- Architecture: HIGH — patterns are direct adaptations of existing v1 code, not novel designs
- Pitfalls: HIGH — most pitfalls are confirmed by examining actual v1 source code (import coupling, stdout purity setup, test fixture patterns)
- Discretion recommendations: MEDIUM — DashMap vs RwLock recommendation is evidence-based but Phase 3 write patterns could change the calculus

**Research date:** 2026-03-10
**Valid until:** 2026-06-10 (stable Rust stdlib + ignore crate patterns change slowly)
