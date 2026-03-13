//! `tokenizor init` command — client-aware Claude/Codex configuration.
//!
//! Strategy:
//! 1. Discover the absolute path of the running tokenizor binary.
//! 2. Configure Claude, Codex, or both based on the selected client target.
//! 3. For Claude, merge tokenizor hook entries into `~/.claude/settings.json`
//!    and register the MCP server in `~/.claude.json`.
//! 4. For Codex, register the MCP server in `~/.codex/config.toml`.
//! 5. Create `.tokenizor/` in the current working directory (runtime needs it).
//!
//! Identification: any hook entry whose `hooks[].command` contains the substring
//! `"tokenizor hook"` is considered a tokenizor-owned entry and will be replaced.

use std::path::PathBuf;

use anyhow::Context;
use serde_json::{Value, json};
use toml_edit::{Array, DocumentMut, Item, Table, value};

use crate::cli::InitClient;

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

#[derive(Debug)]
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
            claude_settings: home.join(".claude").join("settings.json"),
            claude_config: home.join(".claude.json"),
            claude_memory: home.join(".claude").join("CLAUDE.md"),
            codex_config: home.join(".codex").join("config.toml"),
            codex_agents: home.join(".codex").join("AGENTS.md"),
            gemini_settings: home.join(".gemini").join("settings.json"),
            gemini_memory: home.join(".gemini").join("GEMINI.md"),
        }
    }
}

const CODEX_STARTUP_TIMEOUT_SEC: i64 = 30;
const CODEX_TOOL_TIMEOUT_SEC: i64 = 120;
const TOKENIZOR_GUIDANCE_START: &str = "<!-- TOKENIZOR START -->";
const TOKENIZOR_GUIDANCE_END: &str = "<!-- TOKENIZOR END -->";

/// Entry point called by main.rs for `tokenizor init`.
pub fn run_init(client: InitClient) -> anyhow::Result<()> {
    let home = dirs::home_dir().context("cannot determine home directory")?;
    let working_dir =
        std::env::current_dir().context("cannot determine current working directory")?;
    let binary_path = discover_binary_path();

    run_init_with_context(client, &home, &working_dir, &binary_path)
}

/// Testable core for `tokenizor init` with injected paths.
pub fn run_init_with_context(
    client: InitClient,
    home_dir: &std::path::Path,
    working_dir: &std::path::Path,
    binary_path: &std::path::Path,
) -> anyhow::Result<()> {
    let paths = InitPaths::from_home(home_dir);
    let binary_path_str = binary_path.display().to_string();

    if matches!(client, InitClient::Claude | InitClient::All) {
        merge_hooks_into_settings(&paths.claude_settings, binary_path)?;
        eprintln!(
            "Claude hooks installed in {}",
            paths.claude_settings.display()
        );

        register_mcp_server(&paths.claude_config, &binary_path_str)?;
        eprintln!(
            "Claude MCP server registered in {}",
            paths.claude_config.display()
        );

        upsert_guidance_markdown(&paths.claude_memory, &claude_guidance_block())?;
        eprintln!(
            "Claude guidance written to {}",
            paths.claude_memory.display()
        );
    }

    if matches!(client, InitClient::Codex | InitClient::All) {
        register_codex_mcp_server(&paths.codex_config, &binary_path_str)?;
        eprintln!(
            "Codex MCP server registered in {}",
            paths.codex_config.display()
        );

        upsert_guidance_markdown(&paths.codex_agents, &codex_guidance_block())?;
        eprintln!("Codex guidance written to {}", paths.codex_agents.display());
        eprintln!(
            "note: Codex gets MCP tools only. No documented Codex hook/session-start enrichment interface was found, so transparent enrichment remains Claude-only."
        );
    }

    if matches!(client, InitClient::Gemini | InitClient::All) {
        register_gemini_mcp_server(&paths.gemini_settings, &binary_path_str)?;
        eprintln!(
            "Gemini MCP server registered in {}",
            paths.gemini_settings.display()
        );

        upsert_guidance_markdown(&paths.gemini_memory, &gemini_guidance_block())?;
        eprintln!(
            "Gemini guidance written to {}",
            paths.gemini_memory.display()
        );
    }

    std::fs::create_dir_all(working_dir.join(".tokenizor"))
        .with_context(|| format!("creating {}", working_dir.join(".tokenizor").display()))?;

    eprintln!("tokenizor init complete");

    Ok(())
}

/// Merge tokenizor hook entries into `settings_path`, creating it if necessary.
///
/// This is the testable core of `run_init`. Integration tests can pass a temp-dir path
/// instead of the real `~/.claude/settings.json`.
///
/// `binary_path` is the absolute path of the tokenizor binary.
pub fn merge_hooks_into_settings(
    settings_path: &std::path::Path,
    binary_path: &std::path::Path,
) -> anyhow::Result<()> {
    // Ensure parent dir exists.
    if let Some(parent) = settings_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating {}", parent.display()))?;
    }

    // Read existing settings or start with empty object.
    let mut settings: Value = if settings_path.exists() {
        let raw = std::fs::read_to_string(settings_path)
            .with_context(|| format!("reading {}", settings_path.display()))?;
        serde_json::from_str(&raw)
            .with_context(|| format!("parsing {}", settings_path.display()))?
    } else {
        json!({})
    };

    // Normalise binary path to forward slashes for JSON command strings.
    let binary_str = binary_path.display().to_string().replace('\\', "/");

    // Merge hooks in-place.
    merge_tokenizor_hooks(&mut settings, &binary_str);

    // Write back.
    let pretty = serde_json::to_string_pretty(&settings)?;
    std::fs::write(settings_path, pretty)
        .with_context(|| format!("writing {}", settings_path.display()))?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Tool name constants
// ---------------------------------------------------------------------------

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
    "mcp__tokenizor__get_co_changes",
    "mcp__tokenizor__diff_symbols",
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

// ---------------------------------------------------------------------------
// Core merge logic (pub for unit testing)
// ---------------------------------------------------------------------------

/// Merge tokenizor hook entries into an existing `settings` Value in-place.
///
/// `binary_path` is the absolute path of the tokenizor binary (already
/// normalised to forward-slash on Windows).
pub fn merge_tokenizor_hooks(settings: &mut Value, binary_path: &str) {
    // Ensure `hooks` key is an object.
    if !settings["hooks"].is_object() {
        settings["hooks"] = json!({});
    }

    // Build fresh tokenizor entries.
    let post_tool_use_entries = build_post_tool_use_entries(binary_path);
    let session_start_entries = build_session_start_entries(binary_path);
    let user_prompt_submit_entries = build_user_prompt_submit_entries(binary_path);

    {
        let hooks = settings["hooks"]
            .as_object_mut()
            .expect("hooks is an object");
        merge_event_entries(hooks, "PostToolUse", post_tool_use_entries);
        merge_event_entries(hooks, "SessionStart", session_start_entries);
        merge_event_entries(hooks, "UserPromptSubmit", user_prompt_submit_entries);
    }

    merge_allowed_tools(settings);
}

// ---------------------------------------------------------------------------
// Entry builders
// ---------------------------------------------------------------------------

fn build_post_tool_use_entries(binary_path: &str) -> Vec<Value> {
    vec![json!({
        "matcher": "Read|Edit|Write|Grep",
        "hooks": [{"type": "command", "command": format!("{binary_path} hook"), "timeout": 5}]
    })]
}

fn build_session_start_entries(binary_path: &str) -> Vec<Value> {
    vec![json!({
        "matcher": "startup|resume",
        "hooks": [{"type": "command", "command": format!("{binary_path} hook session-start"), "timeout": 5}]
    })]
}

fn build_user_prompt_submit_entries(binary_path: &str) -> Vec<Value> {
    vec![json!({
        "hooks": [{"type": "command", "command": format!("{binary_path} hook prompt-submit"), "timeout": 5}]
    })]
}

// ---------------------------------------------------------------------------
// Merge helpers
// ---------------------------------------------------------------------------

/// Returns `true` if a hook entry array contains a tokenizor hook command.
///
/// The binary may be named `tokenizor` or `tokenizor-mcp` (with optional `.exe`),
/// so we check for "tokenizor" anywhere in the command AND " hook" as the
/// subcommand indicator.
fn is_tokenizor_entry(entry: &Value) -> bool {
    if let Some(hooks) = entry["hooks"].as_array() {
        hooks.iter().any(|h| {
            h["command"]
                .as_str()
                .map(|cmd| cmd.contains("tokenizor") && cmd.contains(" hook"))
                .unwrap_or(false)
        })
    } else {
        false
    }
}

/// Merge `new_entries` into the `event_key` array of the hooks object.
///
/// Existing tokenizor entries (identified by `is_tokenizor_entry`) are filtered
/// out before appending the fresh entries, which achieves idempotency.
fn merge_event_entries(
    hooks: &mut serde_json::Map<String, Value>,
    event_key: &str,
    new_entries: Vec<Value>,
) {
    let existing: Vec<Value> = hooks
        .get(event_key)
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    // Keep only non-tokenizor entries.
    let mut retained: Vec<Value> = existing
        .into_iter()
        .filter(|e| !is_tokenizor_entry(e))
        .collect();

    // Append fresh tokenizor entries at the end.
    retained.extend(new_entries);

    hooks.insert(event_key.to_string(), Value::Array(retained));
}

/// Register tokenizor as an MCP server in `~/.claude.json` using the absolute binary path.
///
/// This ensures Claude Code launches the native binary directly — no shell, no .cmd wrapper,
/// no Node.js intermediary. Works on all platforms.
pub fn register_mcp_server(
    claude_json_path: &std::path::Path,
    binary_path: &str,
) -> anyhow::Result<()> {
    let mut config: Value = if claude_json_path.exists() {
        let raw = std::fs::read_to_string(claude_json_path)
            .with_context(|| format!("reading {}", claude_json_path.display()))?;
        serde_json::from_str(&raw)
            .with_context(|| format!("parsing {}", claude_json_path.display()))?
    } else {
        json!({})
    };

    // Use backslashes on Windows for the command path (Claude Code spawns natively, not via shell).
    let command_path = native_command_path(binary_path);

    // Ensure mcpServers exists and set tokenizor entry.
    if !config["mcpServers"].is_object() {
        config["mcpServers"] = json!({});
    }
    config["mcpServers"]["tokenizor"] = json!({
        "type": "stdio",
        "command": command_path,
        "args": [],
        "env": {}
    });

    let pretty = serde_json::to_string_pretty(&config)?;
    std::fs::write(claude_json_path, pretty)
        .with_context(|| format!("writing {}", claude_json_path.display()))?;

    Ok(())
}

/// Register tokenizor as an MCP server in `~/.codex/config.toml`.
///
/// Codex stores MCP servers under `[mcp_servers.<name>]` tables in TOML.
/// We update only the `tokenizor` entry and preserve the rest of the file.
pub fn register_codex_mcp_server(
    codex_config_path: &std::path::Path,
    binary_path: &str,
) -> anyhow::Result<()> {
    if let Some(parent) = codex_config_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating {}", parent.display()))?;
    }

    let raw = if codex_config_path.exists() {
        std::fs::read_to_string(codex_config_path)
            .with_context(|| format!("reading {}", codex_config_path.display()))?
    } else {
        String::new()
    };

    let mut config = if raw.trim().is_empty() {
        DocumentMut::new()
    } else {
        raw.parse::<DocumentMut>()
            .with_context(|| format!("parsing {}", codex_config_path.display()))?
    };

    merge_tokenizor_codex_server(&mut config, binary_path);

    std::fs::write(codex_config_path, config.to_string())
        .with_context(|| format!("writing {}", codex_config_path.display()))?;

    Ok(())
}

fn merge_tokenizor_codex_server(config: &mut DocumentMut, binary_path: &str) {
    if !config.as_table().contains_key("mcp_servers") || !config["mcp_servers"].is_table() {
        config["mcp_servers"] = Item::Table(Table::new());
    }

    let mcp_servers = config["mcp_servers"]
        .as_table_mut()
        .expect("mcp_servers must be a table");

    if !mcp_servers.contains_key("tokenizor") || !mcp_servers["tokenizor"].is_table() {
        mcp_servers.insert("tokenizor", Item::Table(Table::new()));
    }

    let tokenizor = mcp_servers["tokenizor"]
        .as_table_mut()
        .expect("tokenizor server entry must be a table");

    tokenizor["command"] = value(native_command_path(binary_path));
    tokenizor["startup_timeout_sec"] = value(CODEX_STARTUP_TIMEOUT_SEC);
    tokenizor["tool_timeout_sec"] = value(CODEX_TOOL_TIMEOUT_SEC);

    let mut allow_array = Array::new();
    for tool_name in TOKENIZOR_TOOL_NAMES {
        // Codex uses plain tool names without mcp__ prefix
        let short_name = tool_name
            .strip_prefix("mcp__tokenizor__")
            .unwrap_or(tool_name);
        allow_array.push(short_name);
    }
    tokenizor["allowed_tools"] = value(allow_array);

    merge_codex_project_doc_fallbacks(config);
}

fn merge_codex_project_doc_fallbacks(config: &mut DocumentMut) {
    let key = "project_doc_fallback_filenames";
    if !config.as_table().contains_key(key) || !config[key].is_array() {
        let mut fallbacks = Array::default();
        fallbacks.push("CLAUDE.md");
        config[key] = value(fallbacks);
        return;
    }

    let fallbacks = config[key]
        .as_array_mut()
        .expect("project_doc_fallback_filenames must be an array");
    let has_claude_md = fallbacks
        .iter()
        .any(|entry| entry.as_str() == Some("CLAUDE.md"));
    if !has_claude_md {
        fallbacks.push("CLAUDE.md");
    }
}

/// Register tokenizor as an MCP server in `~/.gemini/settings.json`.
///
/// Gemini CLI stores MCP servers under `mcpServers` in a JSON settings file.
/// We update only the `tokenizor` entry and preserve the rest of the file.
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

fn upsert_guidance_markdown(path: &std::path::Path, guidance_block: &str) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating {}", parent.display()))?;
    }

    let existing = if path.exists() {
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?
    } else {
        String::new()
    };

    let merged = upsert_markdown_block(&existing, guidance_block);
    std::fs::write(path, merged).with_context(|| format!("writing {}", path.display()))?;

    Ok(())
}

fn upsert_markdown_block(existing: &str, guidance_block: &str) -> String {
    if let Some(start) = existing.find(TOKENIZOR_GUIDANCE_START)
        && let Some(end_marker_start) = existing[start..].find(TOKENIZOR_GUIDANCE_END)
    {
        let end = start + end_marker_start + TOKENIZOR_GUIDANCE_END.len();
        let mut merged = String::new();
        merged.push_str(&existing[..start]);
        merged.push_str(guidance_block);
        merged.push_str(&existing[end..]);
        return merged;
    }

    if existing.trim().is_empty() {
        return format!("{guidance_block}\n");
    }

    let mut merged = existing.trim_end_matches(['\r', '\n']).to_string();
    merged.push_str("\n\n");
    merged.push_str(guidance_block);
    merged.push('\n');
    merged
}

fn claude_guidance_block() -> String {
    format!(
        "{TOKENIZOR_GUIDANCE_START}\n## Tokenizor MCP\n- Prefer the Tokenizor MCP for codebase navigation when the `tokenizor` server is connected.\n- Start with `get_repo_map`, `get_repo_outline`, `get_file_context`, or `get_symbol_context` before broad raw file scans.\n- Use `analyze_file_impact` after edits and `what_changed` when resuming work.\n- Use Tokenizor MCP slash commands and `tokenizor://...` resources when Claude surfaces them.\n{TOKENIZOR_GUIDANCE_END}"
    )
}

fn codex_guidance_block() -> String {
    format!(
        "{TOKENIZOR_GUIDANCE_START}\n## Tokenizor MCP\n- Prefer the Tokenizor MCP for codebase navigation when the `tokenizor` server is connected.\n- Start with `get_repo_map`, `get_repo_outline`, `get_file_context`, or `get_symbol_context` before broad raw file scans.\n- Use `analyze_file_impact` after edits and `what_changed` when resuming work.\n- Codex is configured to read `CLAUDE.md` project guidance too, so treat project Tokenizor instructions there as authoritative when `AGENTS.md` is absent.\n{TOKENIZOR_GUIDANCE_END}"
    )
}

fn gemini_guidance_block() -> String {
    format!(
        "{TOKENIZOR_GUIDANCE_START}\n## Tokenizor MCP\n- Prefer the Tokenizor MCP for codebase navigation when the `tokenizor` server is connected.\n- Start with `get_repo_map`, `get_repo_outline`, `get_file_context`, or `get_symbol_context` before broad raw file scans.\n- Use `analyze_file_impact` after edits and `what_changed` when resuming work.\n{TOKENIZOR_GUIDANCE_END}"
    )
}

/// Returns the binary path of the currently running tokenizor executable.
fn discover_binary_path() -> PathBuf {
    match std::env::current_exe() {
        Ok(path) => {
            let s = path.display().to_string();
            // Warn if the binary is running from an unstable location.
            let is_npx_cache = s.contains("_npx") || s.contains("npx-cache");
            let is_node_modules = s.contains("node_modules");
            if is_npx_cache || is_node_modules || s.ends_with(".cmd") {
                eprintln!(
                    "warning: binary is inside node_modules or npx cache ({s}); \
                     updates will fail on Windows. Run: npm install -g tokenizor-mcp && tokenizor-mcp init --client all"
                );
            }
            path
        }
        Err(e) => {
            eprintln!("warning: could not determine tokenizor binary path: {e}");
            PathBuf::from("tokenizor")
        }
    }
}

fn native_command_path(binary_path: &str) -> String {
    if cfg!(windows) {
        binary_path.replace('/', "\\")
    } else {
        binary_path.to_string()
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const FAKE_BINARY: &str = "/usr/local/bin/tokenizor";

    fn run_merge(initial: Value) -> Value {
        let mut settings = initial;
        merge_tokenizor_hooks(&mut settings, FAKE_BINARY);
        settings
    }

    // --- test_init_creates_hooks_in_empty_settings ---

    #[test]
    fn test_init_creates_hooks_in_empty_settings() {
        let result = run_merge(json!({}));

        let post = result["hooks"]["PostToolUse"]
            .as_array()
            .expect("PostToolUse must be an array");
        let session = result["hooks"]["SessionStart"]
            .as_array()
            .expect("SessionStart must be an array");
        let prompt = result["hooks"]["UserPromptSubmit"]
            .as_array()
            .expect("UserPromptSubmit must be an array");

        assert_eq!(
            post.len(),
            1,
            "PostToolUse must have 1 entry (single stdin-routed entry)"
        );
        assert_eq!(session.len(), 1, "SessionStart must have 1 entry");
        assert_eq!(prompt.len(), 1, "UserPromptSubmit must have 1 entry");
    }

    #[test]
    fn test_init_entries_have_correct_commands() {
        let result = run_merge(json!({}));

        let post = &result["hooks"]["PostToolUse"];
        let entry = &post[0];
        let cmd = entry["hooks"][0]["command"].as_str().unwrap();
        assert_eq!(
            cmd, "/usr/local/bin/tokenizor hook",
            "Single PostToolUse hook command must have no subcommand suffix"
        );

        let session = &result["hooks"]["SessionStart"][0];
        let session_cmd = session["hooks"][0]["command"].as_str().unwrap();
        assert_eq!(session_cmd, "/usr/local/bin/tokenizor hook session-start");

        let prompt = &result["hooks"]["UserPromptSubmit"][0];
        let prompt_cmd = prompt["hooks"][0]["command"].as_str().unwrap();
        assert_eq!(prompt_cmd, "/usr/local/bin/tokenizor hook prompt-submit");
    }

    #[test]
    fn test_init_new_entry_matcher_includes_write() {
        let result = run_merge(json!({}));
        let matcher = result["hooks"]["PostToolUse"][0]["matcher"]
            .as_str()
            .unwrap();
        assert_eq!(
            matcher, "Read|Edit|Write|Grep",
            "matcher must include Write"
        );
    }

    // --- test_init_preserves_existing_hooks ---

    #[test]
    fn test_init_preserves_existing_hooks() {
        let initial = json!({
            "hooks": {
                "PostToolUse": [
                    {
                        "matcher": "Bash",
                        "hooks": [{"type": "command", "command": "/some/other/hook bash", "timeout": 10}]
                    }
                ]
            }
        });

        let result = run_merge(initial);
        let post = result["hooks"]["PostToolUse"]
            .as_array()
            .expect("PostToolUse must be an array");

        // 1 existing + 1 tokenizor = 2 total.
        assert_eq!(
            post.len(),
            2,
            "existing hook + 1 tokenizor hook = 2 entries; got {post:?}"
        );

        // The first entry is the preserved non-tokenizor hook.
        let first_cmd = post[0]["hooks"][0]["command"].as_str().unwrap();
        assert_eq!(
            first_cmd, "/some/other/hook bash",
            "non-tokenizor hook must be preserved"
        );
    }

    // --- test_init_migrates_old_three_entry_format ---

    #[test]
    fn test_init_migrates_old_three_entry_format() {
        // Old 3-entry format from Phase 5.
        let old_binary = "/usr/local/bin/tokenizor";
        let initial = json!({
            "hooks": {
                "PostToolUse": [
                    {
                        "matcher": "Read",
                        "hooks": [{"type": "command", "command": format!("{old_binary} hook read"), "timeout": 5}]
                    },
                    {
                        "matcher": "Edit|Write",
                        "hooks": [{"type": "command", "command": format!("{old_binary} hook edit"), "timeout": 5}]
                    },
                    {
                        "matcher": "Grep",
                        "hooks": [{"type": "command", "command": format!("{old_binary} hook grep"), "timeout": 5}]
                    }
                ]
            }
        });

        let result = run_merge(initial);
        let post = result["hooks"]["PostToolUse"].as_array().unwrap();

        // All 3 old entries must be replaced by exactly 1 new entry.
        assert_eq!(
            post.len(),
            1,
            "migration must replace 3 old entries with 1 new entry; got {post:?}"
        );

        let cmd = post[0]["hooks"][0]["command"].as_str().unwrap();
        assert_eq!(
            cmd, "/usr/local/bin/tokenizor hook",
            "migrated entry must use new no-subcommand command format"
        );

        let matcher = post[0]["matcher"].as_str().unwrap();
        assert_eq!(
            matcher, "Read|Edit|Write|Grep",
            "migrated entry must use full matcher"
        );
    }

    // --- test_init_idempotent ---

    #[test]
    fn test_init_idempotent() {
        let mut settings = json!({});
        merge_tokenizor_hooks(&mut settings, FAKE_BINARY);
        let after_first = settings.clone();

        merge_tokenizor_hooks(&mut settings, FAKE_BINARY);
        let after_second = settings.clone();

        assert_eq!(
            after_first, after_second,
            "running merge twice must produce identical output (idempotent)"
        );
    }

    #[test]
    fn test_init_idempotent_entry_count() {
        let mut settings = json!({});
        merge_tokenizor_hooks(&mut settings, FAKE_BINARY);
        let count_first = settings["hooks"]["PostToolUse"].as_array().unwrap().len();

        merge_tokenizor_hooks(&mut settings, FAKE_BINARY);
        let count_second = settings["hooks"]["PostToolUse"].as_array().unwrap().len();

        assert_eq!(
            count_first, count_second,
            "second merge must not add duplicate tokenizor entries"
        );
    }

    // --- test_init_replaces_stale_tokenizor_entries ---

    #[test]
    fn test_init_replaces_stale_tokenizor_entries() {
        let old_binary = "/old/path/to/tokenizor";
        let new_binary = "/new/path/to/tokenizor";

        // Set up settings with the old binary path.
        let initial = json!({
            "hooks": {
                "PostToolUse": [
                    {
                        "matcher": "Read",
                        "hooks": [{"type": "command", "command": format!("{old_binary} hook read"), "timeout": 5}]
                    }
                ]
            }
        });

        let mut settings = initial;
        merge_tokenizor_hooks(&mut settings, new_binary);

        let post = settings["hooks"]["PostToolUse"].as_array().unwrap();

        // Old entry must be gone.
        let has_old = post.iter().any(|e| {
            e["hooks"][0]["command"]
                .as_str()
                .map(|c| c.contains(old_binary))
                .unwrap_or(false)
        });
        assert!(
            !has_old,
            "stale tokenizor entry with old binary path must be removed"
        );

        // New entry must be present.
        let has_new = post.iter().any(|e| {
            e["hooks"][0]["command"]
                .as_str()
                .map(|c| c.contains(new_binary))
                .unwrap_or(false)
        });
        assert!(
            has_new,
            "new tokenizor entry with new binary path must be present"
        );
    }

    // --- is_tokenizor_entry ---

    #[test]
    fn test_is_tokenizor_entry_detects_tokenizor_command() {
        let entry = json!({
            "matcher": "Read",
            "hooks": [{"type": "command", "command": "/path/tokenizor hook read"}]
        });
        assert!(is_tokenizor_entry(&entry));
    }

    #[test]
    fn test_is_tokenizor_entry_detects_tokenizor_mcp_binary() {
        let entry = json!({
            "matcher": "Read|Edit|Write|Grep",
            "hooks": [{"type": "command", "command": "C:/Users/user/node_modules/tokenizor-mcp/bin/tokenizor-mcp.exe hook"}]
        });
        assert!(
            is_tokenizor_entry(&entry),
            "must detect tokenizor-mcp.exe binary name"
        );
    }

    #[test]
    fn test_is_tokenizor_entry_ignores_non_tokenizor() {
        let entry = json!({
            "matcher": "Bash",
            "hooks": [{"type": "command", "command": "/some/other/script bash"}]
        });
        assert!(!is_tokenizor_entry(&entry));
    }

    #[test]
    fn test_merge_adds_allowed_tools() {
        let mut settings = json!({});
        merge_tokenizor_hooks(&mut settings, "/usr/bin/tokenizor-mcp");
        let allowed = settings["allowedTools"]
            .as_array()
            .expect("allowedTools should be array");
        assert!(
            allowed
                .iter()
                .any(|v| v.as_str() == Some("mcp__tokenizor__search_symbols")),
            "should include search_symbols, got: {allowed:?}"
        );
        assert!(
            allowed
                .iter()
                .any(|v| v.as_str() == Some("mcp__tokenizor__get_symbol")),
            "should include get_symbol"
        );
        let first_len = allowed.len();
        // Should not duplicate on re-run
        merge_tokenizor_hooks(&mut settings, "/usr/bin/tokenizor-mcp");
        let allowed2 = settings["allowedTools"].as_array().unwrap();
        assert_eq!(first_len, allowed2.len(), "should not duplicate entries");
    }

    #[test]
    fn test_codex_registration_includes_allow_list() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("config.toml");
        register_codex_mcp_server(&config_path, "/usr/bin/tokenizor-mcp").unwrap();
        let content = std::fs::read_to_string(&config_path).unwrap();
        assert!(
            content.contains("search_symbols"),
            "should contain tool names: {content}"
        );
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

    #[test]
    fn test_gemini_registration_includes_allowed_tools() {
        let dir = tempfile::tempdir().unwrap();
        let settings_path = dir.path().join("settings.json");
        register_gemini_mcp_server(&settings_path, "/usr/bin/tokenizor-mcp").unwrap();
        let content = std::fs::read_to_string(&settings_path).unwrap();
        let config: Value = serde_json::from_str(&content).unwrap();
        let allowed = config["allowedTools"]
            .as_array()
            .expect("allowedTools must be array");
        assert!(
            allowed
                .iter()
                .any(|v| v.as_str() == Some("mcp__tokenizor__search_symbols")),
            "should include search_symbols in allowedTools"
        );
    }

    #[test]
    fn test_gemini_registration_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        let settings_path = dir.path().join("settings.json");
        register_gemini_mcp_server(&settings_path, "/usr/bin/tokenizor-mcp").unwrap();
        register_gemini_mcp_server(&settings_path, "/usr/bin/tokenizor-mcp").unwrap();
        let content = std::fs::read_to_string(&settings_path).unwrap();
        let config: Value = serde_json::from_str(&content).unwrap();
        let allowed = config["allowedTools"]
            .as_array()
            .expect("allowedTools must be array");
        // Count occurrences of a specific tool to verify no duplicates
        let count = allowed
            .iter()
            .filter(|v| v.as_str() == Some("mcp__tokenizor__search_symbols"))
            .count();
        assert_eq!(
            count, 1,
            "allowedTools should not have duplicate entries after idempotent run"
        );
    }
}
