/// All 10 MCP tool handler methods and their input parameter structs.
///
/// Each handler follows the pattern:
/// 1. Acquire read lock (or write lock for `index_folder`)
/// 2. Check loading guard (except `health` which always responds)
/// 3. Extract needed data into owned values
/// 4. Drop lock
/// 5. Call `format::` function
/// 6. Return `String`
///
/// Anti-patterns avoided (per RESEARCH.md):
/// - Never return JSON — always plain text String (AD-6)
/// - Never use MCP error codes for not-found — return helpful text via format functions
/// - Never hold RwLockReadGuard across await points — extract into owned values first
use std::path::PathBuf;
use std::sync::Arc;

use rmcp::handler::server::wrapper::Parameters;
use rmcp::{tool, tool_router};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::live_index::store::IndexState;
use crate::protocol::format;

use super::TokenizorServer;

// ─── Input parameter structs ────────────────────────────────────────────────

/// Input for `get_file_outline`.
#[derive(Deserialize, JsonSchema)]
pub struct GetFileOutlineInput {
    /// Relative path to the file (e.g. "src/lib.rs").
    pub path: String,
}

/// Input for `get_symbol`.
#[derive(Deserialize, JsonSchema)]
pub struct GetSymbolInput {
    /// Relative path to the file.
    pub path: String,
    /// Symbol name to look up.
    pub name: String,
    /// Optional kind filter: "fn", "struct", "enum", "impl", etc.
    pub kind: Option<String>,
}

/// A single target in a `get_symbols` batch request.
///
/// Either provide `name` (symbol lookup) or `start_byte`/`end_byte` (code slice).
#[derive(Deserialize, JsonSchema)]
pub struct SymbolTarget {
    /// Relative file path.
    pub path: String,
    /// Symbol name for symbol lookup (mutually exclusive with byte range).
    pub name: Option<String>,
    /// Kind filter for symbol lookup (e.g., "fn", "struct").
    pub kind: Option<String>,
    /// Start byte offset for code slice (mutually exclusive with name).
    pub start_byte: Option<u32>,
    /// End byte offset for code slice (inclusive).
    pub end_byte: Option<u32>,
}

/// Input for `get_symbols` (batch).
#[derive(Deserialize, JsonSchema)]
pub struct GetSymbolsInput {
    /// List of symbol or code-slice targets.
    pub targets: Vec<SymbolTarget>,
}

/// Input for `search_symbols` and `search_text`.
#[derive(Deserialize, JsonSchema)]
pub struct SearchInput {
    /// Search query (case-insensitive substring match).
    pub query: String,
}

/// Input for `index_folder`.
#[derive(Deserialize, JsonSchema)]
pub struct IndexFolderInput {
    /// Absolute or relative path to the directory to index.
    pub path: String,
}

/// Input for `what_changed`.
#[derive(Deserialize, JsonSchema)]
pub struct WhatChangedInput {
    /// Unix timestamp (seconds since epoch). Files newer than this are returned.
    pub since: i64,
}

/// Input for `get_file_content`.
#[derive(Deserialize, JsonSchema)]
pub struct GetFileContentInput {
    /// Relative path to the file.
    pub path: String,
    /// First line to include (1-indexed).
    pub start_line: Option<u32>,
    /// Last line to include (1-indexed, inclusive).
    pub end_line: Option<u32>,
}

/// Input for `find_references`.
#[derive(Deserialize, JsonSchema)]
pub struct FindReferencesInput {
    /// Symbol name to find references for.
    pub name: String,
    /// Filter by reference kind: "call", "import", "type_usage", or "all" (default: "all").
    pub kind: Option<String>,
}

/// Input for `find_dependents`.
#[derive(Deserialize, JsonSchema)]
pub struct FindDependentsInput {
    /// Relative file path to find dependents for.
    pub path: String,
}

/// Input for `get_context_bundle`.
#[derive(Deserialize, JsonSchema)]
pub struct GetContextBundleInput {
    /// File path containing the symbol.
    pub path: String,
    /// Symbol name to get context for.
    pub name: String,
    /// Optional kind filter for the symbol lookup (e.g., "fn", "struct").
    pub kind: Option<String>,
}

// ─── Tool handlers ───────────────────────────────────────────────────────────

/// Loading guard helper — returns `Some(message)` when index is NOT ready.
///
/// Call at the top of every handler except `health`. If `Some` is returned,
/// return that string immediately. Otherwise continue with the handler body.
macro_rules! loading_guard {
    ($guard:expr) => {
        match $guard.index_state() {
            IndexState::Ready => {}
            IndexState::Empty => return format::empty_guard_message(),
            IndexState::Loading => return format::loading_guard_message(),
            IndexState::CircuitBreakerTripped { summary } => {
                return format!("Index degraded: {summary}");
            }
        }
    };
}

#[tool_router(vis = "pub(crate)")]
impl TokenizorServer {
    /// Return the symbol outline for a file. Shows functions, structs, classes with line ranges.
    #[tool(description = "Return the symbol outline for a file. Shows functions, structs, classes with line ranges.")]
    async fn get_file_outline(&self, params: Parameters<GetFileOutlineInput>) -> String {
        let guard = self.index.read().expect("lock poisoned");
        loading_guard!(guard);
        let result = format::file_outline(&guard, &params.0.path);
        drop(guard);
        result
    }

    /// Look up a specific symbol by file path and name. Returns full source code.
    #[tool(description = "Look up a specific symbol by file path and name. Returns full source code.")]
    async fn get_symbol(&self, params: Parameters<GetSymbolInput>) -> String {
        let guard = self.index.read().expect("lock poisoned");
        loading_guard!(guard);
        let result = format::symbol_detail(
            &guard,
            &params.0.path,
            &params.0.name,
            params.0.kind.as_deref(),
        );
        drop(guard);
        result
    }

    /// Batch lookup of symbols or code slices. Each target can be a symbol name or byte range.
    #[tool(description = "Batch lookup of symbols or code slices. Each target can be a symbol name or byte range.")]
    async fn get_symbols(&self, params: Parameters<GetSymbolsInput>) -> String {
        let guard = self.index.read().expect("lock poisoned");
        loading_guard!(guard);

        let mut results: Vec<String> = Vec::new();
        for target in &params.0.targets {
            let entry = match target.name.as_deref() {
                Some(name) => {
                    // Symbol lookup
                    format::symbol_detail(&guard, &target.path, name, target.kind.as_deref())
                }
                None => {
                    // Code slice by byte range
                    match guard.get_file(&target.path) {
                        None => format::not_found_file(&target.path),
                        Some(file) => {
                            let start = target.start_byte.unwrap_or(0) as usize;
                            let end = target.end_byte.map(|e| e as usize).unwrap_or(file.content.len());
                            let end = end.min(file.content.len());
                            let slice = &file.content[start.min(end)..end];
                            let text = String::from_utf8_lossy(slice).into_owned();
                            format!("{}\n{}", target.path, text)
                        }
                    }
                }
            };
            results.push(entry);
        }
        drop(guard);
        results.join("\n---\n")
    }

    /// Show the file tree with language and symbol counts per file.
    #[tool(description = "Show the file tree with language and symbol counts per file.")]
    async fn get_repo_outline(&self) -> String {
        let guard = self.index.read().expect("lock poisoned");
        loading_guard!(guard);
        let result = format::repo_outline(&guard, &self.project_name.clone());
        drop(guard);
        result
    }

    /// Search for symbols by name substring across all indexed files.
    #[tool(description = "Search for symbols by name substring across all indexed files.")]
    async fn search_symbols(&self, params: Parameters<SearchInput>) -> String {
        let guard = self.index.read().expect("lock poisoned");
        loading_guard!(guard);
        let result = format::search_symbols_result(&guard, &params.0.query);
        drop(guard);
        result
    }

    /// Full-text search across all indexed file contents.
    #[tool(description = "Full-text search across all indexed file contents.")]
    async fn search_text(&self, params: Parameters<SearchInput>) -> String {
        let guard = self.index.read().expect("lock poisoned");
        loading_guard!(guard);
        let result = format::search_text_result(&guard, &params.0.query);
        drop(guard);
        result
    }

    /// Report server health: index status, file counts, load duration, watcher state.
    ///
    /// When the HTTP sidecar is running, also reports token savings from hook fires this session.
    ///
    /// This tool always responds regardless of index state (no loading guard).
    #[tool(description = "Report server health: index status, file counts, load duration, watcher state.")]
    async fn health(&self) -> String {
        let guard = self.index.read().expect("lock poisoned");
        let watcher_guard = self.watcher_info.lock().unwrap();
        let mut result = format::health_report_with_watcher(&guard, &watcher_guard);
        drop(watcher_guard);
        drop(guard);

        // Append token savings section if the sidecar's TokenStats are available.
        if let Some(ref stats) = self.token_stats {
            let snap = stats.summary();
            let savings = format::format_token_savings(&snap);
            if !savings.is_empty() {
                result.push('\n');
                result.push_str(&savings);
            }
        }

        result
    }

    /// Reload the index from a directory path. Replaces current index entirely.
    #[tool(description = "Reload the index from a directory path. Replaces current index entirely.")]
    async fn index_folder(&self, params: Parameters<IndexFolderInput>) -> String {
        let root = PathBuf::from(&params.0.path);
        let mut guard = self.index.write().expect("lock poisoned");
        match guard.reload(&root) {
            Ok(()) => {
                let file_count = guard.file_count();
                let symbol_count = guard.symbol_count();
                drop(guard);

                // Restart the file watcher at the new root so freshness continues.
                crate::watcher::restart_watcher(
                    root.clone(),
                    Arc::clone(&self.index),
                    Arc::clone(&self.watcher_info),
                );
                tracing::info!(root = %root.display(), "file watcher restarted after index_folder");

                format!("Indexed {} files, {} symbols.", file_count, symbol_count)
            }
            Err(e) => format!("Index failed: {e}"),
        }
    }

    /// Show files changed since a Unix timestamp.
    #[tool(description = "Show files changed since a Unix timestamp.")]
    async fn what_changed(&self, params: Parameters<WhatChangedInput>) -> String {
        let guard = self.index.read().expect("lock poisoned");
        loading_guard!(guard);
        let result = format::what_changed_result(&guard, params.0.since);
        drop(guard);
        result
    }

    /// Serve file content from memory with optional line range.
    #[tool(description = "Serve file content from memory with optional line range.")]
    async fn get_file_content(&self, params: Parameters<GetFileContentInput>) -> String {
        let guard = self.index.read().expect("lock poisoned");
        loading_guard!(guard);
        let result = format::file_content(
            &guard,
            &params.0.path,
            params.0.start_line,
            params.0.end_line,
        );
        drop(guard);
        result
    }

    /// Find all references (call sites, imports, type usages) for a symbol across the codebase.
    #[tool(description = "Find all references (call sites, imports, type usages) for a symbol across the codebase")]
    async fn find_references(&self, params: Parameters<FindReferencesInput>) -> String {
        let guard = self.index.read().expect("lock poisoned");
        loading_guard!(guard);
        let input = &params.0;
        let result = format::find_references_result(&guard, &input.name, input.kind.as_deref());
        drop(guard);
        result
    }

    /// Find all files that import or depend on the given file.
    #[tool(description = "Find all files that import or depend on the given file")]
    async fn find_dependents(&self, params: Parameters<FindDependentsInput>) -> String {
        let guard = self.index.read().expect("lock poisoned");
        loading_guard!(guard);
        let input = &params.0;
        let result = format::find_dependents_result(&guard, &input.path);
        drop(guard);
        result
    }

    /// Get full context for a symbol: definition body, callers, callees, and type usages in one call.
    #[tool(description = "Get full context for a symbol: definition body, callers, callees, and type usages in one call")]
    async fn get_context_bundle(&self, params: Parameters<GetContextBundleInput>) -> String {
        let guard = self.index.read().expect("lock poisoned");
        loading_guard!(guard);
        let input = &params.0;
        let result =
            format::context_bundle_result(&guard, &input.path, &input.name, input.kind.as_deref());
        drop(guard);
        result
    }
}

// ─── Unit tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::{Arc, RwLock};
    use std::time::{Duration, Instant};

    use crate::domain::{LanguageId, SymbolKind, SymbolRecord};
    use crate::live_index::store::{
        CircuitBreakerState, IndexedFile, LiveIndex, ParseStatus,
    };
    use crate::protocol::TokenizorServer;
    use rmcp::handler::server::wrapper::Parameters;

    // ── Test helpers ─────────────────────────────────────────────────────────

    fn make_symbol(name: &str, kind: SymbolKind, line_start: u32, line_end: u32) -> SymbolRecord {
        SymbolRecord {
            name: name.to_string(),
            kind,
            depth: 0,
            sort_order: 0,
            byte_range: (0, 10),
            line_range: (line_start, line_end),
        }
    }

    fn make_file(path: &str, content: &[u8], symbols: Vec<SymbolRecord>) -> (String, IndexedFile) {
        (
            path.to_string(),
            IndexedFile {
                relative_path: path.to_string(),
                language: LanguageId::Rust,
                content: content.to_vec(),
                symbols,
                parse_status: ParseStatus::Parsed,
                byte_len: content.len() as u64,
                content_hash: "test".to_string(),
                references: vec![],
                alias_map: std::collections::HashMap::new(),
            },
        )
    }

    fn make_live_index_ready(files: Vec<(String, IndexedFile)>) -> LiveIndex {
        use crate::live_index::trigram::TrigramIndex;
        let files_map = files.into_iter().collect::<HashMap<_, _>>();
        let trigram_index = TrigramIndex::build_from_files(&files_map);
        LiveIndex {
            files: files_map,
            loaded_at: Instant::now(),
            loaded_at_system: std::time::SystemTime::now(),
            load_duration: Duration::from_millis(10),
            cb_state: CircuitBreakerState::new(0.20),
            is_empty: false,
            reverse_index: HashMap::new(),
            trigram_index,
        }
    }

    fn make_live_index_empty() -> LiveIndex {
        use crate::live_index::trigram::TrigramIndex;
        LiveIndex {
            files: HashMap::new(),
            loaded_at: Instant::now(),
            loaded_at_system: std::time::SystemTime::now(),
            load_duration: Duration::ZERO,
            cb_state: CircuitBreakerState::new(0.20),
            is_empty: true,
            reverse_index: HashMap::new(),
            trigram_index: TrigramIndex::new(),
        }
    }

    fn make_live_index_tripped() -> LiveIndex {
        use crate::live_index::trigram::TrigramIndex;
        let cb = CircuitBreakerState::new(0.10);
        for _ in 0..8 {
            cb.record_success();
        }
        for i in 0..2 {
            cb.record_failure(&format!("f{i}.rs"), "err");
        }
        cb.should_abort(); // trips at 20% > 10%
        LiveIndex {
            files: HashMap::new(),
            loaded_at: Instant::now(),
            loaded_at_system: std::time::SystemTime::now(),
            load_duration: Duration::ZERO,
            cb_state: cb,
            is_empty: false,
            reverse_index: HashMap::new(),
            trigram_index: TrigramIndex::new(),
        }
    }

    fn make_server(index: LiveIndex) -> TokenizorServer {
        use std::sync::Mutex;
        use crate::watcher::WatcherInfo;
        let shared = Arc::new(RwLock::new(index));
        let watcher_info = Arc::new(Mutex::new(WatcherInfo::default()));
        TokenizorServer::new(shared, "test_project".to_string(), watcher_info, None, None)
    }

    // ── Loading guard tests ───────────────────────────────────────────────────

    #[tokio::test]
    async fn test_loading_guard_empty_returns_empty_message() {
        let server = make_server(make_live_index_empty());
        // Any non-health tool should return the empty guard message
        let result = server
            .get_file_outline(Parameters(super::GetFileOutlineInput {
                path: "anything.rs".to_string(),
            }))
            .await;
        assert_eq!(
            result,
            crate::protocol::format::empty_guard_message(),
            "empty index should return empty guard message"
        );
    }

    #[tokio::test]
    async fn test_loading_guard_circuit_breaker_returns_degraded_message() {
        let server = make_server(make_live_index_tripped());
        let result = server
            .get_file_outline(Parameters(super::GetFileOutlineInput {
                path: "anything.rs".to_string(),
            }))
            .await;
        assert!(
            result.starts_with("Index degraded:"),
            "tripped CB should return degraded message, got: {result}"
        );
    }

    #[tokio::test]
    async fn test_health_always_responds_on_empty_index() {
        let server = make_server(make_live_index_empty());
        let result = server.health().await;
        // Health should NOT return the guard message; it should return actual health info
        assert!(
            !result.starts_with("Index not loaded"),
            "health should always respond, got: {result}"
        );
        assert!(
            result.contains("Status: Empty"),
            "health of empty index should show Empty, got: {result}"
        );
    }

    // ── Tool handler tests ────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_get_file_outline_delegates_to_formatter() {
        let sym = make_symbol("main", SymbolKind::Function, 1, 5);
        let (key, file) = make_file("src/main.rs", b"fn main() {}", vec![sym]);
        let server = make_server(make_live_index_ready(vec![(key, file)]));
        let result = server
            .get_file_outline(Parameters(super::GetFileOutlineInput {
                path: "src/main.rs".to_string(),
            }))
            .await;
        assert!(result.contains("src/main.rs"), "should contain path");
        assert!(result.contains("main"), "should contain symbol name");
    }

    #[tokio::test]
    async fn test_get_symbol_delegates_to_formatter() {
        let sym = make_symbol("foo", SymbolKind::Function, 1, 3);
        let (key, file) = make_file("src/lib.rs", b"fn foo() {}", vec![sym]);
        let server = make_server(make_live_index_ready(vec![(key, file)]));
        let result = server
            .get_symbol(Parameters(super::GetSymbolInput {
                path: "src/lib.rs".to_string(),
                name: "foo".to_string(),
                kind: None,
            }))
            .await;
        // Should return source body or not-found message — not a guard message
        assert!(
            !result.starts_with("Index"),
            "should not return guard message, got: {result}"
        );
    }

    #[tokio::test]
    async fn test_get_repo_outline_uses_project_name() {
        let (key, file) = make_file("src/main.rs", b"fn main() {}", vec![]);
        let server = make_server(make_live_index_ready(vec![(key, file)]));
        let result = server.get_repo_outline().await;
        assert!(
            result.contains("test_project"),
            "repo outline should use project_name, got: {result}"
        );
    }

    #[tokio::test]
    async fn test_search_symbols_returns_results() {
        let sym = make_symbol("find_user", SymbolKind::Function, 1, 5);
        let (key, file) = make_file("src/lib.rs", b"fn find_user() {}", vec![sym]);
        let server = make_server(make_live_index_ready(vec![(key, file)]));
        let result = server
            .search_symbols(Parameters(super::SearchInput {
                query: "find".to_string(),
            }))
            .await;
        assert!(
            result.contains("find_user"),
            "should find matching symbol, got: {result}"
        );
    }

    #[tokio::test]
    async fn test_search_text_returns_results() {
        let (key, file) = make_file("src/lib.rs", b"fn find_user() {}", vec![]);
        let server = make_server(make_live_index_ready(vec![(key, file)]));
        let result = server
            .search_text(Parameters(super::SearchInput {
                query: "find".to_string(),
            }))
            .await;
        assert!(
            result.contains("find_user"),
            "should find matching text, got: {result}"
        );
    }

    #[tokio::test]
    async fn test_health_returns_status_fields() {
        let (key, file) = make_file("src/lib.rs", b"fn foo() {}", vec![]);
        let server = make_server(make_live_index_ready(vec![(key, file)]));
        let result = server.health().await;
        assert!(result.contains("Status:"), "should have Status field");
        assert!(result.contains("Files:"), "should have Files field");
        assert!(result.contains("Symbols:"), "should have Symbols field");
    }

    #[tokio::test]
    async fn test_get_symbols_batch_symbol_lookup() {
        let sym = make_symbol("bar", SymbolKind::Function, 1, 3);
        let (key, file) = make_file("src/lib.rs", b"fn bar() {}", vec![sym]);
        let server = make_server(make_live_index_ready(vec![(key, file)]));
        let result = server
            .get_symbols(Parameters(super::GetSymbolsInput {
                targets: vec![super::SymbolTarget {
                    path: "src/lib.rs".to_string(),
                    name: Some("bar".to_string()),
                    kind: None,
                    start_byte: None,
                    end_byte: None,
                }],
            }))
            .await;
        assert!(
            !result.starts_with("Index"),
            "should not return guard message, got: {result}"
        );
    }

    #[tokio::test]
    async fn test_get_symbols_batch_code_slice() {
        let content = b"fn foo() { let x = 1; }";
        let (key, file) = make_file("src/lib.rs", content, vec![]);
        let server = make_server(make_live_index_ready(vec![(key, file)]));
        let result = server
            .get_symbols(Parameters(super::GetSymbolsInput {
                targets: vec![super::SymbolTarget {
                    path: "src/lib.rs".to_string(),
                    name: None,
                    kind: None,
                    start_byte: Some(0),
                    end_byte: Some(8),
                }],
            }))
            .await;
        assert!(
            result.contains("src/lib.rs"),
            "code slice should include path header, got: {result}"
        );
        assert!(
            result.contains("fn foo()"),
            "code slice should include content, got: {result}"
        );
    }

    #[tokio::test]
    async fn test_what_changed_returns_result() {
        let (key, file) = make_file("src/lib.rs", b"fn foo() {}", vec![]);
        let server = make_server(make_live_index_ready(vec![(key, file)]));
        // since=0 (far past) → all files are "newer"
        let result = server
            .what_changed(Parameters(super::WhatChangedInput { since: 0 }))
            .await;
        assert!(
            result.contains("src/lib.rs"),
            "what_changed since epoch should list all files, got: {result}"
        );
    }

    #[tokio::test]
    async fn test_get_file_content_returns_content() {
        let content = b"line 1\nline 2\nline 3";
        let (key, file) = make_file("src/lib.rs", content, vec![]);
        let server = make_server(make_live_index_ready(vec![(key, file)]));
        let result = server
            .get_file_content(Parameters(super::GetFileContentInput {
                path: "src/lib.rs".to_string(),
                start_line: None,
                end_line: None,
            }))
            .await;
        assert!(
            result.contains("line 1"),
            "should return file content, got: {result}"
        );
    }

    #[tokio::test]
    async fn test_get_file_content_not_found() {
        let server = make_server(make_live_index_ready(vec![]));
        let result = server
            .get_file_content(Parameters(super::GetFileContentInput {
                path: "nonexistent.rs".to_string(),
                start_line: None,
                end_line: None,
            }))
            .await;
        assert_eq!(result, "File not found: nonexistent.rs");
    }

    // ── INFR-05: No v1 tools in server ──────────────────────────────────────

    #[test]
    fn test_no_v1_tools_in_server() {
        // Build the tool list by inspecting what tool_router() generates
        let server = make_server(make_live_index_ready(vec![]));
        let router = server.tool_router.clone();
        let tool_names: Vec<String> = router
            .list_all()
            .iter()
            .map(|t| t.name.to_string())
            .collect();

        let v1_tools = [
            "cancel_index_run",
            "checkpoint_now",
            "resume_index_run",
            "get_provenance",
            "get_trust",
            "verify_chunk",
        ];

        for v1_tool in &v1_tools {
            assert!(
                !tool_names.iter().any(|n| n == v1_tool),
                "v1 tool '{v1_tool}' must not appear in server tool list (INFR-05); found: {tool_names:?}"
            );
        }
    }

    #[test]
    fn test_exactly_13_tools_registered() {
        let server = make_server(make_live_index_ready(vec![]));
        let tool_count = server.tool_router.list_all().len();
        assert_eq!(
            tool_count, 13,
            "server must expose exactly 13 tools; found {tool_count}"
        );
    }

    #[tokio::test]
    async fn test_find_references_loading_guard_empty() {
        let server = make_server(make_live_index_empty());
        let result = server
            .find_references(Parameters(super::FindReferencesInput {
                name: "process".to_string(),
                kind: None,
            }))
            .await;
        assert_eq!(result, crate::protocol::format::empty_guard_message());
    }

    #[tokio::test]
    async fn test_find_dependents_loading_guard_empty() {
        let server = make_server(make_live_index_empty());
        let result = server
            .find_dependents(Parameters(super::FindDependentsInput {
                path: "src/lib.rs".to_string(),
            }))
            .await;
        assert_eq!(result, crate::protocol::format::empty_guard_message());
    }

    #[tokio::test]
    async fn test_get_context_bundle_loading_guard_empty() {
        let server = make_server(make_live_index_empty());
        let result = server
            .get_context_bundle(Parameters(super::GetContextBundleInput {
                path: "src/lib.rs".to_string(),
                name: "process".to_string(),
                kind: None,
            }))
            .await;
        assert_eq!(result, crate::protocol::format::empty_guard_message());
    }

    #[tokio::test]
    async fn test_find_references_delegates_to_formatter() {
        let server = make_server(make_live_index_ready(vec![]));
        let result = server
            .find_references(Parameters(super::FindReferencesInput {
                name: "nonexistent_xyz".to_string(),
                kind: None,
            }))
            .await;
        // Should get "No references found" not a guard message
        assert!(result.contains("No references found"), "got: {result}");
    }

    #[tokio::test]
    async fn test_find_dependents_delegates_to_formatter() {
        let server = make_server(make_live_index_ready(vec![]));
        let result = server
            .find_dependents(Parameters(super::FindDependentsInput {
                path: "src/nonexistent.rs".to_string(),
            }))
            .await;
        assert!(result.contains("No dependents found"), "got: {result}");
    }
}
