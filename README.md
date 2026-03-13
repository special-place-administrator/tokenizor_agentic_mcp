# Tokenizor MCP

A code-native MCP server that gives AI coding agents structured, symbol-aware access to codebases. Built in Rust with tree-sitter, it replaces raw file scanning with tools that understand code as symbols, references, dependency graphs, and git history — through a single MCP connection.

```bash
npm install -g tokenizor-mcp
```

The installer downloads a platform binary, auto-detects your CLI agents (Claude Code, Codex, Gemini CLI), registers the MCP server, installs hooks, and auto-allows all tools. No manual configuration needed.

## Why Tokenizor

AI coding agents spend most of their token budget on orientation — reading files, grepping for patterns, figuring out what code is where. Tokenizor replaces that with structured tools that resolve symbols, references, and dependencies server-side.

- **Fewer tool calls** — one `get_symbol_context(bundle=true)` replaces 3–5 sequential file reads
- **Lower token cost** — structured responses are 50–90% smaller than raw file content
- **Better accuracy** — symbol-aware search finds the right code faster than text matching
- **Git intelligence** — churn scores, ownership, and co-change coupling inform which files matter most
- **Server-side edits** — 8 edit tools modify code by symbol name, saving 82–99% of tokens vs sending full file content

## Tools (24)

### Orientation

| Tool | Purpose |
|------|---------|
| `health` | Index status, file counts, load time, watcher state, git temporal status |
| `get_repo_map` | Start here. Adjustable detail: compact overview (~500 tokens), `detail='full'` for complete symbol outline, `detail='tree'` for browsable file tree with symbol counts. Includes routing hint for next steps. |
| `explore` | Concept-driven exploration — "how does authentication work?" returns related symbols, patterns, and files. Set `depth=2` for signatures and dependents (~1500 tokens), `depth=3` for implementations and type chains (~3000 tokens). |

### Reading Code

| Tool | Purpose |
|------|---------|
| `get_file_content` | Read files with line ranges, `around_line`, `around_match`, `around_symbol`, or chunked paging |
| `get_file_context` | Rich file summary: symbol outline, imports, consumers, references, git activity. Use `sections=['outline']` for symbol-only outline. |
| `get_symbol` | Look up symbol(s) by file path and name. Single mode or batch mode with `targets[]` array for multiple symbols or byte-range code slices. |
| `get_symbol_context` | Three modes: (1) Default — definition + callers + callees + type usages. (2) `bundle=true` — symbol body + all referenced type definitions, resolved recursively (best for edit prep). (3) `sections=[...]` — comprehensive trace analysis with dependents, siblings, implementations, git activity. |

### Searching

| Tool | Purpose |
|------|---------|
| `search_symbols` | Find symbols by name, filtered by kind/language/path/scope |
| `search_text` | Full-text search with enclosing symbol context, `group_by` modes, `follow_refs` for inline callers |
| `search_files` | Ranked file path discovery. `changed_with=path` for git co-change coupling. `resolve=true` for exact path resolution from partial hints. |

### References and Dependencies

| Tool | Purpose |
|------|---------|
| `find_references` | Two modes: (1) Default — call sites, imports, type usages grouped by file. (2) `mode='implementations'` — trait/interface implementors bidirectionally with `direction` control. |
| `find_dependents` | File-level dependency graph — which files import the given file. Supports Mermaid/Graphviz output. |
| `inspect_match` | Deep-dive a `search_text` match — full symbol context with callers and type dependencies |

### Git Intelligence

| Tool | Purpose |
|------|---------|
| `what_changed` | Files changed since a timestamp, git ref, or uncommitted. Filter with `path_prefix`, `language`, or `code_only=true` to exclude non-source files. |
| `analyze_file_impact` | Re-read a file from disk, update the index, report symbol-level impact. Set `include_co_changes=true` for git temporal coupling data. |
| `diff_symbols` | Symbol-level diff between git refs — added, removed, and modified symbols per file. Filter by `language` or `path_prefix`. |

### Editing — Single Symbol

| Tool | Purpose |
|------|---------|
| `replace_symbol_body` | Replace a symbol's entire definition by name. Auto-indents. Reports stale references on signature changes. |
| `insert_symbol` | Insert code before or after a named symbol. Set `position='before'` or `'after'` (default). Auto-indented. |
| `delete_symbol` | Remove a symbol entirely by name. Cleans up surrounding blank lines. |
| `edit_within_symbol` | Find-and-replace scoped to a symbol's byte range — won't affect code outside it. |

### Editing — Batch Operations

| Tool | Purpose |
|------|---------|
| `batch_edit` | Apply multiple symbol-addressed edits atomically across files. All symbols validated before any writes. |
| `batch_rename` | Rename a symbol and update all references project-wide via the reverse index. |
| `batch_insert` | Insert the same code before/after multiple symbols across files. |

### Indexing

| Tool | Purpose |
|------|---------|
| `index_folder` | Reindex a directory from scratch. Use when switching projects. |

### Token Savings

Structured responses include a footer showing estimated tokens saved compared to raw file reads. Automatic on `get_file_context`, `get_symbol_context`, and `get_symbol_context(bundle=true)`. All applicable tools support `verbosity` levels (`signature`, `compact`, `full`).

## Edit Tools — How They Work

Edit tools accept **symbol names** instead of raw file content. The server resolves byte positions via the index, splices the new content, writes atomically (temp + rename), and re-indexes the file — all in one tool call.

```
Agent sends:  replace_symbol_body(path="src/auth.rs", name="validate_token", new_body="...")
Server does:  resolve symbol → splice bytes → atomic write → reindex → return summary
Agent gets:   "src/auth.rs — replaced fn `validate_token` (342 → 287 bytes)"
```

**Key behaviors:**
- **Auto-indentation** — new code is indented to match the target symbol's context
- **Disambiguation** — use `kind` and `symbol_line` when multiple symbols share a name
- **Stale warnings** — `replace_symbol_body` detects signature changes and lists affected callers
- **Atomic batches** — `batch_edit` validates all symbols before writing anything; overlapping edits are rejected

## Prompts

| Prompt | Purpose |
|--------|---------|
| `tokenizor-review` | Structured code review plan using Tokenizor context surfaces |
| `tokenizor-architecture` | Architecture mapping plan using repo-level context and cross-reference tools |
| `tokenizor-triage` | Debugging and failure-triage plan using health, changed files, and local context |

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

Tree-sitter extractors for 16 languages: Rust, Python, JavaScript, TypeScript, Go, Java, C, C++, C#, Ruby, PHP, Swift, Kotlin, Dart, Perl, Elixir.

## Installation

**Prerequisite:** Node.js 18+

**Prebuilt binaries:** Windows x64, Linux x64, macOS arm64, macOS x64

```bash
npm install -g tokenizor-mcp
```

The installer downloads the platform binary to `~/.tokenizor/bin/`. Set `TOKENIZOR_HOME` to override.

**Updates** work the same way — `npm install -g tokenizor-mcp` replaces the binary. If the binary is locked (active session), it stages a `.pending` update that applies on next launch.

**Auto-init** runs after every install/update: detects Claude Code, Codex, and Gemini CLI, registers the MCP server, installs hooks, and auto-allows all Tokenizor tools.

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

Updates `~/.claude.json`, `~/.claude/settings.json`, `~/.claude/CLAUDE.md`. Installs MCP server registration, hook entries (`read`, `edit`, `write`, `grep`, `session-start`, `prompt-submit`), guidance block, and auto-allows all 24 Tokenizor tools.

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

### Daemon Mode

```bash
tokenizor-mcp daemon
```

The daemon binds to local loopback, tracks projects by canonical root, supports multiple concurrent sessions, and persists metadata (`daemon.port`, `daemon.pid`) under `TOKENIZOR_HOME`.

If the daemon becomes unreachable mid-session, the next tool call automatically reconnects or falls back to local in-process mode.

### Hooks and Sidecar

Claude Code hook integration uses project-local files under `.tokenizor/` (`sidecar.port`, `sidecar.pid`, `sidecar.session`). Hooks intercept read, edit, write, grep, session-start, and prompt-submit events to enrich responses transparently.

### Persistence

Index snapshots persist at `.tokenizor/index.bin` for fast restarts.

### Parameter Handling

All tool parameters accept both native JSON types and stringified values (`"true"` for booleans, `"5"` for numbers) for compatibility with MCP clients that stringify parameters.

## Environment Variables

| Variable | Default | Effect |
|----------|---------|--------|
| `TOKENIZOR_AUTO_INDEX` | `true` | Enables project discovery and startup indexing |
| `TOKENIZOR_CB_THRESHOLD` | `20` | Parse-failure circuit-breaker threshold (percentage) |
| `TOKENIZOR_SIDECAR_BIND` | `127.0.0.1` | Sidecar bind host for local in-process mode |
| `TOKENIZOR_HOME` | `~/.tokenizor` | Home directory for daemon metadata and npm-managed binary |

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
