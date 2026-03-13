# Technology Stack

**Analysis Date:** 2026-03-14

## Languages

**Primary:**
- Rust (Edition 2024) - Core MCP server, all backend logic
- JavaScript/Node.js (18+) - CLI wrapper and installation scripts in `npm/` package
- TypeScript - Not used; codebase is pure Rust

**Secondary:**
- Bash - Build and test scripts
- TOML - Configuration (`Cargo.toml`, `.codex/config.toml`)

## Runtime

**Environment:**
- Tokio (1.48) - Async runtime, multi-threaded
- Node.js 18+ (for npm package installation)

**Package Manager:**
- Cargo (Rust) - Primary build and dependency management
- npm (7.0+) - JavaScript/Node wrapper for npm registry publishing
- Lockfiles: `Cargo.lock` (vendored), `npm/package-lock.json`

## Frameworks

**Core:**
- `rmcp` (1.1.0) - Model Context Protocol (MCP) server framework with stdio transport
- `axum` (0.8) - HTTP web server framework (for HTTP sidecar and daemon)
- `tokio` (1.48) - Async runtime with multi-threading, signals, and time utilities

**Code Parsing & Symbol Analysis:**
- `tree-sitter` (0.26) - Generic parser library
- `tree-sitter-rust` (0.24)
- `tree-sitter-python` (0.25)
- `tree-sitter-javascript` (0.25)
- `tree-sitter-typescript` (0.23.2)
- `tree-sitter-go` (0.25)
- `tree-sitter-java` (0.23.5)
- `tree-sitter-c` (0.24.1)
- `tree-sitter-cpp` (0.23.4)
- `tree-sitter-c-sharp` (0.23.1)
- `tree-sitter-ruby` (0.23.1)
- `tree-sitter-php` (0.24.2)
- `tree-sitter-swift` (0.7.1)
- `tree-sitter-perl` (1.1.2)
- `tree-sitter-kotlin-sg` (0.4.0)
- `tree-sitter-dart` (0.0.4)
- `tree-sitter-elixir` (0.3.5)

**File System & Watching:**
- `notify` (8) - File change notification API
- `notify-debouncer-full` (0.7) - Debounced file watcher
- `ignore` (0.4) - .gitignore-aware traversal
- `globset` (0.4) - Glob pattern matching

**Git Integration:**
- `git2` (0.20) - libgit2 bindings (vendored, no external git binary needed)

**Serialization & Data:**
- `serde` (1.0) with `derive` - Serialization framework
- `serde_json` (1.0) - JSON handling
- `postcard` (1.1) - Compact binary serialization (for index persistence)
- `toml_edit` (0.23) - TOML parsing and editing

**CLI & Observability:**
- `clap` (4) - Command-line argument parsing with derive macros
- `tracing` (0.1) - Structured logging facade
- `tracing-subscriber` (0.3) - Log filtering and output
- `dirs` (6) - Standard directory path resolution

**HTTP & Networking:**
- `reqwest` (0.12) - HTTP client with JSON and rustls-tls support
- `axum::extract::Path`, `Query` - Request extraction

**Error Handling:**
- `anyhow` (1.0) - Flexible error handling and context
- `thiserror` (2.0) - Structured error types with derive macros

**Schema & Reflection:**
- `schemars` (1) - JSON schema generation

**Processing:**
- `rayon` (1.10) - Data parallelism (parallel iterators)
- `regex` (1.11) - Regular expression matching
- `streaming-iterator` (0.1) - Iterator adapters

## Key Dependencies

**Critical:**
- `git2` (0.20 with vendored-libgit2) - Enables in-process git history analysis without spawning child processes
- `tree-sitter` (0.26) + language grammars - Powers symbol extraction across 15+ programming languages
- `rmcp` (1.1.0) - MCP protocol implementation; entire tool surface depends on this
- `axum` (0.8) - HTTP sidecar and daemon control plane routing

**Infrastructure:**
- `tokio` (1.48) - Async runtime used throughout; required for all concurrent operations
- `postcard` (1.1) - Binary serialization for `.tokenizor/index.bin` snapshots
- `notify` (8) + `notify-debouncer-full` (0.7) - File watcher; enables incremental indexing

## Configuration

**Environment Variables:**
Configured via `.env` file (see `.env.example`):
- `TOKENIZOR_CONTROL_PLANE_BACKEND` - Backend selection: `local_registry` (default), `spacetimedb`, or `in_memory`
- `TOKENIZOR_SPACETIMEDB_ENDPOINT` - SpacetimeDB server URL (default: `http://127.0.0.1:3007`)
- `TOKENIZOR_SPACETIMEDB_DATABASE` - Database name (default: `tokenizor`)
- `TOKENIZOR_SPACETIMEDB_MODULE_PATH` - SpacetimeDB module path (default: `spacetime/tokenizor`)
- `TOKENIZOR_SPACETIMEDB_SCHEMA_VERSION` - Schema version (default: `2`)
- `TOKENIZOR_BLOB_ROOT` - Local content-addressable storage root (default: `.tokenizor`)
- `TOKENIZOR_SPACETIMEDB_CLI` - SpacetimeDB CLI binary name (default: `spacetimedb`)
- `TOKENIZOR_REQUIRE_READY_CONTROL_PLANE` - Require readiness checks before serving (default: `true`)
- `TOKENIZOR_AUTO_INDEX` - Auto-index on startup (default: `true`)
- `TOKENIZOR_SIDECAR_BIND` - HTTP sidecar bind address (default: `127.0.0.1`)

**Build Configuration:**
- `Cargo.toml` defines main package and all Rust dependencies
- Rust edition: 2024
- Test framework: built-in Rust `#[test]` attribute
- Feature gates: `v1` (backward-compat gate for retrieval_conformance tests)

## Platform Requirements

**Development:**
- Rust toolchain (current stable)
- Cargo package manager
- Node.js 18+ (for npm package wrapper)
- Git (for repository operations in tests; library uses libgit2)
- Standard dev tools (make, perl — perl ships with Git for Windows)

**Production:**
- Linux, macOS, Windows x86_64 and ARM64 (via multi-platform CI/CD)
- No external runtime beyond what's statically linked into the binary
- SpacetimeDB runtime optional (only if using `spacetimedb` backend)
- Supports both MCP stdio transport and HTTP daemon mode

## Build & Runtime

**Binary Output:**
- Primary: `tokenizor-mcp` (Rust binary) — exposing 24 MCP tools
- Secondary: `tokenizor-mcp` npm package wrapper with postinstall hook
- CLI subcommands: `init`, `daemon`, `hook`

**Execution Modes:**
1. **MCP Stdio (default):** stdin/stdout protocol, single process
2. **Daemon Mode:** long-running HTTP control plane with per-project sessions
3. **Sidecar Mode:** HTTP server alongside MCP stdio for additional querying

---

*Stack analysis: 2026-03-14*
