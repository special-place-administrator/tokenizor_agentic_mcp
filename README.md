# Tokenizor MCP

Rust-native MCP server for local code indexing, retrieval, prompts, and resources.

The npm package installs the executable as `tokenizor-mcp`. In this repository, the same CLI can be run with `cargo run -- ...`.

## Current Scope

- Local same-machine use.
- MCP over stdio.
- Local daemon mode for shared project/session state across concurrent terminals.
- Automated client setup for Claude Code and Codex.
- Automatic hook-based context enrichment for Claude Code only.
- Standard MCP tools, resources, and prompts for all clients.

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

The repo does not currently install hook-based transparent enrichment for Codex. Codex uses the same backend through MCP tools, resources, prompts, and AGENTS guidance.

### Idempotency

The init flow is tested for repeated runs and for preserving unrelated existing Claude and Codex config.

## Runtime Model

### Stdio startup

When the stdio server starts:

1. If `TOKENIZOR_AUTO_INDEX` is not `false`, Tokenizor tries to discover a project root.
2. If a project root is found, Tokenizor tries to connect to or start a local daemon-backed session for that project.
3. If daemon connection fails, Tokenizor falls back to local in-process mode.
4. If auto-indexing is disabled or no project root is found, Tokenizor starts with an empty index.

### Local daemon

The local daemon is started with:

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

## MCP Surface

The server currently exposes tools, prompts, and resources.

### Tools

Registered tool names:

- `health`
- `index_folder`
- `get_file_outline`
- `get_repo_outline`
- `get_repo_map`
- `get_file_context`
- `get_symbol_context`
- `analyze_file_impact`
- `get_file_tree`
- `get_symbol`
- `get_symbols`
- `get_file_content`
- `search_symbols`
- `search_text`
- `find_references`
- `find_dependents`
- `get_context_bundle`
- `what_changed`

### Prompts

Registered prompt names:

- `code-review`
- `architecture-map`
- `failure-triage`

### Static resources

- `tokenizor://repo/health`
- `tokenizor://repo/outline`
- `tokenizor://repo/map`
- `tokenizor://repo/changes/uncommitted`

### Resource templates

- `tokenizor://file/context?path={path}&max_tokens={max_tokens}`
- `tokenizor://file/content?path={path}&start_line={start_line}&end_line={end_line}`
- `tokenizor://symbol/detail?path={path}&name={name}&kind={kind}`
- `tokenizor://symbol/context?name={name}&file={file}`

## Tool Notes

Current query behavior implemented in the server:

- `search_symbols` supports substring matching and an optional `kind` filter.
- `search_text` supports literal search, multi-term OR via `terms`, and regex mode.
- `find_dependents` prefers concrete non-import symbol usage over import stubs when matching module or namespace evidence exists.
- `find_dependents` includes namespace-aware type-usage matching for C# and Java and module-backed symbol/type usage attribution for files that import the target module.
- Rust grouped `use crate::{...}` imports are expanded during reference extraction.
- `get_file_context` builds its key-reference section from attributed file dependents rather than global bare-name matches.
- `get_context_bundle` returns symbol source plus caller, callee, and type-usage context.
- `what_changed` supports uncommitted git changes, git-ref comparisons, and explicit timestamp mode.

## Supported Languages

Current language extractor modules exist for:

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

## Limitations

- Automated client setup is implemented only for Claude Code and Codex.
- Automatic transparent hook enrichment is implemented only for Claude Code.
- The daemon is local-only; this repo does not implement a multi-machine deployment mode.
- The npm installer ships prebuilt binaries only for the platform list in this README.

## License

MIT
