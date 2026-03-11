//! `tokenizor init` command — idempotent hook installation into ~/.claude/settings.json.
//!
//! Strategy:
//! 1. Discover the absolute path of the running tokenizor binary.
//! 2. Read (or create) `~/.claude/settings.json`.
//! 3. Merge tokenizor hook entries, replacing any stale tokenizor entries while
//!    preserving all non-tokenizor hooks.
//! 4. Write the result back with pretty-printing.
//! 5. Create `.tokenizor/` in the current working directory (runtime needs it).
//!
//! Identification: any hook entry whose `hooks[].command` contains the substring
//! `"tokenizor hook"` is considered a tokenizor-owned entry and will be replaced.

use std::path::PathBuf;

use anyhow::Context;
use serde_json::{json, Value};

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Entry point called by main.rs for `tokenizor init`.
pub fn run_init() -> anyhow::Result<()> {
    // Step 1 — discover binary path.
    let binary_path_str = discover_binary_path();
    let binary_path = std::path::Path::new(&binary_path_str);

    // Step 2 — resolve settings path.
    let settings_path = settings_json_path()?;

    // Steps 3-8 — delegate to the testable merge function.
    merge_hooks_into_settings(&settings_path, binary_path)?;

    // Step 9 — create .tokenizor/ directory in cwd if missing.
    std::fs::create_dir_all(".tokenizor")
        .context("creating .tokenizor/ directory in current working directory")?;

    // Step 10 — report to stderr only (stdout must stay clean for hook output purity).
    eprintln!("tokenizor hooks installed in {}", settings_path.display());

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

    let hooks = settings["hooks"].as_object_mut().expect("hooks is an object");

    // Build fresh tokenizor entries.
    let post_tool_use_entries = build_post_tool_use_entries(binary_path);
    let session_start_entries = build_session_start_entries(binary_path);

    merge_event_entries(hooks, "PostToolUse", post_tool_use_entries);
    merge_event_entries(hooks, "SessionStart", session_start_entries);
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

// ---------------------------------------------------------------------------
// Merge helpers
// ---------------------------------------------------------------------------

/// Returns `true` if a hook entry array contains the substring "tokenizor hook"
/// in any of its `command` fields.
fn is_tokenizor_entry(entry: &Value) -> bool {
    if let Some(hooks) = entry["hooks"].as_array() {
        hooks.iter().any(|h| {
            h["command"]
                .as_str()
                .map(|cmd| cmd.contains("tokenizor hook"))
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

// ---------------------------------------------------------------------------
// Path helpers
// ---------------------------------------------------------------------------

/// Returns the path to `~/.claude/settings.json`.
fn settings_json_path() -> anyhow::Result<PathBuf> {
    let home = dirs::home_dir().context("cannot determine home directory")?;
    Ok(home.join(".claude").join("settings.json"))
}

/// Returns the binary path as a forward-slash string suitable for embedding in
/// JSON command strings.
///
/// On Windows, `current_exe()` may return backslash paths or paths through
/// node_modules. We normalise to forward slashes for cross-platform JSON
/// compatibility.
fn discover_binary_path() -> String {
    match std::env::current_exe() {
        Ok(path) => {
            let s = path.display().to_string();
            // Warn if the binary looks like it's running through a node wrapper.
            if s.contains("node_modules") || s.ends_with(".cmd") {
                eprintln!(
                    "warning: tokenizor binary path looks like a node wrapper ({s}); \
                     hooks may not work correctly"
                );
            }
            // Normalise backslashes to forward slashes for JSON command strings.
            s.replace('\\', "/")
        }
        Err(e) => {
            eprintln!("warning: could not determine tokenizor binary path: {e}");
            "tokenizor".to_string()
        }
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

        assert_eq!(post.len(), 1, "PostToolUse must have 1 entry (single stdin-routed entry)");
        assert_eq!(session.len(), 1, "SessionStart must have 1 entry");
    }

    #[test]
    fn test_init_entries_have_correct_commands() {
        let result = run_merge(json!({}));

        let post = &result["hooks"]["PostToolUse"];
        let entry = &post[0];
        let cmd = entry["hooks"][0]["command"].as_str().unwrap();
        assert_eq!(
            cmd,
            "/usr/local/bin/tokenizor hook",
            "Single PostToolUse hook command must have no subcommand suffix"
        );

        let session = &result["hooks"]["SessionStart"][0];
        let session_cmd = session["hooks"][0]["command"].as_str().unwrap();
        assert_eq!(session_cmd, "/usr/local/bin/tokenizor hook session-start");
    }

    #[test]
    fn test_init_new_entry_matcher_includes_write() {
        let result = run_merge(json!({}));
        let matcher = result["hooks"]["PostToolUse"][0]["matcher"].as_str().unwrap();
        assert_eq!(matcher, "Read|Edit|Write|Grep", "matcher must include Write");
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
        assert_eq!(first_cmd, "/some/other/hook bash", "non-tokenizor hook must be preserved");
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
            cmd,
            "/usr/local/bin/tokenizor hook",
            "migrated entry must use new no-subcommand command format"
        );

        let matcher = post[0]["matcher"].as_str().unwrap();
        assert_eq!(matcher, "Read|Edit|Write|Grep", "migrated entry must use full matcher");
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
        assert!(!has_old, "stale tokenizor entry with old binary path must be removed");

        // New entry must be present.
        let has_new = post.iter().any(|e| {
            e["hooks"][0]["command"]
                .as_str()
                .map(|c| c.contains(new_binary))
                .unwrap_or(false)
        });
        assert!(has_new, "new tokenizor entry with new binary path must be present");
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
    fn test_is_tokenizor_entry_ignores_non_tokenizor() {
        let entry = json!({
            "matcher": "Bash",
            "hooks": [{"type": "command", "command": "/some/other/script bash"}]
        });
        assert!(!is_tokenizor_entry(&entry));
    }
}
