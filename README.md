# Tokenizor MCP

A code-native MCP server that replaces raw file scanning with structured, symbol-aware code intelligence. Built in Rust with tree-sitter, it gives AI coding agents fast access to symbols, references, dependencies, git history, and impact analysis — all through a single MCP connection.

```bash
npm install -g tokenizor-mcp
```

That's it. The installer downloads the platform binary, auto-detects your CLI agents (Claude Code, Codex, Gemini CLI), registers the MCP server, installs hooks, and auto-allows all tools. No manual configuration needed.

## Why Tokenizor

AI coding agents spend most of their token budget on orientation — reading files, grepping for patterns, and figuring out what code is where. Tokenizor replaces that with structured tools that understand code as symbols, references, and dependency graphs rather than lines of text.

The result:
- **Fewer tool calls** — one `get_context_bundle` replaces 3–5 sequential file reads
- **Lower token cost** — structured responses are 50–90% smaller than raw file content (savings shown on every response)
- **Better accuracy** — symbol-aware search finds the right code faster than text matching
- **Git intelligence** — churn scores, ownership, and co-change coupling inform which files matter most

## Tools (26)

### Orientation — start a session

| Tool | Purpose |
|------|---------|
| `health` | Index status, file counts, load time, watcher state |
| `get_repo_map` | Compact project overview — file counts, language breakdown, directory structure |
| `get_repo_outline` | Full symbol outline of the entire indexed project |
| `get_file_tree` | Browsable source tree with symbol counts per file and directory |
| `explore` | Concept-driven exploration — ask "how does authentication work?" and get a unified overview of related symbols, patterns, and files |

### Reading code

| Tool | Purpose |
|------|---------|
| `get_file_content` | Read files with line ranges, `around_line`, `around_match`, `around_symbol`, or chunked paging |
| `get_file_outline` | Symbol outline for a single file |
| `get_file_context` | Enriched file summary — imports, exports, symbol outline, references, git activity (churn, ownership, co-change coupling) |
| `get_symbol` | Look up a single symbol by name with fuzzy-match suggestions on miss |
| `get_symbols` | Batch symbol lookup or byte-range code slices |
| `get_symbol_context` | Deep context for a symbol — definition, callers, callees, type dependencies |
| `get_context_bundle` | One-call context package — symbol body + all referenced type definitions, resolved recursively to depth 2 |

### Searching

| Tool | Purpose |
|------|---------|
| `search_symbols` | Find symbols by name substring, filtered by kind/language/path |
| `search_text` | Full-text search with enclosing symbol context, `group_by` (symbol/usage), `follow_refs` for inline callers |
| `search_files` | Ranked file path discovery with optional `changed_with` for git co-change coupling |
| `resolve_path` | Exact path resolution from filenames and partial hints |

### References and dependencies

| Tool | Purpose |
|------|---------|
| `find_references` | Grouped reference navigation with enclosing-symbol annotations |
| `find_dependents` | Module-aware import graph — which files depend on this one. Supports Mermaid and Graphviz output |
| `find_implementations` | Trait/interface implementation mapping — bidirectional search across 8 languages |
| `trace_symbol` | One-call semantic investigation — definition, callers, callees, implementations, type dependencies |
| `inspect_match` | Deep-dive a search_text match line — shows full symbol context with callers and type dependencies |

### Git intelligence

| Tool | Purpose |
|------|---------|
| `what_changed` | Files changed since a timestamp, git ref, or uncommitted |
| `analyze_file_impact` | Re-read a file from disk, update the index, and report symbol-level impact |
| `get_co_changes` | Git temporal coupling — co-changing files ranked by Jaccard coefficient, churn scores, ownership |
| `diff_symbols` | Symbol-level diff between git refs — added, removed, and modified symbols for code review |

### Indexing

| Tool | Purpose |
|------|---------|
| `index_folder` | Reload the index from a directory path |

### Token savings

Structured tool responses include a footer showing estimated tokens saved compared to raw file reads. This happens automatically on `get_file_outline`, `get_file_context`, `get_symbol_context`, and `get_context_bundle`.

## Prompts

- `code-review` — structured review prompt for a file or symbol
- `architecture-map` — high-level architecture analysis
- `failure-triage` — systematic failure investigation

## Resources

Static resources:
- `tokenizor://repo/health`
- `tokenizor://repo/outline`
- `tokenizor://repo/map`
- `tokenizor://repo/changes/uncommitted`

Resource templates:
- `tokenizor://file/context?path={path}&max_tokens={max_tokens}`
- `tokenizor://file/content?path={path}&start_line={start_line}&end_line={end_line}&around_line={around_line}&around_match={around_match}&context_lines={context_lines}&show_line_numbers={show_line_numbers}&header={header}`
- `tokenizor://symbol/detail?path={path}&name={name}&kind={kind}`
- `tokenizor://symbol/context?name={name}&file={file}`

## Supported Languages

tree-sitter extractors for: Rust, Python, JavaScript, TypeScript, Go, Java, C, C++, C#, Ruby, PHP, Swift, Perl, Kotlin, Dart, Elixir.

## Installation

Prerequisite: Node.js 18+.

Prebuilt binaries for: Windows x64, Linux x64, macOS arm64, macOS x64.

```bash
npm install -g tokenizor-mcp
```

The installer downloads the platform binary to `~/.tokenizor/bin/`. Set `TOKENIZOR_HOME` to override this location.

Updates work the same way — `npm install -g tokenizor-mcp` replaces the binary. If the binary is locked (active session), it stages a `.pending` update that applies on next launch.

Auto-init runs after every install/update: detects Claude Code (`~/.claude`), Codex (`~/.codex`), and Gemini CLI (`~/.gemini`), registers the MCP server, installs hooks, and auto-allows all Tokenizor tools.

If your platform isn't listed, build from source instead.

## Client Setup

Auto-configured during install. To re-run manually:

```bash
tokenizor-mcp init                  # auto-detect clients
tokenizor-mcp init --client claude  # Claude Code only
tokenizor-mcp init --client codex   # Codex only
tokenizor-mcp init --client gemini  # Gemini CLI only
tokenizor-mcp init --client all     # all clients
```

### Claude Code

Updates `~/.claude.json`, `~/.claude/settings.json`, `~/.claude/CLAUDE.md`. Installs MCP server registration, hook entries (`read`, `edit`, `write`, `grep`, `session-start`, `prompt-submit`), guidance block, and auto-allows all 26 Tokenizor tools.

### Codex

Updates `~/.codex/config.toml`, `~/.codex/AGENTS.md`. Installs MCP server config with timeouts, allowed tools list, and guidance block.

### Gemini CLI

Updates `~/.gemini/settings.json`, `~/.gemini/GEMINI.md`. Installs MCP server registration and guidance block.

## Runtime Model

### Startup

1. If `TOKENIZOR_AUTO_INDEX` is not `false`, Tokenizor discovers a project root
2. Tries to connect to or start a local daemon for shared state across terminals
3. Falls back to local in-process mode if daemon connection fails
4. Starts with an empty index if no project root is found

### Daemon mode

```bash
tokenizor-mcp daemon
```

The daemon binds to local loopback, tracks projects by canonical root, supports multiple concurrent sessions, and persists metadata (`daemon.port`, `daemon.pid`) under `TOKENIZOR_HOME`.

If the daemon becomes unreachable mid-session, the next tool call automatically reconnects or falls back to local in-process mode. Once in degraded mode, no further reconnect attempts are made.

### Hooks and sidecar

Claude Code hook integration uses project-local files under `.tokenizor/` (`sidecar.port`, `sidecar.pid`, `sidecar.session`). Hooks intercept read, edit, write, grep, session-start, and prompt-submit events to enrich responses transparently.

### Persistence

Index snapshots persist at `.tokenizor/index.bin` for fast restarts.

### Parameter handling

All tool parameters accept both native JSON types and stringified values (`"true"` for booleans, `"5"` for numbers) for compatibility with MCP clients that stringify parameters.

## Environment Variables

| Variable | Default | Effect |
|----------|---------|--------|
| `TOKENIZOR_AUTO_INDEX` | `true` | Enables project discovery and startup indexing |
| `TOKENIZOR_CB_THRESHOLD` | `20` | Parse-failure circuit-breaker threshold (percentage) |
| `TOKENIZOR_SIDECAR_BIND` | `127.0.0.1` | Sidecar bind host for local in-process mode |
| `TOKENIZOR_HOME` | `~/.tokenizor` | Home directory for daemon metadata and npm-managed binary |

## Roadmap

Near-term:
- Non-code text lane for JSON, YAML, TOML, Markdown, logs
- Transparent hook-based enrichment for Codex
- `chunk_count_hint` for `get_file_content`

Longer-term goals tracked in `.planning/milestones/`.

## Build From Source

```bash
cargo build --release
cargo test
```

The Cargo package name is `tokenizor_agentic_mcp`.

## Developer Setup

```powershell
# Windows
.\setup.bat --client all

# Unix
bash scripts/setup.sh --client all
```

## Release Process

Managed through `release-please` + GitHub Actions. Details in [docs/release-process.md](docs/release-process.md).

```bash
python execution/release_ops.py guide     # interactive guide
python execution/release_ops.py status    # current state
python execution/release_ops.py preflight # pre-release checks
python execution/version_sync.py check    # version consistency
```

## License

MIT
