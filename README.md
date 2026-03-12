# Tokenizor MCP

Rust-native MCP server for same-machine code indexing, retrieval, prompts, and resources.

The executable shipped by npm is `tokenizor-mcp`. In this repository, the same CLI can be run with `cargo run -- ...`.

## What This Is Supposed To Be

Tokenizor is being built first as an agent-acceleration layer for coding work.

The main goal is to let an MCP client stay inside a fast, trustworthy, code-aware working loop for as much of a session as possible:

- less raw file scanning
- faster path and symbol resolution
- better impact analysis after edits
- stronger session continuity and context recovery
- lower token waste on repeated codebase exploration

The direct user benefit is faster turnaround and lower token cost. The direct agent benefit is broader, faster, and more reliable use of the codebase while working. The current implementation is not at the final target yet, but that is the direction the project is intentionally optimizing toward.

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
- `search_text` with literal, OR-term, and regex search plus `path_prefix`, `language`, `limit`, `max_per_file`, `glob`, `exclude_glob`, symmetric `context`, `case_sensitive`, `whole_word`, generated/test suppression, and relevance-ranked results (files sorted by match count rather than alphabetically)
- `get_file_content` with full-file reads, explicit line ranges, optional `show_line_numbers` and `header` for full-file or explicit-range reads, `around_line`, first-match `around_match`, exact-path `around_symbol`, and exact-path line-oriented chunked reads via `chunk_index` plus `max_lines`; error messages show valid parameter combinations for the attempted mode
- exact-selector reference navigation through `find_references`, `get_symbol_context`, and `get_context_bundle` using `path`, symbol kind, and symbol line; `find_references` supports `limit` and `max_per_file` for bounded output
- `find_dependents` with module- and namespace-aware attribution, `limit`/`max_per_file` output bounds, and optional `format` parameter for Mermaid flowchart or Graphviz DOT graph output
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
- `tokenizor://file/content?path={path}&start_line={start_line}&end_line={end_line}&around_line={around_line}&around_match={around_match}&context_lines={context_lines}&show_line_numbers={show_line_numbers}&header={header}`
- `tokenizor://symbol/detail?path={path}&name={name}&kind={kind}`
- `tokenizor://symbol/context?name={name}&file={file}`

Important current caveat:

- the tools have moved ahead of the resources in a few areas; for example, the file-content resource template now matches ordinary full-file, explicit-range, and contextual reads, including `show_line_numbers`, `header`, `around_line`, `around_match`, and `context_lines`, but it still does not expose symbolic or chunked file-content modes
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
- `get_file_content chunk_count_hint`
- a lightweight non-code text lane for JSON, YAML, TOML, Markdown, logs, and similar plain-text files
- transparent hook-based enrichment for Codex
- any multi-machine, remote, or authenticated daemon deployment model

## Current Rough Edges and Open Work

- resource and prompt surfaces still lag some of the newer tool capabilities
- the file-content resource template still trails the tool on `around_symbol` and chunked reads
- `get_file_content` still lacks `chunk_count_hint` and the broader lightweight non-code text lane from the source plan
- current indexing is intentionally code-first; non-code text retrieval remains limited
- long-running run management, checkpointing, repair workflows, and idempotent mutation semantics are still architecture goals rather than shipped features
- the shortest path from file discovery to exact symbol/reference inspection is much better now, but a few workflow helpers still remain on the roadmap

## Read Lane Vs Retrieval Lane

Current `get_file_content` chunking is a deterministic read primitive, not an embedding or semantic-retrieval chunker.

- the shipped chunking contract is exact-path, line-oriented, reproducible paging for inspection and tool follow-up
- it is intentionally stable for prompts like "give me chunk 3 of this file" or "show me lines 301-450"
- it does not use overlap, fuzzy boundaries, or token-sized recursive splitting, because those hurt reproducibility for direct reads

If Tokenizor later adds an embedding-backed text lane or hybrid retrieval layer, that future ingestion path may use a different chunking strategy entirely. For code, the intended order remains:

- symbol- and span-aware navigation first
- deterministic line/range/context reads second
- recursive or embedding-oriented chunking only in a future retrieval lane where recall matters more than exact paging stability

## Why It Is Already Better Than Earlier Revisions

- shell fallback is reduced by `search_files`, `resolve_path`, scoped `search_text`, and contextual `get_file_content`
- search lanes now have scope controls, noise suppression (including inline Rust `#[cfg(test)]` module filtering), and relevance ranking that make larger repositories much more usable
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

If `TOKENIZOR_HOME` is set, the npm wrapper and installer use `$TOKENIZOR_HOME/bin` instead of the default home path.

Update the npm install the same way:

```bash
npm install -g tokenizor-mcp
```

Current updater behavior:

- On Windows, the npm installer first tries to replace the installed binary in place.
- If the installed Tokenizor binary is locked, the installer tries to stop running `tokenizor-mcp.exe` processes that are using the installed binary path, then retries the replacement.
- If replacement is still not possible, the installer stages `tokenizor-mcp.pending.exe` and the wrapper applies it on the next successful launch.
- On every launch, the npm wrapper checks that the installed binary version matches the wrapper package version and reruns the installer automatically if the binary is missing or mismatched.
- On every launch, the client also refuses to reuse a recorded daemon unless its reported version and executable path match the current binary; incompatible daemons are replaced automatically.

Release, publish, and recovery procedure lives in `docs/release-process.md`.
Canonical release tags use plain `vX.Y.Z`.

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
| `TOKENIZOR_HOME` | `~/.tokenizor` | Overrides the Tokenizor home directory used by daemon metadata and the npm-managed binary location (`$TOKENIZOR_HOME/bin`). |

## Build From Source

Rust toolchain required.

Build and test:

```bash
cargo build --release
cargo test
```

The Cargo package name in this repository is `tokenizor_agentic_mcp`.

## Developer Setup

Developer setup scripts now use the current `init` flow instead of printing legacy manual config.

Windows:

```powershell
.\setup.bat --client all
```

Unix:

```bash
bash scripts/setup.sh --client all
```

## Release Process

GitHub releases are now managed through `release-please` plus GitHub Actions.

Operational details live in [docs/release-process.md](docs/release-process.md).

Fresh-terminal operator entrypoint:

```bash
python execution/release_ops.py guide
```

Quick checks:

```bash
python execution/release_ops.py status
python execution/release_ops.py preflight
python execution/version_sync.py check
python execution/version_sync.py current
```

## License

MIT
