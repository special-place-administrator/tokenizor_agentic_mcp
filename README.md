# Tokenizor MCP

Rust-native MCP server for same-machine code indexing, retrieval, prompts, and resources.

The executable shipped by npm is `tokenizor-mcp`. In this repository, the same CLI can be run with `cargo run -- ...`.

## Current Reality

Tokenizor is already useful today as a local, code-first MCP. The current implementation provides:

- a stdio MCP server for local clients
- a local daemon mode for shared project/session state across concurrent terminals
- tree-sitter-based symbol extraction across a broad set of programming languages
- hook and sidecar enrichment for Claude Code
- MCP tools, resources, and prompts for Claude Code, Codex, and other stdio MCP clients
- local snapshot persistence and file watching for supported source files

At the time of this README rewrite, `cargo test` is green in this repository.

## What Works Today

### Runtime and setup

- Local same-machine use
- MCP over stdio
- Local daemon mode via `tokenizor-mcp daemon`
- Automatic client setup for Claude Code and Codex via `tokenizor-mcp init`
- Automatic hook registration for Claude Code
- Local snapshot persistence at `.tokenizor/index.bin`
- File watching and incremental re-indexing for supported source files

### Search and navigation gains

- `search_files` for ranked path discovery
- `resolve_path` for exact path resolution from filenames and partial hints
- `search_symbols` with `kind`, `path_prefix`, `language`, `limit`, `include_generated`, and `include_tests`
- `search_text` with literal, OR-term, and regex search plus `path_prefix`, `language`, `limit`, `max_per_file`, `glob`, `exclude_glob`, symmetric `context`, `case_sensitive`, `whole_word`, and generated/test suppression
- `get_file_content` with full-file reads, explicit line ranges, `around_line`, and first-match `around_match`
- exact-selector reference navigation through `find_references`, `get_symbol_context`, and `get_context_bundle` using `path`, symbol kind, and symbol line
- `find_dependents` with module- and namespace-aware attribution
- prompt-submit hook routing that can use file hints, basename/extensionless aliases, module aliases, qualified symbol aliases, and `:line` hints to choose the right file or symbol more reliably

### MCP surface implemented today

Current tools:

- `health`
- `index_folder`
- `get_file_outline`
- `get_repo_outline`
- `get_repo_map`
- `get_file_context`
- `get_symbol_context`
- `analyze_file_impact`
- `search_symbols`
- `search_text`
- `search_files`
- `resolve_path`
- `get_symbol`
- `get_symbols`
- `get_file_content`
- `find_references`
- `find_dependents`
- `get_file_tree`
- `get_context_bundle`
- `what_changed`

Current prompts:

- `code-review`
- `architecture-map`
- `failure-triage`

Current static resources:

- `tokenizor://repo/health`
- `tokenizor://repo/outline`
- `tokenizor://repo/map`
- `tokenizor://repo/changes/uncommitted`

Current resource templates:

- `tokenizor://file/context?path={path}&max_tokens={max_tokens}`
- `tokenizor://file/content?path={path}&start_line={start_line}&end_line={end_line}`
- `tokenizor://symbol/detail?path={path}&name={name}&kind={kind}`
- `tokenizor://symbol/context?name={name}&file={file}`

Important current caveat:

- the tools have moved ahead of the resources in a few areas; for example, `get_file_content` now supports `around_line` and `around_match`, but the file-content resource template still only exposes `start_line` and `end_line`
- exact-selector symbol inputs are implemented on tools, but the symbol-context resource template still exposes only `name` and optional `file`

### Supported languages

Current language extractors exist for:

- Rust
- Python
- JavaScript
- TypeScript
- Go
- Java
- C
- C++
- C#
- Ruby
- PHP
- Swift
- Perl
- Kotlin
- Dart
- Elixir

## What Is Not Implemented Yet

These items are part of the direction for the project, but they are not in the current runtime yet:

- SpacetimeDB as the control plane
- local content-addressed blob storage for raw bytes and large derived artifacts
- `index_repository`, `cancel_index_run`, `checkpoint_now`, and `repair_index`
- `trace_symbol` and `inspect_match`
- `get_file_content` chunking
- `get_file_content around_symbol`
- a general `show_line_numbers` / header contract for ordinary full-file and explicit-range reads
- a lightweight non-code text lane for JSON, YAML, TOML, Markdown, logs, and similar plain-text files
- transparent hook-based enrichment for Codex
- any multi-machine, remote, or authenticated daemon deployment model

## Current Rough Edges and Open Work

- `get_file_content` is much better than before, but it still is not a complete shell replacement for large-file paging until chunking lands
- resource and prompt surfaces still lag some of the newer tool capabilities
- current indexing is intentionally code-first; non-code text retrieval remains limited
- long-running run management, checkpointing, repair workflows, and idempotent mutation semantics are still architecture goals rather than shipped features
- the shortest path from file discovery to exact symbol/reference inspection is much better now, but a few workflow helpers still remain on the roadmap

## Why It Is Already Better Than Earlier Revisions

- shell fallback is reduced by `search_files`, `resolve_path`, scoped `search_text`, and contextual `get_file_content`
- search lanes now have scope controls and noise suppression that make larger repositories much more usable
- exact-selector reference queries avoid many of the common-name collisions that made earlier navigation noisy
- the local daemon lets multiple terminals share one project view instead of rebuilding local state repeatedly
- hook and resource surfaces give the model cheap orientation before expensive tool usage

## Installation

Prerequisite for the npm package: Node.js 18 or newer.

Supported prebuilt npm binaries:

- Windows x64
- Linux x64
- macOS arm64
- macOS x64

Install globally:

```bash
npm install -g tokenizor-mcp
```

The npm installer downloads the platform binary to `~/.tokenizor/bin/tokenizor-mcp` or `~/.tokenizor/bin/tokenizor-mcp.exe`.

Update the npm install the same way:

```bash
npm install -g tokenizor-mcp
```

Current updater behavior:

- On Windows, the npm installer first tries to replace the installed binary in place.
- If the installed Tokenizor binary is locked, the installer tries to stop running `tokenizor-mcp.exe` processes that are using the installed binary path, then retries the replacement.
- If replacement is still not possible, the installer stages `tokenizor-mcp.pending.exe` and the wrapper applies it on the next successful launch.

If your platform is not in the list above, build from source instead.

## CLI

Default invocation starts the stdio MCP server.

Subcommands currently exposed by the CLI:

- `init`
- `daemon`
- `hook`

Current `hook` subcommands:

- `read`
- `edit`
- `write`
- `grep`
- `session-start`
- `prompt-submit`

## Client Initialization

Initialize configured clients:

```bash
tokenizor-mcp init
```

Current client targets:

```bash
tokenizor-mcp init --client claude
tokenizor-mcp init --client codex
tokenizor-mcp init --client all
```

`init` records the absolute path of the executable that is currently running. Run it from the installed binary you intend to keep using.

`init` also creates or reuses the project-local `.tokenizor` directory in the current working directory.

### Claude Code

`tokenizor-mcp init --client claude` updates:

- `~/.claude.json`
- `~/.claude/settings.json`
- `~/.claude/CLAUDE.md`

The Claude setup installs:

- MCP server registration
- hook entries for `read`, `edit`, `write`, `grep`, `session-start`, and `prompt-submit`
- a bounded Tokenizor guidance block in `~/.claude/CLAUDE.md`

### Codex

`tokenizor-mcp init --client codex` updates:

- `~/.codex/config.toml`
- `~/.codex/AGENTS.md`

The Codex setup writes or updates:

- `[mcp_servers.tokenizor]`
- `startup_timeout_sec = 30`
- `tool_timeout_sec = 120`
- `project_doc_fallback_filenames` to ensure `CLAUDE.md` is included
- a bounded Tokenizor guidance block in `~/.codex/AGENTS.md`

Codex currently uses MCP tools, resources, prompts, and AGENTS guidance, but it does not yet get the automatic transparent hook enrichment path that Claude Code gets.

## Runtime Model

### Stdio startup

When the stdio server starts:

1. If `TOKENIZOR_AUTO_INDEX` is not `false`, Tokenizor tries to discover a project root.
2. If a project root is found, Tokenizor tries to connect to or start a local daemon-backed session for that project.
3. If daemon connection fails, Tokenizor falls back to local in-process mode.
4. If auto-indexing is disabled or no project root is found, Tokenizor starts with an empty index.

### Local daemon

Start the local daemon with:

```bash
tokenizor-mcp daemon
```

Current daemon behavior:

- binds to local loopback
- tracks projects by canonical project root
- tracks multiple sessions per project
- serves shared project state across concurrent terminals and clients
- persists daemon metadata under the Tokenizor home directory

Current daemon metadata files:

- `daemon.port`
- `daemon.pid`

`TOKENIZOR_HOME` overrides the default Tokenizor home directory used for daemon metadata.

### Local sidecar files

Hook and sidecar coordination uses project-local files under `.tokenizor`:

- `sidecar.port`
- `sidecar.pid`
- `sidecar.session`

### Persistence

The local runtime can load and save a serialized index snapshot at `.tokenizor/index.bin`.

## Environment Variables

| Variable | Default | Current effect |
| --- | --- | --- |
| `TOKENIZOR_AUTO_INDEX` | `true` | Enables project discovery and startup indexing unless set to `false`. |
| `TOKENIZOR_CB_THRESHOLD` | `20` | Sets the parse-failure circuit-breaker threshold as a percentage. |
| `TOKENIZOR_SIDECAR_BIND` | `127.0.0.1` | Sets the sidecar bind host for local in-process mode. |
| `TOKENIZOR_HOME` | `~/.tokenizor` | Overrides the Tokenizor home directory used by daemon metadata. |

## Build From Source

Rust toolchain required.

Build and test:

```bash
cargo build --release
cargo test
```

The Cargo package name in this repository is `tokenizor_agentic_mcp`.

## License

MIT
