/// Integration tests for `tokenizor init` — proves idempotent hook installation.
///
/// Tests use a temporary directory in place of `~/.claude/settings.json` via the
/// `merge_hooks_into_settings(settings_path, binary_path)` public function.
use tempfile::TempDir;
use tokenizor_agentic_mcp::cli::InitClient;
use tokenizor_agentic_mcp::cli::init::{
    merge_hooks_into_settings, register_codex_mcp_server, run_init_with_context,
};

const FAKE_BINARY: &str = "/usr/local/bin/tokenizor";

fn fake_binary_path() -> std::path::PathBuf {
    std::path::PathBuf::from(FAKE_BINARY)
}

/// Read settings.json from the temp dir.
fn read_settings(dir: &TempDir) -> serde_json::Value {
    let path = dir.path().join("settings.json");
    let raw = std::fs::read_to_string(&path).expect("settings.json must exist");
    serde_json::from_str(&raw).expect("settings.json must be valid JSON")
}

fn read_text(path: &std::path::Path) -> String {
    std::fs::read_to_string(path).expect("text file must exist")
}

// ---------------------------------------------------------------------------
// test_init_writes_hooks: init produces correct hook entries
// ---------------------------------------------------------------------------

#[test]
fn test_init_writes_hooks() {
    let dir = TempDir::new().unwrap();
    let settings_path = dir.path().join("settings.json");

    merge_hooks_into_settings(&settings_path, &fake_binary_path())
        .expect("merge_hooks_into_settings must succeed");

    let settings = read_settings(&dir);

    let post = settings["hooks"]["PostToolUse"]
        .as_array()
        .expect("PostToolUse must be an array");
    let session = settings["hooks"]["SessionStart"]
        .as_array()
        .expect("SessionStart must be an array");
    let prompt = settings["hooks"]["UserPromptSubmit"]
        .as_array()
        .expect("UserPromptSubmit must be an array");

    assert_eq!(
        post.len(),
        1,
        "PostToolUse must have 1 entry (single stdin-routed entry)"
    );
    assert_eq!(session.len(), 1, "SessionStart must have 1 entry");
    assert_eq!(prompt.len(), 1, "UserPromptSubmit must have 1 entry");

    // Verify each entry has the correct binary path embedded.
    let all_commands: Vec<&str> = post
        .iter()
        .chain(session.iter())
        .flat_map(|e| e["hooks"].as_array().unwrap())
        .filter_map(|h| h["command"].as_str())
        .collect();

    for cmd in &all_commands {
        assert!(
            cmd.contains("tokenizor hook"),
            "command must contain 'tokenizor hook': {cmd}"
        );
        assert!(
            cmd.contains(FAKE_BINARY),
            "command must contain binary path {FAKE_BINARY}: {cmd}"
        );
    }

    // Verify the PostToolUse matcher covers all tools.
    let matcher = post[0]["matcher"].as_str().unwrap();
    assert_eq!(
        matcher, "Read|Edit|Write|Grep",
        "matcher must cover all tools"
    );

    // Verify session-start hook is present.
    let has_session = all_commands
        .iter()
        .any(|c| c.ends_with("hook session-start"));
    assert!(has_session, "SessionStart hook must be present");
    let has_prompt_submit = prompt
        .iter()
        .flat_map(|e| e["hooks"].as_array().unwrap())
        .filter_map(|h| h["command"].as_str())
        .any(|c| c.ends_with("hook prompt-submit"));
    assert!(has_prompt_submit, "UserPromptSubmit hook must be present");
}

// ---------------------------------------------------------------------------
// test_init_idempotent: running init twice produces identical output
// ---------------------------------------------------------------------------

#[test]
fn test_init_idempotent() {
    let dir = TempDir::new().unwrap();
    let settings_path = dir.path().join("settings.json");

    merge_hooks_into_settings(&settings_path, &fake_binary_path())
        .expect("first merge must succeed");
    let after_first = std::fs::read_to_string(&settings_path).unwrap();

    merge_hooks_into_settings(&settings_path, &fake_binary_path())
        .expect("second merge must succeed");
    let after_second = std::fs::read_to_string(&settings_path).unwrap();

    assert_eq!(
        after_first, after_second,
        "running merge_hooks_into_settings twice must produce identical output (idempotent)"
    );

    // Also assert entry count didn't grow.
    let settings = read_settings(&dir);
    let post_count = settings["hooks"]["PostToolUse"].as_array().unwrap().len();
    assert_eq!(post_count, 1, "second merge must not add duplicate entries");
}

// ---------------------------------------------------------------------------
// test_init_preserves_other_hooks: non-tokenizor hooks are preserved
// ---------------------------------------------------------------------------

#[test]
fn test_init_preserves_other_hooks() {
    let dir = TempDir::new().unwrap();
    let settings_path = dir.path().join("settings.json");

    // Start with an existing non-tokenizor hook.
    let initial = serde_json::json!({
        "hooks": {
            "PostToolUse": [
                {
                    "matcher": "Bash",
                    "hooks": [{"type": "command", "command": "/some/other/hook bash", "timeout": 10}]
                }
            ]
        }
    });
    std::fs::write(
        &settings_path,
        serde_json::to_string_pretty(&initial).unwrap(),
    )
    .unwrap();

    merge_hooks_into_settings(&settings_path, &fake_binary_path()).expect("merge must succeed");

    let settings = read_settings(&dir);
    let post = settings["hooks"]["PostToolUse"]
        .as_array()
        .expect("PostToolUse must be an array");

    // 1 existing + 1 tokenizor = 2 total.
    assert_eq!(
        post.len(),
        2,
        "existing hook + 1 tokenizor hook = 2 entries"
    );

    // Non-tokenizor hook must still be present.
    let has_bash_hook = post.iter().any(|e| {
        e["hooks"][0]["command"]
            .as_str()
            .map(|c| c == "/some/other/hook bash")
            .unwrap_or(false)
    });
    assert!(
        has_bash_hook,
        "non-tokenizor hook must be preserved after merge"
    );
}

// ---------------------------------------------------------------------------
// test_init_registers_mcp_server: MCP entry written to claude.json
// ---------------------------------------------------------------------------

#[test]
fn test_init_registers_mcp_server() {
    let dir = TempDir::new().unwrap();
    let claude_json_path = dir.path().join(".claude.json");
    let binary_path = "/usr/local/bin/tokenizor";

    tokenizor_agentic_mcp::cli::init::register_mcp_server(&claude_json_path, binary_path)
        .expect("register_mcp_server must succeed");

    let raw = std::fs::read_to_string(&claude_json_path).unwrap();
    let config: serde_json::Value = serde_json::from_str(&raw).unwrap();

    let tok = &config["mcpServers"]["tokenizor"];
    assert_eq!(tok["type"], "stdio");
    // On Windows, forward slashes are converted to backslashes for native process spawning.
    let expected_command = if cfg!(windows) {
        binary_path.replace('/', "\\")
    } else {
        binary_path.to_string()
    };
    assert_eq!(tok["command"], expected_command);
}

#[test]
fn test_init_mcp_registration_idempotent() {
    let dir = TempDir::new().unwrap();
    let claude_json_path = dir.path().join(".claude.json");
    let binary_path = "/usr/local/bin/tokenizor";

    tokenizor_agentic_mcp::cli::init::register_mcp_server(&claude_json_path, binary_path).unwrap();
    let first = std::fs::read_to_string(&claude_json_path).unwrap();

    tokenizor_agentic_mcp::cli::init::register_mcp_server(&claude_json_path, binary_path).unwrap();
    let second = std::fs::read_to_string(&claude_json_path).unwrap();

    assert_eq!(first, second, "register_mcp_server must be idempotent");
}

#[test]
fn test_init_mcp_registration_preserves_other_servers() {
    let dir = TempDir::new().unwrap();
    let claude_json_path = dir.path().join(".claude.json");

    // Pre-populate with another MCP server.
    let initial = serde_json::json!({
        "mcpServers": {
            "other-server": {"type": "stdio", "command": "other-binary"}
        }
    });
    std::fs::write(
        &claude_json_path,
        serde_json::to_string_pretty(&initial).unwrap(),
    )
    .unwrap();

    tokenizor_agentic_mcp::cli::init::register_mcp_server(
        &claude_json_path,
        "/usr/local/bin/tokenizor",
    )
    .unwrap();

    let raw = std::fs::read_to_string(&claude_json_path).unwrap();
    let config: serde_json::Value = serde_json::from_str(&raw).unwrap();

    assert!(
        config["mcpServers"]["other-server"].is_object(),
        "other MCP server must be preserved"
    );
    assert!(
        config["mcpServers"]["tokenizor"].is_object(),
        "tokenizor must be added"
    );
}

#[test]
fn test_init_registers_codex_mcp_server() {
    let dir = TempDir::new().unwrap();
    let codex_config_path = dir.path().join(".codex").join("config.toml");
    let binary_path = r"C:\Users\user\.tokenizor\bin\tokenizor-mcp.exe";

    register_codex_mcp_server(&codex_config_path, binary_path)
        .expect("register_codex_mcp_server must succeed");

    let raw = std::fs::read_to_string(&codex_config_path).unwrap();

    assert!(
        raw.contains("[mcp_servers.tokenizor]"),
        "config must contain a tokenizor MCP table: {raw}"
    );
    assert!(
        raw.contains(binary_path),
        "config must contain the Windows binary path: {raw}"
    );
    assert!(
        raw.contains("startup_timeout_sec"),
        "config must tune Codex MCP startup timeout: {raw}"
    );
    assert!(
        raw.contains("tool_timeout_sec"),
        "config must tune Codex MCP tool timeout: {raw}"
    );
    assert!(
        raw.contains("project_doc_fallback_filenames"),
        "config must configure project doc fallbacks: {raw}"
    );
    assert!(
        raw.contains("CLAUDE.md"),
        "config must include CLAUDE.md as a project doc fallback: {raw}"
    );
}

#[test]
fn test_init_codex_registration_idempotent() {
    let dir = TempDir::new().unwrap();
    let codex_config_path = dir.path().join(".codex").join("config.toml");
    let binary_path = r"C:\Users\user\.tokenizor\bin\tokenizor-mcp.exe";

    register_codex_mcp_server(&codex_config_path, binary_path).unwrap();
    let first = std::fs::read_to_string(&codex_config_path).unwrap();

    register_codex_mcp_server(&codex_config_path, binary_path).unwrap();
    let second = std::fs::read_to_string(&codex_config_path).unwrap();

    assert_eq!(
        first, second,
        "register_codex_mcp_server must be idempotent"
    );
}

#[test]
fn test_init_codex_registration_preserves_other_config() {
    let dir = TempDir::new().unwrap();
    let codex_dir = dir.path().join(".codex");
    let codex_config_path = codex_dir.join("config.toml");
    std::fs::create_dir_all(&codex_dir).unwrap();
    std::fs::write(
        &codex_config_path,
        r#"# keep this comment
model = "gpt-5.4"
project_doc_fallback_filenames = ["README.agent.md"]

[mcp_servers.other]
command = "other.exe"
"#,
    )
    .unwrap();

    register_codex_mcp_server(
        &codex_config_path,
        r"C:\Users\user\.tokenizor\bin\tokenizor-mcp.exe",
    )
    .unwrap();

    let raw = std::fs::read_to_string(&codex_config_path).unwrap();
    assert!(
        raw.contains("# keep this comment"),
        "existing comments should survive"
    );
    assert!(
        raw.contains("model = \"gpt-5.4\""),
        "existing config should survive"
    );
    assert!(
        raw.contains("[mcp_servers.other]"),
        "other MCP servers should survive"
    );
    assert!(
        raw.contains("README.agent.md"),
        "existing project doc fallbacks should survive"
    );
    assert!(
        raw.contains("CLAUDE.md"),
        "Tokenizor should merge CLAUDE.md into project doc fallbacks"
    );
}

#[test]
fn test_run_init_codex_only_updates_codex_files() {
    let home = TempDir::new().unwrap();
    let cwd = TempDir::new().unwrap();
    let binary_path = std::path::PathBuf::from(FAKE_BINARY);

    run_init_with_context(InitClient::Codex, home.path(), cwd.path(), &binary_path)
        .expect("codex init must succeed");

    assert!(
        home.path().join(".codex").join("config.toml").exists(),
        "Codex config must be created"
    );
    assert!(
        home.path().join(".codex").join("AGENTS.md").exists(),
        "Codex global AGENTS guidance must be created"
    );
    assert!(
        !home.path().join(".claude.json").exists(),
        "Claude MCP config must not be created for codex-only init"
    );
    assert!(
        !home.path().join(".claude").join("settings.json").exists(),
        "Claude hooks config must not be created for codex-only init"
    );
    assert!(
        !home.path().join(".claude").join("CLAUDE.md").exists(),
        "Claude memory file must not be created for codex-only init"
    );
    assert!(
        cwd.path().join(".tokenizor").exists(),
        "runtime directory must still be created"
    );
}

#[test]
fn test_run_init_claude_only_updates_claude_files() {
    let home = TempDir::new().unwrap();
    let cwd = TempDir::new().unwrap();
    let binary_path = std::path::PathBuf::from(FAKE_BINARY);

    run_init_with_context(InitClient::Claude, home.path(), cwd.path(), &binary_path)
        .expect("claude init must succeed");

    assert!(
        home.path().join(".claude.json").exists(),
        "Claude MCP config must be created"
    );
    assert!(
        home.path().join(".claude").join("settings.json").exists(),
        "Claude hooks config must be created"
    );
    assert!(
        home.path().join(".claude").join("CLAUDE.md").exists(),
        "Claude guidance memory must be created"
    );
    assert!(
        !home.path().join(".codex").join("config.toml").exists(),
        "Codex config must not be created for claude-only init"
    );
    assert!(
        !home.path().join(".codex").join("AGENTS.md").exists(),
        "Codex AGENTS guidance must not be created for claude-only init"
    );
}

#[test]
fn test_run_init_all_updates_both_clients() {
    let home = TempDir::new().unwrap();
    let cwd = TempDir::new().unwrap();
    let binary_path = std::path::PathBuf::from(FAKE_BINARY);

    run_init_with_context(InitClient::All, home.path(), cwd.path(), &binary_path)
        .expect("all-client init must succeed");

    assert!(
        home.path().join(".codex").join("config.toml").exists(),
        "Codex config must be created"
    );
    assert!(
        home.path().join(".claude.json").exists(),
        "Claude MCP config must be created"
    );
    assert!(
        home.path().join(".claude").join("settings.json").exists(),
        "Claude hooks config must be created"
    );
    assert!(
        home.path().join(".claude").join("CLAUDE.md").exists(),
        "Claude guidance memory must be created"
    );
    assert!(
        home.path().join(".codex").join("AGENTS.md").exists(),
        "Codex AGENTS guidance must be created"
    );
}

#[test]
fn test_run_init_codex_writes_tokenizor_agents_guidance() {
    let home = TempDir::new().unwrap();
    let cwd = TempDir::new().unwrap();
    let binary_path = std::path::PathBuf::from(FAKE_BINARY);

    run_init_with_context(InitClient::Codex, home.path(), cwd.path(), &binary_path)
        .expect("codex init must succeed");

    let agents_path = home.path().join(".codex").join("AGENTS.md");
    let raw = read_text(&agents_path);

    assert!(
        raw.contains("TOKENIZOR START"),
        "Codex AGENTS guidance must include a Tokenizor marker block: {raw}"
    );
    assert!(
        raw.contains("Prefer the Tokenizor MCP"),
        "Codex AGENTS guidance must teach Codex to use Tokenizor: {raw}"
    );
}

#[test]
fn test_run_init_codex_preserves_existing_agents_content_and_is_idempotent() {
    let home = TempDir::new().unwrap();
    let cwd = TempDir::new().unwrap();
    let binary_path = std::path::PathBuf::from(FAKE_BINARY);
    let codex_dir = home.path().join(".codex");
    std::fs::create_dir_all(&codex_dir).unwrap();
    let agents_path = codex_dir.join("AGENTS.md");
    std::fs::write(&agents_path, "# Existing guidance\n\nKeep this line.\n").unwrap();

    run_init_with_context(InitClient::Codex, home.path(), cwd.path(), &binary_path)
        .expect("first codex init must succeed");
    let first = read_text(&agents_path);

    run_init_with_context(InitClient::Codex, home.path(), cwd.path(), &binary_path)
        .expect("second codex init must succeed");
    let second = read_text(&agents_path);

    assert!(
        second.contains("Keep this line."),
        "existing Codex guidance must survive"
    );
    assert_eq!(first, second, "Codex AGENTS guidance must be idempotent");
}

#[test]
fn test_run_init_claude_writes_tokenizor_memory_guidance() {
    let home = TempDir::new().unwrap();
    let cwd = TempDir::new().unwrap();
    let binary_path = std::path::PathBuf::from(FAKE_BINARY);

    run_init_with_context(InitClient::Claude, home.path(), cwd.path(), &binary_path)
        .expect("claude init must succeed");

    let memory_path = home.path().join(".claude").join("CLAUDE.md");
    let raw = read_text(&memory_path);

    assert!(
        raw.contains("TOKENIZOR START"),
        "Claude memory guidance must include a Tokenizor marker block: {raw}"
    );
    assert!(
        raw.contains("Prefer the Tokenizor MCP"),
        "Claude memory guidance must teach Claude to use Tokenizor: {raw}"
    );
}

#[test]
fn test_run_init_claude_preserves_existing_memory_content_and_is_idempotent() {
    let home = TempDir::new().unwrap();
    let cwd = TempDir::new().unwrap();
    let binary_path = std::path::PathBuf::from(FAKE_BINARY);
    let claude_dir = home.path().join(".claude");
    std::fs::create_dir_all(&claude_dir).unwrap();
    let memory_path = claude_dir.join("CLAUDE.md");
    std::fs::write(&memory_path, "# Existing memory\n\nKeep this line.\n").unwrap();

    run_init_with_context(InitClient::Claude, home.path(), cwd.path(), &binary_path)
        .expect("first claude init must succeed");
    let first = read_text(&memory_path);

    run_init_with_context(InitClient::Claude, home.path(), cwd.path(), &binary_path)
        .expect("second claude init must succeed");
    let second = read_text(&memory_path);

    assert!(
        second.contains("Keep this line."),
        "existing Claude memory must survive"
    );
    assert_eq!(first, second, "Claude memory guidance must be idempotent");
}
