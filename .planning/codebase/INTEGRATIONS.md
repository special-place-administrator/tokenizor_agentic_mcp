# External Integrations

**Analysis Date:** 2026-03-14

## APIs & External Services

**Model Context Protocol (MCP):**
- MCP Server - Exposes 24 tools and resource templates to Claude and other MCP clients
  - SDK/Client: `rmcp` (1.1.0)
  - Transport: stdio (primary), HTTP daemon proxy (secondary)
  - Protocol: JSON-RPC 2.0
  - Files: `src/protocol/` (24 tools, resource handlers, input structs)

**HTTP Services:**
- Internal sidecar HTTP server (localhost only)
  - Framework: `axum` (0.8)
  - Endpoints: `/health`, `/outline`, `/impact`, `/symbol-context`, `/repo-map`, `/prompt-context`, `/stats`
  - Binding: Configurable via `TOKENIZOR_SIDECAR_BIND` env var (default: `127.0.0.1`)
  - Port: Auto-assigned, stored in `.tokenizor/daemon.port` and `tokenizor-sidecar.port` files
  - Authentication: None (localhost only)

**Daemon Control Plane:**
- Shared daemon process (optional, daemon mode)
  - Purpose: Multi-session project index management
  - Client: `reqwest` (0.12) HTTP client
  - Communication: HTTP POST/GET to daemon at `127.0.0.1:port`
  - Features: Session pooling, heartbeat monitoring, project isolation
  - Files: `src/daemon.rs`, daemon HTTP endpoints at root path

## Data Storage

**Local File System Storage:**
- Index persistence: `.tokenizor/index.bin` (binary postcard format)
  - Content: Serialized `IndexSnapshot` with all symbols, references, and metadata
  - Update: On clean shutdown, background verification on load
  - Format: postcard (compact binary) for fast round-trips
  - Atomicity: Writes to temp file, then atomic rename to prevent corruption
  - Files: `src/live_index/persist.rs`

- Content-Addressable Storage (CAS):
  - Root directory: Configurable via `TOKENIZOR_BLOB_ROOT` (default: `.tokenizor`)
  - Purpose: Caches file content at specific git refs for temporal analysis
  - File location: `.tokenizor/blobs/` (relative to project root)

**Git Repository (No External DB):**
- Local git repository access via libgit2
  - No external database required
  - All git history read from `.git/` directory
  - In-process via `git2` crate (no child processes)
  - Supports: git log with stats, file history, uncommitted changes, branch diffs
  - Files: `src/git.rs`

**In-Memory Index:**
- `LiveIndex` (in `src/live_index/store.rs`) - primary working set
  - Holds all parsed symbols, references, file metadata
  - Shared via `Arc<RwLock<LiveIndex>>` for thread-safe access
  - Optional: persisted to `.tokenizor/index.bin` on shutdown

**Optional SpacetimeDB Backend:**
- SpacetimeDB control plane (if `TOKENIZOR_CONTROL_PLANE_BACKEND=spacetimedb`)
  - Connection: `TOKENIZOR_SPACETIMEDB_ENDPOINT` (default: `http://127.0.0.1:3007`)
  - Database: `TOKENIZOR_SPACETIMEDB_DATABASE` (default: `tokenizor`)
  - Schema version: `TOKENIZOR_SPACETIMEDB_SCHEMA_VERSION` (default: `2`)
  - CLI tool: `TOKENIZOR_SPACETIMEDB_CLI` (default: `spacetimedb` binary)
  - Purpose: Durable control plane for daemon sessions (optional; not required for basic operation)

## Authentication & Identity

**Auth Provider:**
- None - Tokenizor is a local development tool
- MCP clients authenticate using their own mechanisms (Claude Code, Codex, etc.)
- HTTP sidecar: localhost-only, no authentication required
- Daemon: Project sessions use auto-generated session IDs and project IDs

**File Access:**
- Direct filesystem read/write to project files
- Respects `.gitignore` for file discovery (via `ignore` crate)
- Reads git history from `.git/` directory

## Monitoring & Observability

**Logging:**
- Framework: `tracing` (0.1) facade with `tracing-subscriber` (0.3) output
- Configuration: `RUST_LOG` environment variable controls log level
- Initialization: `observability::init_tracing()` in `src/main.rs`
- Log output: stderr (JSON or pretty-printed based on environment)
- Includes: Index load status, file watcher events, git temporal computation, daemon session lifecycle

**Error Tracking:**
- None (no external error aggregation service)
- Errors logged via `tracing` and returned to MCP clients in tool responses
- Circuit breaker mechanism in index loading (`src/live_index/store.rs`)

**Health Checks:**
- HTTP endpoint: `/health` (sidecar)
- MCP tool: `tokenizor_health` (checks index readiness, file watcher, git temporal state)
- Daemon heartbeat: 15-second intervals between session and daemon

**Performance Metrics:**
- Token usage tracking in sidecar: `TokenStats` in `src/sidecar/mod.rs`
- Index statistics: file count, symbol count, parsed count, failure count
- Timing: Index load duration, git temporal computation duration
- Accessible via `/stats` sidecar endpoint

## CI/CD & Deployment

**Hosting:**
- GitHub Actions (CI/CD automation)
- Binary release platform: GitHub Releases
- Package registry: npm (Node.js registry) for wrapper package

**CI Pipeline:**
- Platform: GitHub Actions
- Triggers: Push to main, PR to main, manual workflow_dispatch
- Build matrix: Windows (x86_64), Linux (x86_64), macOS (arm64 + x86_64)
- Commands: `cargo test`, `cargo fmt --check`, `cargo build --release`
- Release automation: `release-please` bot creates release PRs, auto-merges, publishes

**Deployment Process:**
- Release tags: Created by `release-please` (semantic versioning)
- Binary artifacts: Built via GitHub Actions matrix
- npm publishing: Automatic via GitHub Actions after tag creation
- Postinstall hook: `npm/scripts/install.js` downloads platform-specific binary

## Environment Configuration

**Required Environment Variables:**
- `RUST_LOG` (optional) - Tracing log level (default: info)
- `TOKENIZOR_CONTROL_PLANE_BACKEND` (optional) - Backend mode (default: `local_registry`)

**Optional Control Plane Variables (if using SpacetimeDB):**
- `TOKENIZOR_SPACETIMEDB_ENDPOINT`
- `TOKENIZOR_SPACETIMEDB_DATABASE`
- `TOKENIZOR_SPACETIMEDB_MODULE_PATH`
- `TOKENIZOR_SPACETIMEDB_SCHEMA_VERSION`
- `TOKENIZOR_SPACETIMEDB_CLI`

**Optional Runtime Variables:**
- `TOKENIZOR_BLOB_ROOT` - CAS root directory
- `TOKENIZOR_AUTO_INDEX` - Enable auto-indexing on startup (default: true)
- `TOKENIZOR_SIDECAR_BIND` - HTTP sidecar bind address (default: 127.0.0.1)
- `TOKENIZOR_REQUIRE_READY_CONTROL_PLANE` - Require readiness before serving (default: true)

**Secrets Location:**
- `.env` file (not committed; see `.env.example` for template)
- Environment variables at runtime (set by MCP client or shell)

## Webhooks & Callbacks

**Incoming Webhooks:**
- None - Tokenizor is a pull-only service

**Outgoing Webhooks:**
- None - No outbound event notifications
- MCP clients receive results via normal RPC responses

**Internal Callbacks:**
- File watcher events: Filesystem changes trigger index updates
  - Framework: `notify` (8)
  - Debouncer: `notify-debouncer-full` (0.7)
  - Handler: `src/watcher/` processes changes and updates live index

- Background git temporal computation: Asynchronous symbol enrichment
  - Spawned: At startup and on index reload
  - Handler: `src/live_index/git_temporal.rs::spawn_git_temporal_computation`
  - Updates: File churn scores, ownership, co-change coupling

- Daemon heartbeat: 15-second keep-alive from session to daemon
  - Handler: `src/main.rs` line 110-116 in `run_remote_mcp_server_async`

## Platform Integration Points

**GitHub:**
- Repository: `special-place-administrator/tokenizor_agentic_mcp`
- CI: GitHub Actions (`.github/workflows/`)
- Release automation: `release-please` bot
- Tokens: `RELEASE_PLEASE_TOKEN` (for auto-merge), `GITHUB_TOKEN` (for CI/CD)

**Development Tools (Local):**
- Git: `.git/` directory (read via libgit2)
- npm: For package publishing
- Rust toolchain: For compilation

---

*Integration audit: 2026-03-14*
