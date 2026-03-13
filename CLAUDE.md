# CLAUDE.md — Tokenizor Agentic MCP

## Git & GitHub CLI

**CRITICAL: The `GITHUB_TOKEN` env var is a limited fine-grained PAT injected by Claude Code. It CANNOT create PRs, trigger workflows, or manage releases.**

Prefix ALL `gh` commands with `GITHUB_TOKEN=` to use the keyring token (has `repo` + `workflow` scopes):

```bash
GITHUB_TOKEN= gh pr create ...
GITHUB_TOKEN= gh workflow run ...
GITHUB_TOKEN= gh run list ...
GITHUB_TOKEN= gh run view ...
```

## Deployment Workflow

**Never manually create tags or specify tag numbers in workflow_dispatch. Release-please handles versioning.**

### Standard release flow:

```
1. Feature branch work complete, tests pass
   cargo test && cargo fmt -- --check

2. Create PR to main
   GITHUB_TOKEN= gh pr create --title "feat: description" --body "..." --base main

3. Merge PR (from UI or CLI)
   GITHUB_TOKEN= gh pr merge <number> --merge

4. Push to main triggers Release workflow automatically:
   a. release-please creates release PR (e.g. "chore(main): release 0.19.0")
   b. Auto-merge merges it using RELEASE_PLEASE_TOKEN (GitHub Actions secret)
   c. Second workflow run: release-please creates tag + GitHub release
   d. Build matrix: windows, linux, macos-arm64, macos-x64
   e. npm publish to registry

5. Monitor
   GITHUB_TOKEN= gh run list -L 5
   GITHUB_TOKEN= gh run view <run-id> --log-failed
```

### workflow_dispatch with tag input:
- ONLY for rebuilding an EXISTING release (tag must already exist)
- Do NOT use this to create new releases

### CI failures:
- `cargo fmt` differences between local and CI are common
- Fix: `cargo fmt && git add -A && git commit -m "style: fix rustfmt formatting" && git push`
- Always run `cargo fmt -- --check` before pushing

## Build & Test

```bash
cargo test --all-targets -- --test-threads=1   # match CI config
cargo fmt -- --check                            # match CI check
cargo check                                     # quick compilation check
```

## Architecture

Rust MCP server providing symbol-aware code navigation and editing tools. Currently 24 tools exposed via MCP `tools/list`, with backward-compat aliases for removed tools in `src/daemon.rs`.

Key source files:
- `src/protocol/tools.rs` — Tool handlers, input structs, tests
- `src/protocol/format.rs` — Output formatters
- `src/daemon.rs` — Daemon proxy with backward-compat aliases
- `src/cli/init.rs` — Tool name list for client init
- `src/live_index/query.rs` — Index query functions
- `src/protocol/resources.rs` — MCP resource handlers

## Tool Consolidation Pattern

When merging tools A into B:
1. Add new params to B's input struct (with `#[serde(default)]`)
2. Add mode branch in B's handler
3. Remove `#[tool]` attribute from A (keep the method for internal use)
4. Add backward-compat alias in `src/daemon.rs` `execute_tool_call`
5. Remove A from `TOKENIZOR_TOOL_NAMES` in `src/cli/init.rs`
6. Update cross-reference descriptions in other tools
7. Update tests: add new field initializers, add mode-specific tests
