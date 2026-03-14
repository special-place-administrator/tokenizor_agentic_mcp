# Tokenizor MCP

A code-native MCP server that gives AI coding agents structured, symbol-aware access to codebases. Built in Rust with tree-sitter, it replaces raw file scanning with tools that understand code as symbols, references, dependency graphs, and git history — through a single MCP connection.

```bash
npm install -g tokenizor-mcp
```

The installer downloads a platform binary, auto-detects your CLI agents (Claude Code, Codex, Gemini CLI), registers the MCP server, installs hooks, and auto-allows all tools. No manual configuration needed.

## Why Tokenizor

AI coding agents spend most of their token budget on orientation — reading files, grepping for patterns, figuring out what code is where. Tokenizor replaces that with structured tools that resolve symbols, references, and dependencies server-side.

- **Fewer tool calls** — one `get_symbol_context(bundle=true)` returns a symbol's body plus all referenced type definitions, resolved recursively. That's one call instead of reading 3-5 files sequentially.
- **Lower token cost** — structured responses strip boilerplate, returning only what the agent needs. Measured savings below.
- **Better accuracy** — symbol-aware search finds the right code faster than text matching
- **Git intelligence** — churn scores, ownership, and co-change coupling inform which files matter most
- **Server-side edits** — edit tools modify code by symbol name. The agent sends a name and replacement body; the server resolves byte positions, splices, writes atomically, and re-indexes.

## How It Works

Tokenizor maintains a live index of every file in your project. On startup, it parses all source files using tree-sitter grammars (16 languages), extracts symbols (functions, classes, structs, enums, traits, etc.), their byte ranges, and cross-references between them. This index stays current via a file watcher that re-indexes changed files with debouncing.

**Why this is efficient for LLMs:**

Traditional agent workflows look like this:
```
Agent: read file A (4000 tokens)      → finds import
Agent: read file B (6000 tokens)      → finds function signature
Agent: read file C (3000 tokens)      → finds type definition
Agent: grep for callers (2000 tokens) → finds 3 call sites
Total: 4 tool calls, ~15,000 tokens consumed
```

With Tokenizor:
```
Agent: get_symbol_context(name="handler", bundle=true)
Server: resolves symbol + all referenced types from the index
Agent receives: symbol body + type definitions (~800 tokens)
Total: 1 tool call, ~800 tokens consumed
```

The server does the graph traversal, the agent gets a focused answer. The index lookup is O(1) — no file I/O needed for symbol resolution.

**Key architectural decisions:**
- **Symbol-addressed operations** — tools accept symbol names, not file content. The server resolves names to byte ranges via the index, eliminating the need for agents to track positions.
- **Tree-sitter parsing** — deterministic, incremental parsing across 16 languages. Each symbol gets a byte range, line range, and (since v0.21.0) an attached doc comment range.
- **Persistent snapshots** — the index serializes to `.tokenizor/index.bin` for fast restarts (~88ms for a 326-file project).
- **Daemon mode** — multiple terminal sessions share one index via a local loopback daemon. No redundant re-indexing.

## Token Savings — Measured

Every applicable tool response includes a footer showing estimated tokens saved compared to reading the raw file. These are real measurements from Tokenizor's own codebase (326 files, 6805 symbols):

| Operation | Raw file approach | Tokenizor | Savings |
|-----------|------------------|-----------|---------|
| Understand a 5700-line file's structure | `cat` the file: ~67,000 tokens | `get_file_context(sections=['outline'])`: ~200 tokens | **~66,800 tokens saved (99.7%)** |
| Read a function + all its type dependencies | Read 3-5 files: ~15,000 tokens | `get_symbol_context(bundle=true)`: ~800 tokens | **~64,000 tokens saved (98.8%)** |
| Understand a 1600-line file's structure | `cat` the file: ~16,000 tokens | `get_file_context(sections=['outline'])`: ~800 tokens | **~15,200 tokens saved (95%)** |
| Find callers of a function | grep + read enclosing functions: ~5,000 tokens | `get_symbol_context`: ~700 tokens | **~15,300 tokens saved (96%)** |
| Edit a function by name | Read file, find position, send full content: ~5,000 tokens | `replace_symbol_body(name=..., new_body=...)`: ~200 tokens | **~4,800 tokens saved (96%)** |
| Explore a concept | grep + read results + follow imports: ~10,000 tokens | `explore(query=..., depth=2)`: ~1,500 tokens | **~8,500 tokens saved (85%)** |

Savings scale with file size. On large files (5000+ lines), `get_file_context` routinely saves 50,000-70,000 tokens per call. Over a coding session, cumulative savings typically reach 200,000-400,000 tokens.

Token savings are tracked per-session and reported by the `health` tool. Skeptical? Run a session with Tokenizor, check `health` for cumulative savings, then try the same tasks with raw file reads and compare. The numbers speak for themselves on any codebase.

## Tools (24)

### Orientation

| Tool | Purpose |
|------|---------|
| `health` | Index status, file counts, load time, watcher state, session token savings, git temporal status |
| `get_repo_map` | Start here. Adjustable detail: compact overview (~500 tokens), `detail='full'` for complete symbol outline, `detail='tree'` for browsable file tree with symbol counts |
| `explore` | Concept-driven exploration — "how does authentication work?" returns related symbols, patterns, and files. Multi-term queries score symbols by how many terms match. Set `depth=2` for signatures and dependents, `depth=3` for implementations and type chains |

### Reading Code

| Tool | Purpose |
|------|---------|
| `get_file_content` | Read files with line ranges, `around_line`, `around_match`, `around_symbol`, or chunked paging |
| `get_file_context` | Rich file summary: symbol outline, imports, consumers, references, git activity. Use `sections=['outline']` for symbol-only outline |
| `get_symbol` | Look up symbol(s) by file path and name. Single mode or batch mode with `targets[]` array for multiple symbols or byte-range code slices |
| `get_symbol_context` | Three modes: (1) Default — definition + callers + callees + type usages. (2) `bundle=true` — symbol body + all referenced type definitions, resolved recursively. (3) `sections=[...]` — trace analysis with dependents, siblings, implementations, git activity. Supports `verbosity` levels (`signature`, `compact`, `full`) |

### Searching

| Tool | Purpose |
|------|---------|
| `search_symbols` | Find symbols by name, filtered by kind/language/path/scope |
| `search_text` | Full-text search with enclosing symbol context, `group_by` modes, `follow_refs` for inline callers. Auto-corrects double-escaped regex patterns common in LLM tool calls |
| `search_files` | Ranked file path discovery. `changed_with=path` for git co-change coupling. `resolve=true` for exact path resolution from partial hints |

### References and Dependencies

| Tool | Purpose |
|------|---------|
| `find_references` | Two modes: (1) Default — call sites, imports, type usages grouped by file. (2) `mode='implementations'` — trait/interface implementors bidirectionally with `direction` control |
| `find_dependents` | File-level dependency graph — which files import the given file. Supports Mermaid/Graphviz output |
| `inspect_match` | Deep-dive a `search_text` match — full symbol context with callers and type dependencies |

### Git Intelligence

| Tool | Purpose |
|------|---------|
| `what_changed` | Files changed since a timestamp, git ref, or uncommitted. Filter with `path_prefix`, `language`, or `code_only=true` to exclude non-source files |
| `analyze_file_impact` | Re-read a file from disk, update the index, report symbol-level impact. Set `include_co_changes=true` for git temporal coupling data |
| `diff_symbols` | Symbol-level diff between git refs — added, removed, and modified symbols per file. Filter by `language` or `path_prefix` |

### Editing — Single Symbol

| Tool | Purpose |
|------|---------|
| `replace_symbol_body` | Replace a symbol's entire definition by name. Includes attached doc comments. Auto-indents. Reports stale references on signature changes |
| `insert_symbol` | Insert code before or after a named symbol. Set `position='before'` or `'after'` (default). Inserts above doc comments when targeting a documented symbol. Auto-indented |
| `delete_symbol` | Remove a symbol and its attached doc comments entirely by name. Cleans up surrounding blank lines |
| `edit_within_symbol` | Find-and-replace scoped to a symbol's byte range (including doc comments) — won't affect code outside it |

### Editing — Batch Operations

| Tool | Purpose |
|------|---------|
| `batch_edit` | Apply multiple symbol-addressed edits atomically across files. All symbols validated before any writes. Overlap detection includes doc comment ranges |
| `batch_rename` | Rename a symbol and update all references project-wide via the reverse index |
| `batch_insert` | Insert the same code before/after multiple symbols across files |

### Indexing

| Tool | Purpose |
|------|---------|
| `index_folder` | Reindex a directory from scratch. Use when switching projects |

## Edit Tools — How They Work

Edit tools accept **symbol names** instead of raw file content. The server resolves byte positions via the index, splices the new content, writes atomically (temp + rename), and re-indexes the file — all in one tool call.

```
Agent sends:  replace_symbol_body(path="src/auth.rs", name="validate_token", new_body="...")
Server does:  resolve symbol → splice bytes → atomic write → reindex → return summary
Agent gets:   "src/auth.rs — replaced fn `validate_token` (342 → 287 bytes)"
```

**Key behaviors:**
- **Doc comment awareness** — edit operations (replace, delete, insert_before, edit_within) include attached doc comments (`///`, `/** */`, `#`, etc.) in the operation range. Deleting a function also deletes its doc comments. Inserting before a documented function inserts above the doc comments.
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

Doc comment detection per language — `///`, `/** */`, `#`, `@doc` patterns are recognized and attached to their symbols during parsing.

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

Updates `~/.gemini/settings.json`, `~/.gemini/GEMINI.md`. Registers the MCP server as a stdio transport with `trust: true` (bypasses per-tool confirmation prompts) and a 120-second timeout. Writes a guidance block to `GEMINI.md` so Gemini knows to prefer Tokenizor tools for codebase navigation.

**Manual setup** (if auto-init didn't run or you need to reconfigure):

```bash
tokenizor-mcp init --client gemini
```

**Verify inside Gemini CLI:**

```
/mcp
```

You should see `tokenizor — Ready` with 24 tools listed. If the server shows `DISCONNECTED`, check that the binary exists at `~/.tokenizor/bin/tokenizor-mcp` (or `tokenizor-mcp.exe` on Windows).

### Getting the Most Out of Tokenizor

The `init` command writes a guidance block to your agent's system file (`CLAUDE.md`, `AGENTS.md`, or `GEMINI.md`), but CLI agents don't always follow it — they tend to fall back to built-in file reads and grep out of habit. For best results, add the following to your global or per-project system file so your agent treats Tokenizor as the primary code navigation layer:

```markdown
## Tooling Preference

When Tokenizor MCP is available, prefer its tools for repository and code
inspection before falling back to direct file reads.

Use Tokenizor first for:
- symbol discovery
- text/code search
- file outlines and context
- repository outlines
- targeted symbol/source retrieval
- surgical editing (symbol replacements, renames)
- impact analysis (what changed, what breaks)
- inspection of implementation code under `src/`, `tests/`, and similar
  code-bearing directories

Preferred tools for reading:
- `search_text` — full-text search with enclosing symbol context
- `search_symbols` — find symbols by name, kind, language, path
- `search_files` — ranked file path discovery, co-change coupling
- `get_file_context` — rich file summary with outline, imports, consumers
- `get_file_content` — read files with line ranges or around a symbol
- `get_repo_map` — repository overview at adjustable detail levels
- `get_symbol` — look up symbols by name, batch mode supported
- `get_symbol_context` — symbol body + callers + callees + type deps
- `find_references` — call sites, imports, type usages, implementations
- `find_dependents` — file-level dependency graph
- `inspect_match` — deep-dive a search match with full symbol context
- `analyze_file_impact` — re-read file, update index, report impact
- `what_changed` — files changed since timestamp, ref, or uncommitted
- `diff_symbols` — symbol-level diff between git refs
- `explore` — concept-driven exploration across the codebase

Preferred tools for editing:
- `replace_symbol_body` — replace a symbol's entire definition by name
- `edit_within_symbol` — scoped find-and-replace within a symbol's range
- `insert_symbol` — insert code before or after a named symbol
- `delete_symbol` — remove a symbol and its doc comments by name
- `batch_edit` — multiple symbol-addressed edits atomically across files
- `batch_rename` — rename a symbol and update all references project-wide
- `batch_insert` — insert code before/after multiple symbols across files

Default rule:
- use Tokenizor to narrow and target code inspection first
- use direct file reads only when exact full-file source or surrounding
  context is still required after tool-based narrowing
- use Tokenizor editing tools (`replace_symbol_body`, `batch_edit`,
  `edit_within_symbol`) over text-based find-and-replace whenever
  possible to ensure structural integrity and automatic re-indexing

Direct file reads are still appropriate for:
- exact document text in `docs/` or planning artifacts where literal
  wording matters
- configuration files where exact raw contents are the point of inspection

Do not default to broad raw file reads for source-code inspection when
Tokenizor can answer the question more directly.
```

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
| `TOKENIZOR_CB_THRESHOLD` | `0.20` | Parse-failure circuit-breaker threshold (proportion, e.g. 0.20 = 20%) |
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
