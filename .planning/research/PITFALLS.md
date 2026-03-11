# Pitfalls Research

**Domain:** Code intelligence / code indexing MCP server (Rust, tree-sitter, in-memory LiveIndex, PostToolUse hooks)
**Researched:** 2026-03-10
**Confidence:** HIGH — based on prior project post-mortem (v1 over-engineering, jCodeMunch staleness), official Claude Code hooks documentation, existing research in `docs/summaries/research-xref-extraction-and-file-watching.md`, and current-year web sources

---

## Critical Pitfalls

### Pitfall 1: Infrastructure Accumulation (the v1 trap)

**What goes wrong:**
Each new requirement — reliability, checkpointing, audit history, repair — gets solved by adding a new subsystem rather than by simplifying the core. The result is 20,000+ lines of infrastructure that outlives its justification. The original indexing latency problem (seconds, not hours) did not need a run lifecycle, checkpoint/resume, or operational history — those are solutions to problems that never existed at the project's actual scale.

**Why it happens:**
Engineers solve the problem they imagine (enterprise-scale, distributed, long-running jobs) rather than the problem in front of them (index ~70 files, respond in <500ms). Each subsystem feels reasonable in isolation. Collectively they create a maintenance burden that kills velocity and obscures the actual value proposition.

**How to avoid:**
Define hard performance targets before writing any infrastructure: "initial load <500ms for 70 files, single-file re-index <50ms, all queries <1ms from memory." If a proposed subsystem cannot be directly traced to meeting one of those numbers, cut it. The LiveIndex is the entire architecture — HashMap + file watcher + axum sidecar. That is the budget; spend no more.

**Warning signs:**
- A new `struct` is being added to manage the lifecycle of another struct
- A new module is "just coordination" between two existing modules
- Test setup requires more code than the test itself
- Lines-of-infrastructure exceeds lines-of-domain-logic
- PRs touch 5+ files to change one behavior

**Phase to address:**
LiveIndex foundation phase (first phase). Lock the architecture surface area early. Every subsequent phase adds features on top of that fixed foundation, not new layers underneath it.

---

### Pitfall 2: Staleness by Design (the jCodeMunch trap)

**What goes wrong:**
The index is populated at startup and never updated during a session. The model reads a file, edits it, then asks about call sites — but the index still reflects the pre-edit state. Stale answers are worse than no answers because the model acts on them. jCodeMunch suffered from this: data was always stale, indexing sometimes got stuck, and users lost trust in the tool entirely.

**Why it happens:**
File watching feels like an "enhancement" rather than a correctness requirement. Teams ship the happy path (startup indexing works) without investing in the maintenance path (edits during a session are reflected immediately).

**How to avoid:**
AD-4 (file watcher) is not optional — it is part of the correctness contract. Wire `notify` + `notify-debouncer-full` in the same phase as the LiveIndex. The debounce window should be 200-500ms to collapse rapid save bursts. Filter to only source file extensions. Re-index the affected file only (single-file path, not full re-index). Verify with a test: edit a file while the server is running, confirm the symbol appears in the index within 300ms.

**Warning signs:**
- File watcher is described as "Phase 3" or "later enhancement"
- The index has no `last_updated` timestamp per file
- Tests only cover startup state, not post-edit state
- No test exercises the "edit then query" sequence

**Phase to address:**
File watcher phase, immediately after LiveIndex is functional. If LiveIndex ships without file watching, it will be shipped to users without file watching.

---

### Pitfall 3: Hook Latency Blocking the Model

**What goes wrong:**
PostToolUse hooks are synchronous by default — they block Claude's execution until they complete. A hook that takes 200ms on every Read, Edit, and Grep call adds seconds of dead time per conversation turn. Users notice the slowdown before they notice the value, and they disable the hooks.

**Why it happens:**
Hook enrichment logic does more work than necessary: it deserializes the full index, performs fuzzy matching, computes dependency graphs, and formats rich output for every hook invocation — even when the file hasn't changed since the last invocation.

**How to avoid:**
The HTTP sidecar (axum on localhost with port in `.tokenizor/sidecar.port`) must respond in <50ms including network round-trip. This means: (1) all queries are in-memory HashMap lookups, not scans; (2) hook responses are pre-formatted strings, not dynamically computed; (3) use `async: true` on hooks that don't need to block (index updates after Write/Edit don't need to block the model). Only hooks that inject enrichment context (Read outline, Grep symbol context) need synchronous response. Hook execution timeout defaults to 600s — that is not a safety net, it is a footgun. Set explicit timeouts of 2-3s on all hooks. If the sidecar is unreachable, the hook must fail-open (non-blocking error, not crash).

**Warning signs:**
- Hook round-trip >100ms measured with `time` in shell
- Sidecar startup is not tested as an independent unit
- No timeout configured on hooks
- Hook logic performs file I/O or re-parses source on every invocation
- Any hook performs a full-index scan

**Phase to address:**
HTTP sidecar phase. Establish latency budget as a pass/fail criterion before connecting hooks to the sidecar.

---

### Pitfall 4: stdout Pollution Corrupting the MCP Protocol

**What goes wrong:**
The MCP server communicates over stdio using bare JSON lines. Any `println!`, `eprintln!` to stdout, shell profile output, or debug log that reaches stdout corrupts the JSON-RPC stream. Claude Code receives malformed JSON, the tool call fails, and the error message is opaque ("JSON parse error" with no indication of what was logged).

**Why it happens:**
Developers add debug logging during development, forget it is on stdout, and it survives into production. Shell profiles (`.bashrc`, `.profile`) sometimes print text on startup, which appears before the server's first JSON line. This is especially common on Windows/MSYS where Git Bash profiles may print banner text.

**How to avoid:**
All logging must go to stderr or to a file (`.tokenizor/server.log`). The `rmcp` crate uses bare JSON lines on stdio — confirm this at project start and add a test that spawns the server process and validates that stdout contains only valid JSON lines. The `tokenizor init` command that installs hooks should also warn if `.bashrc` / `.profile` contains `echo` statements.

**Warning signs:**
- Server emits any output during startup before the first handshake
- Any `println!` macro exists anywhere in server code paths
- Log framework is configured to write to stdout
- `RUST_LOG` or `tracing` subscriber targets stdout

**Phase to address:**
MCP server hardening phase (early). Add a test as part of CI: spawn server binary, send initialize, verify only JSON on stdout.

---

### Pitfall 5: Symbol Extraction False Positives from Ambiguous AST Patterns

**What goes wrong:**
Tree-sitter queries match syntactic patterns without semantic understanding. A query for `(type_identifier) @ref.type` in TypeScript captures every identifier that appears in a type position — including built-ins (`string`, `number`, `boolean`), generics (`T`, `K`, `V`), and re-exported names. The index fills with noise: thousands of references to `string` that mean nothing for navigation.

Additionally, aliased imports create silent mismatches: `use std::collections::HashMap as Map` — subsequent uses of `Map` won't match `HashMap` by name, so `find_references(HashMap)` silently misses all callsites that imported it as `Map`.

**Why it happens:**
The queries are written to maximize recall (capture everything that could be a reference) without filtering noise. The aliased import problem is invisible during testing if test repos don't use import aliases.

**How to avoid:**
Filter captured names against a per-language blocklist of built-in types and common single-letter generics. For import aliasing: build a per-file alias map at index time (`{alias -> canonical_name}`) and store both the canonical and alias form in the reference record. When answering `find_references`, search both. Explicitly test with repos that use aliased imports (the Rust stdlib uses re-exports extensively; any JS/TS repo uses barrel exports).

**Warning signs:**
- `find_references("string")` in a TypeScript project returns thousands of results
- Single-letter identifiers (`T`, `E`, `K`) appear in symbol lists as type references
- Test coverage does not include a file with `use X as Y` / `import { X as Y }`
- Reference count per file is disproportionately high (>50 per 100 LOC)

**Phase to address:**
Cross-reference extraction phase. The filter and alias-map logic should be designed alongside the query patterns, not retrofitted after.

---

### Pitfall 6: In-Memory Index Unbounded Growth on Large Repos

**What goes wrong:**
A HashMap that holds every symbol, reference, and file content for a repo works fine at 70 files but consumes multi-gigabyte RAM at 10,000+ files. There is no eviction, no cap, and no feedback to the user. The MCP server process grows until the OS kills it or causes system-wide slowdown.

**Why it happens:**
The design targets "repos <10K files fit in RAM" (AD-1) but the implementation has no enforcement mechanism. Repos acquired over time grow past the original target. Monorepos, vendored dependencies, or `node_modules` accidentally included in the watch path can trigger this immediately.

**How to avoid:**
Implement three safeguards: (1) respect `.gitignore` by default and explicitly exclude `node_modules/`, `vendor/`, `target/`, `.git/` — the `ignore` crate already handles this; (2) add a hard file count cap (e.g., 20,000 files) that logs a warning and stops indexing new files rather than crashing; (3) track per-file byte sizes and log when total index memory exceeds a threshold (e.g., 500MB). Persistence serialization (shutdown → restart) also helps: a cold start re-indexes only changed files if the serialized index is within TTL.

**Warning signs:**
- `node_modules/` appears in file counts during indexing
- No test that verifies `.gitignore` exclusion works
- Process RSS grows linearly with no plateau during a full-repo index
- No cap on maximum files indexed

**Phase to address:**
LiveIndex foundation phase. `.gitignore` exclusion is already validated (requirement), but the file count cap and memory guard are implementation details that must be specified before the index is built.

---

### Pitfall 7: Hook Enrichment Inflating Model Context

**What goes wrong:**
`additionalContext` injected via PostToolUse hooks is appended to Claude's context on every matching tool call. If the hook for `Read` injects a 2,000-token outline every time, and the model reads 20 files in a session, the hook adds 40,000 tokens of context the model never explicitly requested. This defeats the stated goal of 80% token savings — the hooks themselves become the token problem.

**Why it happens:**
Hook authors optimize for richness (inject everything useful) rather than relevance (inject only what changes behavior). Outline injection for a file the model already has in context is pure waste.

**How to avoid:**
Hook responses must be compact by design. A file outline after Read should be 5-15 lines of symbol names with line numbers — not full signatures, not docstrings. A Grep enrichment should add only the enclosing symbol name and its containing file, not the entire symbol body. Measure token count of `additionalContext` for representative sessions before shipping. Budget: Read hook <200 tokens, Grep hook <100 tokens, Edit hook <150 tokens. The `additionalContext` field is "added more discretely" than plain stdout according to the hooks spec — but it still costs tokens.

**Warning signs:**
- A hook response exceeds 500 tokens for any single tool call
- The hook injects information already present in the tool's own response
- No measurement of token cost per hook invocation exists
- Hook output is formatted as JSON objects rather than compact human-readable text

**Phase to address:**
Hook integration phase. Define token budgets as acceptance criteria before writing hook output formatters.

---

### Pitfall 8: Re-index on Every File Event Without Debouncing

**What goes wrong:**
A single save operation in most editors generates 3-6 file system events (create temp file, write, rename, delete temp). Without debouncing, the index re-parses the file 3-6 times per save. At 50ms per re-index, that is 150-300ms of CPU work per save. In an active editing session, this is continuous background noise that degrades everything sharing the process.

**Why it happens:**
The file watcher is wired directly to the re-index function. Each event triggers re-index independently. This works in a test where you generate one event, but fails against real editors.

**How to avoid:**
Use `notify-debouncer-full` with a 200-500ms window (the existing research already recommends this). The debouncer collapses the burst of events from a single save into one event. Test with the actual editor used in development: open a real file, save it, measure how many re-index operations actually occur. Target: exactly one re-index per save operation.

**Warning signs:**
- File watcher uses bare `notify` without a debouncer
- Logs show multiple re-index events within 100ms for the same file
- No test that saves a file and counts re-index invocations

**Phase to address:**
File watcher phase. This is a correctness requirement, not an optimization.

---

## Technical Debt Patterns

Shortcuts that seem reasonable but create long-term problems.

| Shortcut | Immediate Benefit | Long-term Cost | When Acceptable |
|----------|-------------------|----------------|-----------------|
| Manual tree walking instead of query-based extraction | Consistent with existing code, no new API to learn | 6x maintenance burden (one walk function per language), harder to extend to new languages | Never in v2 — queries are the correct pattern |
| Storing full file content in the LiveIndex | Simplifies symbol text lookup without file I/O | Memory doubles; cold-start serialization is large | Acceptable as a cache with eviction if byte ranges are the source of truth |
| Blocking hooks everywhere (not using `async: true`) | Simpler mental model | Dead time in every session; users disable hooks | Acceptable only for hooks that inject context the model needs before its next action |
| Skipping the HTTP sidecar, using IPC via files instead | One fewer moving part | Latency penalty from disk I/O per hook; Windows file locking bugs | Never — the HTTP sidecar latency budget is achievable and necessary |
| Full re-index on any file change | Simple implementation | Re-index storm during active editing; 6+ re-indexes per save | Never — debounced incremental re-index is required |
| Ignoring aliased imports in xref resolution | Faster implementation | Silent miss rate in any TS/JS/Rust repo that uses re-exports | Acceptable in MVP if documented as known limitation; must be addressed before GA |
| No hard file count cap | No cap means never saying no to a large repo | OOM on monorepos; no graceful degradation path | Never — the cap prevents crashes and communicates scope |

---

## Integration Gotchas

Common mistakes when connecting to external services.

| Integration | Common Mistake | Correct Approach |
|-------------|----------------|------------------|
| Claude Code PostToolUse hooks | Printing to stdout in the hook script for debugging | All debug output to stderr; only valid JSON to stdout. Hook receives JSON on stdin, must emit JSON on stdout. Claude Code only processes JSON on exit 0. |
| Claude Code PostToolUse hooks | Assuming `decision: "block"` is supported for PostToolUse | PostToolUse can return `decision: "block"` to prompt Claude with a reason — but the tool has already executed. Use it for feedback, not prevention. Prevention requires PreToolUse. |
| Claude Code hooks `additionalContext` | Using the `hookSpecificOutput.additionalContext` path vs. top-level `additionalContext` | PostToolUse uses top-level `additionalContext` field in the JSON output (not nested under `hookSpecificOutput`). Verify against the hooks spec; the nesting pattern differs per event type. |
| axum HTTP sidecar | Hardcoding the port | Use `localhost:0` (OS-assigned) and write the port to `.tokenizor/sidecar.port`. Hook scripts read that file to find the port. Avoids conflicts on machines with multiple tokenizor instances. |
| MCP stdio transport | Any use of `println!` or stdout-targeting tracing | MCP uses bare JSON lines on stdout. Zero non-JSON output allowed. Use stderr or file logging exclusively. |
| `notify` crate on Windows/MSYS | Assuming MSYS path format (`/c/...`) matches what Windows returns | `ReadDirectoryChangesW` returns Windows paths (`C:\...`). Normalize paths in the event handler before comparing with the index's path keys. |
| tree-sitter grammars | Loading grammar shared objects at startup without error handling | Grammar loading can fail if the shared object is missing or ABI-incompatible. Fail gracefully with a clear error, not a panic. |

---

## Performance Traps

Patterns that work at small scale but fail as usage grows.

| Trap | Symptoms | Prevention | When It Breaks |
|------|----------|------------|----------------|
| Linear scan of all symbols for every query | <1ms at 1,000 symbols; 100ms at 500,000 symbols | Index by name (HashMap<String, Vec<SymbolId>>) and by file (HashMap<PathBuf, FileRecord>). Lookups must be O(1). | ~10,000 symbols in index |
| Re-parsing source text for every hook invocation | Hook latency scales with file size, not query complexity | Parse at index time, store byte ranges. Hooks reconstruct text from stored ranges, not by re-parsing. | Files >10KB |
| Watching `node_modules/` or `target/` | Constant stream of events during build; inotify descriptor exhaustion on Linux | Respect `.gitignore` and add explicit exclusion list. `ignore` crate handles this. | Immediately on any JS project or Rust project with `cargo build` running |
| Serializing the full LiveIndex on every shutdown | Acceptable at 70 files; 2-3s at 5,000 files | Serialize asynchronously. Track dirty flag per file; serialize only changed entries in incremental mode. | ~1,000 files |
| Trigram index built synchronously during initial load | Blocks all queries for 500ms+ on large repos | Build trigram index incrementally as files are indexed, or in a background tokio task. | ~500 files |
| Spawning a new re-index task for every file event | Thread/task explosion during directory moves | Coalesce events by file path in a pending queue; a single worker drains the queue | During any git checkout or bulk file operation |

---

## Security Mistakes

Domain-specific security issues beyond general web security.

| Mistake | Risk | Prevention |
|---------|------|------------|
| Following symlinks outside the repo root | A symlink pointing to `/etc/passwd` causes the server to index and expose sensitive system files | Add a canonical path check: resolved path must be prefixed with the repo root. The `ignore` crate's `with_follow_symlinks(false)` option or explicit canonicalization guard. |
| Serving raw file content via MCP tools | An attacker with MCP access can read any file the process can read | Scope all file reads to paths within the indexed repo root. Reject any path that resolves outside the root. |
| Hook script injection via file paths with shell metacharacters | A file named `; rm -rf /` in the repo causes the hook command to execute arbitrary shell commands | Hook commands must not interpolate file paths into shell strings. Pass data via stdin JSON only. The current architecture (stdin JSON → Rust binary → HTTP sidecar) is correct; do not add shell interpolation. |
| Logging source code content to a world-readable log file | Source code containing secrets (API keys, passwords) appears in `.tokenizor/server.log` | Log paths and symbol names only. Never log file content. If content logging is needed for debugging, gate it behind a debug flag and document the risk. |

---

## UX Pitfalls

Common user experience mistakes in this domain.

| Pitfall | User Impact | Better Approach |
|---------|-------------|-----------------|
| Silent indexing failure (parse error on one file stops the whole index) | Model reports missing symbols with no explanation | Per-file circuit breaker: log the error and continue indexing remaining files. Report parse failures via a status tool, not by silently omitting the file. |
| Hook output that duplicates what the model already sees | Model context inflated; no new information added; feels like noise | Before injecting context, check if the information is already in the tool response. Read already returns file content — the hook should add the outline, not repeat the content. |
| `tokenizor init` that only works if run from the repo root | Users run it from subdirectories and get broken hook paths | Detect the repo root (walk up to `.git`), install hooks relative to that root regardless of cwd. |
| No indication that the server is running (silent startup) | Users don't know if the server started or why tools are unavailable | Emit a startup confirmation to stderr (not stdout): server version, repo root, file count, sidecar port. |
| Verbose JSON tool responses | Model wastes tokens parsing fields it never uses | All tool responses use compact human-readable format matching Read/Grep output style. No JSON envelopes in MCP tool results. |

---

## "Looks Done But Isn't" Checklist

Things that appear complete but are missing critical pieces.

- [ ] **LiveIndex:** Startup indexing works, but verify file watcher is active and re-index fires on save (not just at startup)
- [ ] **File watcher:** Events are received, but verify they are debounced (exactly one re-index per save, not 3-6)
- [ ] **Cross-references:** Queries extract call sites, but verify aliased imports are handled and built-ins are filtered
- [ ] **HTTP sidecar:** Sidecar starts and responds, but verify hook latency end-to-end is <100ms (not just sidecar response time)
- [ ] **MCP server:** Server responds to tool calls, but verify zero non-JSON bytes on stdout under all code paths
- [ ] **`tokenizor init`:** Hook JSON is written, but verify idempotency (running twice doesn't duplicate hook entries)
- [ ] **Persistence:** Index serializes on shutdown, but verify deserialization is validated (corrupted file falls back to full re-index, not crash)
- [ ] **`.gitignore` exclusion:** `ignore` crate integration exists, but verify `node_modules/`, `target/`, `.git/` are excluded by actual measurement (count files, compare with `git ls-files`)
- [ ] **Hook enrichment:** Context is injected, but verify token count per hook invocation is within budget (<200 tokens for Read, <100 for Grep)
- [ ] **Windows paths:** Index works on Linux/Mac, but verify path normalization handles `C:\` vs `/c/` vs `/C/` forms on MSYS2/Git Bash

---

## Recovery Strategies

When pitfalls occur despite prevention, how to recover.

| Pitfall | Recovery Cost | Recovery Steps |
|---------|---------------|----------------|
| Infrastructure accumulation caught mid-phase | MEDIUM | Pause the phase, measure lines-of-infrastructure vs. lines-of-domain-logic, delete everything that doesn't directly serve a performance target. Do not refactor — delete. |
| Stale index discovered post-ship | LOW | File watcher can be added as a patch release. Existing architecture supports it. Prioritize immediately — stale data destroys user trust faster than any other failure. |
| Hook latency too high in production | MEDIUM | Profile the sidecar endpoint, find the hot path. Usually: switch from full-index query to per-file lookup; cache hook responses per file keyed by content hash. |
| stdout corruption in MCP server | LOW | Grep the codebase for `println!` and `stdout`. Add a CI test that pipes server output through `jq` and fails on non-JSON lines. |
| Symbol false positives flooding search results | MEDIUM | Add per-language built-in filter list. Already designed for in the architecture — this is a data update, not a structural change. |
| In-memory index OOM on large repo | LOW | Add file count cap and `.gitignore` enforcement if missing. Expose a `--max-files` flag. For immediate relief: add explicit `exclude` patterns in `.tokenizor/config.toml`. |
| Hook enrichment over-inflating context | LOW | Trim `additionalContext` template strings. Token count is visible in Claude Code transcripts. This is a content change, not a structural change. |

---

## Pitfall-to-Phase Mapping

How roadmap phases should address these pitfalls.

| Pitfall | Prevention Phase | Verification |
|---------|------------------|--------------|
| Infrastructure accumulation | LiveIndex foundation | Line count stays under budget; no new lifecycle management structs |
| Staleness by design | File watcher phase (immediately after LiveIndex) | Test: edit file while server runs, query within 300ms, see updated symbol |
| Hook latency blocking model | HTTP sidecar phase | End-to-end hook round-trip measured at <100ms in CI benchmark |
| stdout pollution | MCP server hardening (early) | CI test: spawn server, send initialize, pipe stdout to `jq`, fail on non-JSON |
| Symbol false positives | Cross-reference extraction phase | Test: TypeScript repo query `find_references("string")` returns <10 results |
| Unbounded index growth | LiveIndex foundation | Test: index a project with `node_modules/` present, verify it is excluded |
| Hook enrichment token inflation | Hook integration phase | Measure `additionalContext` byte count in test sessions; fail if >1,000 bytes per call |
| Re-index without debouncing | File watcher phase | Test: save a file once, count re-index operations, assert exactly 1 |

---

## Sources

- Project post-mortem (v1): `PROJECT.md` — "strips 20,000 lines of over-engineered infrastructure"
- Prior art failure: `PROJECT.md` — "jCodeMunch: indexing gets stuck, data always stale"
- Tree-sitter limitations: `docs/summaries/research-xref-extraction-and-file-watching.md` — "Limitations — What Tree-Sitter Cannot Do"
- notify crate gotchas: `docs/summaries/research-xref-extraction-and-file-watching.md` — "Known Limitations and Gotchas"
- Claude Code hooks spec: [Hooks reference — Claude Code Docs](https://code.claude.com/docs/en/hooks) — PostToolUse schema, `additionalContext` format, exit code behavior, timeout defaults
- MCP stdio corruption: [MCP server stdio mode corrupted by stdout log messages](https://github.com/ruvnet/claude-flow/issues/835)
- Hook async behavior: [feat(hooks): Add async: true support](https://github.com/ruvnet/ruflo/issues/1017)
- Stale index patterns: [Indexing code at scale with Glean](https://engineering.fb.com/2024/12/19/developer-tools/glean-open-source-code-indexing/)
- In-memory cache growth: [How to Implement Caching Strategies in Rust](https://oneuptime.com/blog/post/2026-02-01-rust-caching-strategies/view)
- File watcher debouncing: [How to Build a File Watcher with Debouncing in Rust](https://oneuptime.com/blog/post/2026-01-25-file-watcher-debouncing-rust/view)
- Token efficiency in MCP: [SEP-1576: Mitigating Token Bloat in MCP](https://github.com/modelcontextprotocol/modelcontextprotocol/issues/1576)

---
*Pitfalls research for: code intelligence MCP server (Tokenizor v2)*
*Researched: 2026-03-10*
