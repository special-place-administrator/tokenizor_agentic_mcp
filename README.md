# Tokenizor MCP

In-memory code intelligence for Claude Code. Keeps your entire project indexed in RAM, enriches every Read/Edit/Write/Grep with symbol context via PostToolUse hooks, and saves 80-95% of tokens on code exploration — automatically, with zero behavior change from the model.

## What It Does

Tokenizor runs as an MCP server alongside Claude Code. On startup it indexes your project into an in-memory LiveIndex, then:

- **Read hook** — injects a symbol outline and key references for the file you just read
- **Edit hook** — re-indexes the file and shows callers that may need review (impact analysis)
- **Write hook** — indexes the new file immediately
- **Grep hook** — adds symbol context to matched lines
- **SessionStart hook** — injects a compact repo map (~500 tokens)

All enrichment happens in <100ms via an HTTP sidecar that shares memory with the MCP server. The model never needs to call a special tool — it gets richer context for free.

## Installation

**Prerequisite:** Node.js 18+. No Rust toolchain needed.

Prebuilt binaries: **Windows x64** and **Linux x64**.

### Claude Code

Three commands. Run them in order.

**Step 1 — Install globally**

```bash
npm install -g tokenizor-mcp
```

> **Do NOT use `npx`.** The next step writes the binary's absolute path into your Claude Code settings for hook invocation. `npx` runs from a temporary cache directory that gets cleaned up, which silently breaks your hooks. A global install gives a stable path that survives across sessions.

**Step 2 — Register the MCP server**

```bash
claude mcp add tokenizor -- tokenizor-mcp
```

This tells Claude Code to launch tokenizor as an MCP server on stdio.

**Step 3 — Install hooks**

```bash
tokenizor-mcp init
```

This writes PostToolUse and SessionStart hooks into `~/.claude/settings.json`. The hooks call the tokenizor binary to enrich Read/Edit/Write/Grep results with symbol context automatically.

**Step 4 (optional) — Auto-approve tools**

All tokenizor tools are read-only or local indexing. To skip approval prompts, add this to `~/.claude/settings.json` or your project's `.claude/settings.json`:

```json
{
  "permissions": {
    "allow": ["mcp__tokenizor__*"]
  }
}
```

**Verify it works:**

Start a new Claude Code session in any git repo. You should see:
- No errors on startup (the SessionStart hook fires)
- When you read a file, extra symbol context appears after the file contents

If hooks aren't firing, run `tokenizor-mcp init` again and check that `~/.claude/settings.json` contains hook entries with a stable path (not one containing `_npx` or `npm-cache`).

### Cursor

Add to `.cursor/mcp.json`:

```json
{
  "mcpServers": {
    "tokenizor": {
      "command": "npx",
      "args": ["-y", "tokenizor-mcp"]
    }
  }
}
```

On Windows, use `"command": "cmd"` and `"args": ["/c", "npx", "-y", "tokenizor-mcp"]`.

`npx` is fine for Cursor — MCP servers are launched fresh each session, so there are no persisted hook paths to break.

### Other MCP clients

Standard stdio MCP server:
- **Command:** `tokenizor-mcp` (if installed globally) or `npx -y tokenizor-mcp`
- No environment variables required
- Auto-indexes on startup when `.git` is present in the working directory

### Updating

```bash
npm update -g tokenizor-mcp
tokenizor-mcp init          # re-run to update hook paths if the binary moved
```

### Uninstalling

```bash
npm uninstall -g tokenizor-mcp
```

Then remove the tokenizor entries from `~/.claude/settings.json` (any hook whose command contains `tokenizor`) and run `claude mcp remove tokenizor`.

## MCP Tools (14)

| Tool | Description |
|------|-------------|
| `health` | LiveIndex stats, watcher status, token savings |
| `index_folder` | Trigger full reload of the index |
| `get_file_outline` | Symbol list for a file |
| `get_repo_outline` | File list with coverage stats |
| `get_file_tree` | Directory tree with symbol counts |
| `get_symbol` | Lookup symbol by file + name |
| `get_symbols` | Batch lookup (symbols and code slices) |
| `get_file_content` | Serve file from memory with optional line range |
| `search_symbols` | Substring search with Exact > Prefix > Substring ranking |
| `search_text` | Trigram-accelerated full-text search |
| `find_references` | All call sites for a symbol with context |
| `find_dependents` | Files that import a given file |
| `get_context_bundle` | Full context: symbol + callers + callees + types |
| `what_changed` | Files and symbols modified since timestamp |

## Languages (13)

Full symbol extraction + cross-references:

Rust, Python, JavaScript, TypeScript, Go, Java, C, C++, C#, Ruby, Kotlin, Dart, Elixir

## How It Works

```
┌─────────────┐     stdio      ┌──────────────────┐
│ Claude Code  │◄──────────────►│  MCP Server      │
│             │                │  (14 tools)       │
│  Read file  │                │       │           │
│      │      │                │  ┌────▼────┐      │
│      ▼      │   HTTP <100ms  │  │LiveIndex│      │
│ PostToolUse ├───────────────►│  │  (RAM)  │      │
│   hook      │                │  └────┬────┘      │
│      │      │                │       │           │
│      ▼      │                │  ┌────▼────┐      │
│ +context    │                │  │ Watcher │      │
│ injected    │                │  │ (notify)│      │
└─────────────┘                └──┴─────────┴──────┘
```

1. **Startup**: LiveIndex loads all source files into RAM using tree-sitter parsing. If a serialized snapshot exists, loads from disk in <100ms instead of re-parsing.
2. **File watcher**: notify crate detects changes within 200ms. Content-hash skip prevents redundant reparse.
3. **MCP tools**: Query the LiveIndex with O(1) lookups. All responses are compact human-readable text.
4. **HTTP sidecar**: axum server on ephemeral port, shares `Arc<LiveIndex>` with MCP tools.
5. **Hooks**: Python scripts read stdin JSON, call sidecar, return enrichment as `additionalContext`.
6. **Persistence**: On shutdown, serializes index to disk via postcard. On restart, loads snapshot and verifies in background.

## Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `TOKENIZOR_AUTO_INDEX` | `true` | Auto-index on startup when .git exists |
| `TOKENIZOR_CB_THRESHOLD` | `20` | Circuit breaker: abort if >N% of files fail parsing |
| `TOKENIZOR_SIDECAR_BIND` | `127.0.0.1` | Sidecar bind address |

## Building from Source

Requires [Rust toolchain](https://rustup.rs) (edition 2024).

```bash
cargo build --release
cargo test              # 385+ tests
```

## Tech Stack

- **Rust** (edition 2024) — core engine
- **tree-sitter** 0.26 — parsing and cross-reference extraction for 13 languages
- **rmcp** 1.1 — MCP protocol over stdio
- **tokio** — async runtime
- **axum** 0.8 — HTTP sidecar
- **notify** 8 — file watching with debouncing
- **postcard** 1.1 — index serialization (safe, no RUSTSEC advisories)
- **dashmap** — concurrent HashMap for LiveIndex (via `Arc`)

## License

MIT
