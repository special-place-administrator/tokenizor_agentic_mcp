# Codebase Structure

**Analysis Date:** 2026-03-14

## Directory Layout

```
tokenizor_agentic_mcp/
├── src/
│   ├── main.rs                      # Entry point dispatcher (MCP server, daemon, init, hooks)
│   ├── lib.rs                       # Library public exports (modules)
│   ├── cli/                         # Command-line interface and hook handlers
│   │   ├── mod.rs                   # clap parser, subcommand definitions
│   │   ├── init.rs                  # tokenizor init — Claude/Codex/Gemini setup
│   │   └── hook.rs                  # Hook handlers (read, edit, write, grep, session-start, prompt-submit)
│   ├── daemon.rs                    # Daemon state, session management, HTTP project routing
│   ├── discovery/                   # Project-aware file finding
│   │   └── mod.rs                   # find_project_root(), discover_files()
│   ├── domain/                      # Core data models
│   │   └── index.rs                 # LanguageId, SymbolKind, ReferenceKind, FileClassification
│   ├── error.rs                     # anyhow::Result<T>, custom error types
│   ├── git.rs                       # git2 utilities (blame, commit walking)
│   ├── hash.rs                      # Content hashing (MD5-based)
│   ├── live_index/                  # In-memory symbol index
│   │   ├── mod.rs                   # Public exports (store, query, format views)
│   │   ├── store.rs                 # LiveIndex, IndexState, IndexedFile, ParseStatus, circuit breaker
│   │   ├── query.rs                 # Query functions (symbol lookup, references, dependents) + view types
│   │   ├── search.rs                # Full-text search (trigram accelerator)
│   │   ├── trigram.rs               # 3-byte substring indexing for search
│   │   ├── persist.rs               # Snapshot serialization/deserialization to .tokenizor/index.bin
│   │   ├── git_temporal.rs          # Background git history computation (async)
│   ├── observability.rs             # tracing initialization and configuration
│   ├── parsing/                     # Tree-sitter based source parsing
│   │   ├── mod.rs                   # process_file(), FileProcessingResult pipeline
│   │   ├── xref.rs                  # Cross-reference extraction (references, imports, type deps)
│   │   └── languages/               # Per-language symbol extraction (14 languages)
│   │       ├── rust.rs              # Rust: functions, structs, enums, traits, modules, impls
│   │       ├── python.rs            # Python: functions, classes, decorators
│   │       ├── javascript.rs        # JavaScript/JSX: functions, classes, arrow functions
│   │       ├── typescript.rs        # TypeScript: functions, classes, interfaces, types
│   │       ├── go.rs                # Go: functions, types, interfaces
│   │       ├── java.rs              # Java: classes, interfaces, methods
│   │       ├── c.rs                 # C: functions, structs, typedefs
│   │       ├── cpp.rs               # C++: classes, functions, templates
│   │       ├── csharp.rs            # C#: classes, methods, interfaces
│   │       ├── ruby.rs              # Ruby: methods, classes, modules
│   │       ├── php.rs               # PHP: functions, classes, interfaces
│   │       ├── swift.rs             # Swift: functions, classes, structs, enums
│   │       ├── kotlin.rs            # Kotlin: functions, classes, objects
│   │       ├── dart.rs              # Dart: functions, classes
│   │       ├── perl.rs              # Perl: subroutines
│   │       ├── elixir.rs            # Elixir: functions, modules, macros
│   │       └── mod.rs               # Language registry and dispatch
│   ├── protocol/                    # MCP server and tool/prompt handlers
│   │   ├── mod.rs                   # TokenizorServer (rmcp handler), new(), daemon_proxy mode
│   │   ├── tools.rs                 # 24 tool handlers (get_*, search_*, find_*, analyze_*, etc) + input structs
│   │   ├── format.rs                # Output formatters (plain text views for each tool)
│   │   ├── edit.rs                  # Edit operations (symbol replace, batch edit, insert, delete)
│   │   ├── edit_format.rs           # Diff formatting for edit operations
│   │   ├── explore.rs               # File/directory exploration helpers
│   │   ├── resources.rs             # MCP resource handlers (symbol, file, reference URIs)
│   │   ├── prompts.rs               # MCP prompt handlers (context injection templates)
│   ├── sidecar/                     # HTTP proxy for token hooks
│   │   ├── mod.rs                   # SidecarHandle, TokenStats atomic counters, StatsSnapshot
│   │   ├── server.rs                # Axum HTTP server setup and graceful shutdown
│   │   ├── router.rs                # HTTP route definitions (/query, /stats, /health)
│   │   ├── handlers.rs              # HTTP endpoint handlers (forward calls to protocol tools)
│   │   └── port_file.rs             # Port/PID file I/O for client discovery
│   ├── watcher/                     # Filesystem change detection
│   │   └── mod.rs                   # File watcher (notify+debouncer), adaptive debounce, parse+update loop
│   ├── Cargo.toml                   # Dependencies (rmcp, tokio, tree-sitter, axum, etc)
├── .planning/codebase/              # GSD mapping documents (this directory)
│   ├── ARCHITECTURE.md              # This document — pattern, layers, data flow
│   └── STRUCTURE.md                 # Directory layout and file purposes
├── tests/                           # Integration and unit tests
│   ├── test_*.rs                    # Test files for major modules
├── Cargo.lock                       # Pinned dependency versions
├── Cargo.toml                       # Package metadata and dependencies
├── README.md                        # Project overview
├── CHANGELOG.md                     # Release history
├── CLAUDE.md                        # Project-specific Claude instructions
└── .tokenizor/                      # Runtime directory (created during indexing)
    └── index.bin                    # Serialized snapshot (postcard format)
```

## Directory Purposes

**src/:**
- Purpose: All source code — library and binary
- Contains: Rust modules organized by responsibility
- Key files: `main.rs` (entry), `lib.rs` (exports)

**src/cli/:**
- Purpose: Command-line interface implementation
- Contains: Argument parsing, hook script handlers, init flow
- Key files: `mod.rs` (clap parser), `hook.rs` (PostToolUse handlers)

**src/daemon.rs:**
- Purpose: Persistent daemon for cross-session project state
- Contains: Session/project lifecycle, HTTP server, state structures
- Key files: `daemon.rs` (all-in-one, ~500 lines)

**src/discovery/:**
- Purpose: Project root and file discovery
- Contains: `.gitignore`-respecting walk, language detection
- Key files: `mod.rs` (DiscoveredFile, discover_files())

**src/domain/:**
- Purpose: Core data models (shared across all layers)
- Contains: LanguageId, SymbolKind, ReferenceKind, FileClassification
- Key files: `index.rs` (all domain types)

**src/live_index/:**
- Purpose: In-memory symbol index and queries
- Contains: Store, query, search, persistence, git temporal analysis
- Key files:
  - `store.rs`: LiveIndex, IndexState, circuit breaker
  - `query.rs`: All query functions + view types (80+ type definitions)
  - `search.rs`: Full-text search with trigram acceleration
  - `persist.rs`: Snapshot serialization with mtime verification
  - `git_temporal.rs`: Async git history computation

**src/parsing/:**
- Purpose: Tree-sitter parsing and cross-reference extraction
- Contains: Per-language extractors, xref analysis
- Key files:
  - `mod.rs`: process_file() entry point (panic-safe wrapper)
  - `xref.rs`: Reference extraction and import resolution
  - `languages/`: 14 language-specific extractors

**src/protocol/:**
- Purpose: MCP protocol handlers and formatters
- Contains: 24 tools, prompt handlers, resource handlers
- Key files:
  - `tools.rs`: Tool input structs + handlers (largest file, ~1500 lines)
  - `format.rs`: Plain-text output formatters
  - `edit.rs`: Symbol mutation operations
  - `mod.rs`: TokenizorServer (rmcp handler)

**src/sidecar/:**
- Purpose: HTTP proxy for token hook integration
- Contains: Axum HTTP server, token stats, port management
- Key files:
  - `server.rs`: HTTP server lifecycle
  - `handlers.rs`: Route handlers
  - `mod.rs`: TokenStats definition

**src/watcher/:**
- Purpose: Filesystem change detection and incremental index
- Contains: notify+debouncer integration, event batching
- Key files: `mod.rs` (all-in-one, ~350 lines)

## Key File Locations

**Entry Points:**
- `src/main.rs`: Dispatcher for MCP server, daemon, init, hooks

**Configuration:**
- `Cargo.toml`: Dependencies and build config
- `.env.example`: Environment variable template
- `CLAUDE.md`: Project-specific instructions for Claude Code

**Core Logic:**
- `src/live_index/store.rs`: LiveIndex container and circuit breaker
- `src/live_index/query.rs`: All query functions (100+ functions)
- `src/protocol/tools.rs`: MCP tool implementations
- `src/parsing/mod.rs`: Symbol extraction pipeline

**Testing:**
- `tests/`: Integration tests (check git repo for test files)
- Unit tests: Inline in modules (use `#[cfg(test)]` blocks)

## Naming Conventions

**Files:**
- `mod.rs`: Module barrel file (contains `pub mod` declarations and re-exports)
- `*.rs`: Lowercase snake_case for module files
- No prefixes; organize by responsibility

**Directories:**
- Lowercase snake_case (e.g., `live_index`, `src/cli`)
- Group by architectural layer or domain

**Functions:**
- Lowercase snake_case: `find_symbol()`, `process_file()`, `query_references()`
- Tool handlers: `get_symbol()`, `search_symbols()` (mirrors MCP tool names)
- Format functions: `format_symbol()`, `format_references()` (prefix + plural where applicable)

**Types:**
- PascalCase: `LiveIndex`, `SymbolRecord`, `TokenizorServer`
- Enum variants: PascalCase: `ParseStatus::Parsed`, `FileClassification::Test`
- Input structs for tools: Suffixed `Input` (e.g., `GetSymbolInput`, `SearchSymbolsInput`)
- Output view types: Suffixed `View` (e.g., `SymbolDetailView`, `SearchFilesView`)

**Variables:**
- Lowercase snake_case: `indexed_file`, `symbol_count`, `file_path`
- Constants: UPPERCASE_SNAKE_CASE: `BURST_THRESHOLD`, `BASE_MS`
- Lifetimes: Lowercase single letter: `'a`, `'de`

**Modules (in lib.rs):**
- Public modules exported as `pub mod name;`
- Private implementation details marked `pub(crate) mod name;`

## Where to Add New Code

**New Tool:**
- Implementation: `src/protocol/tools.rs` — add `#[tool]` handler method + input struct
- Formatter: `src/protocol/format.rs` — add `format_*()` function
- Export: `src/protocol/mod.rs` — tool auto-registered via `#[tool_router]` macro
- Hook: If exposing via HTTP sidecar, add route in `src/sidecar/handlers.rs`

**New Language Support:**
- Parser: Add `src/parsing/languages/newlang.rs` with symbol extraction
- Registration: Import in `src/parsing/languages/mod.rs` and add to language dispatch
- Domain: If new symbol kinds needed, extend `src/domain/index.rs`
- Tests: Add parsing tests in the language module

**New Query Type:**
- Query function: `src/live_index/query.rs` — implement function that operates on `IndexState`
- View type: `src/live_index/query.rs` — define output `*View` type (serde-enabled for HTTP)
- Tool handler: `src/protocol/tools.rs` — add tool that calls query function
- Formatter: `src/protocol/format.rs` — format view as plain text

**Utility Functions:**
- Shared helpers: `src/live_index/search.rs` or `src/parsing/xref.rs` (by responsibility)
- Domain logic: `src/domain/index.rs`
- Module-specific utils: Keep in the module (don't create utils directories)

**Tests:**
- Unit tests: Inline in module using `#[cfg(test)]` blocks
- Integration tests: In `tests/` directory (checked into git)
- Fixtures/test data: Use `tempfile` crate for temporary directories; no committed test data

## Special Directories

**`.tokenizor/`:**
- Purpose: Runtime directory for index snapshots
- Generated: Yes (created on shutdown or manual indexing)
- Committed: No (in `.gitignore`)
- Contents: `index.bin` (postcard-encoded LiveIndex snapshot)

**.tmp/:**
- Purpose: Temporary test artifacts (execution tests, etc.)
- Generated: Yes
- Committed: No
- Contents: Test repos, generated test outputs

**docs/:**
- Purpose: Design documents, research notes
- Committed: Yes
- Examples: Design specs, architecture docs

**`.claude/`:**
- Purpose: GSD (get-shit-done) automation workflows
- Committed: Yes
- Contents: Agent definitions, skill references, command manifests

## Dependency Organization

All dependencies in `Cargo.toml`:

**Core Protocol & Async:**
- `rmcp`: MCP server framework (stdio + HTTP transports)
- `tokio`: Async runtime (multi-threaded with signals)
- `axum`: HTTP server framework

**Parsing:**
- `tree-sitter`: Parser framework
- 14 `tree-sitter-*` language grammars

**Indexing & Search:**
- `rayon`: Parallel parsing via worksteal
- `postcard`: Binary serialization (snapshots)
- `regex`: Pattern matching for xref extraction
- `ignore`: `.gitignore`-respecting file walker
- `notify`, `notify-debouncer-full`: Filesystem watching

**Git Operations:**
- `git2`: Repository analysis (blame, commits, history)

**CLI & Config:**
- `clap`: Argument parsing
- `serde`: Serialization framework
- `serde_json`: JSON handling (only for sidecar HTTP)
- `toml_edit`: TOML config reading

**Utilities:**
- `dirs`: Standard directory paths
- `tracing`: Structured logging
- `anyhow`, `thiserror`: Error handling
- `schemars`: JSON schema generation (for MCP tool schemas)

---

*Structure analysis: 2026-03-14*
