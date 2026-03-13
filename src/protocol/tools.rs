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
use serde::{Deserialize, Deserializer, Serialize};

/// Deserialize a `u32` from either a JSON number or a stringified number like `"5"`.
fn lenient_u32<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<u32>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum NumOrStr {
        Num(u32),
        Str(String),
        Null,
    }
    match NumOrStr::deserialize(deserializer)? {
        NumOrStr::Num(n) => Ok(Some(n)),
        NumOrStr::Str(s) if s.is_empty() => Ok(None),
        NumOrStr::Str(s) => s.parse::<u32>().map(Some).map_err(serde::de::Error::custom),
        NumOrStr::Null => Ok(None),
    }
}

/// Deserialize a `bool` from either a JSON boolean or a stringified boolean like `"true"`.
fn lenient_bool<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<bool>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum BoolOrStr {
        Bool(bool),
        Str(String),
        Null,
    }
    match BoolOrStr::deserialize(deserializer)? {
        BoolOrStr::Bool(b) => Ok(Some(b)),
        BoolOrStr::Str(s) => match s.as_str() {
            "true" | "1" => Ok(Some(true)),
            "false" | "0" => Ok(Some(false)),
            "" => Ok(None),
            _ => Err(serde::de::Error::custom(format!(
                "expected boolean or \"true\"/\"false\", got \"{s}\""
            ))),
        },
        BoolOrStr::Null => Ok(None),
    }
}

/// Deserialize a required `u32` from either a JSON number or a stringified number.
fn lenient_u32_required<'de, D: Deserializer<'de>>(deserializer: D) -> Result<u32, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum NumOrStr {
        Num(u32),
        Str(String),
    }
    match NumOrStr::deserialize(deserializer)? {
        NumOrStr::Num(n) => Ok(n),
        NumOrStr::Str(s) => s.parse::<u32>().map_err(serde::de::Error::custom),
    }
}

/// Deserialize a `u64` from either a JSON number or a stringified number.
fn lenient_u64<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<u64>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum NumOrStr {
        Num(u64),
        Str(String),
        Null,
    }
    match NumOrStr::deserialize(deserializer)? {
        NumOrStr::Num(n) => Ok(Some(n)),
        NumOrStr::Str(s) if s.is_empty() => Ok(None),
        NumOrStr::Str(s) => s.parse::<u64>().map(Some).map_err(serde::de::Error::custom),
        NumOrStr::Null => Ok(None),
    }
}

/// Deserialize an `i64` from either a JSON number or a stringified number.
fn lenient_i64<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<i64>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum NumOrStr {
        Num(i64),
        Str(String),
        Null,
    }
    match NumOrStr::deserialize(deserializer)? {
        NumOrStr::Num(n) => Ok(Some(n)),
        NumOrStr::Str(s) if s.is_empty() => Ok(None),
        NumOrStr::Str(s) => s.parse::<i64>().map(Some).map_err(serde::de::Error::custom),
        NumOrStr::Null => Ok(None),
    }
}

use crate::domain::LanguageId;
use crate::live_index::{
    IndexedFile, SearchFilesHit, SearchFilesTier, SearchFilesView, search, store::IndexState,
};
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
    #[serde(default, deserialize_with = "lenient_u32")]
    pub start_byte: Option<u32>,
    /// End byte offset for code slice (inclusive).
    #[serde(default, deserialize_with = "lenient_u32")]
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
    #[serde(default, deserialize_with = "lenient_u32")]
    pub limit: Option<u32>,
    /// When true, include generated files in the result set.
    #[serde(default, deserialize_with = "lenient_bool")]
    pub include_generated: Option<bool>,
    /// When true, include test files in the result set.
    #[serde(default, deserialize_with = "lenient_bool")]
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
    #[serde(default, deserialize_with = "lenient_bool")]
    pub regex: Option<bool>,
    /// Optional relative path prefix scope, for example `src/` or `src/protocol`.
    pub path_prefix: Option<String>,
    /// Optional canonical language name such as `Rust`, `TypeScript`, `C#`, or `C++`.
    pub language: Option<String>,
    /// Optional maximum number of matches to return across all files (default 50).
    #[serde(default, deserialize_with = "lenient_u32")]
    pub limit: Option<u32>,
    /// Optional maximum number of matches to return per file (default 5).
    #[serde(default, deserialize_with = "lenient_u32")]
    pub max_per_file: Option<u32>,
    /// When true, include generated files in the result set.
    #[serde(default, deserialize_with = "lenient_bool")]
    pub include_generated: Option<bool>,
    /// When true, include test files in the result set.
    #[serde(default, deserialize_with = "lenient_bool")]
    pub include_tests: Option<bool>,
    /// Optional repo-relative include glob, for example `src/**/*.ts`.
    pub glob: Option<String>,
    /// Optional repo-relative exclude glob, for example `**/*.spec.ts`.
    pub exclude_glob: Option<String>,
    /// Optional symmetric number of surrounding lines to render around each match.
    #[serde(default, deserialize_with = "lenient_u32")]
    pub context: Option<u32>,
    /// Optional case-sensitivity override. Literal mode defaults to false; regex mode defaults to true.
    #[serde(default, deserialize_with = "lenient_bool")]
    pub case_sensitive: Option<bool>,
    /// When true, require whole-word matches for literal searches. Not supported with `regex=true`.
    #[serde(default, deserialize_with = "lenient_bool")]
    pub whole_word: Option<bool>,
    /// Group matches: "file" (default), "symbol" (one entry per enclosing symbol),
    /// or "usage" (exclude imports and comments).
    pub group_by: Option<String>,
    /// When true, for each match include a compact list of callers of the enclosing symbol.
    #[serde(default, deserialize_with = "lenient_bool")]
    pub follow_refs: Option<bool>,
    /// Max number of file matches to enrich with callers when follow_refs=true (default 3).
    #[serde(default, deserialize_with = "lenient_u32")]
    pub follow_refs_limit: Option<u32>,
}

/// Input for `search_files`.
#[derive(Deserialize, Serialize, JsonSchema)]
pub struct SearchFilesInput {
    /// Filename, folder name, or partial path. Optional when `changed_with` is provided.
    #[serde(default)]
    pub query: String,
    /// Optional maximum number of matches to return (default 20, capped at 50).
    #[serde(default, deserialize_with = "lenient_u32")]
    pub limit: Option<u32>,
    /// Optional current file path to boost local results.
    pub current_file: Option<String>,
    /// Find files that frequently co-change with this file (uses git temporal coupling data).
    pub changed_with: Option<String>,
}

/// Input for `resolve_path`.
#[derive(Deserialize, Serialize, JsonSchema)]
pub struct ResolvePathInput {
    /// Filename, partial path, or ambiguous path hint.
    #[serde(alias = "query")]
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
    #[serde(default, deserialize_with = "lenient_i64")]
    pub since: Option<i64>,
    /// Optional git ref to diff against, for example `HEAD~5` or `branch:main`.
    pub git_ref: Option<String>,
    /// When true, report uncommitted git changes. Defaults to true when no other mode is specified and a repo root exists.
    #[serde(default, deserialize_with = "lenient_bool")]
    pub uncommitted: Option<bool>,
}

/// Input for `get_file_content`.
#[derive(Deserialize, Serialize, JsonSchema)]
pub struct GetFileContentInput {
    /// Relative path to the file.
    pub path: String,
    /// First line to include (1-indexed).
    #[serde(default, deserialize_with = "lenient_u32")]
    pub start_line: Option<u32>,
    /// Last line to include (1-indexed, inclusive).
    #[serde(default, deserialize_with = "lenient_u32")]
    pub end_line: Option<u32>,
    /// Select a 1-based chunk from the file using `max_lines` as the chunk size.
    #[serde(default, deserialize_with = "lenient_u32")]
    pub chunk_index: Option<u32>,
    /// Maximum number of lines to include in a chunked read.
    #[serde(default, deserialize_with = "lenient_u32")]
    pub max_lines: Option<u32>,
    /// Center the read around this 1-indexed line.
    #[serde(default, deserialize_with = "lenient_u32")]
    pub around_line: Option<u32>,
    /// Center the read around the first case-insensitive literal match in the file.
    pub around_match: Option<String>,
    /// Center the read around a symbol in the target file.
    pub around_symbol: Option<String>,
    /// Optional exact-selector line for `around_symbol`.
    #[serde(default, deserialize_with = "lenient_u32")]
    pub symbol_line: Option<u32>,
    /// Number of lines of symmetric context to include around `around_line` or `around_match`.
    #[serde(default, deserialize_with = "lenient_u32")]
    pub context_lines: Option<u32>,
    /// Show 1-indexed line numbers for ordinary full-file or explicit-range reads.
    #[serde(default, deserialize_with = "lenient_bool")]
    pub show_line_numbers: Option<bool>,
    /// Prepend a stable path or path-plus-range header for ordinary full-file or explicit-range reads.
    #[serde(default, deserialize_with = "lenient_bool")]
    pub header: Option<bool>,
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
    #[serde(default, deserialize_with = "lenient_u32")]
    pub symbol_line: Option<u32>,
    /// Maximum number of files to show (default 20, capped at 100).
    #[serde(default, deserialize_with = "lenient_u32")]
    pub limit: Option<u32>,
    /// Maximum number of reference hits per file (default 10, capped at 50).
    #[serde(default, deserialize_with = "lenient_u32")]
    pub max_per_file: Option<u32>,
    /// When true, show compact output: file:line [kind] in symbol — no source text (60-75% smaller).
    #[serde(default, deserialize_with = "lenient_bool")]
    pub compact: Option<bool>,
}

/// Input for `find_dependents`.
#[derive(Deserialize, Serialize, JsonSchema)]
pub struct FindDependentsInput {
    /// Relative file path to find dependents for.
    pub path: String,
    /// Maximum number of dependent files to show (default 20, capped at 100).
    #[serde(default, deserialize_with = "lenient_u32")]
    pub limit: Option<u32>,
    /// Maximum number of reference lines per file (default 10, capped at 50).
    #[serde(default, deserialize_with = "lenient_u32")]
    pub max_per_file: Option<u32>,
    /// Output format: "text" (default), "mermaid", or "dot".
    pub format: Option<String>,
    /// When true, show compact output: file:line [kind] without source text (60-75% smaller).
    #[serde(default, deserialize_with = "lenient_bool")]
    pub compact: Option<bool>,
}

/// Input for `find_implementations`.
#[derive(Deserialize, Serialize, JsonSchema)]
pub struct FindImplementationsInput {
    /// Trait/interface name or implementing type name to search for.
    pub name: String,
    /// Search direction: "trait" (find implementors of a trait), "type" (find traits a type implements), or "auto" (default: search both directions).
    pub direction: Option<String>,
    /// Maximum entries to show (default 200).
    #[serde(default, deserialize_with = "lenient_u32")]
    pub limit: Option<u32>,
}

/// Input for `get_file_tree`.
#[derive(Deserialize, Serialize, JsonSchema)]
pub struct GetFileTreeInput {
    /// Subtree path to browse (default: project root).
    pub path: Option<String>,
    /// Max depth levels to expand (default: 2, max: 5).
    #[serde(default, deserialize_with = "lenient_u32")]
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
    #[serde(default, deserialize_with = "lenient_u32")]
    pub symbol_line: Option<u32>,
    /// Output verbosity: "signature" (name+params+return only, ~80% smaller), "compact" (signature + first doc line), "full" (default — complete body).
    pub verbosity: Option<String>,
}

/// Input for `get_file_context`.
#[derive(Deserialize, Serialize, JsonSchema)]
pub struct GetFileContextInput {
    /// Relative path to the file.
    pub path: String,
    /// Optional max token budget, matching hook behavior.
    #[serde(default, deserialize_with = "lenient_u64")]
    pub max_tokens: Option<u64>,
    /// Optional list of sections to include. Allowed values: "outline", "imports", "consumers", "references", "git". Omit to include all sections.
    #[serde(default)]
    pub sections: Option<Vec<String>>,
}

/// Input for `get_symbol_context`.
#[derive(Deserialize, Serialize, JsonSchema)]
pub struct GetSymbolContextInput {
    /// Symbol name to inspect.
    pub name: String,
    /// Optional file filter.
    pub file: Option<String>,
    /// Optional exact-selector path from `search_symbols`.
    pub path: Option<String>,
    /// Optional selected symbol kind such as `fn`, `class`, or `struct`.
    pub symbol_kind: Option<String>,
    /// Optional selected symbol line from `search_symbols`.
    #[serde(default, deserialize_with = "lenient_u32")]
    pub symbol_line: Option<u32>,
    /// Output verbosity: "signature" (name+params+return only, ~80% smaller), "compact" (signature + first doc line), "full" (default — complete body).
    pub verbosity: Option<String>,
}

/// Input for `analyze_file_impact`.
#[derive(Deserialize, Serialize, JsonSchema)]
pub struct AnalyzeFileImpactInput {
    /// Relative path to the file to re-read from disk.
    pub path: String,
    /// When true, treat the file as newly created and index it.
    #[serde(default, deserialize_with = "lenient_bool")]
    pub new_file: Option<bool>,
}

/// Input for `trace_symbol`.
#[derive(Deserialize, Serialize, JsonSchema)]
pub struct TraceSymbolInput {
    /// File path containing the symbol.
    pub path: String,
    /// Symbol name to trace.
    pub name: String,
    /// Optional kind filter (e.g., "fn", "struct").
    pub kind: Option<String>,
    /// Optional line number to disambiguate overloaded symbols.
    #[serde(default, deserialize_with = "lenient_u32")]
    pub symbol_line: Option<u32>,
    /// Optional list of output sections to include. When omitted, all sections are included.
    /// Valid values: "dependents", "siblings", "implementations", "git".
    pub sections: Option<Vec<String>>,
    /// Output verbosity: "signature" (name+params+return only, ~80% smaller), "compact" (signature + first doc line), "full" (default — complete body).
    pub verbosity: Option<String>,
}

/// Input for `inspect_match`.
#[derive(Deserialize, Serialize, JsonSchema)]
pub struct InspectMatchInput {
    /// Relative path to the file.
    pub path: String,
    /// 1-based line number to inspect.
    #[serde(deserialize_with = "lenient_u32_required")]
    pub line: u32,
    /// Number of context lines to show around the match (default 3).
    #[serde(default, deserialize_with = "lenient_u32")]
    pub context: Option<u32>,
}

/// Input for `explore`.
#[derive(Deserialize, Serialize, JsonSchema)]
pub struct ExploreInput {
    /// Natural-language concept or topic to explore (e.g., "error handling", "concurrency", "database").
    pub query: String,
    /// Maximum number of results per category (default 10).
    #[serde(default, deserialize_with = "lenient_u32")]
    pub limit: Option<u32>,
}

/// Input for `get_co_changes`.
#[derive(Deserialize, Serialize, JsonSchema)]
pub struct GetCoChangesInput {
    /// Relative path to the file to query co-changes for.
    pub path: String,
    /// Maximum number of co-changing files to return (default 10).
    #[serde(default, deserialize_with = "lenient_u32")]
    pub limit: Option<u32>,
}

/// Input for `diff_symbols`.
#[derive(Deserialize, Serialize, JsonSchema)]
pub struct DiffSymbolsInput {
    /// Base git ref to compare from (default: "main").
    pub base: Option<String>,
    /// Target git ref to compare to (default: "HEAD").
    pub target: Option<String>,
    /// Optional path filter — only show diffs for files matching this prefix.
    pub path_prefix: Option<String>,
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

fn enrich_with_callers(
    index: &crate::live_index::LiveIndex,
    result: &mut search::TextSearchResult,
    file_limit: usize,
) {
    use std::collections::HashSet;

    for file_matches in result.files.iter_mut().take(file_limit) {
        // Collect unique enclosing symbol names from this file's matches
        let mut symbol_names: HashSet<String> = HashSet::new();
        for m in &file_matches.matches {
            if let Some(ref enc) = m.enclosing_symbol {
                symbol_names.insert(enc.name.clone());
            }
        }

        if symbol_names.is_empty() {
            continue;
        }

        let mut callers: Vec<search::CallerEntry> = Vec::new();
        let mut seen: HashSet<(String, String)> = HashSet::new(); // (file, symbol) dedup

        for sym_name in &symbol_names {
            let refs = index.find_references_for_name(sym_name, None, false);
            for (ref_file, ref_record) in refs {
                // Skip self-references (same file)
                if ref_file == file_matches.path {
                    continue;
                }
                // Get enclosing symbol of the reference
                let enclosing_name = ref_record
                    .enclosing_symbol_index
                    .and_then(|idx| {
                        index
                            .get_file(ref_file)
                            .and_then(|f| f.symbols.get(idx as usize))
                            .map(|s| s.name.clone())
                    })
                    .unwrap_or_else(|| "(top-level)".to_string());

                let key = (ref_file.to_string(), enclosing_name.clone());
                if seen.insert(key) {
                    callers.push(search::CallerEntry {
                        file: ref_file.to_string(),
                        symbol: enclosing_name,
                        line: ref_record.line_range.0 + 1, // 0-based to 1-based
                    });
                }
            }
        }

        // Cap at 10 callers to avoid noise
        callers.truncate(10);

        if !callers.is_empty() {
            file_matches.callers = Some(callers);
        }
    }
}

fn file_content_options_from_input(
    input: &GetFileContentInput,
) -> Result<search::FileContentOptions, String> {
    let show_line_numbers = input.show_line_numbers.unwrap_or(false);
    let header = input.header.unwrap_or(false);
    let ordinary_read_formatting_requested = show_line_numbers || header;

    if input.symbol_line.is_some() && input.around_symbol.is_none() {
        return Err(
            "Invalid get_file_content request: `symbol_line` requires `around_symbol`.".to_string(),
        );
    }

    if let Some(raw_around_symbol) = input.around_symbol.as_deref() {
        let around_symbol = raw_around_symbol.trim();
        if around_symbol.is_empty() {
            return Err(
                "Invalid get_file_content request: `around_symbol` must not be empty.".to_string(),
            );
        }

        if input.start_line.is_some()
            || input.end_line.is_some()
            || input.around_line.is_some()
            || input.around_match.is_some()
            || input.chunk_index.is_some()
            || input.max_lines.is_some()
        {
            return Err(
                "Invalid get_file_content request: `around_symbol` cannot be combined with `start_line`, `end_line`, `around_line`, `around_match`, `chunk_index`, or `max_lines`. Valid with `around_symbol`: `symbol_line`, `context_lines`."
                    .to_string(),
            );
        }

        if ordinary_read_formatting_requested {
            return Err(
                "Invalid get_file_content request: `show_line_numbers` and `header` are only supported for full-file reads or explicit-range reads (`start_line`/`end_line`)."
                    .to_string(),
            );
        }

        return Ok(
            search::FileContentOptions::for_explicit_path_read_around_symbol(
                input.path.clone(),
                around_symbol,
                input.symbol_line,
                input.context_lines,
            ),
        );
    }

    if input.max_lines.is_some() && input.chunk_index.is_none() {
        return Err(
            "Invalid get_file_content request: `max_lines` requires `chunk_index`.".to_string(),
        );
    }

    if let Some(chunk_index) = input.chunk_index {
        let Some(max_lines) = input.max_lines else {
            return Err(
                "Invalid get_file_content request: `chunk_index` requires `max_lines`.".to_string(),
            );
        };

        if chunk_index == 0 {
            return Err(
                "Invalid get_file_content request: `chunk_index` must be 1 or greater.".to_string(),
            );
        }

        if max_lines == 0 {
            return Err(
                "Invalid get_file_content request: `max_lines` must be 1 or greater.".to_string(),
            );
        }

        if input.start_line.is_some()
            || input.end_line.is_some()
            || input.around_line.is_some()
            || input.around_match.is_some()
        {
            return Err(
                "Invalid get_file_content request: chunked reads (`chunk_index` + `max_lines`) cannot be combined with `start_line`, `end_line`, `around_line`, or `around_match`."
                    .to_string(),
            );
        }

        if ordinary_read_formatting_requested {
            return Err(
                "Invalid get_file_content request: `show_line_numbers` and `header` are only supported for full-file reads or explicit-range reads (`start_line`/`end_line`)."
                    .to_string(),
            );
        }

        return Ok(search::FileContentOptions::for_explicit_path_read_chunk(
            input.path.clone(),
            chunk_index,
            max_lines,
        ));
    }

    if let Some(raw_around_match) = input.around_match.as_deref() {
        let around_match = raw_around_match.trim();
        if around_match.is_empty() {
            return Err(
                "Invalid get_file_content request: `around_match` must not be empty.".to_string(),
            );
        }

        if input.start_line.is_some() || input.end_line.is_some() || input.around_line.is_some() {
            return Err(
                "Invalid get_file_content request: `around_match` cannot be combined with `start_line`, `end_line`, or `around_line`. Valid with `around_match`: `context_lines`."
                    .to_string(),
            );
        }

        if ordinary_read_formatting_requested {
            return Err(
                "Invalid get_file_content request: `show_line_numbers` and `header` are only supported for full-file reads or explicit-range reads (`start_line`/`end_line`)."
                    .to_string(),
            );
        }

        return Ok(
            search::FileContentOptions::for_explicit_path_read_around_match(
                input.path.clone(),
                around_match,
                input.context_lines,
            ),
        );
    }

    if input.around_line.is_some() && (input.start_line.is_some() || input.end_line.is_some()) {
        return Err(
            "Invalid get_file_content request: `around_line` cannot be combined with `start_line` or `end_line`. Valid with `around_line`: `context_lines`."
                .to_string(),
        );
    }

    if input.around_line.is_some() && ordinary_read_formatting_requested {
        return Err(
            "Invalid get_file_content request: `show_line_numbers` and `header` are only supported for full-file reads or explicit-range reads (`start_line`/`end_line`)."
                .to_string(),
        );
    }

    Ok(match input.around_line {
        Some(around_line) => search::FileContentOptions::for_explicit_path_read_around_line(
            input.path.clone(),
            around_line,
            input.context_lines,
        ),
        None => search::FileContentOptions::for_explicit_path_read_with_format(
            input.path.clone(),
            input.start_line,
            input.end_line,
            show_line_numbers,
            header,
        ),
    })
}

fn sidecar_state_for_server(server: &TokenizorServer) -> SidecarState {
    SidecarState {
        index: Arc::clone(&server.index),
        token_stats: server.token_stats.clone().unwrap_or_else(TokenStats::new),
        repo_root: server.capture_repo_root(),
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
    /// Symbol outline for a single file — every function, class, enum, trait with line ranges.
    /// Use to see what symbols exist before reading specific ones with get_symbol.
    /// NOT for understanding a file's role/dependencies (use get_file_context).
    /// NOT for reading symbol bodies (use get_symbol).
    #[tool(
        description = "Symbol outline for a single file — every function, class, enum, trait with line ranges. Use to see what symbols exist before reading specific ones with get_symbol. NOT for understanding a file's role/dependencies (use get_file_context). NOT for reading symbol bodies (use get_symbol)."
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
            Some(file) => {
                let raw_chars = file.content.len();
                let result = format::file_outline_from_indexed_file(file.as_ref());
                let footer = format::compact_savings_footer(result.len(), raw_chars);
                format!("{result}{footer}")
            }
            None => format::not_found_file(&params.0.path),
        }
    }

    /// Look up a single symbol by exact file path and name — returns its full source code.
    /// Use when you know exactly which symbol you need to read.
    /// NOT for finding symbols by name (use search_symbols first).
    /// NOT for understanding who calls it (use find_references or get_symbol_context).
    /// NOT for edit preparation (use get_context_bundle).
    #[tool(
        description = "Look up a single symbol by exact file path and name — returns its full source code. Use when you know exactly which symbol you need to read. NOT for finding symbols by name (use search_symbols first). NOT for understanding who calls it (use find_references or get_symbol_context). NOT for edit preparation (use get_context_bundle)."
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

    /// Batch lookup: retrieve multiple symbols or code slices in one call. Use when you need 2+
    /// symbol bodies — avoids multiple get_symbol round-trips. Each target is a file path + symbol
    /// name or byte range.
    #[tool(
        description = "Batch lookup: retrieve multiple symbols or code slices in one call. Use when you need 2+ symbol bodies — avoids multiple get_symbol round-trips. Each target is a file path + symbol name or byte range."
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

    /// Full symbol outline of the entire project — every file with every symbol, kind, and line range.
    /// Warning: large output. Only use when you genuinely need the complete project symbol map.
    /// For a compact overview use get_repo_map. For one file use get_file_outline.
    #[tool(
        description = "Full symbol outline of the entire project — every file with every symbol, kind, and line range. Warning: large output. Only use when you genuinely need the complete project symbol map. For a compact overview use get_repo_map. For one file use get_file_outline."
    )]
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

    /// Start here. Compact project overview: total files, language breakdown, symbol count, and
    /// directory tree with per-directory stats. ~500 tokens. Use as your first call to orient yourself.
    /// NOT for file details (use get_file_context) or finding symbols (use search_symbols).
    #[tool(
        description = "Start here. Compact project overview: total files, language breakdown, symbol count, and directory tree with per-directory stats. ~500 tokens. Use as your first call to orient yourself. NOT for file details (use get_file_context) or finding symbols (use search_symbols)."
    )]
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

    /// Rich file summary: symbol outline, imports, consumers, references, and git activity.
    /// Use sections=['outline','imports'] to limit output. Best tool for understanding a file before editing.
    /// Much smaller than reading the raw file.
    /// NOT for reading actual source code (use get_file_content or get_symbol).
    #[tool(
        description = "Rich file summary: symbol outline, imports, consumers, references, and git activity. Use sections=['outline','imports'] to limit output. Best tool for understanding a file before editing. Much smaller than reading the raw file. NOT for reading actual source code (use get_file_content or get_symbol)."
    )]
    pub(crate) async fn get_file_context(&self, params: Parameters<GetFileContextInput>) -> String {
        if let Some(result) = self.proxy_tool_call("get_file_context", &params.0).await {
            return result;
        }
        let raw_chars = {
            let guard = self.index.read().expect("lock poisoned");
            loading_guard!(guard);
            let raw = guard
                .capture_shared_file(&params.0.path)
                .map(|f| f.content.len())
                .unwrap_or(0);
            drop(guard);
            raw
        };

        let state = sidecar_state_for_server(self);
        let outline = OutlineParams {
            path: params.0.path.clone(),
            max_tokens: params.0.max_tokens,
            sections: params.0.sections.clone(),
        };
        match outline_tool_text(&state, &outline) {
            Ok(result) => {
                let footer = format::compact_savings_footer(result.len(), raw_chars);
                format!("{result}{footer}")
            }
            Err(StatusCode::NOT_FOUND) => format::not_found_file(&params.0.path),
            Err(StatusCode::INTERNAL_SERVER_ERROR) => {
                "File context failed: internal error.".to_string()
            }
            Err(other) => format!("File context failed: HTTP {}", other.as_u16()),
        }
    }

    /// Symbol definition + callers grouped by file + callees + type usages.
    /// Set verbosity='signature' for ~80% smaller output. Use when you need to understand how a
    /// symbol is used across the codebase.
    /// NOT for just the symbol body (use get_symbol).
    /// NOT for full refactoring context with dependents and git (use trace_symbol).
    #[tool(
        description = "Symbol definition + callers grouped by file + callees + type usages. Set verbosity='signature' for ~80% smaller output. Use when you need to understand how a symbol is used across the codebase. NOT for just the symbol body (use get_symbol). NOT for full refactoring context with dependents and git (use trace_symbol)."
    )]
    pub(crate) async fn get_symbol_context(
        &self,
        params: Parameters<GetSymbolContextInput>,
    ) -> String {
        if let Some(result) = self.proxy_tool_call("get_symbol_context", &params.0).await {
            return result;
        }
        let file_path_hint = params.0.path.as_deref().or(params.0.file.as_deref());
        let verbosity = params.0.verbosity.as_deref().unwrap_or("full");

        // Capture the symbol definition from the index so we can prepend it
        // (the sidecar only returns reference locations, not the definition itself).
        let (symbol_header, raw_chars) = {
            let guard = self.index.read().expect("lock poisoned");
            loading_guard!(guard);

            let file = file_path_hint.and_then(|p| guard.capture_shared_file(p));
            let raw = file.as_ref().map(|f| f.content.len()).unwrap_or(0);

            let header = file.and_then(|f| {
                let sym = f.symbols.iter().find(|s| {
                    s.name == params.0.name
                        && params
                            .0
                            .symbol_kind
                            .as_deref()
                            .map(|k| s.kind.to_string().eq_ignore_ascii_case(k))
                            .unwrap_or(true)
                        && params.0.symbol_line.map(|l| s.line_range.0 == l).unwrap_or(true)
                })?;
                let body = std::str::from_utf8(
                    &f.content[sym.byte_range.0 as usize..sym.byte_range.1 as usize],
                )
                .ok()?;
                let rendered = format::apply_verbosity(body, verbosity);
                Some(format!(
                    "{}\n[{}, {}:{}-{}]",
                    rendered, sym.kind, f.relative_path, sym.line_range.0, sym.line_range.1
                ))
            });

            (header, raw)
        };

        let state = sidecar_state_for_server(self);
        let symbol_context = SymbolContextParams {
            name: params.0.name.clone(),
            file: params.0.file.clone(),
            path: params.0.path.clone(),
            symbol_kind: params.0.symbol_kind.clone(),
            symbol_line: params.0.symbol_line,
        };
        match symbol_context_tool_text(&state, &symbol_context) {
            Ok(refs_text) => {
                let mut output = String::new();
                if let Some(header) = &symbol_header {
                    output.push_str(header);
                    output.push_str("\n\n");
                }
                output.push_str(&refs_text);
                let footer = format::compact_savings_footer(output.len(), raw_chars);
                format!("{output}{footer}")
            }
            Err(StatusCode::INTERNAL_SERVER_ERROR) => {
                "Symbol context failed: internal error.".to_string()
            }
            Err(other) => format!("Symbol context failed: HTTP {}", other.as_u16()),
        }
    }

    /// Call AFTER editing a file. Re-reads from disk, updates the index, reports added/removed/modified
    /// symbols and affected dependents. Always call this after making edits to keep the index current.
    #[tool(
        description = "Call AFTER editing a file. Re-reads from disk, updates the index, reports added/removed/modified symbols and affected dependents. Always call this after making edits to keep the index current."
    )]
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

    /// Find symbols by name substring across the project — returns name, kind, file, line range.
    /// Use when you know part of a symbol name but not the file. Supports kind filter, language filter,
    /// and path prefix scope.
    /// NOT for text content search (use search_text). NOT for file path search (use search_files).
    #[tool(
        description = "Find symbols by name substring across the project — returns name, kind, file, line range. Use when you know part of a symbol name but not the file. Supports kind filter, language filter, and path prefix scope. NOT for text content search (use search_text). NOT for file path search (use search_files)."
    )]
    pub(crate) async fn search_symbols(&self, params: Parameters<SearchSymbolsInput>) -> String {
        if params.0.query.trim().is_empty() {
            return "search_symbols requires a non-empty query. Provide a symbol name or substring to search for.".to_string();
        }
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

    /// Full-text search across file contents — literal, OR-terms, or regex. Shows matches with
    /// enclosing symbol context. Use group_by='symbol' to deduplicate, follow_refs=true to inline
    /// callers (control cost with follow_refs_limit). Use when searching for string patterns in code.
    /// NOT for symbol name search (use search_symbols). NOT for file path search (use search_files).
    #[tool(
        description = "Full-text search across file contents — literal, OR-terms, or regex. Shows matches with enclosing symbol context. Use group_by='symbol' to deduplicate, follow_refs=true to inline callers (control cost with follow_refs_limit). Use when searching for string patterns in code. NOT for symbol name search (use search_symbols). NOT for file path search (use search_files)."
    )]
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
            let mut r = search::search_text_with_options(
                &guard,
                params.0.query.as_deref(),
                params.0.terms.as_deref(),
                params.0.regex.unwrap_or(false),
                &options,
            );
            // Enrich with callers if follow_refs is set
            if params.0.follow_refs.unwrap_or(false) {
                if let Ok(ref mut text_result) = r {
                    let limit = params.0.follow_refs_limit.unwrap_or(3) as usize;
                    enrich_with_callers(&guard, text_result, limit);
                }
            }
            r
        };
        format::search_text_result_view(result, params.0.group_by.as_deref())
    }

    /// Most comprehensive symbol analysis. Definition, callers, callees, implementations, type
    /// dependencies, git activity — all in one call. Set verbosity='signature' for ~80% smaller output.
    /// Use sections=['dependents','git'] to limit output. Use before refactoring when you need the
    /// complete picture.
    /// NOT for quick reads (use get_symbol). NOT for edit prep (use get_context_bundle).
    #[tool(
        description = "Most comprehensive symbol analysis. Definition, callers, callees, implementations, type dependencies, git activity — all in one call. Set verbosity='signature' for ~80% smaller output. Use sections=['dependents','git'] to limit output. Use before refactoring when you need the complete picture. NOT for quick reads (use get_symbol). NOT for edit prep (use get_context_bundle)."
    )]
    pub(crate) async fn trace_symbol(&self, params: Parameters<TraceSymbolInput>) -> String {
        if let Some(result) = self.proxy_tool_call("trace_symbol", &params.0).await {
            return result;
        }

        let mut trace_view = {
            let guard = self.index.read().expect("lock poisoned");
            loading_guard!(guard);
            guard.capture_trace_symbol_view(
                &params.0.path,
                &params.0.name,
                params.0.kind.as_deref(),
                params.0.symbol_line,
                params.0.sections.as_deref(),
            )
        };

        // Fill in git activity if it was requested (or if all sections requested)
        if let crate::live_index::TraceSymbolView::Found(ref mut found) = trace_view {
            let wants_git = params
                .0
                .sections
                .as_ref()
                .map(|s| s.iter().any(|v| v.eq_ignore_ascii_case("git")))
                .unwrap_or(true);

            if wants_git {
                let temporal = self.index.git_temporal();
                if temporal.state == crate::live_index::git_temporal::GitTemporalState::Ready {
                    if let Some(history) = temporal.files.get(&params.0.path) {
                        use crate::live_index::git_temporal::{
                            churn_bar, churn_label, relative_time,
                        };

                        found.git_activity = Some(crate::live_index::GitActivityView {
                            churn_score: history.churn_score,
                            churn_bar: churn_bar(history.churn_score),
                            churn_label: churn_label(history.churn_score).to_string(),
                            commit_count: history.commit_count,
                            last_relative: relative_time(history.last_commit.days_ago),
                            last_hash: history.last_commit.hash.clone(),
                            last_message: history.last_commit.message_head.clone(),
                            last_author: history.last_commit.author.clone(),
                            last_timestamp: history.last_commit.timestamp.clone(),
                            owners: history
                                .contributors
                                .iter()
                                .map(|c| format!("{} {:.0}%", c.author, c.percentage))
                                .collect(),
                            co_changes: history
                                .co_changes
                                .iter()
                                .map(|e| (e.path.clone(), e.coupling_score, e.shared_commits))
                                .collect(),
                        });
                    }
                }
            }
        }

        let verbosity = params.0.verbosity.as_deref().unwrap_or("full");
        format::trace_symbol_result_view(&trace_view, &params.0.name, verbosity)
    }

    /// Deep-dive a search_text match: given path + line number, shows the line in full symbol context
    /// with callers and type deps. Use AFTER search_text to understand a specific hit.
    /// NOT as a first-call tool (search first, then inspect).
    #[tool(
        description = "Deep-dive a search_text match: given path + line number, shows the line in full symbol context with callers and type deps. Use AFTER search_text to understand a specific hit. NOT as a first-call tool (search first, then inspect)."
    )]
    pub(crate) async fn inspect_match(&self, params: Parameters<InspectMatchInput>) -> String {
        if let Some(result) = self.proxy_tool_call("inspect_match", &params.0).await {
            return result;
        }

        let view = {
            let guard = self.index.read().expect("lock poisoned");
            loading_guard!(guard);
            guard.capture_inspect_match_view(&params.0.path, params.0.line, params.0.context)
        };

        format::inspect_match_result_view(&view)
    }

    /// Find files by path, filename, or folder — ranked by relevance. With changed_with=path,
    /// finds co-changing files via git temporal coupling. Use when looking for a file by name or path.
    /// NOT for file content search (use search_text). NOT for symbol names (use search_symbols).
    #[tool(
        description = "Find files by path, filename, or folder — ranked by relevance. With changed_with=path, finds co-changing files via git temporal coupling. Use when looking for a file by name or path. NOT for file content search (use search_text). NOT for symbol names (use search_symbols)."
    )]
    pub(crate) async fn search_files(&self, params: Parameters<SearchFilesInput>) -> String {
        if let Some(result) = self.proxy_tool_call("search_files", &params.0).await {
            return result;
        }

        // Handle changed_with (git temporal coupling)
        if let Some(ref target_path) = params.0.changed_with {
            let temporal = self.index.git_temporal();
            if temporal.state == crate::live_index::git_temporal::GitTemporalState::Ready {
                if let Some(history) = temporal.files.get(target_path.as_str()) {
                    let hits: Vec<SearchFilesHit> = history
                        .co_changes
                        .iter()
                        .map(|entry| SearchFilesHit {
                            tier: SearchFilesTier::CoChange,
                            path: entry.path.clone(),
                            coupling_score: Some(entry.coupling_score),
                            shared_commits: Some(entry.shared_commits),
                        })
                        .collect();
                    let total = hits.len();
                    return format::search_files_result_view(&SearchFilesView::Found {
                        query: format!("co-changes with {target_path}"),
                        total_matches: total,
                        overflow_count: 0,
                        hits,
                    });
                }
                return format!(
                    "No git history found for '{target_path}'. Check the file path is correct."
                );
            }
            return "Git temporal data is not yet available. It loads asynchronously after the first index.".to_string();
        }

        if params.0.query.is_empty() {
            return "search_files requires a non-empty `query` (or use `changed_with` to find co-changing files).".to_string();
        }

        let view = {
            let guard = self.index.read().expect("lock poisoned");
            loading_guard!(guard);
            guard.capture_search_files_view(
                &params.0.query,
                params.0.limit.unwrap_or(20) as usize,
                params.0.current_file.as_deref(),
            )
        };
        format::search_files_result_view(&view)
    }

    /// Resolve an ambiguous filename or partial path to one exact project path. Returns best match
    /// or ranked candidates. Use when you have a partial or ambiguous filename and need the exact path.
    /// NOT for browsing files (use search_files).
    #[tool(
        description = "Resolve an ambiguous filename or partial path to one exact project path. Returns best match or ranked candidates. Use when you have a partial or ambiguous filename and need the exact path. NOT for browsing files (use search_files)."
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

    /// Diagnostic: index status, file/symbol counts, load time, watcher state, token savings,
    /// git temporal status. Always responds even during loading. Use to verify Tokenizor is working.
    #[tool(
        description = "Diagnostic: index status, file/symbol counts, load time, watcher state, token savings, git temporal status. Always responds even during loading. Use to verify Tokenizor is working."
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

        // Append git temporal summary.
        result.push('\n');
        result.push_str(&format::git_temporal_health_line(
            &self.index.git_temporal(),
        ));

        result
    }

    /// Reindex a directory from scratch — replaces the current index, restarts watcher, triggers
    /// git temporal analysis. Use when switching projects. Destructive to current index.
    #[tool(
        description = "Reindex a directory from scratch — replaces the current index, restarts watcher, triggers git temporal analysis. Use when switching projects. Destructive to current index."
    )]
    pub(crate) async fn index_folder(&self, params: Parameters<IndexFolderInput>) -> String {
        if let Some(result) = self.proxy_tool_call("index_folder", &params.0).await {
            // The daemon has rebound the session to the new project. Update our
            // local repo_root so that local-fallback tools (what_changed,
            // analyze_file_impact) and ensure_local_index use the correct root
            // if the daemon connection degrades later.
            if result.starts_with("Indexed ") {
                let new_root = PathBuf::from(&params.0.path);
                self.set_repo_root(Some(new_root));
            }
            return result;
        }
        let root = PathBuf::from(&params.0.path);
        match self.index.reload(&root) {
            Ok(()) => {
                let published = self.index.published_state();
                let file_count = published.file_count;
                let symbol_count = published.symbol_count;

                self.set_repo_root(Some(root.clone()));

                // Restart the file watcher at the new root so freshness continues.
                crate::watcher::restart_watcher(
                    root.clone(),
                    Arc::clone(&self.index),
                    Arc::clone(&self.watcher_info),
                );
                tracing::info!(root = %root.display(), "file watcher restarted after index_folder");

                // Refresh git temporal data for the new root.
                crate::live_index::git_temporal::spawn_git_temporal_computation(
                    Arc::clone(&self.index),
                    root,
                );

                format!("Indexed {} files, {} symbols.", file_count, symbol_count)
            }
            Err(e) => format!("Index failed: {e}"),
        }
    }

    /// List changed files: uncommitted=true for working tree, git_ref for ref comparison, since for
    /// timestamp filter. Use to see what files changed.
    /// NOT for symbol-level diffs (use diff_symbols).
    #[tool(
        description = "List changed files: uncommitted=true for working tree, git_ref for ref comparison, since for timestamp filter. Use to see what files changed. NOT for symbol-level diffs (use diff_symbols)."
    )]
    pub(crate) async fn what_changed(&self, params: Parameters<WhatChangedInput>) -> String {
        if let Some(result) = self.proxy_tool_call("what_changed", &params.0).await {
            return result;
        }
        let repo_root = self.capture_repo_root();
        let mode = match determine_what_changed_mode(&params.0, repo_root.is_some()) {
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

                let Some(repo_root) = repo_root.as_deref() else {
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

                let Some(repo_root) = repo_root.as_deref() else {
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

    /// Read raw file content. Modes: full file, line range, around_line/around_match/around_symbol,
    /// or chunked paging. Only use when you need actual source text that other tools don't provide.
    /// For structured understanding use get_file_outline or get_file_context. For a single function
    /// body use get_symbol.
    #[tool(
        description = "Read raw file content. Modes: full file, line range, around_line/around_match/around_symbol, or chunked paging. Only use when you need actual source text that other tools don't provide. For structured understanding use get_file_outline or get_file_context. For a single function body use get_symbol."
    )]
    pub(crate) async fn get_file_content(&self, params: Parameters<GetFileContentInput>) -> String {
        if let Some(result) = self.proxy_tool_call("get_file_content", &params.0).await {
            return result;
        }
        let options = match file_content_options_from_input(&params.0) {
            Ok(options) => options,
            Err(message) => return message,
        };
        let file = {
            let guard = self.index.read().expect("lock poisoned");
            loading_guard!(guard);
            guard.capture_shared_file_for_scope(&options.path_scope)
        };
        match file {
            Some(file) => format::file_content_from_indexed_file_with_context(
                file.as_ref(),
                options.content_context,
            ),
            None => format::not_found_file(&params.0.path),
        }
    }

    /// Find all references (call sites, imports, type usages) for a symbol, grouped by file.
    /// Set compact=true for ~60-75% smaller output. Use when you need 'who calls this?'
    /// NOT for file-level dependencies (use find_dependents).
    /// NOT for full refactoring context (use trace_symbol).
    #[tool(
        description = "Find all references (call sites, imports, type usages) for a symbol, grouped by file. Set compact=true for ~60-75% smaller output. Use when you need 'who calls this?' Lighter than get_symbol_context (no callee/type resolution). NOT for file-level dependencies (use find_dependents). NOT for full refactoring context (use trace_symbol)."
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
        let limits =
            format::OutputLimits::new(input.limit.unwrap_or(20), input.max_per_file.unwrap_or(10));
        match result {
            Ok(view) if input.compact.unwrap_or(false) => {
                format::find_references_compact_view(&view, &input.name, &limits)
            }
            Ok(view) => format::find_references_result_view(&view, &input.name, &limits),
            Err(error) => error,
        }
    }

    /// File-level dependency graph: which files import the given file. Set compact=true for ~60-75%
    /// smaller output. Supports Mermaid/Graphviz output. Use for "what breaks if I change this file?"
    /// NOT for symbol-level references (use find_references).
    /// NOT for git co-change patterns (use get_co_changes).
    #[tool(
        description = "File-level dependency graph: which files import the given file. Set compact=true for ~60-75% smaller output. Supports Mermaid/Graphviz output. Use for 'what breaks if I change this file?' NOT for symbol-level references (use find_references). NOT for git co-change patterns (use get_co_changes)."
    )]
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
        let limits =
            format::OutputLimits::new(input.limit.unwrap_or(20), input.max_per_file.unwrap_or(10));
        let fmt = input.format.as_deref().unwrap_or("text");
        match fmt {
            "mermaid" => format::find_dependents_mermaid(&view, &input.path, &limits),
            "dot" => format::find_dependents_dot(&view, &input.path, &limits),
            _ if input.compact.unwrap_or(false) => {
                format::find_dependents_compact_view(&view, &input.path, &limits)
            }
            _ => format::find_dependents_result_view(&view, &input.path, &limits),
        }
    }

    /// Find interface/trait implementations bidirectionally: given an interface, find implementors;
    /// given a type, find interfaces it implements. Supports C#, TypeScript, Rust, Java, Python, and more.
    /// Use when tracing interface hierarchies.
    #[tool(
        description = "Find interface/trait implementations bidirectionally: given an interface, find implementors; given a type, find interfaces it implements. Supports C#, TypeScript, Rust, Java, Python, and more. Use when tracing interface hierarchies."
    )]
    pub(crate) async fn find_implementations(
        &self,
        params: Parameters<FindImplementationsInput>,
    ) -> String {
        if let Some(result) = self
            .proxy_tool_call("find_implementations", &params.0)
            .await
        {
            return result;
        }
        let input = &params.0;
        let view = {
            let guard = self.index.read().expect("lock poisoned");
            loading_guard!(guard);
            guard.capture_find_implementations_view(&input.name, input.direction.as_deref())
        };
        let cap = input.limit.unwrap_or(200).min(500);
        let limits = format::OutputLimits::new(cap, cap);
        format::find_implementations_result_view(&view, &input.name, &limits)
    }

    /// Browsable file tree with per-file symbol counts and language tags. Supports subtree path and
    /// depth limit. Use to understand directory structure.
    /// NOT for file contents (use get_file_context) or project-wide overview (use get_repo_map).
    #[tool(
        description = "Browsable file tree with per-file symbol counts and language tags. Supports subtree path and depth limit. Use to understand directory structure. NOT for file contents (use get_file_context) or project-wide overview (use get_repo_map)."
    )]
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

    /// One-call edit preparation: symbol body + full definitions of all referenced custom types,
    /// resolved recursively. Set verbosity='signature' for ~80% smaller output. Best tool before
    /// editing a function — gives you the symbol and everything it depends on.
    /// NOT for understanding callers (use get_symbol_context or find_references).
    #[tool(
        description = "One-call edit preparation: symbol body + full definitions of all referenced custom types, resolved recursively. Set verbosity='signature' for ~80% smaller output. Best tool before editing a function — gives you the symbol and everything it depends on. NOT for understanding callers (use get_symbol_context or find_references)."
    )]
    pub(crate) async fn get_context_bundle(
        &self,
        params: Parameters<GetContextBundleInput>,
    ) -> String {
        if let Some(result) = self.proxy_tool_call("get_context_bundle", &params.0).await {
            return result;
        }
        let input = &params.0;
        let (view, raw_chars) = {
            let guard = self.index.read().expect("lock poisoned");
            loading_guard!(guard);
            let raw = guard
                .capture_shared_file(&input.path)
                .map(|f| f.content.len())
                .unwrap_or(0);
            let v = guard.capture_context_bundle_view(
                &input.path,
                &input.name,
                input.kind.as_deref(),
                input.symbol_line,
            );
            (v, raw)
        };
        let verbosity = input.verbosity.as_deref().unwrap_or("full");
        let result = format::context_bundle_result_view(&view, verbosity);
        let footer = format::compact_savings_footer(result.len(), raw_chars);
        format!("{result}{footer}")
    }

    /// Start here when you don't know where to look. Accepts a natural-language concept
    /// (e.g. 'error handling', 'authentication') and returns a unified overview of related symbols,
    /// patterns, and files. Use for conceptual questions like 'how does X work?'.
    /// NOT for finding a specific symbol by name (use search_symbols).
    /// NOT for text content search (use search_text).
    #[tool(
        description = "Start here when you don't know where to look. Accepts a natural-language concept (e.g. 'error handling', 'authentication') and returns a unified overview of related symbols, patterns, and files. Use for conceptual questions like 'how does X work?'. NOT for finding a specific symbol by name (use search_symbols). NOT for text content search (use search_text)."
    )]
    pub(crate) async fn explore(&self, params: Parameters<ExploreInput>) -> String {
        if let Some(result) = self.proxy_tool_call("explore", &params.0).await {
            return result;
        }
        let limit = params.0.limit.unwrap_or(10) as usize;
        let guard = self.index.read().expect("lock poisoned");
        loading_guard!(guard);

        let concept = super::explore::match_concept(&params.0.query);

        let (label, symbol_queries, text_queries): (String, Vec<String>, Vec<String>) =
            if let Some(c) = concept {
                (
                    c.label.to_string(),
                    c.symbol_queries.iter().map(|s| s.to_string()).collect(),
                    c.text_queries.iter().map(|s| s.to_string()).collect(),
                )
            } else {
                let terms = super::explore::fallback_terms(&params.0.query);
                if terms.is_empty() {
                    return "Explore requires a non-empty query.".to_string();
                }
                (format!("'{}'", params.0.query), terms.clone(), terms)
            };

        // Collect symbol matches
        let mut symbol_hits: Vec<(String, String, String)> = Vec::new(); // (name, kind, path)
        for sq in &symbol_queries {
            let result = search::search_symbols(&guard, sq, None, limit);
            for hit in &result.hits {
                if symbol_hits.len() >= limit {
                    break;
                }
                let entry = (hit.name.clone(), hit.kind.clone(), hit.path.clone());
                if !symbol_hits.contains(&entry) {
                    symbol_hits.push(entry);
                }
            }
        }

        // Collect text pattern matches
        let mut text_hits: Vec<(String, String, usize)> = Vec::new(); // (path, line, line_number)
        for tq in &text_queries {
            let options = search::TextSearchOptions {
                total_limit: limit.min(50),
                max_per_file: 2,
                ..search::TextSearchOptions::for_current_code_search()
            };
            let result = search::search_text_with_options(&guard, Some(tq), None, false, &options);
            if let Ok(r) = result {
                for file in &r.files {
                    for m in &file.matches {
                        if text_hits.len() >= limit {
                            break;
                        }
                        text_hits.push((file.path.clone(), m.line.clone(), m.line_number));
                    }
                }
            }
        }

        // Count files by symbol/text presence
        let mut file_counts: std::collections::HashMap<String, usize> =
            std::collections::HashMap::new();
        for (_, _, path) in &symbol_hits {
            *file_counts.entry(path.clone()).or_default() += 1;
        }
        for (path, _, _) in &text_hits {
            *file_counts.entry(path.clone()).or_default() += 1;
        }
        let mut related_files: Vec<(String, usize)> = file_counts.into_iter().collect();
        related_files.sort_by(|a, b| b.1.cmp(&a.1));
        related_files.truncate(limit);

        format::explore_result_view(&label, &symbol_hits, &text_hits, &related_files)
    }

    /// Git temporal data for a specific file: co-changing files (Jaccard coupling), churn score,
    /// ownership, last commit. Use to discover hidden coupling — files that always change together.
    /// Requires git temporal data to be loaded (check health).
    #[tool(
        description = "Git temporal data for a specific file: co-changing files (Jaccard coupling), churn score, ownership, last commit. Use to discover hidden coupling — files that always change together. Requires git temporal data to be loaded (check health)."
    )]
    pub(crate) async fn get_co_changes(&self, params: Parameters<GetCoChangesInput>) -> String {
        if let Some(result) = self.proxy_tool_call("get_co_changes", &params.0).await {
            return result;
        }

        let temporal = self.index.git_temporal();
        match temporal.state {
            crate::live_index::git_temporal::GitTemporalState::Ready => {}
            crate::live_index::git_temporal::GitTemporalState::Pending
            | crate::live_index::git_temporal::GitTemporalState::Computing => {
                return "Git temporal data is still loading. Try again in a few seconds."
                    .to_string();
            }
            crate::live_index::git_temporal::GitTemporalState::Unavailable(ref reason) => {
                return format!("Git temporal data unavailable: {reason}");
            }
        }

        let limit = params.0.limit.unwrap_or(10) as usize;
        let path = params.0.path.as_str();

        match temporal.files.get(path) {
            Some(history) => format::get_co_changes_result_view(path, history, limit),
            None => format!(
                "No git history found for '{path}'. Check the file path is correct and that the file has been committed."
            ),
        }
    }

    /// Symbol-level diff between two git refs. Shows +added, -removed, ~modified symbols per changed
    /// file. Use for code review to see which functions/classes changed.
    /// NOT for file-level change lists (use what_changed).
    #[tool(
        description = "Symbol-level diff between two git refs. Shows +added, -removed, ~modified symbols per changed file. Use for code review to see which functions/classes changed. NOT for file-level change lists (use what_changed)."
    )]
    pub(crate) async fn diff_symbols(&self, params: Parameters<DiffSymbolsInput>) -> String {
        if let Some(result) = self.proxy_tool_call("diff_symbols", &params.0).await {
            return result;
        }
        let base = params.0.base.as_deref().unwrap_or("main");
        let target = params.0.target.as_deref().unwrap_or("HEAD");

        let repo_root = self.capture_repo_root();

        let Some(repo_root) = repo_root else {
            return "No repository root found.".to_string();
        };

        // Check index is not loading/empty
        {
            let guard = self.index.read().expect("lock poisoned");
            loading_guard!(guard);
        }

        // Get changed files
        let diff_output = match run_git(
            &repo_root,
            &["diff", "--name-only", &format!("{base}...{target}")],
        ) {
            Ok(output) => output,
            Err(e) => return format!("Failed to run git diff: {e}"),
        };

        let changed_files_owned = parse_git_name_only_paths(&diff_output);

        // Apply path_prefix filter
        let changed_files: Vec<&str> = changed_files_owned
            .iter()
            .map(|s| s.as_str())
            .filter(|p| {
                params
                    .0
                    .path_prefix
                    .as_ref()
                    .map_or(true, |prefix| p.starts_with(prefix.as_str()))
            })
            .collect();

        if changed_files.is_empty() {
            return format!("No file changes found between {base} and {target}.");
        }

        format::diff_symbols_result_view(base, target, &changed_files, &repo_root)
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

    struct CwdGuard {
        previous: PathBuf,
    }

    impl CwdGuard {
        fn set(path: &Path) -> Self {
            let previous = std::env::current_dir().expect("current dir");
            std::env::set_current_dir(path).expect("set current dir");
            Self { previous }
        }
    }

    impl Drop for CwdGuard {
        fn drop(&mut self) {
            if std::env::set_current_dir(&self.previous).is_err() {
                std::env::set_current_dir(env!("CARGO_MANIFEST_DIR")).expect("restore current dir");
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
                sections: None,
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
    async fn test_get_file_context_shows_imports_and_used_by_sections() {
        let callee = make_symbol("target", SymbolKind::Function, 1, 3);
        let caller = make_symbol("caller", SymbolKind::Function, 1, 3);
        // caller.rs imports from crate::target and calls target().
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
        let target_file = make_file("src/target.rs", b"fn target() {}", vec![callee]);
        let server = make_server(make_live_index_ready(vec![target_file, caller_file]));

        // Check caller.rs — should have "Imports from" section.
        let caller_result = server
            .get_file_context(Parameters(super::GetFileContextInput {
                path: "src/caller.rs".to_string(),
                max_tokens: Some(2000),
                sections: None,
            }))
            .await;
        assert!(
            caller_result.contains("Imports from"),
            "caller should show imports section; got: {caller_result}"
        );
        assert!(
            caller_result.contains("crate::target"),
            "caller should list crate::target as import source; got: {caller_result}"
        );

        // Check target.rs — should have "Used by" section.
        let target_result = server
            .get_file_context(Parameters(super::GetFileContextInput {
                path: "src/target.rs".to_string(),
                max_tokens: Some(2000),
                sections: None,
            }))
            .await;
        assert!(
            target_result.contains("Used by"),
            "target should show used-by section; got: {target_result}"
        );
        assert!(
            target_result.contains("src/caller.rs"),
            "target should list caller.rs as consumer; got: {target_result}"
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
                sections: None,
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
                path: None,
                symbol_kind: None,
                symbol_line: None,
                verbosity: None,
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
    async fn test_get_symbol_context_exact_selector_excludes_unrelated_same_name_hits() {
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
            .get_symbol_context(Parameters(super::GetSymbolContextInput {
                name: "connect".to_string(),
                file: None,
                path: Some("src/db.rs".to_string()),
                symbol_kind: Some("fn".to_string()),
                symbol_line: Some(1),
                verbosity: None,
            }))
            .await;

        assert!(
            result.contains("src/service.rs"),
            "expected dependent hit: {result}"
        );
        assert!(
            !result.contains("src/other.rs"),
            "unrelated same-name file should be excluded: {result}"
        );
    }

    #[tokio::test]
    async fn test_get_symbol_context_exact_selector_requires_line_for_ambiguous_symbol() {
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
            .get_symbol_context(Parameters(super::GetSymbolContextInput {
                name: "connect".to_string(),
                file: None,
                path: Some("src/db.rs".to_string()),
                symbol_kind: Some("fn".to_string()),
                symbol_line: None,
                verbosity: None,
            }))
            .await;

        assert!(
            result.contains("Ambiguous symbol selector"),
            "got: {result}"
        );
        assert!(result.contains("1"), "got: {result}");
        assert!(result.contains("2"), "got: {result}");
    }

    #[tokio::test]
    async fn test_get_symbol_context_exact_selector_respects_file_filter() {
        let target = make_file(
            "src/db.rs",
            b"pub fn connect() {}\n",
            vec![make_symbol("connect", SymbolKind::Function, 1, 1)],
        );
        let service = make_file_with_refs(
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
        let api = make_file_with_refs(
            "src/api.rs",
            b"use crate::db::connect;\nfn expose() { connect(); }\n",
            vec![make_symbol("expose", SymbolKind::Function, 2, 2)],
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
        let server = make_server(make_live_index_ready(vec![target, service, api]));

        let result = server
            .get_symbol_context(Parameters(super::GetSymbolContextInput {
                name: "connect".to_string(),
                file: Some("src/service.rs".to_string()),
                path: Some("src/db.rs".to_string()),
                symbol_kind: Some("fn".to_string()),
                symbol_line: Some(1),
                verbosity: None,
            }))
            .await;

        assert!(result.contains("src/service.rs"), "got: {result}");
        assert!(!result.contains("src/api.rs"), "got: {result}");
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
    async fn test_index_folder_rebinds_repo_root_for_local_impact_analysis() {
        let repo = TempDir::new().expect("temp repo");
        fs::create_dir_all(repo.path().join("scratch")).expect("scratch dir");
        let source_path = repo.path().join("scratch").join("impact_case.rs");
        fs::write(&source_path, "pub fn old_name() {}\n").expect("write initial source");

        let server = make_server(make_live_index_empty());
        let index_result = server
            .index_folder(Parameters(super::IndexFolderInput {
                path: repo.path().display().to_string(),
            }))
            .await;

        assert!(
            index_result.contains("Indexed 1 files"),
            "index_folder should load the temp repo, got: {index_result}"
        );

        fs::write(&source_path, "pub fn new_name() {}\n").expect("write updated source");
        let outside = TempDir::new().expect("outside cwd");
        let _cwd_guard = CwdGuard::set(outside.path());

        let impact = server
            .analyze_file_impact(Parameters(super::AnalyzeFileImpactInput {
                path: "scratch/impact_case.rs".to_string(),
                new_file: None,
            }))
            .await;

        assert!(
            impact.contains("new_name"),
            "impact analysis should keep using the indexed repo root after index_folder, got: {impact}"
        );

        let outline = server
            .get_file_outline(Parameters(super::GetFileOutlineInput {
                path: "scratch/impact_case.rs".to_string(),
            }))
            .await;

        assert!(
            outline.contains("new_name"),
            "impact analysis must not replace the indexed file with an empty parse, got: {outline}"
        );
    }

    #[tokio::test]
    async fn test_index_folder_rebinds_repo_root_for_local_what_changed_git_mode() {
        let repo = init_git_repo();
        fs::create_dir_all(repo.path().join("src")).expect("create src dir");
        fs::write(repo.path().join("src/lib.rs"), "fn foo() {}\n").expect("write initial file");
        run_git(repo.path(), &["add", "."]);
        run_git(repo.path(), &["commit", "-m", "init", "-q"]);

        let server = make_server(make_live_index_empty());
        let index_result = server
            .index_folder(Parameters(super::IndexFolderInput {
                path: repo.path().display().to_string(),
            }))
            .await;
        assert!(
            index_result.contains("Indexed 1 files"),
            "index_folder should load the temp repo, got: {index_result}"
        );

        fs::write(
            repo.path().join("src/lib.rs"),
            "fn foo() { println!(\"changed\"); }\n",
        )
        .expect("modify tracked file");
        let outside = TempDir::new().expect("outside cwd");
        let _cwd_guard = CwdGuard::set(outside.path());

        let result = server
            .what_changed(Parameters(super::WhatChangedInput {
                since: None,
                git_ref: None,
                uncommitted: None,
            }))
            .await;

        assert!(
            result.contains("src/lib.rs"),
            "what_changed should keep using the indexed repo root after index_folder, got: {result}"
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

        assert!(
            result.contains("class Job"),
            "expected primary hit: {result}"
        );
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

        assert!(
            result.contains("class Job"),
            "expected primary hit: {result}"
        );
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

        assert!(
            result.contains("class Job"),
            "expected primary hit: {result}"
        );
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

        assert!(
            result.contains("1 matches in 1 files"),
            "expected bounded output: {result}"
        );
        assert!(
            result.contains("class JobCard"),
            "expected scoped class hit: {result}"
        );
        assert!(
            !result.contains("JobList"),
            "limit should truncate later hits: {result}"
        );
        assert!(
            !result.contains("src/models/job.rs"),
            "path scope should exclude rust model: {result}"
        );
        assert!(
            !result.contains("fn JobRunner"),
            "kind filter should exclude function: {result}"
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
                group_by: None,
                follow_refs: None,
                follow_refs_limit: None,
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
                group_by: None,
                follow_refs: None,
                follow_refs_limit: None,
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
                group_by: None,
                follow_refs: None,
                follow_refs_limit: None,
            }))
            .await;

        assert!(
            result.contains("src/real.rs"),
            "expected visible file: {result}"
        );
        assert!(
            !result.contains("tests/generated/noise.rs"),
            "generated/test noise should be hidden by default: {result}"
        );
    }

    #[tokio::test]
    async fn test_search_text_tool_respects_scope_language_and_caps() {
        let mut ts_app = make_file(
            "src/app.ts",
            b"needle one\nneedle two\nneedle three\n",
            vec![],
        );
        ts_app.1.language = LanguageId::TypeScript;
        let mut ts_lib = make_file("src/lib.ts", b"needle four\nneedle five\n", vec![]);
        ts_lib.1.language = LanguageId::TypeScript;
        let noise = make_file(
            "tests/generated/noise.ts",
            b"needle hidden\nneedle hidden two\n",
            vec![],
        );
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
                group_by: None,
                follow_refs: None,
                follow_refs_limit: None,
            }))
            .await;

        assert!(result.contains("src/app.ts"), "expected app.ts: {result}");
        assert!(result.contains("src/lib.ts"), "expected lib.ts: {result}");
        assert!(
            !result.contains("needle three"),
            "per-file cap should truncate app.ts: {result}"
        );
        assert!(
            !result.contains("needle five"),
            "total cap should truncate final result set: {result}"
        );
        assert!(
            !result.contains("tests/generated/noise.ts"),
            "noise file should be excluded: {result}"
        );
        assert!(
            !result.contains("src/lib.rs"),
            "language filter should exclude Rust: {result}"
        );
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
                group_by: None,
                follow_refs: None,
                follow_refs_limit: None,
            }))
            .await;

        assert!(
            result.contains("  2: line 2"),
            "context line missing: {result}"
        );
        assert!(
            result.contains("> 3: needle 3"),
            "match marker missing: {result}"
        );
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
                group_by: None,
                follow_refs: None,
                follow_refs_limit: None,
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
                group_by: None,
                follow_refs: None,
                follow_refs_limit: None,
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
                group_by: None,
                follow_refs: None,
                follow_refs_limit: None,
            }))
            .await;

        assert!(
            result.contains("  1: Needle"),
            "exact whole-word match missing: {result}"
        );
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
                group_by: None,
                follow_refs: None,
                follow_refs_limit: None,
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
                current_file: None,
                changed_with: None,
            }))
            .await;
        assert!(result.contains("2 matching files"), "got: {result}");
        assert!(
            result.contains("── Strong path matches ──"),
            "got: {result}"
        );
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
                query: "src/service.rs".to_string(),
                limit: None,
                current_file: None,
                changed_with: None,
            }))
            .await;
        assert_eq!(result, "No indexed source files matching 'src/service.rs'");
    }

    #[tokio::test]
    async fn test_search_files_changed_with_returns_graceful_message() {
        // Without git temporal data loaded, should return informative message
        let (key, file) = make_file("src/daemon.rs", b"fn foo() {}", vec![]);
        let server = make_server(make_live_index_ready(vec![(key, file)]));
        let result = server
            .search_files(Parameters(super::SearchFilesInput {
                query: String::new(),
                limit: None,
                current_file: None,
                changed_with: Some("src/daemon.rs".to_string()),
            }))
            .await;
        // Without git temporal data, should return informative message (not an error/panic)
        assert!(!result.contains("panic"), "should not panic, got: {result}");
        assert!(
            result.contains("temporal") || result.contains("git"),
            "should mention temporal data status, got: {result}"
        );
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
        assert!(
            result.contains("Ambiguous path hint 'lib.rs'"),
            "got: {result}"
        );
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
                chunk_index: None,
                max_lines: None,
                around_line: None,
                around_match: None,
                around_symbol: None,
                symbol_line: None,
                context_lines: None,
                show_line_numbers: None,
                header: None,
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
                chunk_index: None,
                max_lines: None,
                around_line: None,
                around_match: None,
                around_symbol: None,
                symbol_line: None,
                context_lines: None,
                show_line_numbers: None,
                header: None,
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
                chunk_index: None,
                max_lines: None,
                around_line: None,
                around_match: None,
                around_symbol: None,
                symbol_line: None,
                context_lines: None,
                show_line_numbers: None,
                header: None,
            }))
            .await;
        assert_eq!(result, "line 2\nline 3");
    }

    #[tokio::test]
    async fn test_get_file_content_show_line_numbers_renders_numbered_full_read() {
        let content = b"line 1\nline 2\nline 3";
        let (key, file) = make_file("src/lib.rs", content, vec![]);
        let server = make_server(make_live_index_ready(vec![(key, file)]));
        let result = server
            .get_file_content(Parameters(super::GetFileContentInput {
                path: "src/lib.rs".to_string(),
                start_line: None,
                end_line: None,
                chunk_index: None,
                max_lines: None,
                around_line: None,
                around_match: None,
                around_symbol: None,
                symbol_line: None,
                context_lines: None,
                show_line_numbers: Some(true),
                header: None,
            }))
            .await;
        assert_eq!(result, "1: line 1\n2: line 2\n3: line 3");
    }

    #[tokio::test]
    async fn test_get_file_content_header_and_line_numbers_render_range_shell() {
        let content = b"line 1\nline 2\nline 3\nline 4";
        let (key, file) = make_file("src/lib.rs", content, vec![]);
        let server = make_server(make_live_index_ready(vec![(key, file)]));
        let result = server
            .get_file_content(Parameters(super::GetFileContentInput {
                path: "src/lib.rs".to_string(),
                start_line: Some(2),
                end_line: Some(3),
                chunk_index: None,
                max_lines: None,
                around_line: None,
                around_match: None,
                around_symbol: None,
                symbol_line: None,
                context_lines: None,
                show_line_numbers: Some(true),
                header: Some(true),
            }))
            .await;
        assert_eq!(result, "src/lib.rs [lines 2-3]\n2: line 2\n3: line 3");
    }

    #[tokio::test]
    async fn test_get_file_content_around_line_renders_numbered_excerpt() {
        let content = b"line 1\nline 2\nline 3\nline 4\nline 5";
        let (key, file) = make_file("src/lib.rs", content, vec![]);
        let server = make_server(make_live_index_ready(vec![(key, file)]));
        let result = server
            .get_file_content(Parameters(super::GetFileContentInput {
                path: "src/lib.rs".to_string(),
                start_line: None,
                end_line: None,
                chunk_index: None,
                max_lines: None,
                around_line: Some(3),
                around_match: None,
                around_symbol: None,
                symbol_line: None,
                context_lines: Some(1),
                show_line_numbers: None,
                header: None,
            }))
            .await;
        assert_eq!(result, "2: line 2\n3: line 3\n4: line 4");
    }

    #[tokio::test]
    async fn test_get_file_content_rejects_header_with_contextual_read() {
        let content = b"line 1\nline 2\nline 3";
        let (key, file) = make_file("src/lib.rs", content, vec![]);
        let server = make_server(make_live_index_ready(vec![(key, file)]));
        let result = server
            .get_file_content(Parameters(super::GetFileContentInput {
                path: "src/lib.rs".to_string(),
                start_line: None,
                end_line: None,
                chunk_index: None,
                max_lines: None,
                around_line: Some(2),
                around_match: None,
                around_symbol: None,
                symbol_line: None,
                context_lines: Some(1),
                show_line_numbers: None,
                header: Some(true),
            }))
            .await;
        assert_eq!(
            result,
            "Invalid get_file_content request: `show_line_numbers` and `header` are only supported for full-file reads or explicit-range reads (`start_line`/`end_line`)."
        );
    }

    #[tokio::test]
    async fn test_get_file_content_rejects_around_line_with_explicit_range() {
        let content = b"line 1\nline 2\nline 3";
        let (key, file) = make_file("src/lib.rs", content, vec![]);
        let server = make_server(make_live_index_ready(vec![(key, file)]));
        let result = server
            .get_file_content(Parameters(super::GetFileContentInput {
                path: "src/lib.rs".to_string(),
                start_line: Some(2),
                end_line: None,
                chunk_index: None,
                max_lines: None,
                around_line: Some(2),
                around_match: None,
                around_symbol: None,
                symbol_line: None,
                context_lines: Some(1),
                show_line_numbers: None,
                header: None,
            }))
            .await;
        assert_eq!(
            result,
            "Invalid get_file_content request: `around_line` cannot be combined with `start_line` or `end_line`. Valid with `around_line`: `context_lines`."
        );
    }

    #[tokio::test]
    async fn test_get_file_content_around_match_renders_first_numbered_excerpt() {
        let content = b"line 1\nTODO first\nline 3\nTODO second\nline 5";
        let (key, file) = make_file("src/lib.rs", content, vec![]);
        let server = make_server(make_live_index_ready(vec![(key, file)]));
        let result = server
            .get_file_content(Parameters(super::GetFileContentInput {
                path: "src/lib.rs".to_string(),
                start_line: None,
                end_line: None,
                chunk_index: None,
                max_lines: None,
                around_line: None,
                around_match: Some("todo".to_string()),
                around_symbol: None,
                symbol_line: None,
                context_lines: Some(1),
                show_line_numbers: None,
                header: None,
            }))
            .await;
        assert_eq!(result, "1: line 1\n2: TODO first\n3: line 3");
    }

    #[tokio::test]
    async fn test_get_file_content_chunked_read_renders_header_and_numbered_lines() {
        let content = b"line 1\nline 2\nline 3\nline 4\nline 5";
        let (key, file) = make_file("src/lib.rs", content, vec![]);
        let server = make_server(make_live_index_ready(vec![(key, file)]));
        let result = server
            .get_file_content(Parameters(super::GetFileContentInput {
                path: "src/lib.rs".to_string(),
                start_line: None,
                end_line: None,
                chunk_index: Some(2),
                max_lines: Some(2),
                around_line: None,
                around_match: None,
                around_symbol: None,
                symbol_line: None,
                context_lines: None,
                show_line_numbers: None,
                header: None,
            }))
            .await;
        assert_eq!(
            result,
            "src/lib.rs [chunk 2/3, lines 3-4]\n3: line 3\n4: line 4"
        );
    }

    #[tokio::test]
    async fn test_get_file_content_reports_out_of_range_chunk() {
        let content = b"line 1\nline 2\nline 3";
        let (key, file) = make_file("src/lib.rs", content, vec![]);
        let server = make_server(make_live_index_ready(vec![(key, file)]));
        let result = server
            .get_file_content(Parameters(super::GetFileContentInput {
                path: "src/lib.rs".to_string(),
                start_line: None,
                end_line: None,
                chunk_index: Some(3),
                max_lines: Some(2),
                around_line: None,
                around_match: None,
                around_symbol: None,
                symbol_line: None,
                context_lines: None,
                show_line_numbers: None,
                header: None,
            }))
            .await;
        assert_eq!(result, "Chunk 3 out of range for src/lib.rs (2 chunks)");
    }

    #[tokio::test]
    async fn test_get_file_content_reports_missing_around_match() {
        let content = b"line 1\nline 2\nline 3";
        let (key, file) = make_file("src/lib.rs", content, vec![]);
        let server = make_server(make_live_index_ready(vec![(key, file)]));
        let result = server
            .get_file_content(Parameters(super::GetFileContentInput {
                path: "src/lib.rs".to_string(),
                start_line: None,
                end_line: None,
                chunk_index: None,
                max_lines: None,
                around_line: None,
                around_match: Some("needle".to_string()),
                around_symbol: None,
                symbol_line: None,
                context_lines: Some(1),
                show_line_numbers: None,
                header: None,
            }))
            .await;
        assert_eq!(result, "No matches for 'needle' in src/lib.rs");
    }

    #[tokio::test]
    async fn test_get_file_content_around_symbol_renders_numbered_excerpt() {
        let content = b"line 1\nfn connect() {}\nline 3";
        let (key, file) = make_file(
            "src/lib.rs",
            content,
            vec![make_symbol("connect", SymbolKind::Function, 1, 1)],
        );
        let server = make_server(make_live_index_ready(vec![(key, file)]));
        let result = server
            .get_file_content(Parameters(super::GetFileContentInput {
                path: "src/lib.rs".to_string(),
                start_line: None,
                end_line: None,
                chunk_index: None,
                max_lines: None,
                around_line: None,
                around_match: None,
                around_symbol: Some("connect".to_string()),
                symbol_line: None,
                context_lines: Some(1),
                show_line_numbers: None,
                header: None,
            }))
            .await;
        assert_eq!(result, "1: line 1\n2: fn connect() {}\n3: line 3");
    }

    #[tokio::test]
    async fn test_get_file_content_reports_ambiguous_around_symbol_without_symbol_line() {
        let content = b"fn connect() {}\nline 2\nfn connect() {}";
        let (key, file) = make_file(
            "src/lib.rs",
            content,
            vec![
                make_symbol("connect", SymbolKind::Function, 0, 0),
                make_symbol("connect", SymbolKind::Function, 2, 2),
            ],
        );
        let server = make_server(make_live_index_ready(vec![(key, file)]));
        let result = server
            .get_file_content(Parameters(super::GetFileContentInput {
                path: "src/lib.rs".to_string(),
                start_line: None,
                end_line: None,
                chunk_index: None,
                max_lines: None,
                around_line: None,
                around_match: None,
                around_symbol: Some("connect".to_string()),
                symbol_line: None,
                context_lines: Some(1),
                show_line_numbers: None,
                header: None,
            }))
            .await;
        assert_eq!(
            result,
            "Ambiguous symbol selector for connect in src/lib.rs; pass `symbol_line` to disambiguate. Candidates: 0, 2"
        );
    }

    #[tokio::test]
    async fn test_get_file_content_around_symbol_symbol_line_disambiguates() {
        let content = b"fn connect() {}\nline 2\nfn connect() {}";
        let (key, file) = make_file(
            "src/lib.rs",
            content,
            vec![
                make_symbol("connect", SymbolKind::Function, 0, 0),
                make_symbol("connect", SymbolKind::Function, 2, 2),
            ],
        );
        let server = make_server(make_live_index_ready(vec![(key, file)]));
        let result = server
            .get_file_content(Parameters(super::GetFileContentInput {
                path: "src/lib.rs".to_string(),
                start_line: None,
                end_line: None,
                chunk_index: None,
                max_lines: None,
                around_line: None,
                around_match: None,
                around_symbol: Some("connect".to_string()),
                symbol_line: Some(2),
                context_lines: Some(0),
                show_line_numbers: None,
                header: None,
            }))
            .await;
        assert_eq!(result, "3: fn connect() {}");
    }

    #[tokio::test]
    async fn test_get_file_content_reports_missing_around_symbol() {
        let content = b"fn helper() {}\nline 2";
        let (key, file) = make_file(
            "src/lib.rs",
            content,
            vec![make_symbol("helper", SymbolKind::Function, 1, 1)],
        );
        let server = make_server(make_live_index_ready(vec![(key, file)]));
        let result = server
            .get_file_content(Parameters(super::GetFileContentInput {
                path: "src/lib.rs".to_string(),
                start_line: None,
                end_line: None,
                chunk_index: None,
                max_lines: None,
                around_line: None,
                around_match: None,
                around_symbol: Some("connect".to_string()),
                symbol_line: None,
                context_lines: Some(1),
                show_line_numbers: None,
                header: None,
            }))
            .await;
        assert_eq!(
            result,
            "No symbol connect in src/lib.rs. Close matches: helper. Use get_file_outline for the full list (1 symbols)."
        );
    }

    #[tokio::test]
    async fn test_get_file_content_rejects_chunked_read_with_other_selectors() {
        let content = b"line 1\nline 2\nline 3";
        let (key, file) = make_file("src/lib.rs", content, vec![]);
        let server = make_server(make_live_index_ready(vec![(key, file)]));
        let result = server
            .get_file_content(Parameters(super::GetFileContentInput {
                path: "src/lib.rs".to_string(),
                start_line: Some(2),
                end_line: None,
                chunk_index: Some(1),
                max_lines: Some(2),
                around_line: None,
                around_match: None,
                around_symbol: None,
                symbol_line: None,
                context_lines: None,
                show_line_numbers: None,
                header: None,
            }))
            .await;
        assert_eq!(
            result,
            "Invalid get_file_content request: chunked reads (`chunk_index` + `max_lines`) cannot be combined with `start_line`, `end_line`, `around_line`, or `around_match`."
        );
    }

    #[tokio::test]
    async fn test_get_file_content_rejects_chunk_index_without_max_lines() {
        let content = b"line 1\nline 2\nline 3";
        let (key, file) = make_file("src/lib.rs", content, vec![]);
        let server = make_server(make_live_index_ready(vec![(key, file)]));
        let result = server
            .get_file_content(Parameters(super::GetFileContentInput {
                path: "src/lib.rs".to_string(),
                start_line: None,
                end_line: None,
                chunk_index: Some(1),
                max_lines: None,
                around_line: None,
                around_match: None,
                around_symbol: None,
                symbol_line: None,
                context_lines: None,
                show_line_numbers: None,
                header: None,
            }))
            .await;
        assert_eq!(
            result,
            "Invalid get_file_content request: `chunk_index` requires `max_lines`."
        );
    }

    #[tokio::test]
    async fn test_get_file_content_rejects_around_symbol_with_other_selectors() {
        let content = b"line 1\nline 2\nline 3";
        let (key, file) = make_file("src/lib.rs", content, vec![]);
        let server = make_server(make_live_index_ready(vec![(key, file)]));
        let result = server
            .get_file_content(Parameters(super::GetFileContentInput {
                path: "src/lib.rs".to_string(),
                start_line: Some(2),
                end_line: None,
                chunk_index: Some(1),
                max_lines: Some(2),
                around_line: None,
                around_match: None,
                around_symbol: Some("connect".to_string()),
                symbol_line: None,
                context_lines: Some(1),
                show_line_numbers: None,
                header: None,
            }))
            .await;
        assert_eq!(
            result,
            "Invalid get_file_content request: `around_symbol` cannot be combined with `start_line`, `end_line`, `around_line`, `around_match`, `chunk_index`, or `max_lines`. Valid with `around_symbol`: `symbol_line`, `context_lines`."
        );
    }

    #[tokio::test]
    async fn test_get_file_content_rejects_symbol_line_without_around_symbol() {
        let content = b"line 1\nline 2\nline 3";
        let (key, file) = make_file("src/lib.rs", content, vec![]);
        let server = make_server(make_live_index_ready(vec![(key, file)]));
        let result = server
            .get_file_content(Parameters(super::GetFileContentInput {
                path: "src/lib.rs".to_string(),
                start_line: None,
                end_line: None,
                chunk_index: None,
                max_lines: None,
                around_line: None,
                around_match: None,
                around_symbol: None,
                symbol_line: Some(2),
                context_lines: Some(1),
                show_line_numbers: None,
                header: None,
            }))
            .await;
        assert_eq!(
            result,
            "Invalid get_file_content request: `symbol_line` requires `around_symbol`."
        );
    }

    #[tokio::test]
    async fn test_get_file_content_rejects_around_match_with_other_selectors() {
        let content = b"line 1\nline 2\nline 3";
        let (key, file) = make_file("src/lib.rs", content, vec![]);
        let server = make_server(make_live_index_ready(vec![(key, file)]));
        let result = server
            .get_file_content(Parameters(super::GetFileContentInput {
                path: "src/lib.rs".to_string(),
                start_line: None,
                end_line: Some(3),
                chunk_index: None,
                max_lines: None,
                around_line: Some(2),
                around_match: Some("line".to_string()),
                around_symbol: None,
                symbol_line: None,
                context_lines: Some(1),
                show_line_numbers: None,
                header: None,
            }))
            .await;
        assert_eq!(
            result,
            "Invalid get_file_content request: `around_match` cannot be combined with `start_line`, `end_line`, or `around_line`. Valid with `around_match`: `context_lines`."
        );
    }

    // ── Explore tool tests ──────────────────────────────────────────────────

    #[tokio::test]
    async fn test_explore_concept_returns_results() {
        let sym = make_symbol("Error", SymbolKind::Enum, 0, 5);
        let content = b"pub enum Error {\n    NotFound,\n    Io(std::io::Error),\n}\nimpl Error {\n    fn is_retryable(&self) -> bool { false }\n}\n";
        let (key, file) = make_file("src/error.rs", content, vec![sym]);
        let server = make_server(make_live_index_ready(vec![(key, file)]));
        let result = server
            .explore(Parameters(super::ExploreInput {
                query: "error handling".to_string(),
                limit: Some(5),
            }))
            .await;
        assert!(
            result.contains("Exploring: Error Handling"),
            "should have concept label, got: {result}"
        );
        assert!(
            result.contains("Error"),
            "should find Error symbol, got: {result}"
        );
    }

    #[tokio::test]
    async fn test_explore_fallback_returns_results() {
        let content = b"fn process_data() { let x = 42; }\n";
        let sym = make_symbol("process_data", SymbolKind::Function, 0, 0);
        let (key, file) = make_file("src/main.rs", content, vec![sym]);
        let server = make_server(make_live_index_ready(vec![(key, file)]));
        let result = server
            .explore(Parameters(super::ExploreInput {
                query: "process data".to_string(),
                limit: Some(5),
            }))
            .await;
        assert!(
            result.contains("Exploring:"),
            "should have explore header, got: {result}"
        );
    }

    #[tokio::test]
    async fn test_explore_empty_query() {
        let server = make_server(make_live_index_ready(vec![]));
        let result = server
            .explore(Parameters(super::ExploreInput {
                query: "".to_string(),
                limit: None,
            }))
            .await;
        assert!(
            result.contains("Explore requires a non-empty query"),
            "should reject empty query, got: {result}"
        );
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
    fn test_tools_registered_count_is_stable() {
        let server = make_server(make_live_index_ready(vec![]));
        let tool_count = server.tool_router.list_all().len();
        // Sanity check: we should have a reasonable number of tools.
        // Update this lower bound when removing tools; it prevents accidental regressions.
        assert!(
            tool_count >= 24,
            "server should expose at least 24 tools; found {tool_count}"
        );
    }

    #[tokio::test]
    async fn test_trace_symbol_delegates_to_formatter() {
        let target = make_file(
            "src/lib.rs",
            b"fn process() {}\n",
            vec![make_symbol("process", SymbolKind::Function, 1, 1)],
        );
        let server = make_server(make_live_index_ready(vec![target]));

        let result = server
            .trace_symbol(Parameters(super::TraceSymbolInput {
                path: "src/lib.rs".to_string(),
                name: "process".to_string(),
                kind: None,
                symbol_line: None,
                sections: None,
                verbosity: None,
            }))
            .await;

        assert!(result.contains("fn process"), "got: {result}");
        assert!(result.contains("Callers (0)"), "got: {result}");
    }

    #[tokio::test]
    async fn test_inspect_match_delegates_to_formatter() {
        let target = make_file(
            "src/lib.rs",
            b"fn process() {\n    let x = 1;\n}\n",
            vec![make_symbol("process", SymbolKind::Function, 1, 3)],
        );
        let server = make_server(make_live_index_ready(vec![target]));

        let result = server
            .inspect_match(Parameters(super::InspectMatchInput {
                path: "src/lib.rs".to_string(),
                line: 2,
                context: None,
            }))
            .await;

        // Verify excerpt
        assert!(result.contains("2:     let x = 1;"), "got: {result}");
        // Verify enclosing symbol
        assert!(
            result.contains("Enclosing symbol: fn process (lines 1-3)"),
            "got: {result}"
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
                limit: None,
                max_per_file: None,
                compact: None,
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
                limit: None,
                max_per_file: None,
                format: None,
                compact: None,
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
                verbosity: None,
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
                verbosity: None,
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
                verbosity: None,
            }))
            .await;

        assert!(
            result.contains("src/service.rs"),
            "expected dependent hit: {result}"
        );
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
                verbosity: None,
            }))
            .await;

        assert!(
            result.contains("Ambiguous symbol selector"),
            "got: {result}"
        );
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
                limit: None,
                max_per_file: None,
                compact: None,
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
                limit: None,
                max_per_file: None,
                compact: None,
            }))
            .await;

        assert!(
            result.contains("src/service.rs"),
            "expected dependent hit: {result}"
        );
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
                limit: None,
                max_per_file: None,
                compact: None,
            }))
            .await;

        assert!(
            result.contains("Ambiguous symbol selector"),
            "got: {result}"
        );
        assert!(result.contains("1"), "got: {result}");
        assert!(result.contains("10"), "got: {result}");
    }

    #[tokio::test]
    async fn test_find_dependents_delegates_to_formatter() {
        let server = make_server(make_live_index_ready(vec![]));
        let result = server
            .find_dependents(Parameters(super::FindDependentsInput {
                path: "src/nonexistent.rs".to_string(),
                limit: None,
                max_per_file: None,
                format: None,
                compact: None,
            }))
            .await;
        assert!(result.contains("No dependents found"), "got: {result}");
    }

    #[tokio::test]
    async fn test_search_symbols_rejects_empty_query() {
        let sym = make_symbol("Foo", SymbolKind::Class, 1, 3);
        let (key, file) = make_file("src/lib.rs", b"struct Foo {}", vec![sym]);
        let server = make_server(make_live_index_ready(vec![(key, file)]));

        for query in ["", "   ", "\t"] {
            let result = server
                .search_symbols(Parameters(super::SearchSymbolsInput {
                    query: query.to_string(),
                    kind: None,
                    path_prefix: None,
                    language: None,
                    limit: None,
                    include_generated: None,
                    include_tests: None,
                }))
                .await;
            assert!(
                result.contains("non-empty query"),
                "empty query '{query}' should be rejected, got: {result}"
            );
        }
    }

    #[tokio::test]
    async fn test_inspect_match_out_of_bounds_line() {
        let sym = make_symbol("foo", SymbolKind::Function, 0, 0);
        let (key, file) = make_file("src/lib.rs", b"fn foo() {}\n", vec![sym]);
        let server = make_server(make_live_index_ready(vec![(key, file)]));

        let result = server
            .inspect_match(Parameters(super::InspectMatchInput {
                path: "src/lib.rs".to_string(),
                line: 999999,
                context: None,
            }))
            .await;
        assert!(
            result.contains("out of bounds"),
            "should report out of bounds, got: {result}"
        );
    }

    #[tokio::test]
    async fn test_search_text_shows_enclosing_symbol() {
        let sym = make_symbol("handle_request", SymbolKind::Function, 0, 2);
        let content = b"fn handle_request() {\n    let db = connect();\n}\n";
        let (key, file) = make_file("src/handler.rs", content, vec![sym]);
        let server = make_server(make_live_index_ready(vec![(key, file)]));
        let result = server
            .search_text(Parameters(super::SearchTextInput {
                query: Some("connect".to_string()),
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
                group_by: None,
                follow_refs: None,
                follow_refs_limit: None,
            }))
            .await;
        assert!(
            result.contains("handle_request"),
            "should show enclosing symbol name, got: {result}"
        );
        assert!(
            result.contains("in fn handle_request"),
            "should show kind and name, got: {result}"
        );
    }

    #[tokio::test]
    async fn test_search_text_group_by_symbol_deduplicates() {
        let sym = make_symbol("connect", SymbolKind::Function, 0, 4);
        let content = b"fn connect() {\n    let url = db_url();\n    let pool = Pool::new(url);\n    pool.connect()\n}\n";
        let (key, file) = make_file("src/db.rs", content, vec![sym]);
        let server = make_server(make_live_index_ready(vec![(key, file)]));
        let result = server
            .search_text(Parameters(super::SearchTextInput {
                query: Some("pool".to_string()),
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
                group_by: Some("symbol".to_string()),
                follow_refs: None,
                follow_refs_limit: None,
            }))
            .await;
        // With group_by: "symbol", should show symbol name and match count
        assert!(
            result.contains("connect"),
            "should show symbol name: {result}"
        );
        assert!(
            result.contains("2 matches") || result.contains("match"),
            "should show match count: {result}"
        );
    }

    #[tokio::test]
    async fn test_search_text_group_by_usage_filters_imports() {
        let content = b"use crate::db::connect;\nfn handler() { connect() }\n";
        let sym = make_symbol("handler", SymbolKind::Function, 1, 1);
        let (key, file) = make_file("src/api.rs", content, vec![sym]);
        let server = make_server(make_live_index_ready(vec![(key, file)]));
        let result = server
            .search_text(Parameters(super::SearchTextInput {
                query: Some("connect".to_string()),
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
                group_by: Some("usage".to_string()),
                follow_refs: None,
                follow_refs_limit: None,
            }))
            .await;
        // Should exclude the "use" import line
        assert!(
            !result.contains("use crate"),
            "should filter out imports: {result}"
        );
        assert!(
            result.contains("handler"),
            "should keep usage matches: {result}"
        );
    }

    #[tokio::test]
    async fn test_inspect_match_line_zero_is_out_of_bounds() {
        let sym = make_symbol("foo", SymbolKind::Function, 0, 0);
        let (key, file) = make_file("src/lib.rs", b"fn foo() {}\n", vec![sym]);
        let server = make_server(make_live_index_ready(vec![(key, file)]));

        let result = server
            .inspect_match(Parameters(super::InspectMatchInput {
                path: "src/lib.rs".to_string(),
                line: 0,
                context: None,
            }))
            .await;
        assert!(
            result.contains("out of bounds"),
            "line 0 should be out of bounds (1-based), got: {result}"
        );
    }

    #[tokio::test]
    async fn test_search_text_follow_refs_includes_callers() {
        // Build an index with cross-references
        let sym_a = make_symbol("connect", SymbolKind::Function, 0, 1);
        let file_a_content = b"fn connect() {\n    db_open()\n}\n";
        let (key_a, file_a) = make_file("src/db.rs", file_a_content, vec![sym_a]);

        let sym_b = make_symbol("handler", SymbolKind::Function, 0, 1);
        let file_b_content = b"fn handler() {\n    connect()\n}\n";
        let (key_b, file_b) = make_file_with_refs(
            "src/api.rs",
            file_b_content,
            vec![sym_b],
            vec![make_ref("connect", None, ReferenceKind::Call, 1, Some(0))],
        );

        let server = make_server(make_live_index_ready(vec![
            (key_a, file_a),
            (key_b, file_b),
        ]));
        let result = server
            .search_text(Parameters(super::SearchTextInput {
                query: Some("db_open".to_string()),
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
                group_by: None,
                follow_refs: Some(true),
                follow_refs_limit: None,
            }))
            .await;
        // Should show that connect() is called by handler() in src/api.rs
        assert!(
            result.contains("handler") || result.contains("api.rs"),
            "should show callers of enclosing symbol, got: {result}"
        );
        assert!(
            result.contains("Called by"),
            "should have Called by section, got: {result}"
        );
    }

    // ── Lenient deserialization tests ────────────────────────────────────

    #[test]
    fn test_lenient_u32_accepts_string() {
        let json = r#"{"query":"test","limit":"10"}"#;
        let input: super::SearchFilesInput = serde_json::from_str(json).unwrap();
        assert_eq!(input.limit, Some(10));
    }

    #[test]
    fn test_lenient_u32_accepts_number() {
        let json = r#"{"query":"test","limit":10}"#;
        let input: super::SearchFilesInput = serde_json::from_str(json).unwrap();
        assert_eq!(input.limit, Some(10));
    }

    #[test]
    fn test_lenient_u32_accepts_null() {
        let json = r#"{"query":"test","limit":null}"#;
        let input: super::SearchFilesInput = serde_json::from_str(json).unwrap();
        assert_eq!(input.limit, None);
    }

    #[test]
    fn test_lenient_u32_accepts_absent() {
        let json = r#"{"query":"test"}"#;
        let input: super::SearchFilesInput = serde_json::from_str(json).unwrap();
        assert_eq!(input.limit, None);
    }

    #[test]
    fn test_lenient_bool_accepts_string_true() {
        let json = r#"{"uncommitted":"true"}"#;
        let input: super::WhatChangedInput = serde_json::from_str(json).unwrap();
        assert_eq!(input.uncommitted, Some(true));
    }

    #[test]
    fn test_lenient_bool_accepts_string_false() {
        let json = r#"{"uncommitted":"false"}"#;
        let input: super::WhatChangedInput = serde_json::from_str(json).unwrap();
        assert_eq!(input.uncommitted, Some(false));
    }

    #[test]
    fn test_lenient_bool_accepts_native_bool() {
        let json = r#"{"uncommitted":true}"#;
        let input: super::WhatChangedInput = serde_json::from_str(json).unwrap();
        assert_eq!(input.uncommitted, Some(true));
    }

    #[test]
    fn test_lenient_u32_required_accepts_string() {
        let json = r#"{"path":"src/lib.rs","line":"42"}"#;
        let input: super::InspectMatchInput = serde_json::from_str(json).unwrap();
        assert_eq!(input.line, 42);
    }

    #[test]
    fn test_lenient_u32_required_accepts_number() {
        let json = r#"{"path":"src/lib.rs","line":42}"#;
        let input: super::InspectMatchInput = serde_json::from_str(json).unwrap();
        assert_eq!(input.line, 42);
    }

    #[test]
    fn test_lenient_depth_accepts_string() {
        let json = r#"{"depth":"1"}"#;
        let input: super::GetFileTreeInput = serde_json::from_str(json).unwrap();
        assert_eq!(input.depth, Some(1));
    }

    #[test]
    fn test_resolve_path_accepts_query_alias() {
        let json = r#"{"query":"daemon"}"#;
        let input: super::ResolvePathInput = serde_json::from_str(json).unwrap();
        assert_eq!(input.hint, "daemon");
    }

    #[test]
    fn test_resolve_path_accepts_hint() {
        let json = r#"{"hint":"daemon"}"#;
        let input: super::ResolvePathInput = serde_json::from_str(json).unwrap();
        assert_eq!(input.hint, "daemon");
    }

    #[test]
    fn test_get_co_changes_input_deserializes() {
        let json = r#"{"path":"src/lib.rs","limit":5}"#;
        let input: super::GetCoChangesInput = serde_json::from_str(json).unwrap();
        assert_eq!(input.path, "src/lib.rs");
        assert_eq!(input.limit, Some(5));
    }

    #[test]
    fn test_get_co_changes_input_limit_as_string() {
        let json = r#"{"path":"src/lib.rs","limit":"10"}"#;
        let input: super::GetCoChangesInput = serde_json::from_str(json).unwrap();
        assert_eq!(input.limit, Some(10));
    }

    #[tokio::test]
    async fn test_get_co_changes_returns_loading_message_when_no_git_data() {
        // With an empty index (no git temporal data computed), the tool
        // should return the "still loading" or "unavailable" message.
        let server = make_server(make_live_index_empty());
        let result = server
            .get_co_changes(Parameters(super::GetCoChangesInput {
                path: "src/lib.rs".to_string(),
                limit: None,
            }))
            .await;
        // Git temporal starts as Pending in tests (no tokio runtime spawns it)
        assert!(
            result.contains("still loading") || result.contains("unavailable"),
            "expected loading/unavailable message, got: {result}"
        );
    }
}
