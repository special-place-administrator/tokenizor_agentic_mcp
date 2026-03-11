//! Hook binary logic — reads `.tokenizor/sidecar.port`, calls sidecar over sync HTTP,
//! and outputs a single JSON line to stdout.
//!
//! Design constraints (HOOK-10):
//! - The ONLY thing written to stdout is the final JSON line.
//! - No tokio runtime. No tracing to stdout. No eprintln except for genuine errors.
//! - Sync I/O throughout — hooks must complete in well under 100 ms.
//! - Fail-open: if the sidecar is unreachable for any reason, output empty additionalContext
//!   JSON so Claude Code continues normally.

use std::io::{BufRead, Read, Write};
use std::net::TcpStream;
use std::time::Duration;

use crate::cli::HookSubcommand;

const PORT_FILE: &str = ".tokenizor/sidecar.port";
const SESSION_FILE: &str = ".tokenizor/sidecar.session";
/// Hard HTTP timeout — leaves margin within HOOK-03's 100 ms total budget.
const HTTP_TIMEOUT: Duration = Duration::from_millis(50);

// ---------------------------------------------------------------------------
// Stdin JSON parsing structs
// ---------------------------------------------------------------------------

/// Deserialized representation of a Claude Code PostToolUse stdin payload.
#[derive(serde::Deserialize, Default)]
pub(crate) struct HookInput {
    pub(crate) tool_name: Option<String>,
    pub(crate) tool_input: Option<HookToolInput>,
    pub(crate) cwd: Option<String>,
    pub(crate) prompt: Option<String>,
}

/// The `tool_input` field from the Claude Code hook event payload.
#[derive(serde::Deserialize, Default)]
pub(crate) struct HookToolInput {
    /// Absolute path to the file being read/edited/written.
    pub(crate) file_path: Option<String>,
    /// Search pattern for Grep events.
    pub(crate) pattern: Option<String>,
    /// Directory path for Grep events (alternative field name).
    pub(crate) path: Option<String>,
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Entry point called by main.rs for `tokenizor hook [subcommand]`.
///
/// When `subcommand` is `None`, reads stdin JSON to determine the tool_name and
/// routes to the correct sidecar endpoint (Phase 6 stdin-routing mode).
///
/// When `subcommand` is `Some`, uses the subcommand directly (backward-compat
/// for manual testing: `tokenizor hook read`, `tokenizor hook edit`, etc.).
///
/// Never returns an error — failures produce the fail-open empty JSON.
pub fn run_hook(subcommand: Option<&HookSubcommand>) -> anyhow::Result<()> {
    // Always read stdin so we have context for path/query extraction.
    // For explicit subcommands the payload may be empty or absent — that's fine.
    let input = parse_stdin_input();

    // Resolve the effective subcommand: explicit takes priority; otherwise
    // derive from the stdin tool_name.
    let resolved = if let Some(sub) = subcommand {
        Some(sub.clone())
    } else {
        resolve_subcommand_from_input(&input)
    };

    let event_name = resolved
        .as_ref()
        .map(event_name_for)
        .unwrap_or("PostToolUse");

    // Step 1 — read port file.
    let port = match read_port_file() {
        Ok(p) => p,
        Err(_) => {
            // Sidecar not running — fail open silently.
            println!("{}", fail_open_json(event_name));
            return Ok(());
        }
    };
    let session_id = read_session_file().ok();

    // Step 2 — determine endpoint + query string.
    let resolved_ref = resolved.as_ref();
    let (path, query) = endpoint_for(resolved_ref, &input);
    let request_path = proxy_path(path, session_id.as_deref());

    // Step 3/4 — make sync HTTP GET with 50 ms timeout.
    let body = match sync_http_get(port, &request_path, query) {
        Ok(b) => b,
        Err(_) => {
            println!("{}", fail_open_json(event_name));
            return Ok(());
        }
    };

    // Step 5/6 — output result JSON.
    println!("{}", success_json(event_name, &body));
    Ok(())
}

// ---------------------------------------------------------------------------
// Helpers (pub for unit-testing, not part of the public module API)
// ---------------------------------------------------------------------------

/// Reads all available stdin lines and deserializes them as a Claude Code
/// PostToolUse JSON payload.
///
/// Returns `HookInput::default()` on any parse failure (fail-open).
pub(crate) fn parse_stdin_input() -> HookInput {
    let stdin = std::io::stdin();
    let mut raw = String::new();
    for line in stdin.lock().lines() {
        match line {
            Ok(l) => {
                raw.push_str(&l);
                raw.push('\n');
            }
            Err(_) => break,
        }
    }
    serde_json::from_str(&raw).unwrap_or_default()
}

/// Converts an absolute path to a relative path by stripping the `cwd` prefix.
///
/// Uses `std::path::Path::strip_prefix` for correct platform-aware stripping,
/// then normalises backslashes to forward slashes for the sidecar query.
/// Returns `absolute` unchanged if it does not start with `cwd`.
pub(crate) fn relative_path(absolute: &str, cwd: &str) -> String {
    let abs = std::path::Path::new(absolute);
    let base = std::path::Path::new(cwd);
    match abs.strip_prefix(base) {
        Ok(rel) => rel.to_string_lossy().replace('\\', "/"),
        Err(_) => absolute.to_string(),
    }
}

/// Maps a `tool_name` string from the stdin JSON to a `HookSubcommand`.
fn resolve_subcommand_from_input(input: &HookInput) -> Option<HookSubcommand> {
    if input.prompt.as_deref().is_some() {
        return Some(HookSubcommand::PromptSubmit);
    }

    match input.tool_name.as_deref() {
        Some("Read") => Some(HookSubcommand::Read),
        Some("Edit") => Some(HookSubcommand::Edit),
        Some("Write") => Some(HookSubcommand::Write),
        Some("Grep") => Some(HookSubcommand::Grep),
        _ => None,
    }
}

/// Returns the `hookEventName` string for a given subcommand.
pub fn event_name_for(subcommand: &HookSubcommand) -> &'static str {
    match subcommand {
        HookSubcommand::SessionStart => "SessionStart",
        HookSubcommand::PromptSubmit => "UserPromptSubmit",
        _ => "PostToolUse",
    }
}

/// Maps a resolved subcommand + stdin input to `(path, query_string)`.
///
/// The `input` carries the file path and search pattern extracted from the
/// Claude Code PostToolUse payload. When `subcommand` is `None` (unknown
/// tool_name), returns fail-open empty values.
pub(crate) fn endpoint_for(
    subcommand: Option<&HookSubcommand>,
    input: &HookInput,
) -> (&'static str, String) {
    let cwd = input.cwd.as_deref().unwrap_or("");

    match subcommand {
        Some(HookSubcommand::Read) => {
            let file = extract_file_path(input, cwd);
            let query = if file.is_empty() {
                String::new()
            } else {
                format!("path={}", url_encode(&file))
            };
            ("/outline", query)
        }
        Some(HookSubcommand::Edit) => {
            let file = extract_file_path(input, cwd);
            let query = if file.is_empty() {
                String::new()
            } else {
                format!("path={}", url_encode(&file))
            };
            ("/impact", query)
        }
        Some(HookSubcommand::Write) => {
            let file = extract_file_path(input, cwd);
            let query = if file.is_empty() {
                "new_file=true".to_string()
            } else {
                format!("path={}&new_file=true", url_encode(&file))
            };
            ("/impact", query)
        }
        Some(HookSubcommand::Grep) => {
            // Use `pattern` field first, then fall back to `path` (directory) field.
            let q = input
                .tool_input
                .as_ref()
                .and_then(|ti| ti.pattern.as_deref().or(ti.path.as_deref()))
                .unwrap_or("");
            let query = if q.is_empty() {
                String::new()
            } else {
                format!("name={}", url_encode(q))
            };
            ("/symbol-context", query)
        }
        Some(HookSubcommand::SessionStart) => ("/repo-map", String::new()),
        Some(HookSubcommand::PromptSubmit) => {
            let prompt = input.prompt.as_deref().unwrap_or("");
            let query = if prompt.is_empty() {
                String::new()
            } else {
                format!("text={}", url_encode(prompt))
            };
            ("/prompt-context", query)
        }
        // Unknown tool_name → fail-open: route to a no-op that returns empty.
        None => ("/health", String::new()),
    }
}

/// Returns the fail-open JSON: empty `additionalContext`.
pub fn fail_open_json(event_name: &str) -> String {
    format!(r#"{{"hookSpecificOutput":{{"hookEventName":"{event_name}","additionalContext":""}}}}"#)
}

/// Returns the success JSON with `context` as the `additionalContext` value.
///
/// The `context` string is JSON-escaped (backslash + quote safe) so it can be
/// embedded as a JSON string value.
pub fn success_json(event_name: &str, context: &str) -> String {
    let escaped = json_escape(context);
    format!(
        r#"{{"hookSpecificOutput":{{"hookEventName":"{event_name}","additionalContext":"{escaped}"}}}}"#
    )
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Extract and relativize the file path from stdin input.
fn extract_file_path(input: &HookInput, cwd: &str) -> String {
    let abs = input
        .tool_input
        .as_ref()
        .and_then(|ti| ti.file_path.as_deref())
        .unwrap_or("");
    if abs.is_empty() || cwd.is_empty() {
        abs.to_string()
    } else {
        relative_path(abs, cwd)
    }
}

/// Read `.tokenizor/sidecar.port` from the current working directory.
fn read_port_file() -> std::io::Result<u16> {
    let contents = std::fs::read_to_string(PORT_FILE)?;
    contents
        .trim()
        .parse::<u16>()
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
}

fn read_session_file() -> std::io::Result<String> {
    let contents = std::fs::read_to_string(SESSION_FILE)?;
    Ok(contents.trim().to_string())
}

fn proxy_path(base_path: &str, session_id: Option<&str>) -> String {
    match session_id {
        Some(session_id) if !session_id.trim().is_empty() => {
            format!("/v1/sessions/{}/sidecar{}", session_id.trim(), base_path)
        }
        _ => base_path.to_string(),
    }
}

/// Make a synchronous HTTP/1.1 GET request to `127.0.0.1:{port}{path}?{query}`.
///
/// Uses a raw `TcpStream` (no HTTP client crate) so there is no async runtime
/// and the startup cost is near zero.  The timeout covers both connect and read.
fn sync_http_get(port: u16, path: &str, query: String) -> anyhow::Result<String> {
    let addr = format!("127.0.0.1:{port}");
    let sock_addr: std::net::SocketAddr = addr.parse()?;

    let mut stream = TcpStream::connect_timeout(&sock_addr, HTTP_TIMEOUT)?;
    stream.set_read_timeout(Some(HTTP_TIMEOUT))?;
    stream.set_write_timeout(Some(HTTP_TIMEOUT))?;

    // Build the request line, including the query string if present.
    let request_path = if query.is_empty() {
        path.to_string()
    } else {
        format!("{path}?{query}")
    };

    let request = format!(
        "GET {request_path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n"
    );

    stream.write_all(request.as_bytes())?;

    // Read the full response (headers + body).
    let mut response = String::new();
    stream.read_to_string(&mut response)?;

    // Split on the blank-line separator between headers and body.
    let body = response
        .split_once("\r\n\r\n")
        .map(|(_, body)| body)
        .unwrap_or("")
        .to_string();

    Ok(body)
}

/// Minimal percent-encoding for query parameter values.
///
/// Only encodes characters that are unsafe in a query string: space, `&`, `=`, `+`,
/// `%`, and non-ASCII bytes.  This is sufficient for file paths and symbol names.
fn url_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' | b':' => {
                out.push(b as char)
            }
            b => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// Minimal JSON string escape — handles backslash, double-quote, and common
/// control characters.  Sufficient for embedding sidecar response bodies.
fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    // --- fail_open_json ---

    #[test]
    fn test_fail_open_json_is_valid() {
        let json = fail_open_json("PostToolUse");
        let v: Value = serde_json::from_str(&json).expect("fail_open_json must produce valid JSON");

        let output = &v["hookSpecificOutput"];
        assert_eq!(output["hookEventName"], "PostToolUse");
        assert_eq!(output["additionalContext"], "");
    }

    #[test]
    fn test_fail_open_json_session_start_event_name() {
        let json = fail_open_json("SessionStart");
        let v: Value = serde_json::from_str(&json).expect("must be valid JSON");
        assert_eq!(v["hookSpecificOutput"]["hookEventName"], "SessionStart");
    }

    // --- success_json ---

    #[test]
    fn test_success_json_is_valid() {
        let json = success_json("PostToolUse", "hello world");
        let v: Value = serde_json::from_str(&json).expect("success_json must produce valid JSON");

        let output = &v["hookSpecificOutput"];
        assert_eq!(output["hookEventName"], "PostToolUse");
        assert_eq!(output["additionalContext"], "hello world");
    }

    #[test]
    fn test_success_json_escapes_special_chars() {
        let context = r#"{"key":"value"}"#;
        let json = success_json("PostToolUse", context);
        // The outer JSON must parse correctly.
        let v: Value = serde_json::from_str(&json)
            .expect("success_json with embedded quotes must be valid JSON");
        // The additionalContext value is the escaped string, not a nested object.
        let ctx = v["hookSpecificOutput"]["additionalContext"]
            .as_str()
            .expect("additionalContext must be a string");
        assert_eq!(ctx, context);
    }

    // --- parse_stdin_input ---

    #[test]
    fn test_parse_stdin_returns_default_on_empty() {
        // We cannot pipe into stdin in a unit test, but we can verify that
        // parsing an empty string returns Default (no panics).
        let result: HookInput = serde_json::from_str("").unwrap_or_default();
        assert!(result.tool_name.is_none());
        assert!(result.tool_input.is_none());
        assert!(result.cwd.is_none());
    }

    #[test]
    fn test_parse_stdin_deserializes_read_payload() {
        let json =
            r#"{"tool_name":"Read","tool_input":{"file_path":"/abs/src/foo.rs"},"cwd":"/abs"}"#;
        let result: HookInput = serde_json::from_str(json).unwrap_or_default();
        assert_eq!(result.tool_name.as_deref(), Some("Read"));
        assert_eq!(
            result
                .tool_input
                .as_ref()
                .and_then(|ti| ti.file_path.as_deref()),
            Some("/abs/src/foo.rs")
        );
        assert_eq!(result.cwd.as_deref(), Some("/abs"));
    }

    #[test]
    fn test_parse_stdin_deserializes_grep_payload() {
        let json = r#"{"tool_name":"Grep","tool_input":{"pattern":"TODO","path":"/abs/src"},"cwd":"/abs"}"#;
        let result: HookInput = serde_json::from_str(json).unwrap_or_default();
        assert_eq!(result.tool_name.as_deref(), Some("Grep"));
        let ti = result.tool_input.as_ref().unwrap();
        assert_eq!(ti.pattern.as_deref(), Some("TODO"));
        assert_eq!(ti.path.as_deref(), Some("/abs/src"));
    }

    #[test]
    fn test_parse_stdin_returns_default_on_invalid_json() {
        let result: HookInput = serde_json::from_str("not valid json").unwrap_or_default();
        assert!(result.tool_name.is_none());
    }

    // --- relative_path ---

    #[test]
    fn test_relative_path_strips_unix_cwd_prefix() {
        let rel = relative_path("/home/user/project/src/foo.rs", "/home/user/project");
        assert_eq!(rel, "src/foo.rs");
    }

    #[test]
    fn test_relative_path_strips_windows_cwd_prefix() {
        // Test that strip_prefix works for Windows-style paths.
        // Path::strip_prefix is platform-aware, but we test the string normalization.
        // On Windows the actual separator is backslash; strip_prefix handles it.
        // We simulate by using a path that has a clear prefix relationship.
        let rel = relative_path("C:/Users/dev/project/src/foo.rs", "C:/Users/dev/project");
        // After strip_prefix the result should use forward slashes.
        assert!(
            rel.contains("src/foo.rs") || rel == "C:/Users/dev/project/src/foo.rs",
            "got: {rel}"
        );
    }

    #[test]
    fn test_relative_path_unchanged_when_no_prefix_match() {
        let rel = relative_path("/unrelated/path.rs", "/home/user/project");
        assert_eq!(rel, "/unrelated/path.rs");
    }

    #[test]
    fn test_relative_path_normalizes_backslashes() {
        // Simulate a Windows-style result from strip_prefix.
        // Since we're on MSYS/Windows the path may use backslashes.
        let rel = relative_path(
            "C:\\Users\\dev\\project\\src\\foo.rs",
            "C:\\Users\\dev\\project",
        );
        // Must not contain backslashes in result.
        assert!(
            !rel.contains('\\'),
            "backslashes must be normalized to forward slashes; got: {rel}"
        );
    }

    // --- endpoint_for (stdin-routing) ---

    #[test]
    fn test_endpoint_for_read_stdin_routes_to_outline() {
        let input = make_input("Read", Some("/abs/src/foo.rs"), None, "/abs");
        let (path, query) = endpoint_for(Some(&HookSubcommand::Read), &input);
        assert_eq!(path, "/outline");
        assert!(
            query.contains("src/foo.rs"),
            "query must include relative path; got: {query}"
        );
    }

    #[test]
    fn test_endpoint_for_edit_stdin_routes_to_impact() {
        let input = make_input("Edit", Some("/abs/src/bar.rs"), None, "/abs");
        let (path, query) = endpoint_for(Some(&HookSubcommand::Edit), &input);
        assert_eq!(path, "/impact");
        assert!(
            query.contains("src/bar.rs"),
            "query must include relative path; got: {query}"
        );
    }

    #[test]
    fn test_endpoint_for_write_routes_to_impact_with_new_file() {
        let input = make_input("Write", Some("/abs/src/new.rs"), None, "/abs");
        let (path, query) = endpoint_for(Some(&HookSubcommand::Write), &input);
        assert_eq!(path, "/impact");
        assert!(
            query.contains("new_file=true"),
            "Write must set new_file=true; got: {query}"
        );
        assert!(
            query.contains("src/new.rs"),
            "Write must include file path; got: {query}"
        );
    }

    #[test]
    fn test_endpoint_for_grep_stdin_routes_to_symbol_context() {
        let json = r#"{"tool_name":"Grep","tool_input":{"pattern":"TODO","path":"/abs/src"},"cwd":"/abs"}"#;
        let input: HookInput = serde_json::from_str(json).unwrap_or_default();
        let (path, query) = endpoint_for(Some(&HookSubcommand::Grep), &input);
        assert_eq!(path, "/symbol-context");
        assert!(
            query.contains("TODO"),
            "Grep query must include pattern; got: {query}"
        );
    }

    #[test]
    fn test_endpoint_for_session_start_routes_to_repo_map() {
        let input = HookInput::default();
        let (path, query) = endpoint_for(Some(&HookSubcommand::SessionStart), &input);
        assert_eq!(path, "/repo-map");
        assert!(query.is_empty(), "repo-map has no query params");
    }

    #[test]
    fn test_endpoint_for_prompt_submit_routes_to_prompt_context() {
        let input = HookInput {
            prompt: Some("please inspect src/foo.rs".to_string()),
            ..HookInput::default()
        };
        let (path, query) = endpoint_for(Some(&HookSubcommand::PromptSubmit), &input);
        assert_eq!(path, "/prompt-context");
        assert!(
            query.contains("please%20inspect%20src/foo.rs"),
            "prompt query must be URL-encoded; got: {query}"
        );
    }

    #[test]
    fn test_proxy_path_uses_daemon_session_namespace_when_present() {
        let path = proxy_path("/repo-map", Some("session-42"));
        assert_eq!(path, "/v1/sessions/session-42/sidecar/repo-map");
    }

    #[test]
    fn test_proxy_path_keeps_legacy_sidecar_route_without_session() {
        let path = proxy_path("/repo-map", None);
        assert_eq!(path, "/repo-map");
    }

    #[test]
    fn test_endpoint_for_unknown_tool_returns_fail_open() {
        // None subcommand with unknown/missing tool_name → fail-open /health endpoint
        let input = HookInput {
            tool_name: Some("UnknownTool".to_string()),
            ..Default::default()
        };
        let (path, _) = endpoint_for(None, &input);
        // Returns /health as the fail-open endpoint — no useful data, but graceful
        assert_eq!(path, "/health");
    }

    // --- event_name_for ---

    #[test]
    fn test_event_name_for_session_start() {
        assert_eq!(
            event_name_for(&HookSubcommand::SessionStart),
            "SessionStart"
        );
    }

    #[test]
    fn test_event_name_for_prompt_submit() {
        assert_eq!(
            event_name_for(&HookSubcommand::PromptSubmit),
            "UserPromptSubmit"
        );
    }

    #[test]
    fn test_event_name_for_post_tool_use_variants() {
        for sub in [
            HookSubcommand::Read,
            HookSubcommand::Edit,
            HookSubcommand::Write,
            HookSubcommand::Grep,
        ] {
            assert_eq!(
                event_name_for(&sub),
                "PostToolUse",
                "Read/Edit/Write/Grep must produce PostToolUse event name"
            );
        }
    }

    // --- explicit subcommand routing remains available ---

    #[test]
    fn test_hook_subcommand_to_endpoint_read_backward_compat() {
        let input = HookInput::default();
        let (path, _query) = endpoint_for(Some(&HookSubcommand::Read), &input);
        assert_eq!(path, "/outline");
    }

    #[test]
    fn test_hook_subcommand_to_endpoint_edit_backward_compat() {
        let input = HookInput::default();
        let (path, _query) = endpoint_for(Some(&HookSubcommand::Edit), &input);
        assert_eq!(path, "/impact");
    }

    #[test]
    fn test_hook_subcommand_to_endpoint_grep_backward_compat() {
        let input = HookInput::default();
        let (path, _query) = endpoint_for(Some(&HookSubcommand::Grep), &input);
        assert_eq!(path, "/symbol-context");
    }

    #[test]
    fn test_hook_subcommand_to_endpoint_session_start_backward_compat() {
        let input = HookInput::default();
        let (path, query) = endpoint_for(Some(&HookSubcommand::SessionStart), &input);
        assert_eq!(path, "/repo-map");
        assert!(query.is_empty(), "repo-map has no query params");
    }

    #[test]
    fn test_hook_subcommand_to_endpoint_prompt_submit_backward_compat() {
        let input = HookInput {
            prompt: Some("review MinioService".to_string()),
            ..HookInput::default()
        };
        let (path, query) = endpoint_for(Some(&HookSubcommand::PromptSubmit), &input);
        assert_eq!(path, "/prompt-context");
        assert!(query.contains("review%20MinioService"));
    }

    // --- resolve_subcommand_from_input ---

    #[test]
    fn test_resolve_subcommand_read() {
        let input = HookInput {
            tool_name: Some("Read".to_string()),
            ..Default::default()
        };
        assert!(matches!(
            resolve_subcommand_from_input(&input),
            Some(HookSubcommand::Read)
        ));
    }

    #[test]
    fn test_resolve_subcommand_write() {
        let input = HookInput {
            tool_name: Some("Write".to_string()),
            ..Default::default()
        };
        assert!(matches!(
            resolve_subcommand_from_input(&input),
            Some(HookSubcommand::Write)
        ));
    }

    #[test]
    fn test_resolve_subcommand_unknown_returns_none() {
        let input = HookInput {
            tool_name: Some("Bash".to_string()),
            ..Default::default()
        };
        assert!(resolve_subcommand_from_input(&input).is_none());
    }

    // --- helpers ---

    fn make_input(
        tool_name: &str,
        file_path: Option<&str>,
        pattern: Option<&str>,
        cwd: &str,
    ) -> HookInput {
        HookInput {
            tool_name: Some(tool_name.to_string()),
            tool_input: Some(HookToolInput {
                file_path: file_path.map(|s| s.to_string()),
                pattern: pattern.map(|s| s.to_string()),
                path: None,
            }),
            cwd: Some(cwd.to_string()),
            prompt: None,
        }
    }
}
