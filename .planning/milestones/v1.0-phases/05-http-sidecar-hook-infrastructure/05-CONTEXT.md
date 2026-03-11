# Phase 5: HTTP Sidecar + Hook Infrastructure - Context

**Gathered:** 2026-03-10
**Status:** Ready for planning

<domain>
## Phase Boundary

The axum HTTP sidecar is running on an ephemeral port, reachable by external hook scripts, and `tokenizor init` installs hooks into the Claude Code config in one command. Delivers the infrastructure that Phase 6 hooks will use — the sidecar serves LiveIndex data over HTTP, and the hook binary (subcommand of the main tokenizor binary) calls those endpoints.

Requirements: HOOK-01, HOOK-02, HOOK-03, HOOK-10.

</domain>

<decisions>
## Implementation Decisions

### Sidecar API Surface
- Hooks-only subset: `/outline` (Read hook), `/impact` (Edit hook), `/symbol-context` (Grep hook), `/repo-map` (SessionStart hook), plus `/health`
- JSON response format — Python-free, consumed by the Rust hook binary which formats into additionalContext strings
- GET endpoints with query parameters (e.g., `GET /outline?path=src/main.rs`) — simple, curl-testable
- No full LiveIndex mirror — add more endpoints later if needed

### Hook Script Design
- Rust binary: same binary as MCP server, `tokenizor hook <subcommand>` (e.g., `tokenizor hook read`, `tokenizor hook edit`, `tokenizor hook grep`, `tokenizor hook session-start`)
- Zero Python dependency — eliminates ~50ms spawn overhead per hook invocation
- Silent fail-open: if sidecar unreachable (port file missing, connection refused, timeout), return empty/no-op JSON so Claude Code continues normally
- 50ms hard HTTP timeout — leaves margin within HOOK-03's 100ms total budget given near-zero Rust binary spawn cost

### tokenizor init Scope
- Writes to user-global `~/.claude/hooks.json` (not project-local)
- Merge, don't overwrite: read existing hooks.json, add/update only tokenizor entries (identified by command path marker), leave all other hooks untouched
- Idempotent: running twice produces identical result
- Full setup: creates `.tokenizor/` directory if missing (where `sidecar.port` and `sidecar.pid` will be written at runtime)
- Validates binary path exists and is executable — warns if not found but still writes hooks (binary may be installed later)

### Sidecar Lifecycle
- Starts after LiveIndex load, before MCP stdio serving begins — sidecar ready when first hooks fire
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

</decisions>

<specifics>
## Specific Ideas

- Hook binary is a subcommand of the main tokenizor binary — `tokenizor hook read` not a separate `tokenizor-hook-read` binary. This keeps distribution simple (single npm package, single binary).
- The sidecar is a data API (JSON); hooks own the presentation (formatting into additionalContext strings). Clean separation of concerns.
- Blocker from STATE.md: `additionalContext` JSON schema path varies across Claude Code releases — must verify against live hooks spec before Phase 6 implementation begins. Phase 5 builds the infrastructure; Phase 6 wires the actual hook logic.

</specifics>

<code_context>
## Existing Code Insights

### Reusable Assets
- `src/main.rs`: Current startup sequence (index load → watcher spawn → MCP serve) — sidecar spawn inserts between watcher and MCP serve
- `src/protocol/tools.rs`: MCP tool handlers with `loading_guard!` macro — sidecar endpoint handlers can reuse the same LiveIndex query patterns
- `src/protocol/format.rs`: Compact response formatters — sidecar JSON formatters will be new but can share logic with existing formatters
- `src/live_index/store.rs`: `Arc<RwLock<LiveIndex>>` with all query methods — sidecar handlers take the same Arc clone

### Established Patterns
- `Arc<RwLock<LiveIndex>>` shared ownership — sidecar gets another Arc clone, same pattern as watcher
- `loading_guard!` macro for state checking before queries — sidecar handlers need similar guards
- Compact response formatting in `format.rs` — new JSON formatters follow similar structure
- `tokio::spawn` for background tasks (watcher) — sidecar runs as another spawned task

### Integration Points
- `src/main.rs`: Insert sidecar startup between watcher spawn and MCP serve_server call
- `Cargo.toml`: Add `axum` dependency (+ `tokio` already present with required features)
- New `src/sidecar/` module: axum router, endpoint handlers, port/PID file management
- New `src/cli/` or extended `main.rs`: `tokenizor init` and `tokenizor hook` subcommand routing
- `.tokenizor/`: New runtime directory for `sidecar.port` and `sidecar.pid`

</code_context>

<deferred>
## Deferred Ideas

None — discussion stayed within phase scope

</deferred>

---

*Phase: 05-http-sidecar-hook-infrastructure*
*Context gathered: 2026-03-10*
