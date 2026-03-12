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

use crate::domain::LanguageId;
use crate::live_index::{IndexedFile, search, store::IndexState};
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
    /// Optional relative path prefix scope, for example `src/` or `src/protocol`.
    pub path_prefix: Option<String>,
    /// Optional canonical language name such as `Rust`, `TypeScript`, `C#`, or `C++`.
    pub language: Option<String>,
    /// Optional maximum number of matches to return (default 50, capped at 100).
    pub limit: Option<u32>,
    /// When true, include generated files in the result set.
    pub include_generated: Option<bool>,
    /// When true, include test files in the result set.
    pub include_tests: Option<bool>,
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
    /// Optional relative path prefix scope, for example `src/` or `src/protocol`.
    pub path_prefix: Option<String>,
    /// Optional canonical language name such as `Rust`, `TypeScript`, `C#`, or `C++`.
    pub language: Option<String>,
    /// Optional maximum number of matches to return across all files (default 50).
    pub limit: Option<u32>,
    /// Optional maximum number of matches to return per file (default 5).
    pub max_per_file: Option<u32>,
    /// When true, include generated files in the result set.
    pub include_generated: Option<bool>,
    /// When true, include test files in the result set.
    pub include_tests: Option<bool>,
    /// Optional repo-relative include glob, for example `src/**/*.ts`.
    pub glob: Option<String>,
    /// Optional repo-relative exclude glob, for example `**/*.spec.ts`.
    pub exclude_glob: Option<String>,
    /// Optional symmetric number of surrounding lines to render around each match.
    pub context: Option<u32>,
    /// Optional case-sensitivity override. Literal mode defaults to false; regex mode defaults to true.
    pub case_sensitive: Option<bool>,
    /// When true, require whole-word matches for literal searches. Not supported with `regex=true`.
    pub whole_word: Option<bool>,
}

/// Input for `search_files`.
#[derive(Deserialize, Serialize, JsonSchema)]
pub struct SearchFilesInput {
    /// Filename, folder name, or partial path.
    pub query: String,
    /// Optional maximum number of matches to return (default 20, capped at 50).
    pub limit: Option<u32>,
}

/// Input for `resolve_path`.
#[derive(Deserialize, Serialize, JsonSchema)]
pub struct ResolvePathInput {
    /// Filename, partial path, or ambiguous path hint.
    pub hint: String,
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
    /// Optional exact-selector path from `search_symbols`, for example `src/db.rs`.
    pub path: Option<String>,
    /// Optional selected symbol kind such as `fn`, `class`, or `struct`.
    pub symbol_kind: Option<String>,
    /// Optional selected symbol line from `search_symbols`.
    pub symbol_line: Option<u32>,
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
    /// Optional selected symbol line from `search_symbols`.
    pub symbol_line: Option<u32>,
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

fn parse_language_filter(input: Option<&str>) -> Result<Option<LanguageId>, String> {
    let Some(language) = input.map(str::trim).filter(|language| !language.is_empty()) else {
        return Ok(None);
    };

    let normalized = language.to_ascii_lowercase();
    let parsed = match normalized.as_str() {
        "rust" => Some(LanguageId::Rust),
        "python" => Some(LanguageId::Python),
        "javascript" => Some(LanguageId::JavaScript),
        "typescript" => Some(LanguageId::TypeScript),
        "go" => Some(LanguageId::Go),
        "java" => Some(LanguageId::Java),
        "c" => Some(LanguageId::C),
        "c++" => Some(LanguageId::Cpp),
        "c#" => Some(LanguageId::CSharp),
        "ruby" => Some(LanguageId::Ruby),
        "php" => Some(LanguageId::Php),
        "swift" => Some(LanguageId::Swift),
        "kotlin" => Some(LanguageId::Kotlin),
        "dart" => Some(LanguageId::Dart),
        "perl" => Some(LanguageId::Perl),
        "elixir" => Some(LanguageId::Elixir),
        _ => None,
    };

    parsed.map(Some).ok_or_else(|| {
        "Unsupported language filter. Use one of: Rust, Python, JavaScript, TypeScript, Go, Java, C, C++, C#, Ruby, PHP, Swift, Kotlin, Dart, Perl, Elixir.".to_string()
    })
}

fn normalize_path_prefix(input: Option<&str>) -> search::PathScope {
    let Some(prefix) = input.map(str::trim).filter(|prefix| !prefix.is_empty()) else {
        return search::PathScope::any();
    };

    let normalized = prefix
        .replace('\\', "/")
        .trim_start_matches("./")
        .trim_start_matches('/')
        .trim_end_matches('/')
        .to_string();

    if normalized.is_empty() {
        search::PathScope::any()
    } else {
        search::PathScope::prefix(normalized)
    }
}

fn normalize_search_text_glob(input: Option<&str>) -> Option<String> {
    input
        .map(str::trim)
        .filter(|pattern| !pattern.is_empty())
        .map(|pattern| {
            pattern
                .replace('\\', "/")
                .trim_start_matches("./")
                .trim_start_matches('/')
                .to_string()
        })
        .filter(|pattern| !pattern.is_empty())
}

fn search_symbols_options_from_input(
    input: &SearchSymbolsInput,
) -> Result<search::SymbolSearchOptions, String> {
    Ok(search::SymbolSearchOptions {
        path_scope: normalize_path_prefix(input.path_prefix.as_deref()),
        search_scope: search::SearchScope::Code,
        result_limit: search::ResultLimit::new(input.limit.unwrap_or(50).min(100) as usize),
        noise_policy: search::NoisePolicy {
            include_generated: input.include_generated.unwrap_or(false),
            include_tests: input.include_tests.unwrap_or(false),
            include_vendor: true,
        },
        language_filter: parse_language_filter(input.language.as_deref())?,
    })
}

fn search_text_options_from_input(
    input: &SearchTextInput,
) -> Result<search::TextSearchOptions, String> {
    Ok(search::TextSearchOptions {
        path_scope: normalize_path_prefix(input.path_prefix.as_deref()),
        search_scope: search::SearchScope::Code,
        noise_policy: search::NoisePolicy {
            include_generated: input.include_generated.unwrap_or(false),
            include_tests: input.include_tests.unwrap_or(false),
            include_vendor: true,
        },
        language_filter: parse_language_filter(input.language.as_deref())?,
        total_limit: input.limit.unwrap_or(50) as usize,
        max_per_file: input.max_per_file.unwrap_or(5) as usize,
        glob: normalize_search_text_glob(input.glob.as_deref()),
        exclude_glob: normalize_search_text_glob(input.exclude_glob.as_deref()),
        context: input.context.map(|context| context as usize),
        case_sensitive: input.case_sensitive,
        whole_word: input.whole_word.unwrap_or(false),
    })
}

fn sidecar_state_for_server(server: &TokenizorServer) -> SidecarState {
    SidecarState {
        index: Arc::clone(&server.index),
        token_stats: server.token_stats.clone().unwrap_or_else(TokenStats::new),
        repo_root: server.repo_root.clone(),
        symbol_cache: Arc::new(RwLock::new(HashMap::new())),
    }
}

enum CapturedGetSymbolsEntry {
    SymbolLookup {
        file: Arc<IndexedFile>,
        name: String,
        kind: Option<String>,
    },
    CodeSlice {
        file: Arc<IndexedFile>,
        start_byte: usize,
        end_byte: Option<usize>,
    },
    FileNotFound {
        path: String,
    },
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

fn loading_guard_message_from_published(
    published: &crate::live_index::PublishedIndexState,
) -> Option<String> {
    match published.status {
        crate::live_index::PublishedIndexStatus::Ready => None,
        crate::live_index::PublishedIndexStatus::Empty => Some(format::empty_guard_message()),
        crate::live_index::PublishedIndexStatus::Loading => Some(format::loading_guard_message()),
        crate::live_index::PublishedIndexStatus::Degraded => Some(format!(
            "Index degraded: {}",
            published
                .degraded_summary
                .as_deref()
                .unwrap_or("circuit breaker tripped")
        )),
    }
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
        let file = {
            let guard = self.index.read().expect("lock poisoned");
            loading_guard!(guard);
            guard.capture_shared_file(&params.0.path)
        };
        match file {
            Some(file) => format::file_outline_from_indexed_file(file.as_ref()),
            None => format::not_found_file(&params.0.path),
        }
    }

    /// Look up a specific symbol by file path and name. Returns full source code.
    #[tool(
        description = "Look up a specific symbol by file path and name. Returns full source code."
    )]
    pub(crate) async fn get_symbol(&self, params: Parameters<GetSymbolInput>) -> String {
        if let Some(result) = self.proxy_tool_call("get_symbol", &params.0).await {
            return result;
        }
        let file = {
            let guard = self.index.read().expect("lock poisoned");
            loading_guard!(guard);
            guard.capture_shared_file(&params.0.path)
        };
        match file {
            Some(file) => format::symbol_detail_from_indexed_file(
                file.as_ref(),
                &params.0.name,
                params.0.kind.as_deref(),
            ),
            None => format::not_found_file(&params.0.path),
        }
    }

    /// Batch lookup of symbols or code slices. Each target can be a symbol name or byte range.
    #[tool(
        description = "Batch lookup of symbols or code slices. Each target can be a symbol name or byte range."
    )]
    pub(crate) async fn get_symbols(&self, params: Parameters<GetSymbolsInput>) -> String {
        if let Some(result) = self.proxy_tool_call("get_symbols", &params.0).await {
            return result;
        }
        let captured = {
            let guard = self.index.read().expect("lock poisoned");
            loading_guard!(guard);

            params
                .0
                .targets
                .iter()
                .map(|target| match target.name.as_deref() {
                    Some(name) => match guard.capture_shared_file(&target.path) {
                        Some(file) => CapturedGetSymbolsEntry::SymbolLookup {
                            file,
                            name: name.to_string(),
                            kind: target.kind.clone(),
                        },
                        None => CapturedGetSymbolsEntry::FileNotFound {
                            path: target.path.clone(),
                        },
                    },
                    None => match guard.capture_shared_file(&target.path) {
                        None => CapturedGetSymbolsEntry::FileNotFound {
                            path: target.path.clone(),
                        },
                        Some(file) => CapturedGetSymbolsEntry::CodeSlice {
                            file,
                            start_byte: target.start_byte.unwrap_or(0) as usize,
                            end_byte: target.end_byte.map(|e| e as usize),
                        },
                    },
                })
                .collect::<Vec<_>>()
        };

        captured
            .into_iter()
            .map(|entry| match entry {
                CapturedGetSymbolsEntry::SymbolLookup { file, name, kind } => {
                    format::symbol_detail_from_indexed_file(file.as_ref(), &name, kind.as_deref())
                }
                CapturedGetSymbolsEntry::CodeSlice {
                    file,
                    start_byte,
                    end_byte,
                } => format::code_slice_from_indexed_file(file.as_ref(), start_byte, end_byte),
                CapturedGetSymbolsEntry::FileNotFound { path } => format::not_found_file(&path),
            })
            .collect::<Vec<_>>()
            .join("\n---\n")
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
        let published = self.index.published_state();
        if let Some(message) = loading_guard_message_from_published(&published) {
            return message;
        }
        let view = self.index.published_repo_outline();
        format::repo_outline_view(&view, &self.project_name)
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
        let options = match search_symbols_options_from_input(&params.0) {
            Ok(options) => options,
            Err(message) => return message,
        };
        let result = {
            let guard = self.index.read().expect("lock poisoned");
            loading_guard!(guard);
            search::search_symbols_with_options(
                &guard,
                &params.0.query,
                params.0.kind.as_deref(),
                &options,
            )
        };
        format::search_symbols_result_view(&result, &params.0.query)
    }

    /// Full-text search across all indexed file contents.
    #[tool(description = "Full-text search across all indexed file contents.")]
    pub(crate) async fn search_text(&self, params: Parameters<SearchTextInput>) -> String {
        if let Some(result) = self.proxy_tool_call("search_text", &params.0).await {
            return result;
        }
        let options = match search_text_options_from_input(&params.0) {
            Ok(options) => options,
            Err(message) => return message,
        };
        let result = {
            let guard = self.index.read().expect("lock poisoned");
            loading_guard!(guard);
            search::search_text_with_options(
                &guard,
                params.0.query.as_deref(),
                params.0.terms.as_deref(),
                params.0.regex.unwrap_or(false),
                &options,
            )
        };
        format::search_text_result_view(result)
    }

    /// Search indexed file paths using bounded ranked code-lane discovery.
    #[tool(description = "Search indexed file paths using bounded ranked code-lane discovery.")]
    pub(crate) async fn search_files(&self, params: Parameters<SearchFilesInput>) -> String {
        if let Some(result) = self.proxy_tool_call("search_files", &params.0).await {
            return result;
        }
        let view = {
            let guard = self.index.read().expect("lock poisoned");
            loading_guard!(guard);
            guard.capture_search_files_view(&params.0.query, params.0.limit.unwrap_or(20) as usize)
        };
        format::search_files_result_view(&view)
    }

    /// Resolve filenames, partial paths, and ambiguous path hints to one exact indexed project path.
    #[tool(
        description = "Resolve filenames, partial paths, and ambiguous path hints to one exact indexed project path."
    )]
    pub(crate) async fn resolve_path(&self, params: Parameters<ResolvePathInput>) -> String {
        if let Some(result) = self.proxy_tool_call("resolve_path", &params.0).await {
            return result;
        }
        let view = {
            let guard = self.index.read().expect("lock poisoned");
            loading_guard!(guard);
            guard.capture_resolve_path_view(&params.0.hint)
        };
        format::resolve_path_result_view(&view)
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
        let published = self.index.published_state();
        let watcher_guard = self.watcher_info.lock().unwrap();
        let mut result = format::health_report_from_published_state(&published, &watcher_guard);

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
        match self.index.reload(&root) {
            Ok(()) => {
                let published = self.index.published_state();
                let file_count = published.file_count;
                let symbol_count = published.symbol_count;

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
                let view = {
                    let guard = self.index.read().expect("lock poisoned");
                    loading_guard!(guard);
                    guard.capture_what_changed_timestamp_view()
                };
                format::what_changed_timestamp_view(&view, since_ts)
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
        let options = search::FileContentOptions::for_explicit_path_read(
            params.0.path.clone(),
            params.0.start_line,
            params.0.end_line,
        );
        let file = {
            let guard = self.index.read().expect("lock poisoned");
            loading_guard!(guard);
            guard.capture_shared_file_for_scope(&options.path_scope)
        };
        match file {
            Some(file) => {
                format::file_content_from_indexed_file_with_context(
                    file.as_ref(),
                    options.content_context,
                )
            }
            None => format::not_found_file(&params.0.path),
        }
    }

    /// Find all references (call sites, imports, type usages) for a symbol across the codebase.
    #[tool(
        description = "Find all references (call sites, imports, type usages) for a symbol across the codebase"
    )]
    pub(crate) async fn find_references(&self, params: Parameters<FindReferencesInput>) -> String {
        if let Some(result) = self.proxy_tool_call("find_references", &params.0).await {
            return result;
        }
        let input = &params.0;
        let result = {
            let guard = self.index.read().expect("lock poisoned");
            loading_guard!(guard);
            if let Some(path) = input.path.as_deref() {
                guard.capture_find_references_view_for_symbol(
                    path,
                    &input.name,
                    input.symbol_kind.as_deref(),
                    input.symbol_line,
                    input.kind.as_deref(),
                )
            } else {
                Ok(guard.capture_find_references_view(&input.name, input.kind.as_deref()))
            }
        };
        match result {
            Ok(view) => format::find_references_result_view(&view, &input.name),
            Err(error) => error,
        }
    }

    /// Find all files that import or depend on the given file.
    #[tool(description = "Find all files that import or depend on the given file")]
    pub(crate) async fn find_dependents(&self, params: Parameters<FindDependentsInput>) -> String {
        if let Some(result) = self.proxy_tool_call("find_dependents", &params.0).await {
            return result;
        }
        let input = &params.0;
        let view = {
            let guard = self.index.read().expect("lock poisoned");
            loading_guard!(guard);
            guard.capture_find_dependents_view(&input.path)
        };
        format::find_dependents_result_view(&view, &input.path)
    }

    /// Browse the source file tree with symbol counts per file and directory.
    #[tool(description = "Browse the source file tree with symbol counts per file and directory.")]
    pub(crate) async fn get_file_tree(&self, params: Parameters<GetFileTreeInput>) -> String {
        if let Some(result) = self.proxy_tool_call("get_file_tree", &params.0).await {
            return result;
        }
        let published = self.index.published_state();
        if let Some(message) = loading_guard_message_from_published(&published) {
            return message;
        }
        let path = params.0.path.as_deref().unwrap_or("");
        let depth = params.0.depth.unwrap_or(2).min(5);
        let view = self.index.published_repo_outline();
        format::file_tree_view(&view.files, path, depth)
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
        let input = &params.0;
        let view = {
            let guard = self.index.read().expect("lock poisoned");
            loading_guard!(guard);
            guard.capture_context_bundle_view(
                &input.path,
                &input.name,
                input.kind.as_deref(),
                input.symbol_line,
            )
        };
        format::context_bundle_result_view(&view)
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
    use std::sync::Arc;
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
                classification: crate::domain::FileClassification::for_code_path(path),
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

    fn make_ref(
        name: &str,
        qualified_name: Option<&str>,
        kind: ReferenceKind,
        line: u32,
        enclosing_symbol_index: Option<u32>,
    ) -> ReferenceRecord {
        ReferenceRecord {
            name: name.to_string(),
            qualified_name: qualified_name.map(str::to_string),
            kind,
            byte_range: (line * 10, line * 10 + 6),
            line_range: (line, line),
            enclosing_symbol_index,
        }
    }

    fn make_live_index_ready(files: Vec<(String, IndexedFile)>) -> LiveIndex {
        use crate::live_index::trigram::TrigramIndex;
        let files_map = files
            .into_iter()
            .map(|(path, file)| (path, std::sync::Arc::new(file)))
            .collect::<HashMap<_, _>>();
        let trigram_index = TrigramIndex::build_from_files(&files_map);
        let mut index = LiveIndex {
            files: files_map,
            loaded_at: Instant::now(),
            loaded_at_system: std::time::SystemTime::now(),
            load_duration: Duration::from_millis(10),
            cb_state: CircuitBreakerState::new(0.20),
            is_empty: false,
            load_source: crate::live_index::store::IndexLoadSource::FreshLoad,
            snapshot_verify_state: crate::live_index::store::SnapshotVerifyState::NotNeeded,
            reverse_index: HashMap::new(),
            files_by_basename: HashMap::new(),
            files_by_dir_component: HashMap::new(),
            trigram_index,
        };
        index.rebuild_reverse_index();
        index.rebuild_path_indices();
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
            load_source: crate::live_index::store::IndexLoadSource::EmptyBootstrap,
            snapshot_verify_state: crate::live_index::store::SnapshotVerifyState::NotNeeded,
            reverse_index: HashMap::new(),
            files_by_basename: HashMap::new(),
            files_by_dir_component: HashMap::new(),
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
            load_source: crate::live_index::store::IndexLoadSource::FreshLoad,
            snapshot_verify_state: crate::live_index::store::SnapshotVerifyState::NotNeeded,
            reverse_index: HashMap::new(),
            files_by_basename: HashMap::new(),
            files_by_dir_component: HashMap::new(),
            trigram_index: TrigramIndex::new(),
        }
    }

    fn make_server_with_root(index: LiveIndex, repo_root: Option<PathBuf>) -> TokenizorServer {
        use crate::watcher::WatcherInfo;
        use std::sync::Mutex;
        let shared = crate::live_index::SharedIndexHandle::shared(index);
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
    async fn test_get_repo_outline_loading_guard_empty() {
        let server = make_server(make_live_index_empty());
        let result = server.get_repo_outline().await;
        assert_eq!(result, crate::protocol::format::empty_guard_message());
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
                path_prefix: None,
                language: None,
                limit: None,
                include_generated: None,
                include_tests: None,
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
                path_prefix: None,
                language: None,
                limit: None,
                include_generated: None,
                include_tests: None,
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
    async fn test_search_symbols_hides_generated_and_test_noise_by_default() {
        let server = make_server(make_live_index_ready(vec![
            make_file(
                "src/job.rs",
                b"struct Job {}\n",
                vec![make_symbol("Job", SymbolKind::Class, 1, 1)],
            ),
            make_file(
                "src/generated/job_generated.rs",
                b"struct JobGenerated {}\n",
                vec![make_symbol("JobGenerated", SymbolKind::Class, 2, 2)],
            ),
            make_file(
                "tests/job_test.rs",
                b"struct JobTest {}\n",
                vec![make_symbol("JobTest", SymbolKind::Class, 3, 3)],
            ),
        ]));

        let result = server
            .search_symbols(Parameters(super::SearchSymbolsInput {
                query: "job".to_string(),
                kind: Some("class".to_string()),
                path_prefix: None,
                language: None,
                limit: None,
                include_generated: None,
                include_tests: None,
            }))
            .await;

        assert!(result.contains("class Job"), "expected primary hit: {result}");
        assert!(
            !result.contains("JobGenerated"),
            "generated symbol noise should be hidden by default: {result}"
        );
        assert!(
            !result.contains("JobTest"),
            "test symbol noise should be hidden by default: {result}"
        );
    }

    #[tokio::test]
    async fn test_search_symbols_tool_can_include_generated_without_tests() {
        let server = make_server(make_live_index_ready(vec![
            make_file(
                "src/job.rs",
                b"struct Job {}\n",
                vec![make_symbol("Job", SymbolKind::Class, 1, 1)],
            ),
            make_file(
                "src/generated/job_generated.rs",
                b"struct JobGenerated {}\n",
                vec![make_symbol("JobGenerated", SymbolKind::Class, 2, 2)],
            ),
            make_file(
                "tests/job_test.rs",
                b"struct JobTest {}\n",
                vec![make_symbol("JobTest", SymbolKind::Class, 3, 3)],
            ),
        ]));

        let result = server
            .search_symbols(Parameters(super::SearchSymbolsInput {
                query: "job".to_string(),
                kind: Some("class".to_string()),
                path_prefix: None,
                language: None,
                limit: None,
                include_generated: Some(true),
                include_tests: None,
            }))
            .await;

        assert!(result.contains("class Job"), "expected primary hit: {result}");
        assert!(
            result.contains("JobGenerated"),
            "generated symbol should be visible when opted in: {result}"
        );
        assert!(
            !result.contains("JobTest"),
            "test noise should stay hidden without explicit opt-in: {result}"
        );
    }

    #[tokio::test]
    async fn test_search_symbols_tool_can_include_tests_without_generated() {
        let server = make_server(make_live_index_ready(vec![
            make_file(
                "src/job.rs",
                b"struct Job {}\n",
                vec![make_symbol("Job", SymbolKind::Class, 1, 1)],
            ),
            make_file(
                "src/generated/job_generated.rs",
                b"struct JobGenerated {}\n",
                vec![make_symbol("JobGenerated", SymbolKind::Class, 2, 2)],
            ),
            make_file(
                "tests/job_test.rs",
                b"struct JobTest {}\n",
                vec![make_symbol("JobTest", SymbolKind::Class, 3, 3)],
            ),
        ]));

        let result = server
            .search_symbols(Parameters(super::SearchSymbolsInput {
                query: "job".to_string(),
                kind: Some("class".to_string()),
                path_prefix: None,
                language: None,
                limit: None,
                include_generated: None,
                include_tests: Some(true),
            }))
            .await;

        assert!(result.contains("class Job"), "expected primary hit: {result}");
        assert!(
            !result.contains("JobGenerated"),
            "generated noise should stay hidden without explicit opt-in: {result}"
        );
        assert!(
            result.contains("JobTest"),
            "test symbol should be visible when opted in: {result}"
        );
    }

    #[tokio::test]
    async fn test_search_symbols_tool_respects_scope_language_limit_and_kind() {
        let rust_model = make_file(
            "src/models/job.rs",
            b"struct Job {}\nfn JobRunner() {}\n",
            vec![
                make_symbol("Job", SymbolKind::Class, 1, 1),
                make_symbol("JobRunner", SymbolKind::Function, 2, 2),
            ],
        );
        let mut ts_ui = make_file(
            "src/ui/job.ts",
            b"class JobCard {}\nclass JobList {}\n",
            vec![
                make_symbol("JobCard", SymbolKind::Class, 1, 1),
                make_symbol("JobList", SymbolKind::Class, 2, 2),
            ],
        );
        ts_ui.1.language = LanguageId::TypeScript;
        let server = make_server(make_live_index_ready(vec![rust_model, ts_ui]));

        let result = server
            .search_symbols(Parameters(super::SearchSymbolsInput {
                query: "job".to_string(),
                kind: Some("class".to_string()),
                path_prefix: Some("src/ui".to_string()),
                language: Some("TypeScript".to_string()),
                limit: Some(1),
                include_generated: None,
                include_tests: None,
            }))
            .await;

        assert!(result.contains("1 matches in 1 files"), "expected bounded output: {result}");
        assert!(result.contains("class JobCard"), "expected scoped class hit: {result}");
        assert!(!result.contains("JobList"), "limit should truncate later hits: {result}");
        assert!(
            !result.contains("src/models/job.rs"),
            "path scope should exclude rust model: {result}"
        );
        assert!(!result.contains("fn JobRunner"), "kind filter should exclude function: {result}");
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
                path_prefix: None,
                language: None,
                limit: None,
                max_per_file: None,
                include_generated: None,
                include_tests: None,
                glob: None,
                exclude_glob: None,
                context: None,
                case_sensitive: None,
                whole_word: None,
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
                path_prefix: None,
                language: None,
                limit: None,
                max_per_file: None,
                include_generated: None,
                include_tests: None,
                glob: None,
                exclude_glob: None,
                context: None,
                case_sensitive: None,
                whole_word: None,
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
    async fn test_search_text_hides_generated_and_test_noise_by_default() {
        let server = make_server(make_live_index_ready(vec![
            make_file("src/real.rs", b"needle visible", vec![]),
            make_file("tests/generated/noise.rs", b"needle hidden", vec![]),
        ]));
        let result = server
            .search_text(Parameters(super::SearchTextInput {
                query: Some("needle".to_string()),
                terms: None,
                regex: None,
                path_prefix: None,
                language: None,
                limit: None,
                max_per_file: None,
                include_generated: None,
                include_tests: None,
                glob: None,
                exclude_glob: None,
                context: None,
                case_sensitive: None,
                whole_word: None,
            }))
            .await;

        assert!(result.contains("src/real.rs"), "expected visible file: {result}");
        assert!(
            !result.contains("tests/generated/noise.rs"),
            "generated/test noise should be hidden by default: {result}"
        );
    }

    #[tokio::test]
    async fn test_search_text_tool_respects_scope_language_and_caps() {
        let mut ts_app = make_file("src/app.ts", b"needle one\nneedle two\nneedle three\n", vec![]);
        ts_app.1.language = LanguageId::TypeScript;
        let mut ts_lib = make_file("src/lib.ts", b"needle four\nneedle five\n", vec![]);
        ts_lib.1.language = LanguageId::TypeScript;
        let noise = make_file("tests/generated/noise.ts", b"needle hidden\nneedle hidden two\n", vec![]);
        let rust = make_file("src/lib.rs", b"needle rust\nneedle rust two\n", vec![]);
        let server = make_server(make_live_index_ready(vec![ts_app, ts_lib, noise, rust]));

        let result = server
            .search_text(Parameters(super::SearchTextInput {
                query: Some("needle".to_string()),
                terms: None,
                regex: None,
                path_prefix: Some("src".to_string()),
                language: Some("TypeScript".to_string()),
                limit: Some(3),
                max_per_file: Some(2),
                include_generated: Some(false),
                include_tests: Some(false),
                glob: None,
                exclude_glob: None,
                context: None,
                case_sensitive: None,
                whole_word: None,
            }))
            .await;

        assert!(result.contains("src/app.ts"), "expected app.ts: {result}");
        assert!(result.contains("src/lib.ts"), "expected lib.ts: {result}");
        assert!(!result.contains("needle three"), "per-file cap should truncate app.ts: {result}");
        assert!(!result.contains("needle five"), "total cap should truncate final result set: {result}");
        assert!(
            !result.contains("tests/generated/noise.ts"),
            "noise file should be excluded: {result}"
        );
        assert!(!result.contains("src/lib.rs"), "language filter should exclude Rust: {result}");
    }

    #[tokio::test]
    async fn test_search_text_tool_context_renders_windows() {
        let server = make_server(make_live_index_ready(vec![make_file(
            "src/lib.rs",
            b"line 1\nline 2\nneedle 3\nline 4\nneedle 5\nline 6\nline 7\nline 8\nneedle 9\nline 10\n",
            vec![],
        )]));

        let result = server
            .search_text(Parameters(super::SearchTextInput {
                query: Some("needle".to_string()),
                terms: None,
                regex: None,
                path_prefix: None,
                language: None,
                limit: None,
                max_per_file: None,
                include_generated: None,
                include_tests: None,
                glob: None,
                exclude_glob: None,
                context: Some(1),
                case_sensitive: None,
                whole_word: None,
            }))
            .await;

        assert!(result.contains("  2: line 2"), "context line missing: {result}");
        assert!(result.contains("> 3: needle 3"), "match marker missing: {result}");
        assert!(result.contains("  ..."), "separator missing: {result}");
    }

    #[tokio::test]
    async fn test_search_text_tool_respects_glob_and_exclude_glob() {
        let server = make_server(make_live_index_ready(vec![
            make_file("src/app.ts", b"needle app\n", vec![]),
            make_file("src/app.spec.ts", b"needle spec\n", vec![]),
            make_file("src/nested/feature.ts", b"needle nested\n", vec![]),
            make_file("src/lib.rs", b"needle rust\n", vec![]),
        ]));

        let result = server
            .search_text(Parameters(super::SearchTextInput {
                query: Some("needle".to_string()),
                terms: None,
                regex: None,
                path_prefix: Some("src".to_string()),
                language: None,
                limit: None,
                max_per_file: None,
                include_generated: None,
                include_tests: None,
                glob: Some("src/**/*.ts".to_string()),
                exclude_glob: Some("**/*.spec.ts".to_string()),
                context: None,
                case_sensitive: None,
                whole_word: None,
            }))
            .await;

        assert!(result.contains("src/app.ts"), "expected app.ts: {result}");
        assert!(
            result.contains("src/nested/feature.ts"),
            "expected nested ts file: {result}"
        );
        assert!(
            !result.contains("src/app.spec.ts"),
            "exclude_glob should suppress spec file: {result}"
        );
        assert!(
            !result.contains("src/lib.rs"),
            "include glob should suppress rust file: {result}"
        );
    }

    #[tokio::test]
    async fn test_search_text_tool_reports_invalid_glob() {
        let server = make_server(make_live_index_ready(vec![make_file(
            "src/app.ts",
            b"needle app\n",
            vec![],
        )]));

        let result = server
            .search_text(Parameters(super::SearchTextInput {
                query: Some("needle".to_string()),
                terms: None,
                regex: None,
                path_prefix: None,
                language: None,
                limit: None,
                max_per_file: None,
                include_generated: None,
                include_tests: None,
                glob: Some("[".to_string()),
                exclude_glob: None,
                context: None,
                case_sensitive: None,
                whole_word: None,
            }))
            .await;

        assert!(
            result.contains("Invalid glob for `glob`"),
            "expected invalid glob error, got: {result}"
        );
    }

    #[tokio::test]
    async fn test_search_text_tool_respects_case_sensitive_and_whole_word() {
        let server = make_server(make_live_index_ready(vec![make_file(
            "src/lib.rs",
            b"Needle\nneedle\nNeedleCase\nNeedle suffix\n",
            vec![],
        )]));

        let result = server
            .search_text(Parameters(super::SearchTextInput {
                query: Some("Needle".to_string()),
                terms: None,
                regex: None,
                path_prefix: None,
                language: None,
                limit: None,
                max_per_file: None,
                include_generated: None,
                include_tests: None,
                glob: None,
                exclude_glob: None,
                context: None,
                case_sensitive: Some(true),
                whole_word: Some(true),
            }))
            .await;

        assert!(result.contains("  1: Needle"), "exact whole-word match missing: {result}");
        assert!(
            result.contains("  4: Needle suffix"),
            "whole-word prefix match on a line should remain visible: {result}"
        );
        assert!(
            !result.contains("  2: needle"),
            "case-sensitive search should exclude lowercase line: {result}"
        );
        assert!(
            !result.contains("  3: NeedleCase"),
            "whole-word search should exclude embedded identifier match: {result}"
        );
    }

    #[tokio::test]
    async fn test_search_text_tool_reports_regex_whole_word_rejection() {
        let server = make_server(make_live_index_ready(vec![make_file(
            "src/lib.rs",
            b"needle\n",
            vec![],
        )]));

        let result = server
            .search_text(Parameters(super::SearchTextInput {
                query: Some("needle".to_string()),
                terms: None,
                regex: Some(true),
                path_prefix: None,
                language: None,
                limit: None,
                max_per_file: None,
                include_generated: None,
                include_tests: None,
                glob: None,
                exclude_glob: None,
                context: None,
                case_sensitive: None,
                whole_word: Some(true),
            }))
            .await;

        assert!(
            result.contains("whole_word is not supported when `regex=true`"),
            "expected regex/whole_word rejection, got: {result}"
        );
    }

    #[tokio::test]
    async fn test_search_files_returns_ranked_paths() {
        let server = make_server(make_live_index_ready(vec![
            make_file("src/protocol/tools.rs", b"fn a() {}", vec![]),
            make_file("src/sidecar/tools.rs", b"fn b() {}", vec![]),
            make_file("src/protocol/tools_helper.rs", b"fn c() {}", vec![]),
        ]));
        let result = server
            .search_files(Parameters(super::SearchFilesInput {
                query: "protocol/tools.rs".to_string(),
                limit: Some(20),
            }))
            .await;
        assert!(result.contains("2 matching files"), "got: {result}");
        assert!(result.contains("── Strong path matches ──"), "got: {result}");
        assert!(result.contains("── Basename matches ──"), "got: {result}");
        assert!(result.contains("src/protocol/tools.rs"), "got: {result}");
        assert!(result.contains("src/sidecar/tools.rs"), "got: {result}");
        assert!(!result.contains("tools_helper.rs"), "got: {result}");
    }

    #[tokio::test]
    async fn test_search_files_not_found() {
        let server = make_server(make_live_index_ready(vec![]));
        let result = server
            .search_files(Parameters(super::SearchFilesInput {
                query: "README.md".to_string(),
                limit: None,
            }))
            .await;
        assert_eq!(result, "No indexed source files matching 'README.md'");
    }

    #[tokio::test]
    async fn test_resolve_path_returns_exact_match() {
        let (key, file) = make_file("src/protocol/tools.rs", b"fn tool() {}", vec![]);
        let server = make_server(make_live_index_ready(vec![(key, file)]));
        let result = server
            .resolve_path(Parameters(super::ResolvePathInput {
                hint: "src/protocol/tools.rs".to_string(),
            }))
            .await;
        assert_eq!(result, "src/protocol/tools.rs");
    }

    #[tokio::test]
    async fn test_resolve_path_returns_ambiguous_matches() {
        let server = make_server(make_live_index_ready(vec![
            make_file("src/lib.rs", b"fn src_lib() {}", vec![]),
            make_file("tests/lib.rs", b"fn test_lib() {}", vec![]),
        ]));
        let result = server
            .resolve_path(Parameters(super::ResolvePathInput {
                hint: "lib.rs".to_string(),
            }))
            .await;
        assert!(result.contains("Ambiguous path hint 'lib.rs'"), "got: {result}");
        assert!(result.contains("src/lib.rs"), "got: {result}");
        assert!(result.contains("tests/lib.rs"), "got: {result}");
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
        assert!(
            result.contains("fn bar() {"),
            "symbol lookup branch should return symbol body, got: {result}"
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

    #[tokio::test]
    async fn test_get_file_content_line_range_preserves_public_contract() {
        let content = b"line 1\nline 2\nline 3\nline 4";
        let (key, file) = make_file("src/lib.rs", content, vec![]);
        let server = make_server(make_live_index_ready(vec![(key, file)]));
        let result = server
            .get_file_content(Parameters(super::GetFileContentInput {
                path: "src/lib.rs".to_string(),
                start_line: Some(2),
                end_line: Some(3),
            }))
            .await;
        assert_eq!(result, "line 2\nline 3");
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
    fn test_exactly_20_tools_registered() {
        let server = make_server(make_live_index_ready(vec![]));
        let tool_count = server.tool_router.list_all().len();
        assert_eq!(
            tool_count, 20,
            "server must expose exactly 20 tools after adding search_files; found {tool_count}"
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
                path: None,
                symbol_kind: None,
                symbol_line: None,
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
                symbol_line: None,
            }))
            .await;
        assert_eq!(result, crate::protocol::format::empty_guard_message());
    }

    #[tokio::test]
    async fn test_get_context_bundle_delegates_to_formatter() {
        let server = make_server(make_live_index_ready(vec![]));
        let result = server
            .get_context_bundle(Parameters(super::GetContextBundleInput {
                path: "src/nonexistent.rs".to_string(),
                name: "process".to_string(),
                kind: None,
                symbol_line: None,
            }))
            .await;
        assert!(result.contains("File not found"), "got: {result}");
    }

    #[tokio::test]
    async fn test_get_context_bundle_exact_selector_uses_line_and_exact_callers() {
        let target = make_file(
            "src/db.rs",
            b"fn connect() { first(); }\nfn connect() { second(); }\n",
            vec![
                make_symbol("connect", SymbolKind::Function, 1, 1),
                make_symbol("connect", SymbolKind::Function, 2, 2),
            ],
        );
        let dependent = make_file_with_refs(
            "src/service.rs",
            b"use crate::db::connect;\nfn run() { connect(); }\n",
            vec![make_symbol("run", SymbolKind::Function, 2, 2)],
            vec![
                make_ref("db", Some("crate::db"), ReferenceKind::Import, 0, None),
                make_ref(
                    "connect",
                    Some("crate::db::connect"),
                    ReferenceKind::Call,
                    1,
                    Some(0),
                ),
            ],
        );
        let unrelated = make_file_with_refs(
            "src/other.rs",
            b"fn run() { connect(); }\n",
            vec![make_symbol("run", SymbolKind::Function, 1, 1)],
            vec![make_ref("connect", None, ReferenceKind::Call, 0, Some(0))],
        );
        let server = make_server(make_live_index_ready(vec![target, dependent, unrelated]));

        let result = server
            .get_context_bundle(Parameters(super::GetContextBundleInput {
                path: "src/db.rs".to_string(),
                name: "connect".to_string(),
                kind: Some("fn".to_string()),
                symbol_line: Some(2),
            }))
            .await;

        assert!(result.contains("src/service.rs"), "expected dependent hit: {result}");
        assert!(
            !result.contains("src/other.rs"),
            "unrelated same-name file should be excluded: {result}"
        );
    }

    #[tokio::test]
    async fn test_get_context_bundle_exact_selector_requires_line_for_ambiguous_symbol() {
        let target = make_file(
            "src/db.rs",
            b"fn connect() {}\nfn connect() {}\n",
            vec![
                make_symbol("connect", SymbolKind::Function, 1, 1),
                make_symbol("connect", SymbolKind::Function, 2, 2),
            ],
        );
        let server = make_server(make_live_index_ready(vec![target]));

        let result = server
            .get_context_bundle(Parameters(super::GetContextBundleInput {
                path: "src/db.rs".to_string(),
                name: "connect".to_string(),
                kind: Some("fn".to_string()),
                symbol_line: None,
            }))
            .await;

        assert!(result.contains("Ambiguous symbol selector"), "got: {result}");
        assert!(result.contains("1"), "got: {result}");
        assert!(result.contains("2"), "got: {result}");
    }

    #[tokio::test]
    async fn test_find_references_delegates_to_formatter() {
        let server = make_server(make_live_index_ready(vec![]));
        let result = server
            .find_references(Parameters(super::FindReferencesInput {
                name: "nonexistent_xyz".to_string(),
                kind: None,
                path: None,
                symbol_kind: None,
                symbol_line: None,
            }))
            .await;
        // Should get "No references found" not a guard message
        assert!(result.contains("No references found"), "got: {result}");
    }

    #[tokio::test]
    async fn test_find_references_exact_selector_excludes_unrelated_same_name_hits() {
        let target = make_file(
            "src/db.rs",
            b"pub fn connect() {}\n",
            vec![make_symbol("connect", SymbolKind::Function, 1, 1)],
        );
        let dependent = make_file_with_refs(
            "src/service.rs",
            b"use crate::db::connect;\nfn run() { connect(); }\n",
            vec![make_symbol("run", SymbolKind::Function, 2, 2)],
            vec![
                make_ref("db", Some("crate::db"), ReferenceKind::Import, 0, None),
                make_ref(
                    "connect",
                    Some("crate::db::connect"),
                    ReferenceKind::Call,
                    1,
                    Some(0),
                ),
            ],
        );
        let unrelated = make_file_with_refs(
            "src/other.rs",
            b"fn run() { connect(); }\n",
            vec![make_symbol("run", SymbolKind::Function, 1, 1)],
            vec![make_ref("connect", None, ReferenceKind::Call, 0, Some(0))],
        );
        let server = make_server(make_live_index_ready(vec![target, dependent, unrelated]));

        let result = server
            .find_references(Parameters(super::FindReferencesInput {
                name: "connect".to_string(),
                kind: Some("call".to_string()),
                path: Some("src/db.rs".to_string()),
                symbol_kind: Some("fn".to_string()),
                symbol_line: Some(1),
            }))
            .await;

        assert!(result.contains("src/service.rs"), "expected dependent hit: {result}");
        assert!(
            !result.contains("src/other.rs"),
            "unrelated same-name file should be excluded: {result}"
        );
    }

    #[tokio::test]
    async fn test_find_references_exact_selector_requires_line_for_ambiguous_symbol() {
        let target = make_file(
            "src/db.rs",
            b"fn connect() {}\nfn connect() {}\n",
            vec![
                make_symbol("connect", SymbolKind::Function, 1, 1),
                make_symbol("connect", SymbolKind::Function, 10, 10),
            ],
        );
        let server = make_server(make_live_index_ready(vec![target]));

        let result = server
            .find_references(Parameters(super::FindReferencesInput {
                name: "connect".to_string(),
                kind: Some("call".to_string()),
                path: Some("src/db.rs".to_string()),
                symbol_kind: Some("fn".to_string()),
                symbol_line: None,
            }))
            .await;

        assert!(result.contains("Ambiguous symbol selector"), "got: {result}");
        assert!(result.contains("1"), "got: {result}");
        assert!(result.contains("10"), "got: {result}");
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
