# Tokenizor Superiority Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make Tokenizor categorically superior to grep/glob for every common code navigation task, add Gemini CLI support, and auto-allow all Tokenizor tools so users never see permission prompts.

**Architecture:** Eight independent workstreams: (1) symbol-aware context in search_text, (2) git temporal coupling in search_files, (3) semantic dedup/group_by in search_text, (4) follow_refs in search_text, (5) explore tool, (6) Gemini CLI integration, (7) auto-allow/YOLO permissions for all CLIs, (8) commit bug fixes from current session. Each workstream modifies distinct files and can be parallelized.

**Tech Stack:** Rust, tree-sitter (existing), clap (existing CLI), serde_json/toml_edit (config), rmcp (MCP protocol)

---

## Chunk 1: Bug Fixes + Auto-Allow + Gemini CLI

### Task 1: Commit Current Bug Fixes

The following fixes are already implemented and tested in the working tree:
- `index_folder` split-brain: proxy now updates `repo_root` after daemon rebind
- `search_symbols("")` empty query guard
- `inspect_match` out-of-bounds line returns clear error

**Files:**
- Modified: `src/protocol/tools.rs` (3 changes)
- Modified: `src/protocol/format.rs` (LineOutOfBounds arm)
- Modified: `src/live_index/query.rs` (LineOutOfBounds variant + early return)

- [ ] **Step 1: Run full test suite**

Run: `cargo test --all-targets -- --test-threads=1`
Expected: All tests pass

- [ ] **Step 2: Run cargo fmt check**

Run: `cargo fmt --all --check`
Expected: No diffs

- [ ] **Step 3: Commit**

```bash
git checkout -b feat/tokenizor-superiority
git add src/protocol/tools.rs src/protocol/format.rs src/live_index/query.rs
git commit -m "fix: split-brain after index_folder, empty search_symbols guard, inspect_match bounds check"
```

---

### Task 2: Auto-Allow Tokenizor Tools (YOLO Permissions)

Users are annoyed by constant permission prompts. All Tokenizor tools are read-only (except `index_folder` which only reads files into memory) — they're safe to auto-allow.

**Claude Code:** Add `"allowedTools"` array to `~/.claude/settings.json` during `tokenizor init`.
**Codex:** Add `allow` list to `~/.codex/config.toml` under the `[mcp_servers.tokenizor]` table.
**Gemini:** Will be handled in Task 3.

**Files:**
- Modify: `src/cli/init.rs:162` (`merge_tokenizor_hooks` — add allowedTools merge)
- Modify: `src/cli/init.rs:297` (`register_codex_mcp_server` — add allow list)
- Test: `src/cli/init.rs` (existing test module)

- [ ] **Step 1: Write failing test for Claude allowedTools**

In the test module of `src/cli/init.rs`, add:

```rust
#[test]
fn test_merge_adds_allowed_tools() {
    let mut settings = json!({});
    merge_tokenizor_hooks(&mut settings, "/usr/bin/tokenizor-mcp");
    let allowed = settings["allowedTools"].as_array().expect("allowedTools should be array");
    assert!(allowed.iter().any(|v| v.as_str() == Some("mcp__tokenizor__search_symbols")),
        "should include search_symbols, got: {allowed:?}");
    assert!(allowed.iter().any(|v| v.as_str() == Some("mcp__tokenizor__get_symbol")),
        "should include get_symbol");
    // Should not duplicate on re-run
    merge_tokenizor_hooks(&mut settings, "/usr/bin/tokenizor-mcp");
    let allowed2 = settings["allowedTools"].as_array().unwrap();
    assert_eq!(allowed.len(), allowed2.len(), "should not duplicate entries");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --lib test_merge_adds_allowed_tools`
Expected: FAIL — `allowedTools` not set

- [ ] **Step 3: Implement Claude allowedTools in merge_tokenizor_hooks**

In `src/cli/init.rs`, add a new function and call it from `merge_tokenizor_hooks`:

```rust
const TOKENIZOR_TOOL_NAMES: &[&str] = &[
    "mcp__tokenizor__health",
    "mcp__tokenizor__index_folder",
    "mcp__tokenizor__get_file_outline",
    "mcp__tokenizor__get_file_content",
    "mcp__tokenizor__get_file_tree",
    "mcp__tokenizor__get_symbol",
    "mcp__tokenizor__get_symbols",
    "mcp__tokenizor__get_repo_outline",
    "mcp__tokenizor__get_repo_map",
    "mcp__tokenizor__get_file_context",
    "mcp__tokenizor__get_symbol_context",
    "mcp__tokenizor__get_context_bundle",
    "mcp__tokenizor__search_symbols",
    "mcp__tokenizor__search_text",
    "mcp__tokenizor__search_files",
    "mcp__tokenizor__resolve_path",
    "mcp__tokenizor__find_references",
    "mcp__tokenizor__find_dependents",
    "mcp__tokenizor__find_implementations",
    "mcp__tokenizor__trace_symbol",
    "mcp__tokenizor__inspect_match",
    "mcp__tokenizor__analyze_file_impact",
    "mcp__tokenizor__what_changed",
];

fn merge_allowed_tools(settings: &mut Value) {
    if !settings["allowedTools"].is_array() {
        settings["allowedTools"] = json!([]);
    }
    let allowed = settings["allowedTools"].as_array_mut().expect("is array");
    for tool_name in TOKENIZOR_TOOL_NAMES {
        let val = Value::String(tool_name.to_string());
        if !allowed.contains(&val) {
            allowed.push(val);
        }
    }
}
```

Call `merge_allowed_tools(settings)` at the end of `merge_tokenizor_hooks`.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --lib test_merge_adds_allowed_tools`
Expected: PASS

- [ ] **Step 5: Write failing test for Codex allow list**

```rust
#[test]
fn test_codex_registration_includes_allow_list() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("config.toml");
    register_codex_mcp_server(&config_path, "/usr/bin/tokenizor-mcp").unwrap();
    let content = std::fs::read_to_string(&config_path).unwrap();
    assert!(content.contains("search_symbols"), "should contain tool names: {content}");
}
```

- [ ] **Step 6: Implement Codex allow list**

In `register_codex_mcp_server`, after setting `command`, add the `allowed_tools` array to the TOML table:

```rust
let mut allow_array = Array::new();
for tool_name in TOKENIZOR_TOOL_NAMES {
    // Codex uses plain tool names without mcp__ prefix
    let short_name = tool_name.strip_prefix("mcp__tokenizor__").unwrap_or(tool_name);
    allow_array.push(short_name);
}
table["allowed_tools"] = value(allow_array);
```

Note: Verify Codex's exact config format for allowed tools. If Codex uses `full_auto = true` instead, use that.

- [ ] **Step 7: Run all init tests**

Run: `cargo test --lib cli::init`
Expected: All pass

- [ ] **Step 8: Commit**

```bash
git add src/cli/init.rs
git commit -m "feat: auto-allow all Tokenizor tools during init (no more permission prompts)"
```

---

### Task 3: Gemini CLI Integration

Add Gemini as a third supported CLI target. Gemini CLI uses `~/.gemini/settings.json` for MCP config and `GEMINI.md` for instructions.

**Files:**
- Modify: `src/cli/mod.rs:47-51` (add `Gemini` variant to `InitClient`)
- Modify: `src/cli/init.rs` (add Gemini init path, config writer, detection)
- Modify: `npm/scripts/install.js` (add Gemini detection)
- Create: No new files needed

- [ ] **Step 1: Add Gemini to InitClient enum**

In `src/cli/mod.rs`:

```rust
#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum InitClient {
    Claude,
    Codex,
    Gemini,
    All,
}
```

- [ ] **Step 2: Add Gemini paths to InitPaths**

In `src/cli/init.rs`:

```rust
struct InitPaths {
    claude_settings: PathBuf,
    claude_config: PathBuf,
    claude_memory: PathBuf,
    codex_config: PathBuf,
    codex_agents: PathBuf,
    gemini_settings: PathBuf,
    gemini_memory: PathBuf,
}

impl InitPaths {
    fn from_home(home: &std::path::Path) -> Self {
        Self {
            // ... existing ...
            gemini_settings: home.join(".gemini").join("settings.json"),
            gemini_memory: home.join(".gemini").join("GEMINI.md"),
        }
    }
}
```

- [ ] **Step 3: Implement Gemini MCP registration**

Gemini CLI uses a JSON config similar to Claude. Add:

```rust
pub fn register_gemini_mcp_server(
    gemini_settings_path: &std::path::Path,
    binary_path: &str,
) -> anyhow::Result<()> {
    if let Some(parent) = gemini_settings_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating {}", parent.display()))?;
    }

    let mut config: Value = if gemini_settings_path.exists() {
        let raw = std::fs::read_to_string(gemini_settings_path)
            .with_context(|| format!("reading {}", gemini_settings_path.display()))?;
        serde_json::from_str(&raw)
            .with_context(|| format!("parsing {}", gemini_settings_path.display()))?
    } else {
        json!({})
    };

    let command_path = native_command_path(binary_path);

    if !config["mcpServers"].is_object() {
        config["mcpServers"] = json!({});
    }
    config["mcpServers"]["tokenizor"] = json!({
        "command": command_path,
        "args": [],
        "timeout": 120
    });

    // Auto-allow all tools
    merge_allowed_tools(&mut config);

    let pretty = serde_json::to_string_pretty(&config)?;
    std::fs::write(gemini_settings_path, pretty)
        .with_context(|| format!("writing {}", gemini_settings_path.display()))?;
    Ok(())
}
```

- [ ] **Step 4: Wire Gemini into run_init**

Add the Gemini path in `run_init` / `run_init_with_context`, mirroring the Claude/Codex pattern:

```rust
InitClient::Gemini | InitClient::All => {
    register_gemini_mcp_server(&paths.gemini_settings, binary_path)?;
    upsert_guidance_markdown(&paths.gemini_memory, "GEMINI.md")?;
    println!("  ✓ Gemini: {}", paths.gemini_settings.display());
}
```

- [ ] **Step 5: Add Gemini detection in npm installer**

In `npm/scripts/install.js`, update `detectClients()`:

```javascript
const geminiDir = path.join(os.homedir(), '.gemini');
const hasGemini = fs.existsSync(geminiDir);
```

And update the return logic to include gemini.

- [ ] **Step 6: Write tests**

```rust
#[test]
fn test_init_accepts_gemini_client() {
    let cli = Cli::parse_from(["tokenizor", "init", "--client", "gemini"]);
    match cli.command {
        Some(Commands::Init { client }) => assert_eq!(client, InitClient::Gemini),
        _ => panic!("expected init command"),
    }
}

#[test]
fn test_gemini_registration_creates_config() {
    let dir = tempfile::tempdir().unwrap();
    let settings_path = dir.path().join("settings.json");
    register_gemini_mcp_server(&settings_path, "/usr/bin/tokenizor-mcp").unwrap();
    let content = std::fs::read_to_string(&settings_path).unwrap();
    let config: Value = serde_json::from_str(&content).unwrap();
    assert!(config["mcpServers"]["tokenizor"]["command"].is_string());
}
```

- [ ] **Step 7: Commit**

```bash
git add src/cli/mod.rs src/cli/init.rs npm/scripts/install.js
git commit -m "feat: add Gemini CLI support (init, MCP registration, auto-allow)"
```

---

## Chunk 2: Symbol-Aware Context in search_text

The #1 easiest win. When search_text finds a match at line N, annotate it with the enclosing symbol name and its line range. Data already exists in the index.

### Task 4: Add Enclosing Symbol to TextLineMatch

**Files:**
- Modify: `src/live_index/search.rs:451-454` (add `enclosing_symbol` field to `TextLineMatch`)
- Modify: `src/live_index/search.rs:~750` (in `collect_text_matches`, look up enclosing symbol)
- Modify: `src/protocol/format.rs:269-325` (render enclosing symbol in output)
- Test: `tests/xref_integration.rs` or `src/protocol/tools.rs` tests

- [ ] **Step 1: Write failing test**

In `src/protocol/tools.rs` tests:

```rust
#[tokio::test]
async fn test_search_text_shows_enclosing_symbol() {
    let sym = make_symbol("handle_request", SymbolKind::Function, 0, 2);
    let content = b"fn handle_request() {\n    let db = connect();\n}\n";
    let (key, file) = make_file("src/handler.rs", content, vec![sym]);
    let server = make_server(make_live_index_ready(vec![(key, file)]));
    let result = server.search_text(Parameters(super::SearchTextInput {
        query: Some("connect".to_string()),
        terms: None, regex: None, path_prefix: None, language: None,
        limit: None, max_per_file: None, include_generated: None,
        include_tests: None, glob: None, exclude_glob: None,
        context: None, case_sensitive: None, whole_word: None,
    })).await;
    assert!(result.contains("handle_request"),
        "should show enclosing symbol name, got: {result}");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --lib test_search_text_shows_enclosing_symbol`
Expected: FAIL — output doesn't contain "handle_request"

- [ ] **Step 3: Add enclosing_symbol to TextLineMatch**

In `src/live_index/search.rs`:

```rust
pub struct TextLineMatch {
    pub line_number: usize,
    pub line: String,
    pub enclosing_symbol: Option<EnclosingMatchSymbol>,
}

pub struct EnclosingMatchSymbol {
    pub name: String,
    pub kind: String,
    pub line_range: (u32, u32),
}
```

- [ ] **Step 4: Populate enclosing_symbol in collect_text_matches**

In the `collect_text_matches` function (around line 750), after finding a match at `line_number`, look up the enclosing symbol from the file's symbol table:

```rust
let enclosing_symbol = file.symbols.iter()
    .filter(|s| s.line_range.0 <= (line_number as u32).saturating_sub(1)
             && s.line_range.1 >= (line_number as u32).saturating_sub(1))
    .max_by_key(|s| s.depth)
    .map(|s| EnclosingMatchSymbol {
        name: s.name.clone(),
        kind: s.kind.to_string(),
        line_range: s.line_range,
    });
```

Note: `line_number` in TextLineMatch is 1-based, symbol `line_range` is 0-based.

- [ ] **Step 5: Update format to show enclosing symbol**

In `src/protocol/format.rs`, in `search_text_result_view`, change the match line rendering:

Current:
```
  {line_number}: {line}
```

New:
```
src/handler.rs
  in fn handle_request (lines 1-3):
    > 2: let db = connect();
```

When context mode is off and there's an enclosing symbol, group matches by enclosing symbol within each file. When two adjacent matches share the same enclosing symbol, show the header once.

- [ ] **Step 6: Run tests**

Run: `cargo test --all-targets -- --test-threads=1`
Expected: All pass (update any broken format assertions)

- [ ] **Step 7: Commit**

```bash
git add src/live_index/search.rs src/protocol/format.rs src/protocol/tools.rs
git commit -m "feat: symbol-aware context in search_text — show enclosing symbol for each match"
```

---

## Chunk 3: Git Temporal Coupling in search_files

Unique capability grep can never match. "Show me files that co-change with X."

### Task 5: Add `changed_with` Parameter to search_files

**Files:**
- Modify: `src/protocol/tools.rs:134-141` (add `changed_with` to `SearchFilesInput`)
- Modify: `src/live_index/query.rs:~1047` (add co-change lookup path in `capture_search_files_view`)
- Modify: `src/live_index/query.rs:618-630` (add `CoChange` tier to `SearchFilesView`)
- Modify: `src/protocol/format.rs:808-850` (format co-change results)
- Modify: `src/protocol/tools.rs:1108-1122` (pass git_temporal to the view capture)
- Test: `src/protocol/tools.rs` tests

- [ ] **Step 1: Write failing test**

```rust
#[tokio::test]
async fn test_search_files_changed_with_returns_coupled_files() {
    // This test will need git temporal data mocked into the index.
    // For now, test that the parameter is accepted and returns a meaningful response.
    let (key, file) = make_file("src/daemon.rs", b"fn foo() {}", vec![]);
    let server = make_server(make_live_index_ready(vec![(key, file)]));
    let result = server.search_files(Parameters(super::SearchFilesInput {
        query: String::new(),
        limit: None,
        current_file: None,
        changed_with: Some("src/daemon.rs".to_string()),
    })).await;
    // Without git temporal data loaded, should return informative message
    assert!(!result.contains("error"), "should not error, got: {result}");
}
```

- [ ] **Step 2: Add `changed_with` to SearchFilesInput**

```rust
pub struct SearchFilesInput {
    pub query: String,
    pub limit: Option<u32>,
    pub current_file: Option<String>,
    /// Find files that frequently co-change with this file (uses git temporal coupling data).
    pub changed_with: Option<String>,
}
```

- [ ] **Step 3: Add CoChange tier to SearchFilesView**

In `src/live_index/query.rs`, add:

```rust
pub enum SearchFilesTier {
    CoChange,    // New — git temporal coupling
    StrongPath,
    Basename,
    LoosePath,
}
```

And add a `CoChangeHit` variant or extend `SearchFilesHit`:

```rust
pub struct SearchFilesHit {
    pub tier: SearchFilesTier,
    pub path: String,
    pub coupling_score: Option<f32>,
    pub shared_commits: Option<u32>,
}
```

- [ ] **Step 4: Implement co-change lookup in the handler**

In `src/protocol/tools.rs`, the `search_files` handler needs to check `changed_with` and if set, query git temporal:

```rust
if let Some(ref target_path) = params.0.changed_with {
    let temporal = self.index.git_temporal();
    if temporal.state == GitTemporalState::Ready {
        if let Some(history) = temporal.files.get(target_path.as_str()) {
            // Return co-changes as SearchFilesView::Found with CoChange tier
            let hits: Vec<SearchFilesHit> = history.co_changes.iter()
                .map(|entry| SearchFilesHit {
                    tier: SearchFilesTier::CoChange,
                    path: entry.path.clone(),
                    coupling_score: Some(entry.coupling_score),
                    shared_commits: Some(entry.shared_commits),
                })
                .collect();
            return format::search_files_result_view(&SearchFilesView::Found {
                query: format!("co-changes with {target_path}"),
                total_matches: hits.len(),
                overflow_count: 0,
                hits,
            });
        }
    }
    return format!("No git temporal data available for '{target_path}'. Git history may still be loading.");
}
```

- [ ] **Step 5: Update format for CoChange tier**

In `format.rs`, add the CoChange header:

```rust
SearchFilesTier::CoChange => "── Co-changed files (git temporal coupling) ──",
```

And if `coupling_score` is present, show it:

```rust
if let (Some(score), Some(shared)) = (hit.coupling_score, hit.shared_commits) {
    lines.push(format!("  {}  ({:.0}% coupled, {} shared commits)", hit.path, score * 100.0, shared));
} else {
    lines.push(format!("  {}", hit.path));
}
```

- [ ] **Step 6: Update daemon execute_tool_call**

In `src/daemon.rs`, the `execute_tool_call` for `search_files` already passes through — no change needed since the handler reads `self.index.git_temporal()` directly.

- [ ] **Step 7: Run tests and commit**

```bash
cargo test --all-targets -- --test-threads=1
git add src/protocol/tools.rs src/live_index/query.rs src/protocol/format.rs
git commit -m "feat: search_files changed_with parameter — find co-changing files via git temporal coupling"
```

---

## Chunk 4: Semantic Dedup / group_by in search_text

### Task 6: Add `group_by` Parameter to search_text

Reduce noise by grouping matches by enclosing symbol instead of showing every raw hit.

**Files:**
- Modify: `src/protocol/tools.rs:101-130` (add `group_by` to `SearchTextInput`)
- Modify: `src/protocol/format.rs:269-325` (implement grouping logic)
- Test: `src/protocol/tools.rs` tests

- [ ] **Step 1: Write failing test**

```rust
#[tokio::test]
async fn test_search_text_group_by_symbol_deduplicates() {
    let sym = make_symbol("connect", SymbolKind::Function, 0, 4);
    let content = b"fn connect() {\n    let url = db_url();\n    let pool = Pool::new(url);\n    pool.connect()\n}\n";
    let (key, file) = make_file("src/db.rs", content, vec![sym]);
    let server = make_server(make_live_index_ready(vec![(key, file)]));
    let result = server.search_text(Parameters(super::SearchTextInput {
        query: Some("pool".to_string()),
        terms: None, regex: None, path_prefix: None, language: None,
        limit: None, max_per_file: None, include_generated: None,
        include_tests: None, glob: None, exclude_glob: None,
        context: None, case_sensitive: None, whole_word: None,
        group_by: Some("symbol".to_string()),
    })).await;
    // With group_by: "symbol", multiple matches in same symbol should show once
    assert!(result.contains("connect"), "should show symbol name: {result}");
}
```

- [ ] **Step 2: Add group_by to SearchTextInput**

```rust
pub struct SearchTextInput {
    // ... existing fields ...
    /// Group matches: "file" (default), "symbol" (one entry per enclosing symbol),
    /// or "usage" (exclude imports and comments).
    pub group_by: Option<String>,
}
```

- [ ] **Step 3: Implement grouping in format**

The grouping happens at the format layer — the search returns all matches, and `search_text_result_view` groups them:

- `group_by: "symbol"` — For each file, group matches by their `enclosing_symbol`. Show one block per symbol with all match lines inside it.
- `group_by: "usage"` — Filter out matches where the line starts with `use `, `import `, `from ... import`, `require(`, or is inside a comment (heuristic: line starts with `//`, `#`, `*`, `/*`).

- [ ] **Step 4: Run tests and commit**

```bash
cargo test --all-targets -- --test-threads=1
git add src/protocol/tools.rs src/protocol/format.rs
git commit -m "feat: search_text group_by parameter — deduplicate by symbol or filter imports"
```

---

## Chunk 5: follow_refs in search_text

### Task 7: Add `follow_refs` Parameter to search_text

For each match, inline the enclosing symbol's callers — collapses multi-step grep workflows into one call.

**Files:**
- Modify: `src/protocol/tools.rs:101-130` (add `follow_refs` to `SearchTextInput`)
- Modify: `src/protocol/tools.rs:~1001` (search_text handler — after search, enrich with refs)
- Modify: `src/live_index/search.rs:451-454` (add `callers` field to `TextLineMatch` or `TextFileMatches`)
- Modify: `src/protocol/format.rs` (render callers inline)
- Test: `src/protocol/tools.rs` tests

- [ ] **Step 1: Write failing test**

```rust
#[tokio::test]
async fn test_search_text_follow_refs_includes_callers() {
    // Build an index with cross-references
    let mut sym_a = make_symbol("connect", SymbolKind::Function, 0, 1);
    let file_a_content = b"fn connect() { db_open() }\n";
    let (key_a, file_a) = make_file("src/db.rs", file_a_content, vec![sym_a]);

    let mut sym_b = make_symbol("handler", SymbolKind::Function, 0, 1);
    let file_b_content = b"fn handler() { connect() }\n";
    let (key_b, mut file_b) = make_file("src/api.rs", file_b_content, vec![sym_b]);
    // Add reference from handler -> connect
    file_b.references.push(ReferenceRecord {
        target_name: "connect".to_string(),
        kind: ReferenceKind::Call,
        location: ReferenceLocation { byte_range: (15, 24), line: 0 },
        source_file: None,
    });

    let server = make_server(make_live_index_ready(vec![(key_a, file_a), (key_b, file_b)]));
    let result = server.search_text(Parameters(super::SearchTextInput {
        query: Some("db_open".to_string()),
        terms: None, regex: None, path_prefix: None, language: None,
        limit: None, max_per_file: None, include_generated: None,
        include_tests: None, glob: None, exclude_glob: None,
        context: None, case_sensitive: None, whole_word: None,
        group_by: None,
        follow_refs: Some(true),
    })).await;
    // Should show that connect() is called by handler() in src/api.rs
    assert!(result.contains("handler") || result.contains("api.rs"),
        "should show callers of enclosing symbol, got: {result}");
}
```

- [ ] **Step 2: Add follow_refs to SearchTextInput**

```rust
pub struct SearchTextInput {
    // ... existing fields ...
    pub group_by: Option<String>,
    /// When true, for each match include a compact list of callers of the enclosing symbol.
    pub follow_refs: Option<bool>,
}
```

- [ ] **Step 3: Implement follow_refs enrichment**

In the `search_text` handler, after getting results, if `follow_refs` is true:

1. Collect unique enclosing symbol names from all matches
2. For each, call `find_references_for_name` on the index
3. Attach a compact caller list to each `TextFileMatches` entry

Add to `TextFileMatches`:
```rust
pub struct TextFileMatches {
    pub path: String,
    pub matches: Vec<TextLineMatch>,
    pub rendered_lines: Option<Vec<TextDisplayLine>>,
    pub callers: Option<Vec<CallerEntry>>,
}

pub struct CallerEntry {
    pub file: String,
    pub symbol: String,
    pub line: u32,
}
```

- [ ] **Step 4: Update format to render callers**

After the match lines for each file, if callers exist:
```
    Called by: handler (src/api.rs:1), process (src/worker.rs:45)
```

- [ ] **Step 5: Run tests and commit**

```bash
cargo test --all-targets -- --test-threads=1
git add src/protocol/tools.rs src/live_index/search.rs src/protocol/format.rs
git commit -m "feat: search_text follow_refs — inline callers of enclosing symbol"
```

---

## Chunk 6: Explore Tool

### Task 8: Add `explore` Tool

High-level concept search that maps natural-language-ish queries to combined symbol + text patterns.

**Files:**
- Modify: `src/protocol/tools.rs` (add `ExploreInput` struct and `explore` handler)
- Create: `src/protocol/explore.rs` (concept → pattern mapping)
- Modify: `src/protocol/mod.rs` (add `pub mod explore`)
- Modify: `src/protocol/format.rs` (format explore results)
- Modify: `src/daemon.rs:~1189` (add `explore` to `execute_tool_call` match)
- Test: `src/protocol/tools.rs` tests

- [ ] **Step 1: Define concept map**

Create `src/protocol/explore.rs`:

```rust
pub struct ConceptPattern {
    pub label: &'static str,
    pub symbol_queries: &'static [&'static str],
    pub text_queries: &'static [&'static str],
    pub kind_filters: &'static [&'static str],
}

pub const CONCEPT_MAP: &[(&str, ConceptPattern)] = &[
    ("error handling", ConceptPattern {
        label: "Error Handling",
        symbol_queries: &["Error", "Result", "anyhow", "bail", "catch"],
        text_queries: &["unwrap()", ".expect(", "return Err(", "try {", "catch"],
        kind_filters: &["struct", "enum", "fn"],
    }),
    ("concurrency", ConceptPattern {
        label: "Concurrency",
        symbol_queries: &["Mutex", "RwLock", "Atomic", "channel", "spawn", "async"],
        text_queries: &["tokio::spawn", "thread::spawn", ".lock()", ".read()", ".write()"],
        kind_filters: &[],
    }),
    ("authentication", ConceptPattern {
        label: "Authentication",
        symbol_queries: &["auth", "login", "session", "token", "credential", "password"],
        text_queries: &["Bearer", "JWT", "OAuth", "verify_token", "authenticate"],
        kind_filters: &[],
    }),
    ("database", ConceptPattern {
        label: "Database",
        symbol_queries: &["query", "migrate", "schema", "pool", "connection", "transaction"],
        text_queries: &["SELECT", "INSERT", "CREATE TABLE", "sqlx", "diesel", "TypeORM"],
        kind_filters: &[],
    }),
    ("testing", ConceptPattern {
        label: "Testing",
        symbol_queries: &["test", "mock", "fixture", "assert", "expect"],
        text_queries: &["#[test]", "#[tokio::test]", "describe(", "it(", "pytest"],
        kind_filters: &["fn"],
    }),
    ("api", ConceptPattern {
        label: "API / HTTP",
        symbol_queries: &["handler", "route", "endpoint", "controller", "request", "response"],
        text_queries: &["GET", "POST", "PUT", "DELETE", "Router", "axum", "actix", "express"],
        kind_filters: &["fn"],
    }),
    ("configuration", ConceptPattern {
        label: "Configuration",
        symbol_queries: &["config", "settings", "env", "options", "params"],
        text_queries: &["dotenv", "env::var", "process.env", "serde", "toml", "yaml"],
        kind_filters: &["struct"],
    }),
];

/// Find the best matching concept for a query.
pub fn match_concept(query: &str) -> Option<&'static ConceptPattern> {
    let lower = query.to_ascii_lowercase();
    CONCEPT_MAP.iter()
        .find(|(key, _)| lower.contains(key))
        .map(|(_, pattern)| pattern)
}

/// For queries that don't match a concept, split into search terms.
pub fn fallback_terms(query: &str) -> Vec<String> {
    query.split_whitespace()
        .map(|w| w.to_lowercase())
        .filter(|w| w.len() >= 2)
        .collect()
}
```

- [ ] **Step 2: Add ExploreInput and handler**

In `src/protocol/tools.rs`:

```rust
#[derive(Deserialize, Serialize, JsonSchema)]
pub struct ExploreInput {
    /// Natural-language concept or topic to explore (e.g., "error handling", "concurrency", "database").
    pub query: String,
    /// Maximum number of results per category (default 10).
    pub limit: Option<u32>,
}
```

Handler:
```rust
#[tool(description = "Explore a concept across the codebase — finds related symbols, patterns, and files.")]
pub(crate) async fn explore(&self, params: Parameters<ExploreInput>) -> String {
    if let Some(result) = self.proxy_tool_call("explore", &params.0).await {
        return result;
    }
    // 1. Match concept
    // 2. Run symbol searches for each symbol_query
    // 3. Run text searches for each text_query
    // 4. Deduplicate and rank results
    // 5. Format as a unified "concept report"
}
```

- [ ] **Step 3: Add to daemon's execute_tool_call**

```rust
"explore" => Ok(server
    .explore(Parameters(decode_params::<ExploreInput>(params)?))
    .await),
```

- [ ] **Step 4: Format explore results**

Group results into sections: "Symbols", "Code patterns", "Related files":

```
── Exploring: Concurrency ──

Symbols (12 found):
  struct RwLock          src/live_index/store.rs:314
  struct SharedIndexHandle  src/live_index/store.rs:314
  fn start_watcher       src/watcher/mod.rs:271
  ...

Code patterns (8 found):
  src/daemon.rs
    in fn close_session (lines 257-290):
      > 262: let sessions = self.sessions.read().expect("lock poisoned");
  src/live_index/store.rs
    in fn reload (lines 377-382):
      > 378: let mut live = self.live.write().expect("lock poisoned");
  ...

Related files:
  src/daemon.rs          (5 concurrency symbols)
  src/live_index/store.rs (3 concurrency symbols)
  src/watcher/mod.rs     (2 concurrency symbols)
```

- [ ] **Step 5: Write tests and commit**

```bash
cargo test --all-targets -- --test-threads=1
git add src/protocol/explore.rs src/protocol/mod.rs src/protocol/tools.rs src/protocol/format.rs src/daemon.rs
git commit -m "feat: explore tool — concept-based codebase exploration"
```

---

## Execution Order

| Priority | Task | Workstream | Estimated Complexity |
|----------|------|-----------|---------------------|
| 1 | Task 1 | Bug fixes commit | Trivial (already done) |
| 2 | Task 2 | Auto-allow/YOLO | Small |
| 3 | Task 3 | Gemini CLI | Medium |
| 4 | Task 4 | Symbol-aware context | Medium |
| 5 | Task 5 | Git temporal coupling | Medium |
| 6 | Task 6 | Semantic dedup | Medium |
| 7 | Task 7 | follow_refs | Medium-Large |
| 8 | Task 8 | Explore tool | Large |

Tasks 1-3 can be done in one commit batch (Chunk 1).
Tasks 4-5 are independent and can be parallelized.
Tasks 6-7 depend on Task 4 (enclosing symbol data).
Task 8 is independent but largest.

---
