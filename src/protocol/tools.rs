/// MCP tool handler methods and their input parameter structs.
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
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Command;
use std::sync::{Arc, RwLock};

use axum::http::StatusCode;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::{tool, tool_router};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::live_index::store::IndexState;
use crate::protocol::format;
use crate::sidecar::handlers::{
    ImpactParams, OutlineParams, SymbolContextParams, impact_tool_text, outline_tool_text,
    repo_map_text, symbol_context_tool_text,
};
use crate::sidecar::{SidecarState, TokenStats};

use super::TokenizorServer;

// ─── Input parameter structs ────────────────────────────────────────────────

/// Input for `get_file_outline`.
#[derive(Deserialize, Serialize, JsonSchema)]
pub struct GetFileOutlineInput {
    /// Relative path to the file (e.g. "src/lib.rs").
    pub path: String,
}

/// Input for `get_symbol`.
#[derive(Deserialize, Serialize, JsonSchema)]
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
#[derive(Deserialize, Serialize, JsonSchema)]
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
#[derive(Deserialize, Serialize, JsonSchema)]
pub struct GetSymbolsInput {
    /// List of symbol or code-slice targets.
    pub targets: Vec<SymbolTarget>,
}

/// Input for `search_symbols`.
#[derive(Deserialize, Serialize, JsonSchema)]
pub struct SearchSymbolsInput {
    /// Search query (case-insensitive substring match).
    pub query: String,
    /// Optional kind filter using display names such as `fn`, `class`, or `interface`.
    pub kind: Option<String>,
}

/// Input for `search_text`.
#[derive(Deserialize, Serialize, JsonSchema)]
pub struct SearchTextInput {
    /// Search query (case-insensitive substring match unless `regex` is true).
    pub query: Option<String>,
    /// Optional list of terms to match with OR semantics.
    pub terms: Option<Vec<String>>,
    /// Interpret `query` as a regex pattern instead of a literal substring.
    pub regex: Option<bool>,
}

/// Input for `index_folder`.
#[derive(Deserialize, Serialize, JsonSchema)]
pub struct IndexFolderInput {
    /// Absolute or relative path to the directory to index.
    pub path: String,
}

/// Input for `what_changed`.
#[derive(Deserialize, Serialize, JsonSchema)]
pub struct WhatChangedInput {
    /// Optional Unix timestamp (seconds since epoch). Files newer than this are returned.
    pub since: Option<i64>,
    /// Optional git ref to diff against, for example `HEAD~5` or `branch:main`.
    pub git_ref: Option<String>,
    /// When true, report uncommitted git changes. Defaults to true when no other mode is specified and a repo root exists.
    pub uncommitted: Option<bool>,
}

/// Input for `get_file_content`.
#[derive(Deserialize, Serialize, JsonSchema)]
pub struct GetFileContentInput {
    /// Relative path to the file.
    pub path: String,
    /// First line to include (1-indexed).
    pub start_line: Option<u32>,
    /// Last line to include (1-indexed, inclusive).
    pub end_line: Option<u32>,
}

/// Input for `find_references`.
#[derive(Deserialize, Serialize, JsonSchema)]
pub struct FindReferencesInput {
    /// Symbol name to find references for.
    pub name: String,
    /// Filter by reference kind: "call", "import", "type_usage", or "all" (default: "all").
    pub kind: Option<String>,
}

/// Input for `find_dependents`.
#[derive(Deserialize, Serialize, JsonSchema)]
pub struct FindDependentsInput {
    /// Relative file path to find dependents for.
    pub path: String,
}

/// Input for `get_file_tree`.
#[derive(Deserialize, Serialize, JsonSchema)]
pub struct GetFileTreeInput {
    /// Subtree path to browse (default: project root).
    pub path: Option<String>,
    /// Max depth levels to expand (default: 2, max: 5).
    pub depth: Option<u32>,
}

/// Input for `get_context_bundle`.
#[derive(Deserialize, Serialize, JsonSchema)]
pub struct GetContextBundleInput {
    /// File path containing the symbol.
    pub path: String,
    /// Symbol name to get context for.
    pub name: String,
    /// Optional kind filter for the symbol lookup (e.g., "fn", "struct").
    pub kind: Option<String>,
}

/// Input for `get_file_context`.
#[derive(Deserialize, Serialize, JsonSchema)]
pub struct GetFileContextInput {
    /// Relative path to the file.
    pub path: String,
    /// Optional max token budget, matching hook behavior.
    pub max_tokens: Option<u64>,
}

/// Input for `get_symbol_context`.
#[derive(Deserialize, Serialize, JsonSchema)]
pub struct GetSymbolContextInput {
    /// Symbol name to inspect.
    pub name: String,
    /// Optional file filter.
    pub file: Option<String>,
}

/// Input for `analyze_file_impact`.
#[derive(Deserialize, Serialize, JsonSchema)]
pub struct AnalyzeFileImpactInput {
    /// Relative path to the file to re-read from disk.
    pub path: String,
    /// When true, treat the file as newly created and index it.
    pub new_file: Option<bool>,
}

enum WhatChangedMode {
    Timestamp(i64),
    GitRef(String),
    Uncommitted,
}

fn determine_what_changed_mode(
    input: &WhatChangedInput,
    has_repo_root: bool,
) -> Result<WhatChangedMode, String> {
    if let Some(git_ref) = input
        .git_ref
        .as_deref()
        .map(str::trim)
        .filter(|git_ref| !git_ref.is_empty())
    {
        return if has_repo_root {
            Ok(WhatChangedMode::GitRef(
                git_ref
                    .strip_prefix("branch:")
                    .unwrap_or(git_ref)
                    .to_string(),
            ))
        } else {
            Err("Git change detection unavailable; pass `since` for timestamp mode.".to_string())
        };
    }

    if input.uncommitted.unwrap_or(false) || (input.since.is_none() && has_repo_root) {
        return if has_repo_root {
            Ok(WhatChangedMode::Uncommitted)
        } else {
            Err("Git change detection unavailable; pass `since` for timestamp mode.".to_string())
        };
    }

    if let Some(since) = input.since {
        Ok(WhatChangedMode::Timestamp(since))
    } else {
        Err(
            "what_changed requires either `since`, `git_ref`, or an available repo root."
                .to_string(),
        )
    }
}

fn run_git(repo_root: &std::path::Path, args: &[&str]) -> Result<String, String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo_root)
        .args(args)
        .output()
        .map_err(|error| format!("failed to start git: {error}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let message = if stderr.is_empty() {
            format!("git exited with status {}", output.status)
        } else {
            stderr
        };
        return Err(message);
    }

    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn parse_git_status_paths(output: &str) -> Vec<String> {
    output
        .lines()
        .filter_map(|line| {
            let raw_path = line.get(3..)?.trim();
            if raw_path.is_empty() {
                return None;
            }
            let normalized = raw_path
                .rsplit(" -> ")
                .next()
                .unwrap_or(raw_path)
                .trim_matches('"')
                .replace('\\', "/");
            if normalized.is_empty() {
                None
            } else {
                Some(normalized)
            }
        })
        .collect()
}

fn parse_git_name_only_paths(output: &str) -> Vec<String> {
    output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(|line| line.trim_matches('"').replace('\\', "/"))
        .collect()
}

fn sidecar_state_for_server(server: &TokenizorServer) -> SidecarState {
    SidecarState {
        index: Arc::clone(&server.index),
        token_stats: server.token_stats.clone().unwrap_or_else(TokenStats::new),
        repo_root: server.repo_root.clone(),
        symbol_cache: Arc::new(RwLock::new(HashMap::new())),
    }
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
    #[tool(
        description = "Return the symbol outline for a file. Shows functions, structs, classes with line ranges."
    )]
    pub(crate) async fn get_file_outline(&self, params: Parameters<GetFileOutlineInput>) -> String {
        if let Some(result) = self.proxy_tool_call("get_file_outline", &params.0).await {
            return result;
        }
        let guard = self.index.read().expect("lock poisoned");
        loading_guard!(guard);
        let result = format::file_outline(&guard, &params.0.path);
        drop(guard);
        result
    }

    /// Look up a specific symbol by file path and name. Returns full source code.
    #[tool(
        description = "Look up a specific symbol by file path and name. Returns full source code."
    )]
    pub(crate) async fn get_symbol(&self, params: Parameters<GetSymbolInput>) -> String {
        if let Some(result) = self.proxy_tool_call("get_symbol", &params.0).await {
            return result;
        }
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
    #[tool(
        description = "Batch lookup of symbols or code slices. Each target can be a symbol name or byte range."
    )]
    pub(crate) async fn get_symbols(&self, params: Parameters<GetSymbolsInput>) -> String {
        if let Some(result) = self.proxy_tool_call("get_symbols", &params.0).await {
            return result;
        }
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
                            let end = target
                                .end_byte
                                .map(|e| e as usize)
                                .unwrap_or(file.content.len());
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
    pub(crate) async fn get_repo_outline(&self) -> String {
        if let Some(result) = self
            .proxy_tool_call_without_params("get_repo_outline")
            .await
        {
            return result;
        }
        let guard = self.index.read().expect("lock poisoned");
        loading_guard!(guard);
        let result = format::repo_outline(&guard, &self.project_name.clone());
        drop(guard);
        result
    }

    /// Show the compact repo map used for session-start enrichment.
    #[tool(description = "Show the compact repo map used for session-start enrichment.")]
    pub(crate) async fn get_repo_map(&self) -> String {
        if let Some(result) = self.proxy_tool_call_without_params("get_repo_map").await {
            return result;
        }
        let guard = self.index.read().expect("lock poisoned");
        loading_guard!(guard);
        drop(guard);

        let state = sidecar_state_for_server(self);
        match repo_map_text(&state) {
            Ok(result) => result,
            Err(StatusCode::NOT_FOUND) => "Repository map unavailable.".to_string(),
            Err(StatusCode::INTERNAL_SERVER_ERROR) => {
                "Repository map failed: internal error.".to_string()
            }
            Err(other) => format!("Repository map failed: HTTP {}", other.as_u16()),
        }
    }

    /// Show enriched file context with symbol outline and key external references.
    #[tool(
        description = "Show enriched file context with symbol outline and key external references."
    )]
    pub(crate) async fn get_file_context(&self, params: Parameters<GetFileContextInput>) -> String {
        if let Some(result) = self.proxy_tool_call("get_file_context", &params.0).await {
            return result;
        }
        let guard = self.index.read().expect("lock poisoned");
        loading_guard!(guard);
        drop(guard);

        let state = sidecar_state_for_server(self);
        let outline = OutlineParams {
            path: params.0.path.clone(),
            max_tokens: params.0.max_tokens,
        };
        match outline_tool_text(&state, &outline) {
            Ok(result) => result,
            Err(StatusCode::NOT_FOUND) => format::not_found_file(&params.0.path),
            Err(StatusCode::INTERNAL_SERVER_ERROR) => {
                "File context failed: internal error.".to_string()
            }
            Err(other) => format!("File context failed: HTTP {}", other.as_u16()),
        }
    }

    /// Show grouped references for a symbol with enclosing-symbol annotations.
    #[tool(description = "Show grouped references for a symbol with enclosing-symbol annotations.")]
    pub(crate) async fn get_symbol_context(
        &self,
        params: Parameters<GetSymbolContextInput>,
    ) -> String {
        if let Some(result) = self.proxy_tool_call("get_symbol_context", &params.0).await {
            return result;
        }
        let guard = self.index.read().expect("lock poisoned");
        loading_guard!(guard);
        drop(guard);

        let state = sidecar_state_for_server(self);
        let symbol_context = SymbolContextParams {
            name: params.0.name.clone(),
            file: params.0.file.clone(),
        };
        match symbol_context_tool_text(&state, &symbol_context) {
            Ok(result) => result,
            Err(StatusCode::INTERNAL_SERVER_ERROR) => {
                "Symbol context failed: internal error.".to_string()
            }
            Err(other) => format!("Symbol context failed: HTTP {}", other.as_u16()),
        }
    }

    /// Re-read a file from disk, update the index, and report symbol impact.
    #[tool(description = "Re-read a file from disk, update the index, and report symbol impact.")]
    pub(crate) async fn analyze_file_impact(
        &self,
        params: Parameters<AnalyzeFileImpactInput>,
    ) -> String {
        if let Some(result) = self.proxy_tool_call("analyze_file_impact", &params.0).await {
            return result;
        }
        {
            let guard = self.index.read().expect("lock poisoned");
            loading_guard!(guard);
        }

        let state = sidecar_state_for_server(self);
        let impact = ImpactParams {
            path: params.0.path.clone(),
            new_file: params.0.new_file,
        };
        match impact_tool_text(state, &impact).await {
            Ok(result) => result,
            Err(StatusCode::NOT_FOUND) => format!("File not found on disk: {}", params.0.path),
            Err(StatusCode::INTERNAL_SERVER_ERROR) => {
                "Impact analysis failed: internal error.".to_string()
            }
            Err(other) => format!("Impact analysis failed: HTTP {}", other.as_u16()),
        }
    }

    /// Search for symbols by name substring across all indexed files.
    #[tool(description = "Search for symbols by name substring across all indexed files.")]
    pub(crate) async fn search_symbols(&self, params: Parameters<SearchSymbolsInput>) -> String {
        if let Some(result) = self.proxy_tool_call("search_symbols", &params.0).await {
            return result;
        }
        let guard = self.index.read().expect("lock poisoned");
        loading_guard!(guard);
        let result = format::search_symbols_result_with_kind(
            &guard,
            &params.0.query,
            params.0.kind.as_deref(),
        );
        drop(guard);
        result
    }

    /// Full-text search across all indexed file contents.
    #[tool(description = "Full-text search across all indexed file contents.")]
    pub(crate) async fn search_text(&self, params: Parameters<SearchTextInput>) -> String {
        if let Some(result) = self.proxy_tool_call("search_text", &params.0).await {
            return result;
        }
        let guard = self.index.read().expect("lock poisoned");
        loading_guard!(guard);
        let result = format::search_text_result_with_options(
            &guard,
            params.0.query.as_deref(),
            params.0.terms.as_deref(),
            params.0.regex.unwrap_or(false),
        );
        drop(guard);
        result
    }

    /// Report server health: index status, file counts, load duration, watcher state.
    ///
    /// When the HTTP sidecar is running, also reports token savings from hook fires this session.
    ///
    /// This tool always responds regardless of index state (no loading guard).
    #[tool(
        description = "Report server health: index status, file counts, load duration, watcher state."
    )]
    pub(crate) async fn health(&self) -> String {
        if let Some(result) = self.proxy_tool_call_without_params("health").await {
            return result;
        }
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
    #[tool(
        description = "Reload the index from a directory path. Replaces current index entirely."
    )]
    pub(crate) async fn index_folder(&self, params: Parameters<IndexFolderInput>) -> String {
        if let Some(result) = self.proxy_tool_call("index_folder", &params.0).await {
            return result;
        }
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

    /// Show files changed since a Unix timestamp, git ref, or current uncommitted state.
    #[tool(
        description = "Show files changed since a Unix timestamp, git ref, or current uncommitted state."
    )]
    pub(crate) async fn what_changed(&self, params: Parameters<WhatChangedInput>) -> String {
        if let Some(result) = self.proxy_tool_call("what_changed", &params.0).await {
            return result;
        }
        let mode = match determine_what_changed_mode(&params.0, self.repo_root.is_some()) {
            Ok(mode) => mode,
            Err(message) => return message,
        };

        match mode {
            WhatChangedMode::Timestamp(since_ts) => {
                let guard = self.index.read().expect("lock poisoned");
                loading_guard!(guard);
                let result = format::what_changed_result(&guard, since_ts);
                drop(guard);
                result
            }
            WhatChangedMode::Uncommitted => {
                let guard = self.index.read().expect("lock poisoned");
                loading_guard!(guard);
                drop(guard);

                let Some(repo_root) = self.repo_root.as_deref() else {
                    return "Git change detection unavailable; pass `since` for timestamp mode."
                        .to_string();
                };
                match run_git(
                    repo_root,
                    &["status", "--porcelain", "--untracked-files=all"],
                ) {
                    Ok(output) => format::what_changed_paths_result(
                        &parse_git_status_paths(&output),
                        "No uncommitted changes detected.",
                    ),
                    Err(error) => format!("Git change detection failed: {error}"),
                }
            }
            WhatChangedMode::GitRef(git_ref) => {
                let guard = self.index.read().expect("lock poisoned");
                loading_guard!(guard);
                drop(guard);

                let Some(repo_root) = self.repo_root.as_deref() else {
                    return "Git change detection unavailable; pass `since` for timestamp mode."
                        .to_string();
                };
                match run_git(repo_root, &["diff", "--name-only", &git_ref, "--"]) {
                    Ok(output) => format::what_changed_paths_result(
                        &parse_git_name_only_paths(&output),
                        &format!("No changes detected relative to git ref '{git_ref}'."),
                    ),
                    Err(error) => format!("Git change detection failed: {error}"),
                }
            }
        }
    }

    /// Serve file content from memory with optional line range.
    #[tool(description = "Serve file content from memory with optional line range.")]
    pub(crate) async fn get_file_content(&self, params: Parameters<GetFileContentInput>) -> String {
        if let Some(result) = self.proxy_tool_call("get_file_content", &params.0).await {
            return result;
        }
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
    #[tool(
        description = "Find all references (call sites, imports, type usages) for a symbol across the codebase"
    )]
    pub(crate) async fn find_references(&self, params: Parameters<FindReferencesInput>) -> String {
        if let Some(result) = self.proxy_tool_call("find_references", &params.0).await {
            return result;
        }
        let guard = self.index.read().expect("lock poisoned");
        loading_guard!(guard);
        let input = &params.0;
        let result = format::find_references_result(&guard, &input.name, input.kind.as_deref());
        drop(guard);
        result
    }

    /// Find all files that import or depend on the given file.
    #[tool(description = "Find all files that import or depend on the given file")]
    pub(crate) async fn find_dependents(&self, params: Parameters<FindDependentsInput>) -> String {
        if let Some(result) = self.proxy_tool_call("find_dependents", &params.0).await {
            return result;
        }
        let guard = self.index.read().expect("lock poisoned");
        loading_guard!(guard);
        let input = &params.0;
        let result = format::find_dependents_result(&guard, &input.path);
        drop(guard);
        result
    }

    /// Browse the source file tree with symbol counts per file and directory.
    #[tool(description = "Browse the source file tree with symbol counts per file and directory.")]
    pub(crate) async fn get_file_tree(&self, params: Parameters<GetFileTreeInput>) -> String {
        if let Some(result) = self.proxy_tool_call("get_file_tree", &params.0).await {
            return result;
        }
        let guard = self.index.read().expect("lock poisoned");
        loading_guard!(guard);
        let path = params.0.path.as_deref().unwrap_or("");
        let depth = params.0.depth.unwrap_or(2).min(5);
        let result = format::file_tree(&guard, path, depth);
        drop(guard);
        result
    }

    /// Get full context for a symbol: definition body, callers, callees, and type usages in one call.
    #[tool(
        description = "Get full context for a symbol: definition body, callers, callees, and type usages in one call"
    )]
    pub(crate) async fn get_context_bundle(
        &self,
        params: Parameters<GetContextBundleInput>,
    ) -> String {
        if let Some(result) = self.proxy_tool_call("get_context_bundle", &params.0).await {
            return result;
        }
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
    use std::ffi::OsString;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::process::Command;
    use std::sync::{Arc, RwLock};
    use std::time::{Duration, Instant};

    use crate::domain::{LanguageId, ReferenceKind, ReferenceRecord, SymbolKind, SymbolRecord};
    use crate::live_index::store::{CircuitBreakerState, IndexedFile, LiveIndex, ParseStatus};
    use crate::protocol::TokenizorServer;
    use rmcp::handler::server::wrapper::Parameters;
    use tempfile::TempDir;

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

    fn make_file_with_refs(
        path: &str,
        content: &[u8],
        symbols: Vec<SymbolRecord>,
        references: Vec<ReferenceRecord>,
    ) -> (String, IndexedFile) {
        let (key, mut file) = make_file(path, content, symbols);
        file.references = references;
        (key, file)
    }

    fn make_live_index_ready(files: Vec<(String, IndexedFile)>) -> LiveIndex {
        use crate::live_index::trigram::TrigramIndex;
        let files_map = files.into_iter().collect::<HashMap<_, _>>();
        let trigram_index = TrigramIndex::build_from_files(&files_map);
        let mut index = LiveIndex {
            files: files_map,
            loaded_at: Instant::now(),
            loaded_at_system: std::time::SystemTime::now(),
            load_duration: Duration::from_millis(10),
            cb_state: CircuitBreakerState::new(0.20),
            is_empty: false,
            reverse_index: HashMap::new(),
            trigram_index,
        };
        index.rebuild_reverse_index();
        index
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

    fn make_server_with_root(index: LiveIndex, repo_root: Option<PathBuf>) -> TokenizorServer {
        use crate::watcher::WatcherInfo;
        use std::sync::Mutex;
        let shared = Arc::new(RwLock::new(index));
        let watcher_info = Arc::new(Mutex::new(WatcherInfo::default()));
        TokenizorServer::new(
            shared,
            "test_project".to_string(),
            watcher_info,
            repo_root,
            None,
        )
    }

    fn make_server(index: LiveIndex) -> TokenizorServer {
        make_server_with_root(index, None)
    }

    struct EnvVarGuard {
        key: &'static str,
        previous: Option<OsString>,
    }

    impl EnvVarGuard {
        fn set_path(key: &'static str, value: &Path) -> Self {
            let previous = std::env::var_os(key);
            unsafe {
                std::env::set_var(key, value);
            }
            Self { key, previous }
        }
    }

    impl Drop for EnvVarGuard {
        fn drop(&mut self) {
            match &self.previous {
                Some(previous) => unsafe {
                    std::env::set_var(self.key, previous);
                },
                None => unsafe {
                    std::env::remove_var(self.key);
                },
            }
        }
    }

    fn run_git(repo_root: &Path, args: &[&str]) {
        let output = Command::new("git")
            .arg("-C")
            .arg(repo_root)
            .args(args)
            .output()
            .expect("git command should start");
        assert!(
            output.status.success(),
            "git {:?} failed: {}",
            args,
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn init_git_repo() -> TempDir {
        let dir = TempDir::new().expect("temp git repo");
        run_git(dir.path(), &["init", "-q"]);
        run_git(
            dir.path(),
            &["config", "user.email", "tokenizor-tests@example.com"],
        );
        run_git(dir.path(), &["config", "user.name", "Tokenizor Tests"]);
        dir
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
    async fn test_get_repo_outline_proxies_to_daemon_session() {
        let daemon_home = TempDir::new().expect("daemon home");
        let _env_guard = EnvVarGuard::set_path("TOKENIZOR_HOME", daemon_home.path());
        let project = TempDir::new().expect("project dir");
        fs::create_dir_all(project.path().join("src")).expect("src dir");
        fs::write(project.path().join("src").join("main.rs"), "fn main() {}\n")
            .expect("write source");

        let handle = crate::daemon::spawn_daemon("127.0.0.1")
            .await
            .expect("spawn daemon");
        let base_url = format!("http://127.0.0.1:{}", handle.port);
        let opened = reqwest::Client::new()
            .post(format!("{base_url}/v1/sessions/open"))
            .json(&crate::daemon::OpenProjectRequest {
                project_root: project.path().display().to_string(),
                client_name: "codex".to_string(),
                pid: Some(1234),
            })
            .send()
            .await
            .expect("open request")
            .error_for_status()
            .expect("open status")
            .json::<crate::daemon::OpenProjectResponse>()
            .await
            .expect("open body");

        let daemon_client = crate::daemon::DaemonSessionClient::new_for_test(
            base_url,
            opened.project_id,
            opened.session_id,
            opened.project_name,
        );
        let server = TokenizorServer::new_daemon_proxy(daemon_client);

        let result = server.get_repo_outline().await;
        assert!(
            result.contains("main.rs"),
            "remote repo outline should come from daemon project instance, got: {result}"
        );

        let _ = handle.shutdown_tx.send(());
    }

    #[tokio::test]
    async fn test_get_repo_map_returns_directory_breakdown() {
        let sym = make_symbol("main", SymbolKind::Function, 1, 3);
        let (key, file) = make_file("src/main.rs", b"fn main() {}", vec![sym]);
        let server = make_server(make_live_index_ready(vec![(key, file)]));

        let result = server.get_repo_map().await;

        assert!(
            result.contains("Index: 1 files, 1 symbols"),
            "repo map should include totals header; got: {result}"
        );
        assert!(
            result.contains("src"),
            "repo map should include directory breakdown; got: {result}"
        );
    }

    #[tokio::test]
    async fn test_get_file_context_returns_outline_and_key_references() {
        let callee = make_symbol("target", SymbolKind::Function, 1, 3);
        let caller = make_symbol("caller", SymbolKind::Function, 1, 3);
        let target_file = make_file("src/target.rs", b"fn target() {}", vec![callee]);
        let caller_file = make_file_with_refs(
            "src/caller.rs",
            b"use crate::target;\nfn caller() { target(); }",
            vec![caller],
            vec![
                ReferenceRecord {
                    name: "target".to_string(),
                    qualified_name: Some("crate::target".to_string()),
                    kind: ReferenceKind::Import,
                    byte_range: (4, 10),
                    line_range: (0, 0),
                    enclosing_symbol_index: None,
                },
                ReferenceRecord {
                    name: "target".to_string(),
                    qualified_name: None,
                    kind: ReferenceKind::Call,
                    byte_range: (30, 36),
                    line_range: (1, 1),
                    enclosing_symbol_index: Some(0),
                },
            ],
        );
        let server = make_server(make_live_index_ready(vec![target_file, caller_file]));

        let result = server
            .get_file_context(Parameters(super::GetFileContextInput {
                path: "src/target.rs".to_string(),
                max_tokens: None,
            }))
            .await;

        assert!(
            result.contains("src/target.rs"),
            "file context should include file header; got: {result}"
        );
        assert!(
            result.contains("Key references"),
            "file context should include reference section; got: {result}"
        );
        assert!(
            result.contains("src/caller.rs"),
            "file context should include caller file; got: {result}"
        );
    }

    #[tokio::test]
    async fn test_get_file_context_ignores_generic_name_noise_without_real_dependency() {
        let target = make_symbol("main", SymbolKind::Function, 1, 3);
        let helper = make_symbol("helper", SymbolKind::Function, 1, 4);
        let helper_main = make_symbol("main", SymbolKind::Function, 5, 7);
        let target_file = make_file("src/target.py", b"def main():\n    pass\n", vec![target]);
        let helper_file = make_file_with_refs(
            "scripts/helper.py",
            b"def helper():\n    main()\n\ndef main():\n    pass\n",
            vec![helper, helper_main],
            vec![ReferenceRecord {
                name: "main".to_string(),
                qualified_name: None,
                kind: ReferenceKind::Call,
                byte_range: (18, 22),
                line_range: (1, 1),
                enclosing_symbol_index: Some(0),
            }],
        );
        let server = make_server(make_live_index_ready(vec![target_file, helper_file]));

        let result = server
            .get_file_context(Parameters(super::GetFileContextInput {
                path: "src/target.py".to_string(),
                max_tokens: None,
            }))
            .await;

        assert!(
            !result.contains("scripts/helper.py"),
            "generic-name local calls should not be attributed as key references: {result}"
        );
    }

    #[tokio::test]
    async fn test_get_symbol_context_returns_grouped_references() {
        let caller = make_symbol("caller", SymbolKind::Function, 1, 3);
        let caller_file = make_file_with_refs(
            "src/caller.rs",
            b"fn caller() { target(); }",
            vec![caller],
            vec![ReferenceRecord {
                name: "target".to_string(),
                qualified_name: None,
                kind: ReferenceKind::Call,
                byte_range: (12, 18),
                line_range: (1, 1),
                enclosing_symbol_index: Some(0),
            }],
        );
        let server = make_server(make_live_index_ready(vec![caller_file]));

        let result = server
            .get_symbol_context(Parameters(super::GetSymbolContextInput {
                name: "target".to_string(),
                file: None,
            }))
            .await;

        assert!(
            result.contains("src/caller.rs"),
            "symbol context should group matches by file; got: {result}"
        );
        assert!(
            result.contains("in fn caller"),
            "symbol context should include enclosing symbol names; got: {result}"
        );
    }

    #[tokio::test]
    async fn test_analyze_file_impact_reports_symbol_change() {
        let repo = TempDir::new().expect("temp repo");
        fs::create_dir_all(repo.path().join("src")).expect("src dir");
        let source_path = repo.path().join("src").join("lib.rs");
        fs::write(&source_path, "pub fn new_name() {}\n").expect("write updated source");

        let old_symbol = make_symbol("old_name", SymbolKind::Function, 1, 1);
        let (key, file) = make_file("src/lib.rs", b"pub fn old_name() {}\n", vec![old_symbol]);
        let server = make_server_with_root(
            make_live_index_ready(vec![(key, file)]),
            Some(repo.path().to_path_buf()),
        );

        let result = server
            .analyze_file_impact(Parameters(super::AnalyzeFileImpactInput {
                path: "src/lib.rs".to_string(),
                new_file: None,
            }))
            .await;

        assert!(
            result.contains("new_name"),
            "impact tool should re-read the file from repo_root and report new symbols; got: {result}"
        );
    }

    #[tokio::test]
    async fn test_search_symbols_returns_results() {
        let sym = make_symbol("find_user", SymbolKind::Function, 1, 5);
        let (key, file) = make_file("src/lib.rs", b"fn find_user() {}", vec![sym]);
        let server = make_server(make_live_index_ready(vec![(key, file)]));
        let result = server
            .search_symbols(Parameters(super::SearchSymbolsInput {
                query: "find".to_string(),
                kind: None,
            }))
            .await;
        assert!(
            result.contains("find_user"),
            "should find matching symbol, got: {result}"
        );
    }

    #[tokio::test]
    async fn test_search_symbols_kind_filter_returns_only_requested_kind() {
        let function = make_symbol("JobRunner", SymbolKind::Function, 1, 5);
        let class = make_symbol("Job", SymbolKind::Class, 6, 10);
        let (key, file) = make_file(
            "src/lib.rs",
            b"fn JobRunner() {}\nstruct Job {}",
            vec![function, class],
        );
        let server = make_server(make_live_index_ready(vec![(key, file)]));
        let result = server
            .search_symbols(Parameters(super::SearchSymbolsInput {
                query: "job".to_string(),
                kind: Some("class".to_string()),
            }))
            .await;
        assert!(
            result.contains("class Job"),
            "class should remain visible: {result}"
        );
        assert!(
            !result.contains("fn JobRunner"),
            "function should be filtered out: {result}"
        );
    }

    #[tokio::test]
    async fn test_search_text_returns_results() {
        let (key, file) = make_file("src/lib.rs", b"fn find_user() {}", vec![]);
        let server = make_server(make_live_index_ready(vec![(key, file)]));
        let result = server
            .search_text(Parameters(super::SearchTextInput {
                query: Some("find".to_string()),
                terms: None,
                regex: None,
            }))
            .await;
        assert!(
            result.contains("find_user"),
            "should find matching text, got: {result}"
        );
    }

    #[tokio::test]
    async fn test_search_text_terms_or_returns_results() {
        let (key, file) = make_file(
            "src/lib.rs",
            b"// TODO: first\n// FIXME: second\n// NOTE: ignored",
            vec![],
        );
        let server = make_server(make_live_index_ready(vec![(key, file)]));
        let result = server
            .search_text(Parameters(super::SearchTextInput {
                query: None,
                terms: Some(vec!["TODO".to_string(), "FIXME".to_string()]),
                regex: None,
            }))
            .await;
        assert!(
            result.contains("TODO: first"),
            "TODO term should match: {result}"
        );
        assert!(
            result.contains("FIXME: second"),
            "FIXME term should match: {result}"
        );
        assert!(
            !result.contains("NOTE: ignored"),
            "unmatched line should be absent: {result}"
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
            .what_changed(Parameters(super::WhatChangedInput {
                since: Some(0),
                git_ref: None,
                uncommitted: None,
            }))
            .await;
        assert!(
            result.contains("src/lib.rs"),
            "what_changed since epoch should list all files, got: {result}"
        );
    }

    #[tokio::test]
    async fn test_what_changed_defaults_to_uncommitted_git_changes() {
        let repo = init_git_repo();
        fs::create_dir_all(repo.path().join("src")).expect("create src dir");
        fs::write(repo.path().join("src/lib.rs"), "fn foo() {}\n").expect("write initial file");
        run_git(repo.path(), &["add", "."]);
        run_git(repo.path(), &["commit", "-m", "init", "-q"]);
        fs::write(
            repo.path().join("src/lib.rs"),
            "fn foo() { println!(\"changed\"); }\n",
        )
        .expect("modify tracked file");

        let (key, file) = make_file(
            "src/lib.rs",
            b"fn foo() { println!(\"changed\"); }\n",
            vec![],
        );
        let server = make_server_with_root(
            make_live_index_ready(vec![(key, file)]),
            Some(repo.path().to_path_buf()),
        );
        let result = server
            .what_changed(Parameters(super::WhatChangedInput {
                since: None,
                git_ref: None,
                uncommitted: None,
            }))
            .await;
        assert!(
            result.contains("src/lib.rs"),
            "default mode should surface uncommitted git changes: {result}"
        );
    }

    #[tokio::test]
    async fn test_what_changed_git_ref_reports_diffed_files() {
        let repo = init_git_repo();
        fs::create_dir_all(repo.path().join("src")).expect("create src dir");
        fs::write(repo.path().join("src/lib.rs"), "fn foo() {}\n").expect("write initial file");
        run_git(repo.path(), &["add", "."]);
        run_git(repo.path(), &["commit", "-m", "init", "-q"]);
        fs::write(
            repo.path().join("src/lib.rs"),
            "fn foo() { println!(\"changed\"); }\n",
        )
        .expect("modify tracked file");

        let (key, file) = make_file(
            "src/lib.rs",
            b"fn foo() { println!(\"changed\"); }\n",
            vec![],
        );
        let server = make_server_with_root(
            make_live_index_ready(vec![(key, file)]),
            Some(repo.path().to_path_buf()),
        );
        let result = server
            .what_changed(Parameters(super::WhatChangedInput {
                since: None,
                git_ref: Some("HEAD".to_string()),
                uncommitted: None,
            }))
            .await;
        assert!(
            result.contains("src/lib.rs"),
            "git_ref mode should show changed files: {result}"
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
    fn test_exactly_18_tools_registered() {
        let server = make_server(make_live_index_ready(vec![]));
        let tool_count = server.tool_router.list_all().len();
        assert_eq!(
            tool_count, 18,
            "server must expose exactly 18 tools after adding shared hook-parity tools; found {tool_count}"
        );
    }

    #[tokio::test]
    async fn test_get_file_tree_returns_tree() {
        let sym = make_symbol("main", SymbolKind::Function, 1, 5);
        let (key, file) = make_file("src/main.rs", b"fn main() {}", vec![sym]);
        let server = make_server(make_live_index_ready(vec![(key, file)]));
        let result = server
            .get_file_tree(Parameters(super::GetFileTreeInput {
                path: None,
                depth: None,
            }))
            .await;
        assert!(
            result.contains("main.rs"),
            "get_file_tree should include file name; got: {result}"
        );
        assert!(
            result.contains("symbol"),
            "get_file_tree should show symbol count; got: {result}"
        );
    }

    #[tokio::test]
    async fn test_get_file_tree_loading_guard_empty() {
        let server = make_server(make_live_index_empty());
        let result = server
            .get_file_tree(Parameters(super::GetFileTreeInput {
                path: None,
                depth: None,
            }))
            .await;
        assert_eq!(result, crate::protocol::format::empty_guard_message());
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
