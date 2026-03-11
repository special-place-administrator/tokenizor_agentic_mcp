# Phase 5: HTTP Sidecar + Hook Infrastructure — Research

**Researched:** 2026-03-10
**Domain:** axum HTTP server (Rust), Claude Code hooks (settings.json), clap CLI subcommands, sidecar process management
**Confidence:** HIGH

---

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions

**Sidecar API Surface**
- Hooks-only subset: `/outline` (Read hook), `/impact` (Edit hook), `/symbol-context` (Grep hook), `/repo-map` (SessionStart hook), plus `/health`
- JSON response format — consumed by the Rust hook binary which formats into additionalContext strings
- GET endpoints with query parameters (e.g., `GET /outline?path=src/main.rs`) — simple, curl-testable
- No full LiveIndex mirror — add more endpoints later if needed

**Hook Script Design**
- Rust binary: same binary as MCP server, `tokenizor hook <subcommand>` (e.g., `tokenizor hook read`, `tokenizor hook edit`, `tokenizor hook grep`, `tokenizor hook session-start`)
- Zero Python dependency — eliminates ~50ms spawn overhead per hook invocation
- Silent fail-open: if sidecar unreachable (port file missing, connection refused, timeout), return empty/no-op JSON so Claude Code continues normally
- 50ms hard HTTP timeout — leaves margin within HOOK-03's 100ms total budget given near-zero Rust binary spawn cost

**tokenizor init Scope**
- Writes to user-global `~/.claude/settings.json` (not project-local)
- Merge, don't overwrite: read existing settings.json, add/update only tokenizor entries (identified by command path marker), leave all other hooks untouched
- Idempotent: running twice produces identical result
- Full setup: creates `.tokenizor/` directory if missing
- Validates binary path exists and is executable — warns if not found but still writes hooks

**Sidecar Lifecycle**
- Starts after LiveIndex load, before MCP stdio serving begins
- Port file: `.tokenizor/sidecar.port` written immediately after bind
- PID file: `.tokenizor/sidecar.pid` written alongside port file
- Shutdown cleanup: delete both port and PID files on clean exit
- Startup check: if PID file exists and process is stale, clean up and rebind
- Bind address: configurable via `TOKENIZOR_SIDECAR_BIND` env var, defaults to `127.0.0.1`
- Port: always OS-assigned (port 0) — no configurable override, port file is the only discovery mechanism
- Shares `Arc<LiveIndex>` with MCP tools in the same process (HOOK-02) — zero data duplication

### Claude's Discretion
- Exact axum router setup and middleware choices
- JSON response schema details for each endpoint
- PID file format and stale-process detection implementation
- Hook subcommand CLI argument parsing (clap or manual)
- Error types and HTTP status codes for sidecar responses
- How `tokenizor init` discovers the binary path to write into hooks.json

### Deferred Ideas (OUT OF SCOPE)
None — discussion stayed within phase scope
</user_constraints>

---

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-----------------|
| HOOK-01 | HTTP sidecar (axum) on localhost:0, port written to .tokenizor/sidecar.port | axum TcpListener::bind("127.0.0.1:0") + local_addr() pattern; tokio::spawn for background task |
| HOOK-02 | Sidecar shares Arc<LiveIndex> with MCP tools — zero data duplication | Arc::clone() into axum State extractor; same pattern as watcher spawn already in main.rs |
| HOOK-03 | Hook response latency <100ms total (Rust spawn + HTTP + query) | Rust binary spawn ~1ms; 50ms HTTP timeout in hook binary; LiveIndex read ~<5ms; total well under 100ms |
| HOOK-10 | Hook stdout is valid JSON only — no debug output corruption | Existing RELY-04 pattern: tracing writes to stderr only; hook binary must write only JSON to stdout |
</phase_requirements>

---

## Summary

Phase 5 builds the infrastructure layer between the LiveIndex (built in Phases 1-4) and the hook enrichment logic (Phase 6). It has three independent deliverables: (1) an axum HTTP sidecar running on an ephemeral port, (2) the `tokenizor hook <subcommand>` CLI dispatch layer, and (3) the `tokenizor init` command that writes hooks into `~/.claude/settings.json`.

The axum 0.8 pattern for ephemeral ports is well-established and simple: `TcpListener::bind("127.0.0.1:0")` followed by `listener.local_addr().unwrap().port()` to discover the OS-assigned port. The sidecar runs as a `tokio::spawn`ed background task alongside the existing watcher task, receiving an `Arc::clone()` of the SharedIndex — the identical pattern already used by the watcher. Shutdown is coordinated via a `tokio::sync::oneshot` channel.

Claude Code hooks live in the `hooks` section of `~/.claude/settings.json` (not a separate hooks.json). The confirmed schema uses `PostToolUse` entries with a regex `matcher` field for tool names — `"Read"`, `"Edit"`, `"Grep"` are the exact native tool names. Hook commands output JSON to stdout with a `hookSpecificOutput.additionalContext` field for context injection. A critical known limitation: `additionalContext` does NOT work for MCP tool calls (only native tools), but the Phase 5 hooks target native Read/Edit/Grep tools, so this limitation does not affect this phase.

**Primary recommendation:** Add `axum = "0.8"` and `clap = "4"` (with derive feature) to Cargo.toml; structure the sidecar as a new `src/sidecar/` module with sub-modules for the router, port/PID management, and endpoint handlers; extend `main.rs` to accept CLI subcommands before the MCP server path; write all hook output to stdout as clean JSON with tracing to stderr only.

---

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| axum | 0.8 | HTTP server for sidecar | Tokio-native, already used by rmcp ecosystem; ergonomic Router + State |
| clap | 4 | CLI subcommand parsing | De-facto Rust CLI standard; derive API keeps arg structs colocated with handlers |
| tokio (existing) | 1.48 | Async runtime | Already in Cargo.toml; axum requires tokio |

### Supporting
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| serde_json (existing) | 1.0 | JSON serialization for sidecar responses | Already present; sidecar handlers `Json(response_struct)` |
| serde (existing) | 1.0 | Derive Serialize/Deserialize for response structs | Already present |
| tokio::sync::oneshot | (tokio stdlib) | Shutdown signal to sidecar task | When MCP server exits, signal sidecar to stop |
| std::process::id() | (stdlib) | Write current PID to PID file | Cross-platform, no external crate needed |

### Stale-Process Detection
| Option | Approach | Verdict |
|--------|----------|---------|
| sysinfo crate | Full process table query | Too heavy; adds 500KB+ binary size for one operation |
| process_alive crate | Lightweight PID check | Works but Unix-only |
| std-only approach | Try to bind same port; if PID file exists parse it, try OS signal 0 on Unix / OpenProcess on Windows | Recommended — no new dependency |
| Recommended: file-age heuristic | If PID file exists and sidecar.port responds to /health, sidecar is alive; otherwise stale | Best UX: avoids signal handling entirely |

**Recommendation for stale detection:** Read port file → attempt `GET /health` with 200ms timeout → if connection refused, PID file is stale; clean up and rebind. No new crate needed.

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| axum | hyper directly | More boilerplate, no Router/State ergonomics |
| axum | warp | Less active maintenance, different tower integration |
| clap 4 derive | manual arg parsing | clap adds ~100KB to binary but saves error-prone manual code |
| oneshot for shutdown | watch channel | watch is multi-consumer; oneshot is simpler for single-receiver shutdown |

**Installation:**
```bash
cargo add axum@0.8
cargo add clap@4 --features derive
```

Add to Cargo.toml:
```toml
axum = "0.8"
clap = { version = "4", features = ["derive"] }
```

tokio needs `"sync"` feature for oneshot — add to existing features list:
```toml
tokio = { version = "1.48", features = ["io-std", "macros", "rt-multi-thread", "time", "sync"] }
```

---

## Architecture Patterns

### Recommended Project Structure
```
src/
├── main.rs              # CLI dispatch: parse Cli → route to hook|init|serve
├── sidecar/
│   ├── mod.rs           # pub use; SidecarHandle struct
│   ├── server.rs        # spawn_sidecar(): TcpListener + axum::serve + oneshot shutdown
│   ├── router.rs        # build_router(): Router with all GET endpoints + State<Arc<RwLock<LiveIndex>>>
│   ├── handlers.rs      # outline_handler, impact_handler, symbol_context_handler, repo_map_handler, health_handler
│   └── port_file.rs     # write_port_file(), write_pid_file(), cleanup_files(), check_stale()
├── cli/
│   ├── mod.rs           # Cli struct (Parser), Commands enum (Subcommand)
│   ├── init.rs          # run_init(): reads ~/.claude/settings.json, merges hooks, writes back
│   └── hook.rs          # run_hook(): reads port file, calls sidecar, writes JSON to stdout
└── protocol/            # (existing, unchanged)
```

### Pattern 1: Ephemeral Port Bind + Background Spawn

**What:** Bind sidecar to port 0, OS assigns port, write assigned port to file, spawn as tokio task.
**When to use:** Always for sidecar startup — the only discovery mechanism is the port file.

```rust
// Source: axum docs.rs + confirmed community pattern
use tokio::net::TcpListener;
use tokio::sync::oneshot;

pub struct SidecarHandle {
    pub port: u16,
    pub shutdown_tx: oneshot::Sender<()>,
}

pub async fn spawn_sidecar(
    index: SharedIndex,
    bind_host: &str,
) -> anyhow::Result<SidecarHandle> {
    let addr = format!("{bind_host}:0");
    let listener = TcpListener::bind(&addr).await?;
    let port = listener.local_addr()?.port();

    // Write port and PID files immediately after bind
    port_file::write_port_file(port)?;
    port_file::write_pid_file(std::process::id())?;

    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
    let router = router::build_router(index);

    tokio::spawn(async move {
        axum::serve(listener, router)
            .with_graceful_shutdown(async move {
                let _ = shutdown_rx.await;
            })
            .await
            .expect("sidecar server error");
        // Cleanup on graceful exit
        port_file::cleanup_files();
    });

    Ok(SidecarHandle { port, shutdown_tx })
}
```

### Pattern 2: axum Router with Shared State

**What:** Inject `Arc<RwLock<LiveIndex>>` into axum via `State` extractor — same Arc already in MCP server.
**When to use:** All sidecar endpoint handlers need LiveIndex access.

```rust
// Source: axum docs.rs extract::State
use axum::{extract::{Query, State}, routing::get, Router, Json};
use std::sync::{Arc, RwLock};
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
struct OutlineParams {
    path: String,
}

#[derive(Serialize)]
struct OutlineResponse {
    path: String,
    symbols: Vec<SymbolEntry>,
}

async fn outline_handler(
    State(index): State<SharedIndex>,
    Query(params): Query<OutlineParams>,
) -> Result<Json<OutlineResponse>, StatusCode> {
    let guard = index.read().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    loading_guard_http!(guard)?;
    let symbols = collect_outline(&guard, &params.path);
    drop(guard);
    Ok(Json(OutlineResponse { path: params.path, symbols }))
}

pub fn build_router(index: SharedIndex) -> Router {
    Router::new()
        .route("/health", get(health_handler))
        .route("/outline", get(outline_handler))
        .route("/impact", get(impact_handler))
        .route("/symbol-context", get(symbol_context_handler))
        .route("/repo-map", get(repo_map_handler))
        .with_state(index)
}
```

### Pattern 3: CLI Subcommand Dispatch (clap derive)

**What:** Single binary handles MCP serve (no subcommand = default), `hook <subcommand>`, and `init`.
**When to use:** Entry point in main.rs must dispatch before any async runtime setup.

```rust
// Source: clap docs.rs derive tutorial
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "tokenizor", about = "Tokenizor MCP server")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Install hooks into ~/.claude/settings.json
    Init,
    /// Hook subcommands called by Claude Code PostToolUse hooks
    Hook {
        #[command(subcommand)]
        subcommand: HookSubcommand,
    },
}

#[derive(Subcommand)]
enum HookSubcommand {
    Read,
    Edit,
    Grep,
    SessionStart,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        None => {
            // Default: run MCP server (existing behavior)
            run_mcp_server()
        }
        Some(Commands::Init) => cli::init::run_init(),
        Some(Commands::Hook { subcommand }) => cli::hook::run_hook(subcommand),
    }
}
```

Note: `run_mcp_server()` needs its own `#[tokio::main]` or explicit `tokio::runtime::Builder`. The hook subcommands are sync (read port file, make HTTP request, exit) — they should NOT spawn the full tokio runtime. Use `reqwest::blocking` or hand-roll a sync TCP connection for the HTTP call in hook subcommands to keep binary startup fast.

**Alternative for hook HTTP:** Use `std::net::TcpStream` with manual HTTP/1.1 GET to avoid any async runtime overhead. Given 50ms timeout budget and the simplicity of the requests, a sync approach is fine.

### Pattern 4: Hook Binary Output (HOOK-10)

**What:** Hook binary writes only valid JSON to stdout. All other output (tracing, errors) goes to stderr.
**When to use:** All `tokenizor hook <subcommand>` code paths including error/fail-open paths.

```rust
// Silent fail-open pattern
fn run_hook(subcommand: HookSubcommand) -> anyhow::Result<()> {
    // Read port file — if missing, fail-open silently
    let port = match port_file::read_port() {
        Ok(p) => p,
        Err(_) => {
            // Sidecar not running — output empty additionalContext
            println!("{}", serde_json::json!({
                "hookSpecificOutput": {
                    "hookEventName": "PostToolUse",
                    "additionalContext": ""
                }
            }));
            return Ok(());
        }
    };

    // 50ms HTTP timeout
    let response = call_sidecar(port, subcommand, Duration::from_millis(50));

    let additional_context = match response {
        Ok(text) => text,
        Err(_) => String::new(), // Fail-open: empty context
    };

    println!("{}", serde_json::json!({
        "hookSpecificOutput": {
            "hookEventName": "PostToolUse",
            "additionalContext": additional_context
        }
    }));
    Ok(())
}
```

### Pattern 5: tokenizor init — Idempotent Merge into settings.json

**What:** Read `~/.claude/settings.json`, add/update tokenizor PostToolUse hook entries, write back.
**Idempotency marker:** Hook entries are identified by command string containing a unique marker (e.g., the binary name `tokenizor`).

```rust
// Merge pattern — HIGH confidence (well-established JSON merge)
fn run_init() -> anyhow::Result<()> {
    let settings_path = dirs::home_dir()
        .unwrap()
        .join(".claude")
        .join("settings.json");

    // Read existing (or create empty)
    let mut settings: serde_json::Value = if settings_path.exists() {
        serde_json::from_str(&std::fs::read_to_string(&settings_path)?)?
    } else {
        serde_json::json!({})
    };

    // Discover binary path
    let binary_path = std::env::current_exe()?;

    // Build tokenizor hook entries
    let tokenizor_hooks = build_hook_entries(&binary_path);

    // Merge: remove existing tokenizor entries, add fresh ones
    merge_hooks(&mut settings, tokenizor_hooks);

    // Write back with pretty-printing
    std::fs::create_dir_all(settings_path.parent().unwrap())?;
    std::fs::write(&settings_path, serde_json::to_string_pretty(&settings)?)?;
    Ok(())
}
```

### Anti-Patterns to Avoid

- **Holding RwLockReadGuard across await points:** Extract data into owned values, drop guard, then return — same pattern as existing MCP tool handlers.
- **Writing anything to stdout in the MCP server path:** The existing RELY-04 guard already enforces this; sidecar routes use a separate HTTP server on a different fd.
- **Using Python for hook scripts:** Zero Python dependency is a locked decision. The Rust binary handles everything.
- **Blocking the tokio executor in hook HTTP call:** Hook subcommands are short-lived CLI invocations — they should use sync I/O (no tokio runtime), not `tokio::main`.
- **Using separate per-subcommand binaries:** Distribution stays as a single binary per the locked decision.
- **Writing tracing output to stdout in hook subcommands:** All `tracing::*` calls must be configured to write to stderr in the hook code path. Consider disabling tracing entirely for hook subcommands.

---

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| HTTP server routing | Custom TCP handler | axum 0.8 | Router, extractors, State injection, error handling all provided |
| CLI argument parsing | Manual argv iteration | clap 4 derive | Automatic --help, error messages, subcommand dispatch |
| JSON serialization of responses | Manual string formatting | serde + serde_json | Escaping, correctness, nested structures |
| Ephemeral port detection | Custom port scan | `TcpListener::bind("0:0")` + `local_addr()` | OS guarantees no conflict |
| Settings.json merging | Complex diff algorithm | serde_json `Value` manipulation | Tree structure, easy field-level merge |
| HTTP client for hook binary | Custom TCP GET | `std::net::TcpStream` with minimal HTTP/1.1 | Simple GET, no authentication, fixed timeout — stdlib sufficient |

**Key insight:** The sidecar is a thin data API layer over an already-built LiveIndex. The implementation risk is in the plumbing (port files, PID management, CLI dispatch), not the business logic.

---

## Common Pitfalls

### Pitfall 1: Tracing Output Corrupting Hook JSON
**What goes wrong:** The hook binary (`tokenizor hook read`) initializes tracing to stdout and the tracing output is prepended to the JSON, causing Claude Code JSON parse failure. Claude Code docs warn explicitly about shell profile echo statements causing this.
**Why it happens:** `observability::init_tracing()` is called in main() before the subcommand dispatch; if tracing writes to stdout, any log message corrupts the hook output.
**How to avoid:** In hook subcommand paths, do NOT initialize tracing at all, OR initialize it to stderr only. Simpler: move `init_tracing()` call to after the subcommand dispatch, and skip it for hook subcommands.
**Warning signs:** `jq` fails to parse hook output; Claude Code shows "JSON validation failed" error.

### Pitfall 2: tokio Runtime in Hook Binary
**What goes wrong:** Using `#[tokio::main]` on the binary entry point means the full async runtime is always started, even for `tokenizor hook read` which only needs a synchronous HTTP call.
**Why it happens:** Natural instinct to make `main()` async since the MCP server path requires it.
**How to avoid:** Make `fn main()` synchronous. Only the MCP server path (`None` subcommand) creates a tokio runtime. Hook subcommands use sync I/O.
**Warning signs:** Hook binary startup time > 10ms; tokio worker thread pool visible in profiler during hook invocation.

### Pitfall 3: Port File Race — Sidecar Not Ready When Hooks Fire
**What goes wrong:** Port file is written before axum's `serve()` call completes binding, so a hook that fires during server startup reads the port but gets connection refused.
**Why it happens:** `TcpListener::bind()` succeeds immediately and port is known, but `axum::serve()` hasn't started accepting yet.
**How to avoid:** `TcpListener::bind()` is sufficient — the OS is ready to accept connections as soon as bind() returns, even before serve() is called. The port file is written after `bind()` completes, which is correct.
**Warning signs:** Intermittent "connection refused" errors in hooks during the first few seconds of server startup.

### Pitfall 4: RwLock Held Across Await in Sidecar Handler
**What goes wrong:** Sidecar handler acquires `index.read()` and holds the guard while returning the async response, causing deadlock with watcher's write lock.
**Why it happens:** Axum handlers are async; it's tempting to hold the guard across `.await`.
**How to avoid:** Extract owned data (Vec, String) from the index under the read guard, drop the guard, then return the response. Identical to existing MCP tool handler pattern.
**Warning signs:** Watcher updates stall; health reports watcher as "stuck".

### Pitfall 5: Non-Idempotent settings.json Merge
**What goes wrong:** Running `tokenizor init` twice appends duplicate hook entries, resulting in hooks firing multiple times per tool call.
**Why it happens:** Simple append logic without checking for existing tokenizor entries.
**How to avoid:** When merging, filter out existing entries where command contains "tokenizor" before adding fresh ones. The identification strategy must be consistent.
**Warning signs:** Duplicate additionalContext injections visible in Claude conversation; hook fires twice per file read.

### Pitfall 6: PID File False-Positive (Stale PID Reused)
**What goes wrong:** Process with old tokenizor PID has been replaced by an unrelated process with the same PID. Stale detection incorrectly concludes sidecar is alive.
**Why it happens:** OS reuses PIDs; kill(0) returns 0 even for unrelated processes.
**How to avoid:** Use the health check approach instead: attempt GET /health on the stored port. If the response is valid JSON from the tokenizor sidecar (check a known field), it's alive; otherwise clean up. The port-based check is more reliable than PID-based.
**Warning signs:** Sidecar fails to start after previous unclean exit; port file contains port that's not serving tokenizor.

---

## Code Examples

Verified patterns from official sources:

### Ephemeral Port Bind (axum 0.8)
```rust
// Source: axum docs.rs + confirmed community pattern (HIGH confidence)
// Pattern is identical in axum 0.7 and 0.8
let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
let port = listener.local_addr()?.port();
// Write port to file BEFORE spawning — listener is already bound
std::fs::write(".tokenizor/sidecar.port", port.to_string())?;
tokio::spawn(async move {
    axum::serve(listener, router).await.unwrap();
});
```

### axum Query Extractor with Optional Fields
```rust
// Source: docs.rs/axum/latest/axum/extract/struct.Query.html (HIGH confidence)
#[derive(serde::Deserialize)]
struct OutlineParams {
    path: String,
    limit: Option<usize>,  // optional — missing param → None, not 400
}

async fn outline_handler(
    Query(params): Query<OutlineParams>,
) -> Result<Json<OutlineResponse>, StatusCode> {
    // params.path is required; returns 400 if missing
    // params.limit is Option<usize>; None if not in query string
    todo!()
}
```

### axum IntoResponse for Error Cases
```rust
// Source: docs.rs/axum (HIGH confidence)
use axum::http::StatusCode;

async fn handler(
    State(index): State<SharedIndex>,
    Query(params): Query<OutlineParams>,
) -> Result<Json<OutlineResponse>, StatusCode> {
    let guard = index.read().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if !guard.contains_file(&params.path) {
        return Err(StatusCode::NOT_FOUND);
    }
    // ...
}
```

### Graceful Shutdown with oneshot Channel
```rust
// Source: axum community pattern, multiple sources (MEDIUM confidence)
use tokio::sync::oneshot;

let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();

tokio::spawn(async move {
    axum::serve(listener, router)
        .with_graceful_shutdown(async move {
            let _ = shutdown_rx.await;  // Wait for shutdown signal
        })
        .await
        .unwrap();
    // Cleanup port/PID files after shutdown completes
    let _ = std::fs::remove_file(".tokenizor/sidecar.port");
    let _ = std::fs::remove_file(".tokenizor/sidecar.pid");
});

// Caller holds shutdown_tx; drop it or call send(()) to trigger shutdown
```

### Claude Code hooks Section in settings.json (authoritative format)
```json
// Source: code.claude.com/docs/en/hooks-guide (HIGH confidence — official docs)
{
  "hooks": {
    "PostToolUse": [
      {
        "matcher": "Read",
        "hooks": [
          {
            "type": "command",
            "command": "/path/to/tokenizor hook read",
            "timeout": 5
          }
        ]
      },
      {
        "matcher": "Edit|Write",
        "hooks": [
          {
            "type": "command",
            "command": "/path/to/tokenizor hook edit",
            "timeout": 5
          }
        ]
      },
      {
        "matcher": "Grep",
        "hooks": [
          {
            "type": "command",
            "command": "/path/to/tokenizor hook grep",
            "timeout": 5
          }
        ]
      }
    ],
    "SessionStart": [
      {
        "matcher": "startup|resume",
        "hooks": [
          {
            "type": "command",
            "command": "/path/to/tokenizor hook session-start",
            "timeout": 5
          }
        ]
      }
    ]
  }
}
```

### Hook Binary stdout Output (additionalContext injection)
```json
// Source: code.claude.com/docs/en/hooks (HIGH confidence — official docs)
// Exit code 0 + this JSON on stdout → additionalContext injected into Claude's context
{
  "hookSpecificOutput": {
    "hookEventName": "PostToolUse",
    "additionalContext": "Symbol outline for src/main.rs:\n  fn main() [line 7-75]\n  ..."
  }
}
```

### Fail-Open (no-op) Hook Output
```json
// Source: official docs pattern (HIGH confidence)
// Empty additionalContext — Claude Code processes this as "no additional context"
{
  "hookSpecificOutput": {
    "hookEventName": "PostToolUse",
    "additionalContext": ""
  }
}
```

### clap 4 Nested Subcommands
```rust
// Source: docs.rs/clap derive tutorial (HIGH confidence)
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "tokenizor")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    Init,
    Hook {
        #[command(subcommand)]
        subcommand: HookSubcommand,
    },
}

#[derive(Subcommand)]
enum HookSubcommand {
    Read,
    Edit,
    Grep,
    #[command(name = "session-start")]
    SessionStart,
}
```

---

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| `axum::Server::bind()` | `axum::serve(TcpListener, Router)` | axum 0.7 → 0.8 | Old API removed; new API takes pre-bound TcpListener |
| `#[async_trait]` on extractors | Native RPITIT (return-position impl Trait in traits) | axum 0.8 + Rust 1.75 | No macro needed for custom extractors |
| Route params `/:param` | Route params `/{param}` | axum 0.8 | Path syntax changed; GET with query params unaffected |
| Hooks in separate hooks.json file | Hooks in `hooks` section of settings.json | Claude Code (current) | No separate hooks.json — everything in settings.json |

**Deprecated/outdated:**
- `axum::Server::bind()`: Removed in 0.8. Use `TcpListener::bind()` + `axum::serve()` instead.
- Separate `hooks.json` file: Does not exist in current Claude Code. Hooks live in `settings.json` under the `hooks` key.
- Python hook scripts: Viable but 50ms spawn overhead eliminated by the Rust-binary approach.

---

## Open Questions

1. **SessionStart hook — `additionalContext` vs plain stdout**
   - What we know: For `SessionStart`, the official docs state "any text your command writes to stdout is added to Claude's context." For `PostToolUse`, `hookSpecificOutput.additionalContext` is the structured path.
   - What's unclear: Phase 5 uses `PostToolUse` hooks for Read/Edit/Grep, and `SessionStart` for repo-map. The `SessionStart` hook may simply echo plain text to stdout rather than using `hookSpecificOutput`. Needs verification in Phase 6.
   - Recommendation: Phase 5 implements the binary and sidecar; Phase 6 wires the actual output format. Note this discrepancy in Phase 6 research.

2. **additionalContext for MCP tool PostToolUse (CONFIRMED LIMITATION)**
   - What we know: GitHub issue #24788 confirms `additionalContext` does NOT surface when PostToolUse is triggered by an MCP tool call. It DOES work for native tools (Read, Edit, Grep, Bash).
   - What's unclear: Whether this is fixed in current Claude Code (issue is open as of research date).
   - Recommendation: Phase 5 and 6 target only native tools (Read, Edit, Grep, SessionStart) — this limitation does NOT affect the current phase scope.

3. **`tokenizor init` binary path discovery**
   - What we know: `std::env::current_exe()` returns the path of the running binary.
   - What's unclear: When installed via npm on Windows, the binary path may be wrapped in a node shim. The actual `.exe` path from `current_exe()` should still be correct since Rust binaries are native.
   - Recommendation: Use `current_exe()` as primary; add a warning if the path contains `node_modules` or ends in `.cmd`.

4. **tokio `sync` feature requirement**
   - What we know: `tokio::sync::oneshot` requires the `sync` feature in Cargo.toml.
   - What's unclear: Whether rmcp already enables this transitively.
   - Recommendation: Add `sync` explicitly to the tokio features list; explicit is better than relying on transitive enablement.

---

## Validation Architecture

nyquist_validation is enabled in .planning/config.json.

### Test Framework
| Property | Value |
|----------|-------|
| Framework | Rust built-in test harness + integration tests |
| Config file | Cargo.toml (no separate test config) |
| Quick run command | `cargo test --test sidecar_integration -- --nocapture` |
| Full suite command | `cargo test` |

### Phase Requirements → Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| HOOK-01 | Sidecar binds to OS-assigned port and writes .tokenizor/sidecar.port | integration | `cargo test --test sidecar_integration test_sidecar_binds_ephemeral_port` | ❌ Wave 0 |
| HOOK-01 | /health endpoint responds within 50ms | integration | `cargo test --test sidecar_integration test_health_endpoint_latency` | ❌ Wave 0 |
| HOOK-01 | /outline?path= returns valid JSON | integration | `cargo test --test sidecar_integration test_outline_endpoint` | ❌ Wave 0 |
| HOOK-02 | Sidecar and MCP tools share same Arc<LiveIndex> (verify via mutation) | integration | `cargo test --test sidecar_integration test_shared_index_mutation` | ❌ Wave 0 |
| HOOK-03 | Hook binary read subcommand completes in <100ms end-to-end | integration | `cargo test --test sidecar_integration test_hook_binary_latency` | ❌ Wave 0 |
| HOOK-10 | Hook binary stdout is valid JSON for all subcommands | unit | `cargo test -p tokenizor_agentic_mcp test_hook_output_is_valid_json` | ❌ Wave 0 |
| HOOK-10 | Hook binary fail-open returns valid JSON (sidecar unreachable) | unit | `cargo test -p tokenizor_agentic_mcp test_hook_failopen_valid_json` | ❌ Wave 0 |
| INFR-01 | tokenizor init writes valid PostToolUse hooks to settings.json | integration | `cargo test --test init_integration test_init_writes_hooks` | ❌ Wave 0 |
| INFR-01 | tokenizor init is idempotent (running twice produces same result) | integration | `cargo test --test init_integration test_init_idempotent` | ❌ Wave 0 |
| existing RELY-04 | MCP server stdout purity still holds after sidecar addition | integration | `cargo test --test live_index_integration test_stdout_purity` | ✅ |

Note: INFR-01 is officially Phase 6, but tokenizor init is being built in Phase 5. The planner should include it here.

### Sampling Rate
- **Per task commit:** `cargo test -p tokenizor_agentic_mcp` (unit tests only, ~5s)
- **Per wave merge:** `cargo test` (full suite including integration, ~30s)
- **Phase gate:** Full suite green before `/gsd:verify-work`

### Wave 0 Gaps
- [ ] `tests/sidecar_integration.rs` — covers HOOK-01, HOOK-02, HOOK-03
- [ ] `tests/init_integration.rs` — covers tokenizor init idempotency
- [ ] Unit tests in `src/cli/hook.rs` — covers HOOK-10 fail-open JSON validity
- [ ] tokio features: add `sync` to Cargo.toml before Wave 1

---

## Sources

### Primary (HIGH confidence)
- [axum docs.rs](https://docs.rs/axum/latest/axum/) — Query extractor, State, Router, serve()
- [axum 0.8.0 announcement](https://tokio.rs/blog/2025-01-01-announcing-axum-0-8-0) — Breaking changes, TcpListener API
- [code.claude.com/docs/en/hooks-guide](https://code.claude.com/docs/en/hooks-guide) — Complete hooks schema, settings.json structure, PostToolUse matcher syntax, additionalContext output format (authoritative)
- [code.claude.com/docs/en/hooks](https://code.claude.com/docs/en/hooks) — Detailed reference schema for each event
- [clap docs.rs derive tutorial](https://docs.rs/clap/latest/clap/_derive/_tutorial/index.html) — Subcommand derive patterns

### Secondary (MEDIUM confidence)
- [claudefa.st hooks guide](https://claudefa.st/blog/tools/hooks/hooks-guide) — Confirmed hooks.json schema structure matches official docs
- [Smartscope hooks guide (Feb 2026)](https://smartscope.blog/en/generative-ai/claude/claude-code-hooks-guide/) — Confirmed hooks live in settings.json not a separate file
- [FrancisBourre gist](https://gist.github.com/FrancisBourre/50dca37124ecc43eaf08328cdcccdb34) — hookSpecificOutput structure with additionalContext

### Tertiary (LOW confidence — needs validation)
- Multiple community sources on axum ephemeral port pattern — cross-verified across 3+ sources, treating as MEDIUM
- GitHub issue #24788 (anthropics/claude-code) — additionalContext MCP limitation — direct GitHub issue, HIGH confidence for the limitation claim

---

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH — axum 0.8 and clap 4 are stable, well-documented, actively maintained
- Architecture: HIGH — patterns directly mirror existing watcher/index patterns in the codebase
- Hooks schema: HIGH — fetched from official Claude Code documentation
- Pitfalls: HIGH — tracing/stdout corruption pitfall is directly warned about in official docs; others derived from code patterns
- PID/port file management: MEDIUM — stale detection approach recommended (health check) is pragmatic but not documented anywhere specific

**Research date:** 2026-03-10
**Valid until:** 2026-06-10 (axum/clap APIs stable; Claude Code hooks schema may change faster — re-verify before Phase 6)

**Key constraint noted for Phase 6:** `additionalContext` is confirmed to NOT work for MCP tool PostToolUse. Hooks must target native tools (Read, Edit, Grep) only. Phase 6 planner must verify the additionalContext schema path is still correct in the Claude Code version available at implementation time (open blocker from STATE.md).
