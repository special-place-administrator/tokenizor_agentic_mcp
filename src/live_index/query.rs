use std::collections::{HashSet, VecDeque};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use crate::domain::{LanguageId, ReferenceKind, ReferenceRecord, SymbolKind, SymbolRecord};
use crate::watcher::{WatcherInfo, WatcherState};

use super::search::PathScope;
use super::store::{IndexState, IndexedFile, LiveIndex, ParseStatus};

// ---------------------------------------------------------------------------
// Module path resolution for find_dependents
// ---------------------------------------------------------------------------

/// Resolve the logical module path for a file based on language conventions.
///
/// Returns `None` if the file doesn't follow a recognized module convention.
///
/// Examples (Rust):
///   "src/lib.rs"              → Some("crate")
///   "src/main.rs"             → Some("crate")
///   "src/error.rs"            → Some("crate::error")
///   "src/live_index/mod.rs"   → Some("crate::live_index")
///   "src/live_index/store.rs" → Some("crate::live_index::store")
///
/// Examples (Python):
///   "src/__init__.py"         → Some("src")
///   "src/foo.py"              → Some("src.foo")
///   "src/foo/__init__.py"     → Some("src.foo")
///
/// Examples (JS/TS):
///   "src/index.js"            → Some("src")
///   "src/utils/index.ts"      → Some("src/utils")
fn resolve_module_path(file_path: &str, language: &LanguageId) -> Option<String> {
    let path = std::path::Path::new(file_path);

    match language {
        LanguageId::Rust => {
            // Strip up to and including "src/" — handles both root projects ("src/lib.rs")
            // and workspace crates ("crates/aap-core/src/types.rs").
            let after_src: String = if let Ok(stripped) = path.strip_prefix("src") {
                stripped.to_string_lossy().into_owned()
            } else {
                // Workspace layout: find "/src/" component anywhere in path
                let normalized = file_path.replace('\\', "/");
                let src_idx = normalized.find("/src/")?;
                normalized[src_idx + 5..].to_string() // skip "/src/"
            };
            let stripped = std::path::Path::new(&after_src);
            let mut components: Vec<String> = stripped
                .components()
                .filter_map(|c| c.as_os_str().to_str().map(String::from))
                .collect();

            // Remove extension from last component
            if let Some(last) = components.last_mut()
                && let Some(stem) = std::path::Path::new(last.as_str())
                    .file_stem()
                    .and_then(|s| s.to_str())
            {
                *last = stem.to_string();
            }

            // Drop "lib", "main", "mod" — these map to their parent module
            if matches!(
                components.last().map(|s| s.as_str()),
                Some("lib" | "main" | "mod")
            ) {
                components.pop();
            }

            if components.is_empty() {
                Some("crate".to_string())
            } else {
                Some(format!("crate::{}", components.join("::")))
            }
        }
        LanguageId::Python => {
            let mut components: Vec<String> = path
                .components()
                .filter_map(|c| c.as_os_str().to_str().map(String::from))
                .collect();

            // Remove extension from last component
            if let Some(last) = components.last_mut()
                && let Some(stem) = std::path::Path::new(last.as_str())
                    .file_stem()
                    .and_then(|s| s.to_str())
            {
                *last = stem.to_string();
            }

            // Drop __init__ — maps to the package (parent directory)
            if matches!(components.last().map(|s| s.as_str()), Some("__init__")) {
                components.pop();
            }

            if components.is_empty() {
                None
            } else {
                Some(components.join("."))
            }
        }
        LanguageId::JavaScript | LanguageId::TypeScript => {
            let mut components: Vec<String> = path
                .components()
                .filter_map(|c| c.as_os_str().to_str().map(String::from))
                .collect();

            // Remove extension from last component
            if let Some(last) = components.last_mut()
                && let Some(stem) = std::path::Path::new(last.as_str())
                    .file_stem()
                    .and_then(|s| s.to_str())
            {
                *last = stem.to_string();
            }

            // Drop "index" — maps to the directory
            if matches!(components.last().map(|s| s.as_str()), Some("index")) {
                components.pop();
            }

            if components.is_empty() {
                None
            } else {
                Some(components.join("/"))
            }
        }
        _ => None,
    }
}

/// Check if an import reference in a file is a `pub use` (re-export).
///
/// Looks at the source content just before the import's byte range to find a `pub` keyword.
fn is_pub_use_import(file: &IndexedFile, reference: &ReferenceRecord) -> bool {
    if reference.kind != ReferenceKind::Import {
        return false;
    }
    // Look back from the import start to find `pub use` or `pub(crate) use`, etc.
    let start = reference.byte_range.0 as usize;
    // Grab up to 30 bytes before the reference start (enough for `pub(crate) use `)
    let lookback_start = start.saturating_sub(30);
    if lookback_start >= file.content.len() || start > file.content.len() {
        return false;
    }
    let prefix = &file.content[lookback_start..start];
    let prefix_str = std::str::from_utf8(prefix).unwrap_or("");
    // Check if the line containing this import starts with `pub`
    let line_start = prefix_str.rfind('\n').map(|i| i + 1).unwrap_or(0);
    let line_prefix = prefix_str[line_start..].trim_start();
    line_prefix.starts_with("pub ") || line_prefix.starts_with("pub(") || line_prefix == "pub"
}

/// Find files that re-export symbols from `target_module_path` via `pub use`.
///
/// Returns the file paths of re-exporter files.
fn find_reexporters<'a>(
    files: &'a std::collections::HashMap<String, Arc<IndexedFile>>,
    target_path: &str,
    target_module_path: Option<&str>,
    target_language: &LanguageId,
    target_stem: &str,
) -> Vec<&'a str> {
    if *target_language != LanguageId::Rust {
        return vec![];
    }

    let mut reexporters = Vec::new();
    for (file_path, file) in files {
        if file_path.as_str() == target_path {
            continue;
        }
        if file.language != *target_language {
            continue;
        }
        for reference in &file.references {
            if matches_target_import(target_language, reference, target_stem, target_module_path)
                && is_pub_use_import(file, reference)
            {
                reexporters.push(file_path.as_str());
                break;
            }
        }
    }
    reexporters
}

fn declared_scope(file: &IndexedFile) -> Option<String> {
    let content = String::from_utf8_lossy(&file.content);
    match file.language {
        LanguageId::CSharp => parse_declared_scope(&content, "namespace"),
        LanguageId::Java => parse_declared_scope(&content, "package"),
        _ => None,
    }
}

fn parse_declared_scope(content: &str, keyword: &str) -> Option<String> {
    content.lines().find_map(|line| {
        let trimmed = line.split("//").next().unwrap_or("").trim();
        let rest = trimmed.strip_prefix(keyword)?.trim_start();
        let scope: String = rest
            .chars()
            .take_while(|ch| ch.is_ascii_alphanumeric() || *ch == '_' || *ch == '.')
            .collect();
        if scope.is_empty() { None } else { Some(scope) }
    })
}

fn imported_scope(language: &LanguageId, reference: &ReferenceRecord) -> Option<String> {
    if reference.kind != ReferenceKind::Import {
        return None;
    }

    let qualified_name = reference.qualified_name.as_deref()?;
    match language {
        LanguageId::CSharp => Some(qualified_name.to_string()),
        LanguageId::Java => {
            let trimmed = qualified_name.trim_end_matches(".*");
            match trimmed.rsplit_once('.') {
                Some((scope, _)) if !scope.is_empty() => Some(scope.to_string()),
                _ if !trimmed.is_empty() => Some(trimmed.to_string()),
                _ => None,
            }
        }
        _ => None,
    }
}

fn can_match_type_dependents(
    dependent_file: &IndexedFile,
    target_language: &LanguageId,
    target_scope: Option<&str>,
) -> bool {
    if &dependent_file.language != target_language {
        return false;
    }

    match target_language {
        LanguageId::CSharp | LanguageId::Java => {
            let Some(target_scope) = target_scope else {
                return true;
            };

            if declared_scope(dependent_file).as_deref() == Some(target_scope) {
                return true;
            }

            dependent_file
                .references
                .iter()
                .filter_map(|reference| imported_scope(&dependent_file.language, reference))
                .any(|scope| scope == target_scope)
        }
        _ => false,
    }
}

fn matches_target_stem(text: &str, stem: &str) -> bool {
    text == stem
        || text.ends_with(&format!("/{stem}"))
        || text.ends_with(&format!("::{stem}"))
        || text.ends_with(&format!(".{stem}"))
        || text.contains(&format!("/{stem}/"))
        || text.contains(&format!("::{stem}::"))
        // Relative imports: `index::Thing` matches stem "index"
        || text.starts_with(&format!("{stem}::"))
        || text.starts_with(&format!("{stem}/"))
        || text.starts_with(&format!("{stem}."))
}

fn matches_target_module(language: &LanguageId, text: &str, module_path: Option<&str>) -> bool {
    let Some(module_path) = module_path else {
        return false;
    };

    let module_sep = match language {
        LanguageId::Python => ".",
        LanguageId::JavaScript | LanguageId::TypeScript => "/",
        _ => "::",
    };

    if text == module_path || text.starts_with(&format!("{module_path}{module_sep}")) {
        return true;
    }

    if *language == LanguageId::Rust
        && let Some(tail) = module_path.strip_prefix("crate::")
    {
        return text == tail
            || text.ends_with(&format!("::{tail}"))
            || text.contains(&format!("::{tail}::"));
    }

    false
}

fn matches_target_import(
    language: &LanguageId,
    reference: &ReferenceRecord,
    stem: &str,
    module_path: Option<&str>,
) -> bool {
    if reference.kind != ReferenceKind::Import {
        return false;
    }

    matches_target_stem(&reference.name, stem)
        || reference
            .qualified_name
            .as_deref()
            .map(|text| {
                matches_target_stem(text, stem)
                    || matches_target_module(language, text, module_path)
            })
            .unwrap_or(false)
        || matches_target_module(language, &reference.name, module_path)
}

fn parse_reference_kind_filter(kind_filter: Option<&str>) -> Option<ReferenceKind> {
    match kind_filter {
        Some("call") => Some(ReferenceKind::Call),
        Some("import") => Some(ReferenceKind::Import),
        Some("type_usage") => Some(ReferenceKind::TypeUsage),
        Some("macro_use") => Some(ReferenceKind::MacroUse),
        Some("all") | None => None,
        _ => None,
    }
}

fn matches_exact_symbol_qualified_name(
    language: &LanguageId,
    qualified_name: &str,
    target_name: &str,
    module_path: Option<&str>,
) -> bool {
    let separator = match language {
        LanguageId::Python => ".",
        LanguageId::JavaScript | LanguageId::TypeScript => "/",
        _ => "::",
    };

    module_path
        .map(|module_path| qualified_name == format!("{module_path}{separator}{target_name}"))
        .unwrap_or(false)
}

fn matches_exact_symbol_reference(
    reference: &ReferenceRecord,
    target_name: &str,
    target_language: &LanguageId,
    module_path: Option<&str>,
    kind_filter: Option<ReferenceKind>,
) -> bool {
    if let Some(kind_filter) = kind_filter
        && reference.kind != kind_filter
    {
        return false;
    }

    reference.name == target_name
        || reference
            .qualified_name
            .as_deref()
            .map(|qualified_name| {
                matches_exact_symbol_qualified_name(
                    target_language,
                    qualified_name,
                    target_name,
                    module_path,
                )
            })
            .unwrap_or(false)
}

pub(crate) enum SymbolSelectorMatch<'a> {
    Selected(usize, &'a SymbolRecord),
    NotFound,
    Ambiguous(Vec<u32>),
}

pub(crate) fn resolve_symbol_selector<'a>(
    file: &'a IndexedFile,
    name: &str,
    symbol_kind: Option<&str>,
    symbol_line: Option<u32>,
) -> SymbolSelectorMatch<'a> {
    let mut candidates: Vec<(usize, &SymbolRecord)> = file
        .symbols
        .iter()
        .enumerate()
        .filter(|(_, symbol)| {
            symbol.name == name
                && symbol_kind
                    .map(|kind| symbol.kind.to_string().eq_ignore_ascii_case(kind))
                    .unwrap_or(true)
        })
        .collect();

    if let Some(symbol_line) = symbol_line {
        candidates.retain(|(_, symbol)| symbol.line_range.0 == symbol_line);
    }

    match candidates.len() {
        0 => SymbolSelectorMatch::NotFound,
        1 => {
            let (idx, symbol) = candidates.remove(0);
            SymbolSelectorMatch::Selected(idx, symbol)
        }
        _ => SymbolSelectorMatch::Ambiguous(
            candidates
                .into_iter()
                .map(|(_, symbol)| symbol.line_range.0)
                .collect(),
        ),
    }
}

pub(crate) fn render_symbol_selector(
    name: &str,
    symbol_kind: Option<&str>,
    symbol_line: Option<u32>,
) -> String {
    match (symbol_kind, symbol_line) {
        (Some(symbol_kind), Some(symbol_line)) => {
            format!("{symbol_kind} {name} at line {symbol_line}")
        }
        (Some(symbol_kind), None) => format!("{symbol_kind} {name}"),
        (None, Some(symbol_line)) => format!("{name} at line {symbol_line}"),
        (None, None) => name.to_string(),
    }
}

// ---------------------------------------------------------------------------
// Built-in type filter lists (per-language)
// ---------------------------------------------------------------------------

const RUST_BUILTINS: &[&str] = &[
    "i8", "i16", "i32", "i64", "i128", "isize", "u8", "u16", "u32", "u64", "u128", "usize", "f32",
    "f64", "bool", "char", "str", "String", "Self", "self",
];

const PYTHON_BUILTINS: &[&str] = &[
    "int", "float", "str", "bool", "list", "dict", "tuple", "set", "None", "bytes", "object",
    "type",
];

const JS_BUILTINS: &[&str] = &[
    "string",
    "number",
    "boolean",
    "undefined",
    "null",
    "Object",
    "Array",
    "Function",
    "Symbol",
    "Promise",
    "Error",
];

const TS_BUILTINS: &[&str] = &[
    "string",
    "number",
    "boolean",
    "undefined",
    "null",
    "void",
    "never",
    "any",
    "unknown",
    "Object",
    "Array",
    "Function",
    "Symbol",
    "Promise",
    "Error",
    "Record",
    "Partial",
    "Required",
    "Readonly",
    "Pick",
    "Omit",
];

const GO_BUILTINS: &[&str] = &[
    "int",
    "int8",
    "int16",
    "int32",
    "int64",
    "uint",
    "uint8",
    "uint16",
    "uint32",
    "uint64",
    "float32",
    "float64",
    "complex64",
    "complex128",
    "bool",
    "string",
    "byte",
    "rune",
    "error",
    "any",
];

const JAVA_BUILTINS: &[&str] = &[
    "int",
    "long",
    "short",
    "byte",
    "float",
    "double",
    "boolean",
    "char",
    "void",
    "String",
    "Object",
    "Integer",
    "Long",
    "Short",
    "Byte",
    "Float",
    "Double",
    "Boolean",
    "Character",
];

/// Single-letter generic type parameter names that are almost always noise.
const SINGLE_LETTER_GENERICS: &[&str] = &[
    "T", "K", "V", "E", "R", "S", "A", "B", "C", "D", "N", "M", "P", "U", "W", "X", "Y", "Z",
];

/// Returns `true` when `name` is a known built-in primitive/stdlib type or a
/// single-letter generic parameter that would generate false-positive matches.
///
/// This is a coarse, language-agnostic filter applied at query time. It checks
/// all language lists so that cross-language repos are handled uniformly.
fn is_filtered_name(name: &str) -> bool {
    SINGLE_LETTER_GENERICS.contains(&name)
        || RUST_BUILTINS.contains(&name)
        || PYTHON_BUILTINS.contains(&name)
        || JS_BUILTINS.contains(&name)
        || TS_BUILTINS.contains(&name)
        || GO_BUILTINS.contains(&name)
        || JAVA_BUILTINS.contains(&name)
}

fn normalize_path_query(raw: &str) -> String {
    let mut normalized = raw.trim().replace('\\', "/");
    while normalized.starts_with("./") {
        normalized = normalized[2..].to_string();
    }
    normalized.trim_matches('/').to_string()
}

fn tokenize_path_query(normalized_query: &str) -> Vec<String> {
    normalized_query
        .split(|ch: char| ch == '/' || ch.is_whitespace())
        .filter(|part| !part.is_empty())
        .map(|part| part.to_ascii_lowercase())
        .collect()
}

fn path_has_component(path: &str, component: &str) -> bool {
    path.split('/')
        .any(|part| part.eq_ignore_ascii_case(component))
}

fn shared_directory_prefix_len(path_a: &str, path_b: &str) -> usize {
    let parts_a: Vec<&str> = path_a.split('/').collect();
    let parts_b: Vec<&str> = path_b.split('/').collect();

    // Skip the basename of the current file if it's a file path
    let dirs_a = if parts_a.len() > 1 {
        &parts_a[..parts_a.len() - 1]
    } else {
        &parts_a[..]
    };

    let mut shared = 0;
    for (a, b) in dirs_a.iter().zip(parts_b.iter()) {
        if a.eq_ignore_ascii_case(b) {
            shared += 1;
        } else {
            break;
        }
    }
    shared
}

/// Summary health statistics for the LiveIndex.
#[derive(Debug, Clone)]
pub struct HealthStats {
    pub file_count: usize,
    pub symbol_count: usize,
    pub parsed_count: usize,
    pub partial_parse_count: usize,
    pub failed_count: usize,
    pub load_duration: Duration,
    /// Current state of the file watcher.
    pub watcher_state: WatcherState,
    /// Total number of file-system events processed by the watcher.
    pub events_processed: u64,
    /// Wall-clock time of the most recent event processed, if any.
    pub last_event_at: Option<SystemTime>,
    /// Effective debounce window in milliseconds.
    pub debounce_window_ms: u64,
}

/// Owned entry used to render the repo outline after releasing the index lock.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoOutlineFileView {
    pub relative_path: String,
    pub language: LanguageId,
    pub symbol_count: usize,
}

/// Owned compatibility/test view for file outline rendering.
///
/// Hot-path readers should prefer `capture_shared_file()` and format from `&IndexedFile`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileOutlineView {
    pub relative_path: String,
    pub symbols: Vec<SymbolRecord>,
}

/// Owned compatibility/test view for symbol detail rendering.
///
/// Hot-path readers should prefer `capture_shared_file()` and format from `&IndexedFile`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SymbolDetailView {
    pub relative_path: String,
    pub content: Vec<u8>,
    pub symbols: Vec<SymbolRecord>,
}

/// Owned compatibility/test view for file content rendering.
///
/// Hot-path readers should prefer `capture_shared_file()` and format from `&IndexedFile`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileContentView {
    pub relative_path: String,
    pub content: Vec<u8>,
}

/// Owned timestamp/path view used by `what_changed` timestamp mode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WhatChangedTimestampView {
    pub loaded_secs: i64,
    pub paths: Vec<String>,
}

/// Owned path-resolution result for `resolve_path`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolvePathView {
    EmptyHint,
    Resolved {
        path: String,
    },
    NotFound {
        hint: String,
    },
    Ambiguous {
        hint: String,
        matches: Vec<String>,
        overflow_count: usize,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SearchFilesTier {
    CoChange,
    StrongPath,
    Basename,
    LoosePath,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SearchFilesHit {
    pub tier: SearchFilesTier,
    pub path: String,
    pub coupling_score: Option<f32>,
    pub shared_commits: Option<u32>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SearchFilesView {
    EmptyQuery,
    NotFound {
        query: String,
    },
    Found {
        query: String,
        total_matches: usize,
        overflow_count: usize,
        hits: Vec<SearchFilesHit>,
    },
}

/// One rendered dependent-reference line captured under the read lock.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DependentLineView {
    pub line_number: u32,
    pub line_content: String,
    pub kind: String,
}

/// One dependent file entry captured for `find_dependents`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DependentFileView {
    pub file_path: String,
    pub lines: Vec<DependentLineView>,
}

/// Owned grouped view for `find_dependents`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FindDependentsView {
    pub files: Vec<DependentFileView>,
}

/// One context line for a reference hit captured under the read lock.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReferenceContextLineView {
    pub line_number: u32,
    pub text: String,
    pub is_reference_line: bool,
    pub enclosing_annotation: Option<String>,
}

/// One reference hit with its surrounding context.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReferenceHitView {
    pub context_lines: Vec<ReferenceContextLineView>,
}

/// One file entry in a grouped references result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReferenceFileView {
    pub file_path: String,
    pub hits: Vec<ReferenceHitView>,
}

/// Owned grouped view for `find_references`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FindReferencesView {
    pub total_refs: usize,
    pub total_files: usize,
    pub files: Vec<ReferenceFileView>,
}

/// One entry in a `find_implementations` result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImplementationEntryView {
    /// The trait/interface name.
    pub trait_name: String,
    /// The implementing type name.
    pub implementor: String,
    /// File where the implements reference was found.
    pub file_path: String,
    /// Line of the implements reference.
    pub line: u32,
}

/// Owned grouped view for `find_implementations`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FindImplementationsView {
    pub entries: Vec<ImplementationEntryView>,
}

/// One compact reference entry rendered inside a context-bundle section.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextBundleReferenceView {
    pub display_name: String,
    pub file_path: String,
    pub line_number: u32,
    pub enclosing: Option<String>,
}

/// One owned section inside a captured context bundle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextBundleSectionView {
    pub total_count: usize,
    pub overflow_count: usize,
    pub entries: Vec<ContextBundleReferenceView>,
}

/// A resolved type definition included as a dependency of a context bundle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeDependencyView {
    /// Type name (e.g. "UserConfig").
    pub name: String,
    /// Kind label (e.g. "struct", "enum", "trait").
    pub kind_label: String,
    /// File where the type is defined.
    pub file_path: String,
    /// Line range of the definition.
    pub line_range: (u32, u32),
    /// Source code body of the definition.
    pub body: String,
    /// Recursion depth at which this dependency was discovered (0 = direct, 1 = transitive).
    pub depth: u8,
}

/// Owned definition-and-sections view for `get_context_bundle`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextBundleFoundView {
    pub file_path: String,
    pub body: String,
    pub kind_label: String,
    pub line_range: (u32, u32),
    pub byte_count: usize,
    pub callers: ContextBundleSectionView,
    pub callees: ContextBundleSectionView,
    pub type_usages: ContextBundleSectionView,
    /// Resolved type definitions used by this symbol (recursive, depth-limited).
    pub dependencies: Vec<TypeDependencyView>,
}

/// Owned result view for `get_context_bundle`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContextBundleView {
    FileNotFound {
        path: String,
    },
    AmbiguousSymbol {
        path: String,
        name: String,
        candidate_lines: Vec<u32>,
    },
    SymbolNotFound {
        relative_path: String,
        symbol_names: Vec<String>,
        name: String,
    },
    Found(ContextBundleFoundView),
}

/// A sibling symbol at the same depth within the same file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SiblingSymbolView {
    pub name: String,
    pub kind_label: String,
    pub line_range: (u32, u32),
}

/// Git activity snapshot for a single file (owned, display-ready).
#[derive(Debug, Clone, PartialEq)]
pub struct GitActivityView {
    pub churn_score: f32,
    pub churn_bar: String,
    pub churn_label: String,
    pub commit_count: u32,
    pub last_relative: String,
    pub last_hash: String,
    pub last_message: String,
    pub last_author: String,
    pub last_timestamp: String,
    pub owners: Vec<String>,
    pub co_changes: Vec<(String, f32, u32)>,
}

/// Full trace result for a single symbol.
#[derive(Debug, Clone, PartialEq)]
pub struct TraceSymbolFoundView {
    pub context_bundle: ContextBundleFoundView,
    pub dependents: FindDependentsView,
    pub siblings: Vec<SiblingSymbolView>,
    pub implementations: FindImplementationsView,
    pub git_activity: Option<GitActivityView>,
}

/// Owned result view for `trace_symbol`.
#[derive(Debug, Clone, PartialEq)]
pub enum TraceSymbolView {
    FileNotFound {
        path: String,
    },
    AmbiguousSymbol {
        path: String,
        name: String,
        candidate_lines: Vec<u32>,
    },
    SymbolNotFound {
        relative_path: String,
        symbol_names: Vec<String>,
        name: String,
    },
    Found(TraceSymbolFoundView),
}

/// A focused symbol summary for `inspect_match`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnclosingSymbolView {
    pub name: String,
    pub kind_label: String,
    pub line_range: (u32, u32),
}

/// Found result for `inspect_match`.
#[derive(Debug, Clone, PartialEq)]
pub struct InspectMatchFoundView {
    pub path: String,
    pub line: u32,
    pub excerpt: String,
    pub enclosing: Option<EnclosingSymbolView>,
    pub siblings: Vec<SiblingSymbolView>,
}

/// Owned result view for `inspect_match`.
#[derive(Debug, Clone, PartialEq)]
pub enum InspectMatchView {
    FileNotFound {
        path: String,
    },
    LineOutOfBounds {
        path: String,
        line: u32,
        total_lines: usize,
    },
    Found(InspectMatchFoundView),
}

/// Owned repo outline view captured under a short read lock.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoOutlineView {
    pub total_files: usize,
    pub total_symbols: usize,
    pub files: Vec<RepoOutlineFileView>,
}

impl LiveIndex {
    /// O(1) lookup of a file by its relative path.
    pub fn get_file(&self, relative_path: &str) -> Option<&IndexedFile> {
        self.files.get(relative_path).map(|file| file.as_ref())
    }

    /// Returns the symbol slice for a file, or an empty slice if not found.
    pub fn symbols_for_file(&self, relative_path: &str) -> &[SymbolRecord] {
        self.files
            .get(relative_path)
            .map(|file| file.symbols.as_slice())
            .unwrap_or(&[])
    }

    /// Iterate all (path, file) pairs in the index.
    pub fn all_files(&self) -> impl Iterator<Item = (&String, &IndexedFile)> {
        self.files.iter().map(|(path, file)| (path, file.as_ref()))
    }

    /// Capture a shared immutable file entry under the read lock.
    pub fn capture_shared_file(&self, relative_path: &str) -> Option<Arc<IndexedFile>> {
        self.files.get(relative_path).cloned()
    }

    /// Capture one shared immutable file entry selected by an internal path scope.
    pub fn capture_shared_file_for_scope(
        &self,
        path_scope: &PathScope,
    ) -> Option<Arc<IndexedFile>> {
        match path_scope {
            PathScope::Any => None,
            PathScope::Exact(path) => self.capture_shared_file(path),
            PathScope::Prefix(prefix) => {
                let mut matching_paths: Vec<&String> = self
                    .files
                    .keys()
                    .filter(|path| path.starts_with(prefix))
                    .collect();
                matching_paths.sort();
                if matching_paths.len() == 1 {
                    self.capture_shared_file(matching_paths[0])
                } else {
                    None
                }
            }
        }
    }

    /// Capture an owned compatibility/test outline view.
    ///
    /// New hot-path readers should prefer `capture_shared_file()`.
    pub fn capture_file_outline_view(&self, relative_path: &str) -> Option<FileOutlineView> {
        let file = self.get_file(relative_path)?;
        Some(FileOutlineView {
            relative_path: file.relative_path.clone(),
            symbols: file.symbols.clone(),
        })
    }

    /// Capture an owned compatibility/test symbol-detail view.
    ///
    /// New hot-path readers should prefer `capture_shared_file()`.
    pub fn capture_symbol_detail_view(&self, relative_path: &str) -> Option<SymbolDetailView> {
        let file = self.get_file(relative_path)?;
        Some(SymbolDetailView {
            relative_path: file.relative_path.clone(),
            content: file.content.clone(),
            symbols: file.symbols.clone(),
        })
    }

    /// Capture an owned compatibility/test file-content view.
    ///
    /// New hot-path readers should prefer `capture_shared_file()`.
    pub fn capture_file_content_view(&self, relative_path: &str) -> Option<FileContentView> {
        let file = self.get_file(relative_path)?;
        Some(FileContentView {
            relative_path: file.relative_path.clone(),
            content: file.content.clone(),
        })
    }

    /// Capture the data needed for `what_changed` timestamp mode without holding the read lock.
    pub fn capture_what_changed_timestamp_view(&self) -> WhatChangedTimestampView {
        use std::time::UNIX_EPOCH;

        let loaded_secs = self
            .loaded_at_system()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);

        let mut paths: Vec<String> = self.all_files().map(|(path, _)| path.clone()).collect();
        paths.sort();

        WhatChangedTimestampView { loaded_secs, paths }
    }

    /// Resolve a path hint to one exact indexed path, or a bounded ambiguous result.
    pub fn capture_resolve_path_view(&self, hint: &str) -> ResolvePathView {
        const RESOLVE_PATH_AMBIGUOUS_CAP: usize = 10;
        let normalized_hint = normalize_path_query(hint);
        if normalized_hint.is_empty() {
            return ResolvePathView::EmptyHint;
        }

        if self.get_file(&normalized_hint).is_some() {
            return ResolvePathView::Resolved {
                path: normalized_hint,
            };
        }

        let normalized_hint_lower = normalized_hint.to_ascii_lowercase();
        let parts: Vec<&str> = normalized_hint
            .split('/')
            .filter(|part| !part.is_empty())
            .collect();
        let basename = parts.last().copied().unwrap_or("");
        let dir_components = if parts.len() > 1 {
            &parts[..parts.len() - 1]
        } else {
            &[][..]
        };

        let mut candidates: Vec<String> = self
            .find_files_by_basename(basename)
            .into_iter()
            .map(|path| path.to_string())
            .collect();

        if candidates.is_empty() {
            candidates = self
                .all_files()
                .map(|(path, _)| path.as_str())
                .filter(|path| {
                    let path_lower = path.to_ascii_lowercase();
                    path_lower.ends_with(&normalized_hint_lower)
                        || path_lower.contains(&normalized_hint_lower)
                })
                .map(|path| path.to_string())
                .collect();
        }

        for component in dir_components {
            let component_matches: HashSet<&str> = self
                .find_files_by_dir_component(component)
                .into_iter()
                .collect();
            candidates.retain(|path| component_matches.contains(path.as_str()));
        }

        candidates.sort_by(|left, right| {
            let left_lower = left.to_ascii_lowercase();
            let right_lower = right.to_ascii_lowercase();
            let left_suffix = left_lower.ends_with(&normalized_hint_lower);
            let right_suffix = right_lower.ends_with(&normalized_hint_lower);
            right_suffix
                .cmp(&left_suffix)
                .then(left.len().cmp(&right.len()))
                .then(left.cmp(right))
        });
        candidates.dedup();

        match candidates.len() {
            0 => ResolvePathView::NotFound {
                hint: normalized_hint,
            },
            1 => ResolvePathView::Resolved {
                path: candidates.pop().expect("single candidate"),
            },
            len => {
                let overflow_count = len.saturating_sub(RESOLVE_PATH_AMBIGUOUS_CAP);
                ResolvePathView::Ambiguous {
                    hint: normalized_hint,
                    matches: candidates
                        .into_iter()
                        .take(RESOLVE_PATH_AMBIGUOUS_CAP)
                        .collect(),
                    overflow_count,
                }
            }
        }
    }

    /// Search for indexed files matching a path query with bounded tiered output.
    pub fn capture_search_files_view(
        &self,
        query: &str,
        limit: usize,
        current_file: Option<&str>,
    ) -> SearchFilesView {
        let limit = limit.clamp(1, 50);
        let normalized_query = normalize_path_query(query);
        if normalized_query.is_empty() {
            return SearchFilesView::EmptyQuery;
        }

        let normalized_query_lower = normalized_query.to_ascii_lowercase();
        let tokens = tokenize_path_query(&normalized_query);
        let basename_token = tokens.last().map(String::as_str).unwrap_or("");
        let component_tokens = if tokens.len() > 1 {
            &tokens[..tokens.len() - 1]
        } else {
            &[][..]
        };
        let has_path_context = normalized_query.contains('/');

        let mut strong_hits: Vec<String> = self
            .all_files()
            .map(|(path, _)| path.as_str())
            .filter(|path| {
                let path_lower = path.to_ascii_lowercase();
                path_lower == normalized_query_lower
                    || (has_path_context && path_lower.ends_with(&normalized_query_lower))
            })
            .map(|path| path.to_string())
            .collect();

        let basename_hits: Vec<String> = if basename_token.is_empty() {
            Vec::new()
        } else {
            self.find_files_by_basename(basename_token)
                .into_iter()
                .map(|path| path.to_string())
                .collect()
        };

        if !component_tokens.is_empty() {
            strong_hits.extend(
                basename_hits
                    .iter()
                    .filter(|path| {
                        component_tokens
                            .iter()
                            .all(|component| path_has_component(path, component))
                    })
                    .cloned(),
            );
        }

        let shared_len = |path: &str| -> usize {
            current_file
                .map(|cur| shared_directory_prefix_len(cur, path))
                .unwrap_or(0)
        };

        strong_hits.sort_by(|left, right| {
            let left_lower = left.to_ascii_lowercase();
            let right_lower = right.to_ascii_lowercase();
            let left_exact = left_lower == normalized_query_lower;
            let right_exact = right_lower == normalized_query_lower;
            let left_suffix = has_path_context && left_lower.ends_with(&normalized_query_lower);
            let right_suffix = has_path_context && right_lower.ends_with(&normalized_query_lower);

            right_exact
                .cmp(&left_exact)
                .then(right_suffix.cmp(&left_suffix))
                .then(shared_len(right).cmp(&shared_len(left)))
                .then(left.len().cmp(&right.len()))
                .then(left.cmp(right))
        });
        strong_hits.dedup();

        let strong_set: HashSet<&str> = strong_hits.iter().map(String::as_str).collect();
        let mut basename_only_hits: Vec<String> = basename_hits
            .into_iter()
            .filter(|path| !strong_set.contains(path.as_str()))
            .collect();
        basename_only_hits.sort_by(|left, right| {
            shared_len(right)
                .cmp(&shared_len(left))
                .then(left.len().cmp(&right.len()))
                .then(left.cmp(right))
        });
        basename_only_hits.dedup();

        // Prefix matches on basenames (e.g., "orchestrat" matches "orchestrator.rs" and "orchestration.rs")
        let basename_set: HashSet<&str> = strong_hits
            .iter()
            .chain(basename_only_hits.iter())
            .map(String::as_str)
            .collect();
        let mut prefix_hits: Vec<String> = if basename_token.len() >= 3 {
            self.all_files()
                .map(|(path, _)| path.as_str())
                .filter(|path| {
                    let file_basename = std::path::Path::new(path)
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or("");
                    file_basename
                        .to_ascii_lowercase()
                        .starts_with(&basename_token.to_ascii_lowercase())
                })
                .filter(|path| !basename_set.contains(path))
                .map(|path| path.to_string())
                .collect()
        } else {
            Vec::new()
        };
        prefix_hits.sort_by(|left, right| {
            shared_len(right)
                .cmp(&shared_len(left))
                .then(left.len().cmp(&right.len()))
                .then(left.cmp(right))
        });
        prefix_hits.dedup();

        let strong_or_basename_or_prefix_set: HashSet<&str> = strong_hits
            .iter()
            .chain(basename_only_hits.iter())
            .chain(prefix_hits.iter())
            .map(String::as_str)
            .collect();
        let mut loose_hits: Vec<String> = self
            .all_files()
            .map(|(path, _)| path.as_str())
            .filter(|path| !strong_or_basename_or_prefix_set.contains(*path))
            .filter(|path| {
                let path_lower = path.to_ascii_lowercase();
                tokens.iter().all(|token| path_lower.contains(token))
            })
            .map(|path| path.to_string())
            .collect();
        loose_hits.sort_by(|left, right| {
            shared_len(right)
                .cmp(&shared_len(left))
                .then(left.len().cmp(&right.len()))
                .then(left.cmp(right))
        });
        loose_hits.dedup();

        let total_matches =
            strong_hits.len() + basename_only_hits.len() + prefix_hits.len() + loose_hits.len();
        if total_matches == 0 {
            return SearchFilesView::NotFound {
                query: normalized_query,
            };
        }

        let mut hits: Vec<SearchFilesHit> = Vec::new();
        hits.extend(strong_hits.into_iter().map(|path| SearchFilesHit {
            tier: SearchFilesTier::StrongPath,
            path,
            coupling_score: None,
            shared_commits: None,
        }));
        hits.extend(basename_only_hits.into_iter().map(|path| SearchFilesHit {
            tier: SearchFilesTier::Basename,
            path,
            coupling_score: None,
            shared_commits: None,
        }));
        hits.extend(prefix_hits.into_iter().map(|path| SearchFilesHit {
            tier: SearchFilesTier::LoosePath,
            path,
            coupling_score: None,
            shared_commits: None,
        }));
        hits.extend(loose_hits.into_iter().map(|path| SearchFilesHit {
            tier: SearchFilesTier::LoosePath,
            path,
            coupling_score: None,
            shared_commits: None,
        }));

        let overflow_count = total_matches.saturating_sub(limit);
        hits.truncate(limit);

        SearchFilesView::Found {
            query: normalized_query,
            total_matches,
            overflow_count,
            hits,
        }
    }

    /// Capture the grouped data needed for `find_dependents` without holding the read lock.
    pub fn capture_find_dependents_view(&self, target_path: &str) -> FindDependentsView {
        let deps = self.find_dependents_for_file(target_path);
        let mut by_file: std::collections::BTreeMap<String, Vec<DependentLineView>> =
            std::collections::BTreeMap::new();

        for (file_path, reference) in deps {
            let line_number = reference.line_range.0 + 1;
            let line_content = self
                .get_file(file_path)
                .map(|file| {
                    String::from_utf8_lossy(&file.content)
                        .lines()
                        .nth(reference.line_range.0 as usize)
                        .unwrap_or("")
                        .to_string()
                })
                .unwrap_or_default();

            by_file
                .entry(file_path.to_string())
                .or_default()
                .push(DependentLineView {
                    line_number,
                    line_content,
                    kind: reference.kind.to_string(),
                });
        }

        FindDependentsView {
            files: by_file
                .into_iter()
                .map(|(file_path, lines)| DependentFileView { file_path, lines })
                .collect(),
        }
    }

    /// Capture the grouped data needed for `find_references` without holding the read lock.
    pub fn capture_find_references_view(
        &self,
        name: &str,
        kind_filter: Option<&str>,
        total_limit: usize,
    ) -> FindReferencesView {
        let kind_enum = parse_reference_kind_filter(kind_filter);
        let refs = self.find_references_for_name(name, kind_enum, false);
        self.build_find_references_view(&refs, total_limit)
    }

    /// Find all implementations of a trait/interface, or all traits a type implements.
    ///
    /// `name` is the trait or type to search for.
    /// `direction`: `None` or `Some("auto")` searches both directions.
    ///   `Some("trait")` treats `name` as a trait and returns implementors.
    ///   `Some("type")` treats `name` as a type and returns its traits.
    pub fn capture_find_implementations_view(
        &self,
        name: &str,
        direction: Option<&str>,
    ) -> FindImplementationsView {
        let mut entries: Vec<ImplementationEntryView> = Vec::new();

        for (file_path, file) in &self.files {
            for reference in &file.references {
                if reference.kind != ReferenceKind::Implements {
                    continue;
                }
                let trait_name = &reference.name;
                let implementor = match &reference.qualified_name {
                    Some(qn) => qn,
                    None => continue,
                };

                let matches = match direction {
                    Some("trait") => trait_name == name,
                    Some("type") => implementor == name,
                    _ => trait_name == name || implementor == name,
                };

                if matches {
                    entries.push(ImplementationEntryView {
                        trait_name: trait_name.clone(),
                        implementor: implementor.clone(),
                        file_path: file_path.clone(),
                        line: reference.line_range.0,
                    });
                }
            }
        }

        // Sort: group by trait name, then by implementor
        entries.sort_by(|a, b| {
            a.trait_name
                .cmp(&b.trait_name)
                .then(a.implementor.cmp(&b.implementor))
        });

        FindImplementationsView { entries }
    }

    pub fn capture_find_references_view_for_symbol(
        &self,
        path: &str,
        name: &str,
        symbol_kind: Option<&str>,
        symbol_line: Option<u32>,
        kind_filter: Option<&str>,
        total_limit: usize,
    ) -> Result<FindReferencesView, String> {
        let kind_enum = parse_reference_kind_filter(kind_filter);
        let refs =
            self.find_exact_references_for_symbol(path, name, symbol_kind, symbol_line, kind_enum)?;
        Ok(self.build_find_references_view(&refs, total_limit))
    }

    pub fn find_exact_references_for_symbol<'a>(
        &'a self,
        path: &'a str,
        name: &str,
        symbol_kind: Option<&str>,
        symbol_line: Option<u32>,
        kind_filter: Option<ReferenceKind>,
    ) -> Result<Vec<(&'a str, &'a ReferenceRecord)>, String> {
        let Some(file) = self.get_file(path) else {
            return Err(format!("File not found: {path}"));
        };

        match resolve_symbol_selector(file, name, symbol_kind, symbol_line) {
            SymbolSelectorMatch::NotFound => {
                let selector = render_symbol_selector(name, symbol_kind, symbol_line);
                Err(format!("Symbol not found in {path}: {selector}"))
            }
            SymbolSelectorMatch::Ambiguous(candidate_lines) => {
                let candidate_lines = candidate_lines
                    .iter()
                    .map(u32::to_string)
                    .collect::<Vec<_>>()
                    .join(", ");
                Err(format!(
                    "Ambiguous symbol selector for {name} in {path}; pass `symbol_line` to disambiguate. Candidates: {candidate_lines}"
                ))
            }
            SymbolSelectorMatch::Selected(_, _) => {
                Ok(self.collect_exact_symbol_references(path, file, name, kind_filter))
            }
        }
    }

    fn collect_exact_symbol_references<'a>(
        &'a self,
        path: &'a str,
        file: &'a IndexedFile,
        target_name: &str,
        kind_filter: Option<ReferenceKind>,
    ) -> Vec<(&'a str, &'a ReferenceRecord)> {
        let module_path = resolve_module_path(path, &file.language);
        let mut refs: Vec<(&str, &ReferenceRecord)> = file
            .references
            .iter()
            .filter(|reference| {
                matches_exact_symbol_reference(
                    reference,
                    target_name,
                    &file.language,
                    module_path.as_deref(),
                    kind_filter,
                )
            })
            .map(|reference| (path, reference))
            .collect();

        let dependent_refs = self.find_dependents_for_file(path);
        let dependent_paths: HashSet<&str> = dependent_refs.iter().map(|(fp, _)| *fp).collect();

        refs.extend(
            dependent_refs
                .into_iter()
                .filter(|(_file_path, reference)| {
                    matches_exact_symbol_reference(
                        reference,
                        target_name,
                        &file.language,
                        module_path.as_deref(),
                        kind_filter,
                    )
                }),
        );

        // Also check the reverse index for the target name, but scoped to
        // dependent files only.  This catches references that
        // find_dependents_for_file returned under a different symbol name
        // (e.g. it returned `Result` refs but skipped the `TokenizorError` import).
        for (ref_path, ref_record) in self.find_references_for_name(target_name, kind_filter, false)
        {
            if ref_path == path || !dependent_paths.contains(ref_path) {
                continue;
            }
            if !refs
                .iter()
                .any(|(p, r)| *p == ref_path && r.byte_range == ref_record.byte_range)
            {
                refs.push((ref_path, ref_record));
            }
        }

        refs.sort_by(|a, b| {
            a.0.cmp(b.0)
                .then(a.1.line_range.0.cmp(&b.1.line_range.0))
                .then(a.1.byte_range.0.cmp(&b.1.byte_range.0))
        });

        refs
    }

    fn build_find_references_view(
        &self,
        refs: &[(&str, &ReferenceRecord)],
        total_limit: usize,
    ) -> FindReferencesView {
        let mut by_file: std::collections::BTreeMap<String, Vec<ReferenceHitView>> =
            std::collections::BTreeMap::new();

        let mut built = 0usize;
        for (file_path, reference) in refs {
            if built >= total_limit {
                break;
            }
            let Some(file) = self.get_file(file_path) else {
                continue;
            };
            let content = String::from_utf8_lossy(&file.content);
            let content_lines: Vec<&str> = content.lines().collect();
            let ref_line_0 = reference.line_range.0 as usize;
            let ctx_start = ref_line_0.saturating_sub(1);
            let ctx_end = if content_lines.is_empty() {
                0
            } else {
                (ref_line_0 + 1).min(content_lines.len() - 1)
            };
            let enclosing_annotation = reference
                .enclosing_symbol_index
                .and_then(|idx| file.symbols.get(idx as usize))
                .map(|sym| format!("  [in {} {}]", sym.kind, sym.name));

            let context_lines = if content_lines.is_empty() {
                Vec::new()
            } else {
                content_lines[ctx_start..=ctx_end]
                    .iter()
                    .enumerate()
                    .map(|(i, line)| {
                        let zero_based_line = ctx_start + i;
                        ReferenceContextLineView {
                            line_number: (zero_based_line + 1) as u32,
                            text: (*line).to_string(),
                            is_reference_line: zero_based_line == ref_line_0,
                            enclosing_annotation: if zero_based_line == ref_line_0 {
                                enclosing_annotation.clone()
                            } else {
                                None
                            },
                        }
                    })
                    .collect()
            };

            by_file
                .entry((*file_path).to_string())
                .or_default()
                .push(ReferenceHitView { context_lines });
            built += 1;
        }

        let total_files = refs
            .iter()
            .map(|(f, _)| *f)
            .collect::<std::collections::BTreeSet<_>>()
            .len();
        FindReferencesView {
            total_refs: refs.len(),
            total_files,
            files: by_file
                .into_iter()
                .map(|(file_path, hits)| ReferenceFileView { file_path, hits })
                .collect(),
        }
    }

    /// Capture the full owned data needed for `get_context_bundle`.
    pub fn capture_context_bundle_view(
        &self,
        path: &str,
        name: &str,
        kind_filter: Option<&str>,
        symbol_line: Option<u32>,
    ) -> ContextBundleView {
        use crate::domain::ReferenceKind;

        const CONTEXT_BUNDLE_SECTION_CAP: usize = 20;

        let Some(file) = self.get_file(path) else {
            return ContextBundleView::FileNotFound {
                path: path.to_string(),
            };
        };

        let (sym_idx, sym_rec) = match resolve_symbol_selector(file, name, kind_filter, symbol_line)
        {
            SymbolSelectorMatch::Selected(sym_idx, sym_rec) => (sym_idx, sym_rec),
            SymbolSelectorMatch::NotFound => {
                return ContextBundleView::SymbolNotFound {
                    relative_path: file.relative_path.clone(),
                    symbol_names: file
                        .symbols
                        .iter()
                        .map(|symbol| symbol.name.clone())
                        .collect(),
                    name: name.to_string(),
                };
            }
            SymbolSelectorMatch::Ambiguous(candidate_lines) => {
                return ContextBundleView::AmbiguousSymbol {
                    path: file.relative_path.clone(),
                    name: name.to_string(),
                    candidate_lines,
                };
            }
        };

        let start = sym_rec.effective_start() as usize;
        let end = sym_rec.byte_range.1 as usize;
        let body = if end <= file.content.len() {
            String::from_utf8_lossy(&file.content[start..end]).into_owned()
        } else {
            String::from_utf8_lossy(&file.content).into_owned()
        };
        let byte_count = end.saturating_sub(start);

        let capture_section = |refs: &[(&str, &ReferenceRecord)]| -> ContextBundleSectionView {
            let entries: Vec<ContextBundleReferenceView> = refs
                .iter()
                .take(CONTEXT_BUNDLE_SECTION_CAP)
                .map(|(file_path, reference)| {
                    let enclosing = self.get_file(file_path).and_then(|f| {
                        reference
                            .enclosing_symbol_index
                            .and_then(|idx| f.symbols.get(idx as usize))
                            .map(|symbol| format!("in {} {}", symbol.kind, symbol.name))
                    });

                    ContextBundleReferenceView {
                        display_name: reference
                            .qualified_name
                            .as_deref()
                            .unwrap_or(&reference.name)
                            .to_string(),
                        file_path: (*file_path).to_string(),
                        line_number: reference.line_range.0 + 1,
                        enclosing,
                    }
                })
                .collect();

            ContextBundleSectionView {
                total_count: refs.len(),
                overflow_count: refs.len().saturating_sub(entries.len()),
                entries,
            }
        };

        let callers =
            self.collect_exact_symbol_references(path, file, name, Some(ReferenceKind::Call));
        let callees = self.callees_for_symbol(path, sym_idx);
        let callee_pairs: Vec<(&str, &ReferenceRecord)> =
            callees.iter().map(|reference| (path, *reference)).collect();
        let type_usages =
            self.collect_exact_symbol_references(path, file, name, Some(ReferenceKind::TypeUsage));

        // Resolve type dependencies: collect type names referenced within this symbol,
        // then find their definitions across the index (recursive, depth-limited to 2).
        let type_refs = self.type_refs_for_symbol(path, sym_idx);
        let type_names: Vec<&str> = type_refs
            .iter()
            .map(|r| r.name.as_str())
            // Exclude the target symbol's own name to avoid self-referential dependencies.
            .filter(|n| *n != name)
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();
        let dependencies = self.resolve_type_dependencies(&type_names, 2);

        ContextBundleView::Found(ContextBundleFoundView {
            file_path: file.relative_path.clone(),
            body,
            kind_label: sym_rec.kind.to_string(),
            line_range: sym_rec.line_range,
            byte_count,
            callers: capture_section(&callers),
            callees: capture_section(&callee_pairs),
            type_usages: capture_section(&type_usages),
            dependencies,
        })
    }

    /// Capture a full trace view for a symbol, composing existing captures.
    ///
    /// This is the one-call semantic investigation that replaces the common
    /// search_symbols → get_context_bundle → find_dependents → get_file_context pattern.
    pub fn capture_trace_symbol_view(
        &self,
        path: &str,
        name: &str,
        kind_filter: Option<&str>,
        symbol_line: Option<u32>,
        sections: Option<&[String]>,
    ) -> TraceSymbolView {
        // Reuse context_bundle for the core symbol + callers + callees + type_usages + deps.
        let bundle = self.capture_context_bundle_view(path, name, kind_filter, symbol_line);

        let found = match bundle {
            ContextBundleView::FileNotFound { path } => {
                return TraceSymbolView::FileNotFound { path };
            }
            ContextBundleView::AmbiguousSymbol {
                path,
                name,
                candidate_lines,
            } => {
                return TraceSymbolView::AmbiguousSymbol {
                    path,
                    name,
                    candidate_lines,
                };
            }
            ContextBundleView::SymbolNotFound {
                relative_path,
                symbol_names,
                name,
            } => {
                return TraceSymbolView::SymbolNotFound {
                    relative_path,
                    symbol_names,
                    name,
                };
            }
            ContextBundleView::Found(view) => view,
        };

        let wants = |section: &str| -> bool {
            sections
                .map(|s| s.iter().any(|v| v.eq_ignore_ascii_case(section)))
                .unwrap_or(true)
        };

        // Dependents: files that import the target file.
        let dependents = if wants("dependents") {
            self.capture_find_dependents_view(path)
        } else {
            FindDependentsView { files: vec![] }
        };

        // Siblings: symbols at the same depth in the same file.
        let siblings = if wants("siblings") {
            self.get_file(path)
                .map(|file| {
                    // Find the target symbol to get its depth.
                    let target_depth = file
                        .symbols
                        .iter()
                        .find(|s| {
                            s.name == name
                                && kind_filter
                                    .map(|k| s.kind.to_string().eq_ignore_ascii_case(k))
                                    .unwrap_or(true)
                                && symbol_line.map(|l| s.line_range.0 == l).unwrap_or(true)
                        })
                        .map(|s| s.depth)
                        .unwrap_or(0);

                    file.symbols
                        .iter()
                        .filter(|s| s.depth == target_depth && s.name != name)
                        .map(|s| SiblingSymbolView {
                            name: s.name.clone(),
                            kind_label: s.kind.to_string(),
                            line_range: s.line_range,
                        })
                        .collect()
                })
                .unwrap_or_default()
        } else {
            vec![]
        };

        // Trait implementations.
        let implementations = if wants("implementations") {
            self.capture_find_implementations_view(name, None)
        } else {
            FindImplementationsView { entries: vec![] }
        };

        TraceSymbolView::Found(TraceSymbolFoundView {
            context_bundle: found,
            dependents,
            siblings,
            implementations,
            git_activity: None, // Filled in by the tool method which has access to git_temporal.
        })
    }

    /// Capture a focused inspection view around a specific line.
    pub fn capture_inspect_match_view(
        &self,
        path: &str,
        line: u32,
        context_lines: Option<u32>,
    ) -> InspectMatchView {
        let Some(file) = self.get_file(path) else {
            return InspectMatchView::FileNotFound {
                path: path.to_string(),
            };
        };

        // 1. Render excerpt (simple around-line logic).
        let content = String::from_utf8_lossy(&file.content);
        let lines: Vec<&str> = content.lines().collect();

        if line as usize > lines.len() || line == 0 {
            return InspectMatchView::LineOutOfBounds {
                path: file.relative_path.clone(),
                line,
                total_lines: lines.len(),
            };
        }

        let anchor = line as usize;
        let context = context_lines.unwrap_or(3) as usize;
        let start = anchor.saturating_sub(context).max(1);
        let end = anchor.saturating_add(context).min(lines.len());

        let excerpt = if start > end || start > lines.len() {
            String::new()
        } else {
            (start..=end)
                .map(|ln| format!("{ln}: {}", lines[ln - 1]))
                .collect::<Vec<_>>()
                .join("\n")
        };

        // 2. Find enclosing symbol (deepest symbol containing the line).
        // `line` input is 1-based, symbol ranges are 0-based inclusive.
        let target_line_0 = line.saturating_sub(1);
        let enclosing_symbol = file
            .symbols
            .iter()
            .filter(|s| s.line_range.0 <= target_line_0 && s.line_range.1 >= target_line_0)
            .max_by_key(|s| s.depth);

        let enclosing = enclosing_symbol.map(|s| EnclosingSymbolView {
            name: s.name.clone(),
            kind_label: s.kind.to_string(),
            line_range: (s.line_range.0 + 1, s.line_range.1 + 1),
        });

        // 3. Find siblings (same depth as enclosing, or depth 0).
        let target_depth = enclosing_symbol.map(|s| s.depth).unwrap_or(0);
        let siblings = file
            .symbols
            .iter()
            .filter(|s| s.depth == target_depth)
            .map(|s| SiblingSymbolView {
                name: s.name.clone(),
                kind_label: s.kind.to_string(),
                line_range: (s.line_range.0 + 1, s.line_range.1 + 1),
            })
            .collect();

        InspectMatchView::Found(InspectMatchFoundView {
            path: file.relative_path.clone(),
            line,
            excerpt,
            enclosing,
            siblings,
        })
    }

    /// Capture the data needed to render `repo_outline` without holding the read lock.
    pub fn capture_repo_outline_view(&self) -> RepoOutlineView {
        let mut files: Vec<RepoOutlineFileView> = self
            .all_files()
            .map(|(relative_path, file)| RepoOutlineFileView {
                relative_path: relative_path.clone(),
                language: file.language.clone(),
                symbol_count: file.symbols.len(),
            })
            .collect();
        files.sort_by(|a, b| a.relative_path.cmp(&b.relative_path));

        let total_symbols = files.iter().map(|file| file.symbol_count).sum();

        RepoOutlineView {
            total_files: files.len(),
            total_symbols,
            files,
        }
    }

    /// Return sorted relative paths matching a basename, case-insensitively.
    pub fn find_files_by_basename(&self, basename: &str) -> Vec<&str> {
        self.files_by_basename
            .get(&basename.to_ascii_lowercase())
            .map(|paths| paths.iter().map(|path| path.as_str()).collect())
            .unwrap_or_default()
    }

    /// Return sorted relative paths containing the given directory component, case-insensitively.
    pub fn find_files_by_dir_component(&self, component: &str) -> Vec<&str> {
        self.files_by_dir_component
            .get(&component.to_ascii_lowercase())
            .map(|paths| paths.iter().map(|path| path.as_str()).collect())
            .unwrap_or_default()
    }

    /// Number of indexed files.
    pub fn file_count(&self) -> usize {
        self.files.len()
    }

    /// Total symbols across all indexed files.
    pub fn symbol_count(&self) -> usize {
        self.files.values().map(|f| f.symbols.len()).sum()
    }

    /// `true` when the index has been loaded and the circuit breaker has NOT tripped.
    pub fn is_ready(&self) -> bool {
        if self.is_empty {
            return false;
        }
        !self.cb_state.is_tripped()
    }

    /// Returns the current index state.
    pub fn index_state(&self) -> IndexState {
        if self.is_empty {
            return IndexState::Empty;
        }
        if self.cb_state.is_tripped() {
            IndexState::CircuitBreakerTripped {
                summary: self.cb_state.summary(),
            }
        } else {
            IndexState::Ready
        }
    }

    /// Returns the wall-clock time when the index was last loaded.
    pub fn loaded_at_system(&self) -> SystemTime {
        self.loaded_at_system
    }

    /// Compute health statistics for the index.
    ///
    /// Watcher fields are populated with safe defaults (Off state, zero counts).
    /// Use `health_stats_with_watcher` when a watcher is active.
    pub fn health_stats(&self) -> HealthStats {
        let mut parsed_count = 0usize;
        let mut partial_parse_count = 0usize;
        let mut failed_count = 0usize;
        let mut symbol_count = 0usize;

        for file in self.files.values() {
            symbol_count += file.symbols.len();
            match &file.parse_status {
                ParseStatus::Parsed => parsed_count += 1,
                ParseStatus::PartialParse { .. } => partial_parse_count += 1,
                ParseStatus::Failed { .. } => failed_count += 1,
            }
        }

        HealthStats {
            file_count: self.files.len(),
            symbol_count,
            parsed_count,
            partial_parse_count,
            failed_count,
            load_duration: self.load_duration,
            watcher_state: WatcherState::Off,
            events_processed: 0,
            last_event_at: None,
            debounce_window_ms: 200,
        }
    }

    /// Compute health statistics, populating watcher fields from the provided `WatcherInfo`.
    ///
    /// Use this variant when the file watcher is active and its state should be reflected
    /// in health reports.
    pub fn health_stats_with_watcher(&self, watcher: &WatcherInfo) -> HealthStats {
        let mut stats = self.health_stats();
        stats.watcher_state = watcher.state.clone();
        stats.events_processed = watcher.events_processed;
        stats.last_event_at = watcher.last_event_at;
        stats.debounce_window_ms = watcher.debounce_window_ms;
        stats
    }

    // -----------------------------------------------------------------------
    // Cross-reference query methods (Phase 4, Plan 02)
    // -----------------------------------------------------------------------

    /// Find all `ReferenceRecord`s across the repo that match `name`.
    ///
    /// # Arguments
    /// * `name` — the reference name to look up. If it contains `::` or `.`,
    ///   it is treated as a qualified name and matched against `qualified_name`.
    ///   Otherwise matched against `name`.
    /// * `kind_filter` — when `Some(k)`, only references of kind `k` are returned.
    /// * `include_filtered` — when `false` (default), built-in type names and
    ///   single-letter generic parameters are silently filtered out (returns empty).
    ///   Set to `true` to bypass that filter.
    ///
    /// # Alias resolution (XREF-05)
    /// In addition to the direct reverse-index lookup, the method also checks
    /// every file's `alias_map`. If a file declares `alias_map["Map"] = "HashMap"`,
    /// then searching for `"HashMap"` will also yield references stored under `"Map"`.
    ///
    /// Returns a `Vec` of `(file_path, &ReferenceRecord)` tuples.
    pub fn find_references_for_name(
        &self,
        name: &str,
        kind_filter: Option<ReferenceKind>,
        include_filtered: bool,
    ) -> Vec<(&str, &ReferenceRecord)> {
        // Apply built-in / generic filter first.
        if !include_filtered && is_filtered_name(name) {
            return vec![];
        }

        let is_qualified = name.contains("::") || name.contains('.');

        let mut results: Vec<(&str, &ReferenceRecord)> = Vec::new();

        if is_qualified {
            // Qualified lookup: the reverse index is keyed by simple name, not qualified name.
            // We must scan all files and match against the qualified_name field.
            for (file_path, file) in &self.files {
                for reference in &file.references {
                    if let Some(qn) = reference.qualified_name.as_deref() {
                        if qn != name {
                            continue;
                        }
                    } else {
                        continue;
                    }
                    if let Some(kf) = kind_filter
                        && reference.kind != kf
                    {
                        continue;
                    }
                    results.push((file_path.as_str(), reference));
                }
            }
        } else {
            // Simple lookup: use the reverse index for O(1) name lookup.
            self.collect_refs_for_key(name, kind_filter, &mut results);

            // Alias resolution: find any alias that resolves to `name`.
            // e.g. alias_map["Map"] = "HashMap" means we also look up "Map".
            // Collect aliases first to avoid re-borrowing self during mutation of results.
            let aliases: Vec<String> = self
                .files
                .values()
                .flat_map(|file| {
                    file.alias_map
                        .iter()
                        .filter(|(_alias, original)| original.as_str() == name)
                        .map(|(alias, _)| alias.clone())
                })
                .collect();

            for alias in &aliases {
                self.collect_refs_for_key(alias, kind_filter, &mut results);
            }
        }

        results
    }

    /// Internal helper: look up `lookup_key` in `reverse_index`, resolve each location,
    /// apply kind filter (no qualified-name check), and append matching results.
    ///
    /// Only used for simple (non-qualified) name lookups.
    fn collect_refs_for_key<'a>(
        &'a self,
        lookup_key: &str,
        kind_filter: Option<ReferenceKind>,
        results: &mut Vec<(&'a str, &'a ReferenceRecord)>,
    ) {
        if let Some(locations) = self.reverse_index.get(lookup_key) {
            for loc in locations {
                let file = match self.files.get(&loc.file_path) {
                    Some(f) => f,
                    None => continue,
                };
                let reference = match file.references.get(loc.reference_idx as usize) {
                    Some(r) => r,
                    None => continue,
                };
                if let Some(kf) = kind_filter
                    && reference.kind != kf
                {
                    continue;
                }
                results.push((loc.file_path.as_str(), reference));
            }
        }
    }

    /// Find all files that import (depend on) `target_path`.
    ///
    /// Uses two strategies:
    /// 1. **Stem matching** — the import's `name`/`qualified_name` contains the file stem
    ///    as a path segment (e.g. `import db` matches `src/db.rs`).
    /// 2. **Module path matching** — resolves the file to its logical module path
    ///    (e.g. `src/live_index/mod.rs` → `crate::live_index`) and checks if any import
    ///    starts with that module path. This handles `lib.rs`, `mod.rs`, `__init__.py`,
    ///    and `index.js`/`index.ts` which stem matching misses.
    ///
    /// Returns a `Vec` of `(importing_file_path, &import_reference)` tuples.
    pub fn find_dependents_for_file(&self, target_path: &str) -> Vec<(&str, &ReferenceRecord)> {
        let Some(target_file) = self.files.get(target_path) else {
            return vec![];
        };

        // Extract the file stem: "src/db.rs" → "db"
        let stem = std::path::Path::new(target_path)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or(target_path);

        // Resolve the logical module path for the target file.
        let module_path = resolve_module_path(target_path, &target_file.language);

        let target_language = target_file.language.clone();
        let target_scope = declared_scope(target_file);
        let target_symbol_names: HashSet<&str> = target_file
            .symbols
            .iter()
            .map(|symbol| symbol.name.as_str())
            .filter(|name| !name.is_empty())
            .collect();

        let mut results = Vec::new();

        for (file_path, file) in &self.files {
            // Don't report a file as depending on itself.
            if file_path.as_str() == target_path {
                continue;
            }

            let matching_imports: Vec<&ReferenceRecord> = file
                .references
                .iter()
                .filter(|reference| {
                    matches_target_import(&target_language, reference, stem, module_path.as_deref())
                })
                .collect();

            if !target_symbol_names.is_empty()
                && (can_match_type_dependents(file, &target_language, target_scope.as_deref())
                    || !matching_imports.is_empty())
            {
                let symbol_refs: Vec<&ReferenceRecord> = file
                    .references
                    .iter()
                    .filter(|reference| {
                        reference.kind != ReferenceKind::Import
                            && target_symbol_names.contains(reference.name.as_str())
                    })
                    .collect();

                if !symbol_refs.is_empty() {
                    results.extend(
                        symbol_refs
                            .into_iter()
                            .map(|reference| (file_path.as_str(), reference)),
                    );
                    continue;
                }
            }

            results.extend(
                matching_imports
                    .into_iter()
                    .map(|reference| (file_path.as_str(), reference)),
            );
        }

        // --- Re-export chain resolution (Rust only, max 2 hops) ---
        //
        // If file X is re-exported via `pub use` in file Y, then files that
        // import from Y are also dependents of X.  We use a BFS with a depth
        // limit to avoid infinite loops and keep the search bounded.
        if target_language == LanguageId::Rust {
            let already_found: HashSet<&str> = results.iter().map(|(path, _)| *path).collect();

            let mut visited: HashSet<&str> = HashSet::new();
            visited.insert(target_path);

            // Seed: find files that pub-use-re-export from target
            let mut queue: VecDeque<(&str, u8)> = VecDeque::new();
            let reexporters = find_reexporters(
                &self.files,
                target_path,
                module_path.as_deref(),
                &target_language,
                stem,
            );
            for re_path in reexporters {
                if visited.insert(re_path) {
                    queue.push_back((re_path, 1));
                }
            }

            while let Some((reexporter_path, depth)) = queue.pop_front() {
                let Some(re_file) = self.files.get(reexporter_path) else {
                    continue;
                };

                let re_stem = std::path::Path::new(reexporter_path)
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or(reexporter_path);
                let re_module_path = resolve_module_path(reexporter_path, &re_file.language);

                // Find files that import from the re-exporter
                for (file_path, file) in &self.files {
                    if file_path.as_str() == target_path
                        || file_path.as_str() == reexporter_path
                        || already_found.contains(file_path.as_str())
                    {
                        continue;
                    }

                    let transitive_imports: Vec<&ReferenceRecord> = file
                        .references
                        .iter()
                        .filter(|reference| {
                            matches_target_import(
                                &target_language,
                                reference,
                                re_stem,
                                re_module_path.as_deref(),
                            )
                        })
                        .collect();

                    if !transitive_imports.is_empty() {
                        // Prefer symbol-level usage refs when target symbol names match
                        if !target_symbol_names.is_empty() {
                            let symbol_refs: Vec<&ReferenceRecord> = file
                                .references
                                .iter()
                                .filter(|reference| {
                                    reference.kind != ReferenceKind::Import
                                        && target_symbol_names.contains(reference.name.as_str())
                                })
                                .collect();
                            if !symbol_refs.is_empty() {
                                results.extend(
                                    symbol_refs.into_iter().map(|r| (file_path.as_str(), r)),
                                );
                                continue;
                            }
                        }

                        results.extend(
                            transitive_imports
                                .into_iter()
                                .map(|r| (file_path.as_str(), r)),
                        );
                    }
                }

                // Follow the chain one more hop if the re-exporter is itself
                // re-exported by another file (depth limit: 2).
                if depth < 2 {
                    let next_reexporters = find_reexporters(
                        &self.files,
                        reexporter_path,
                        re_module_path.as_deref(),
                        &target_language,
                        re_stem,
                    );
                    for next_path in next_reexporters {
                        if visited.insert(next_path) {
                            queue.push_back((next_path, depth + 1));
                        }
                    }
                }
            }
        }

        results.sort_by(|a, b| {
            a.0.cmp(b.0)
                .then(a.1.line_range.0.cmp(&b.1.line_range.0))
                .then(a.1.byte_range.0.cmp(&b.1.byte_range.0))
        });

        // Deduplicate (same file + same byte range = same reference)
        results.dedup_by(|a, b| a.0 == b.0 && a.1.byte_range == b.1.byte_range);

        results
    }

    /// Returns all `Call` references inside the given file whose
    /// `enclosing_symbol_index` equals `symbol_index`.
    ///
    /// These are the "callees" — functions called from within the target symbol.
    /// Consumed by `get_context_bundle` (Plan 03).
    pub fn callees_for_symbol(
        &self,
        file_path: &str,
        symbol_index: usize,
    ) -> Vec<&ReferenceRecord> {
        match self.files.get(file_path) {
            None => vec![],
            Some(file) => {
                let symbol_range = file
                    .symbols
                    .get(symbol_index)
                    .map(|symbol| symbol.line_range);
                file.references
                    .iter()
                    .filter(|reference| {
                        if reference.kind != ReferenceKind::Call {
                            return false;
                        }

                        // Filter stdlib/iterator noise from callees (same filter as find_references).
                        if is_filtered_name(&reference.name) {
                            return false;
                        }

                        if let Some((start_line, end_line)) = symbol_range {
                            reference.line_range.0 >= start_line
                                && reference.line_range.1 <= end_line
                        } else {
                            reference.enclosing_symbol_index == Some(symbol_index as u32)
                        }
                    })
                    .collect()
            }
        }
    }

    /// Returns all `TypeUsage` references inside the given symbol's line range.
    pub fn type_refs_for_symbol(
        &self,
        file_path: &str,
        symbol_index: usize,
    ) -> Vec<&ReferenceRecord> {
        match self.files.get(file_path) {
            None => vec![],
            Some(file) => {
                let symbol_range = file
                    .symbols
                    .get(symbol_index)
                    .map(|symbol| symbol.line_range);
                file.references
                    .iter()
                    .filter(|reference| {
                        if reference.kind != ReferenceKind::TypeUsage {
                            return false;
                        }
                        if let Some((start_line, end_line)) = symbol_range {
                            reference.line_range.0 >= start_line
                                && reference.line_range.1 <= end_line
                        } else {
                            reference.enclosing_symbol_index == Some(symbol_index as u32)
                        }
                    })
                    .collect()
            }
        }
    }

    /// Resolve type names to their definitions across the index.
    ///
    /// Returns definitions for custom types found in the index, excluding
    /// built-in/primitive types. Recurses up to `max_depth` levels to include
    /// transitive type dependencies.
    pub fn resolve_type_dependencies(
        &self,
        type_names: &[&str],
        max_depth: u8,
    ) -> Vec<TypeDependencyView> {
        const TYPE_DEF_KINDS: &[SymbolKind] = &[
            SymbolKind::Struct,
            SymbolKind::Enum,
            SymbolKind::Type,
            SymbolKind::Interface,
            SymbolKind::Class,
            SymbolKind::Trait,
        ];
        const MAX_DEPENDENCIES: usize = 15;

        let mut resolved: Vec<TypeDependencyView> = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();
        let mut queue: Vec<(String, u8)> = type_names
            .iter()
            .filter(|n| !is_filtered_name(n))
            .map(|n| (n.to_string(), 0u8))
            .collect();

        while let Some((name, depth)) = queue.pop() {
            if seen.contains(&name) || resolved.len() >= MAX_DEPENDENCIES {
                continue;
            }
            seen.insert(name.clone());

            // Search all files for a matching type definition.
            let mut found = false;
            for file in self.files.values() {
                for sym in &file.symbols {
                    if sym.name == name && TYPE_DEF_KINDS.contains(&sym.kind) && sym.depth == 0 {
                        let start = sym.byte_range.0 as usize;
                        let end = sym.byte_range.1 as usize;
                        let body = if end <= file.content.len() {
                            String::from_utf8_lossy(&file.content[start..end]).into_owned()
                        } else {
                            continue;
                        };

                        // If recursion budget remains, extract type refs from this definition.
                        if depth < max_depth {
                            for reference in &file.references {
                                if reference.kind == ReferenceKind::TypeUsage
                                    && reference.line_range.0 >= sym.line_range.0
                                    && reference.line_range.1 <= sym.line_range.1
                                    && !is_filtered_name(&reference.name)
                                    && !seen.contains(&reference.name)
                                {
                                    queue.push((reference.name.clone(), depth + 1));
                                }
                            }
                        }

                        resolved.push(TypeDependencyView {
                            name: name.clone(),
                            kind_label: sym.kind.to_string(),
                            file_path: file.relative_path.clone(),
                            line_range: sym.line_range,
                            body,
                            depth,
                        });
                        found = true;
                        break;
                    }
                }
                if found {
                    break;
                }
            }
        }

        resolved.sort_by(|a, b| a.depth.cmp(&b.depth).then(a.name.cmp(&b.name)));
        resolved
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ContextBundleView, ResolvePathView, SearchFilesHit, SearchFilesTier, SearchFilesView,
    };
    use crate::domain::{LanguageId, ReferenceKind, ReferenceRecord, SymbolKind, SymbolRecord};
    use crate::live_index::store::{
        CircuitBreakerState, IndexState, IndexedFile, LiveIndex, ParseStatus,
    };
    use crate::watcher::{WatcherInfo, WatcherState};
    use std::collections::HashMap;
    use std::time::{Duration, Instant, SystemTime};

    fn make_symbol(name: &str) -> SymbolRecord {
        SymbolRecord {
            name: name.to_string(),
            kind: SymbolKind::Function,
            depth: 0,
            sort_order: 0,
            byte_range: (0, 10),
            line_range: (0, 1),
            doc_byte_range: None,
        }
    }

    fn make_indexed_file(
        path: &str,
        symbols: Vec<SymbolRecord>,
        status: ParseStatus,
    ) -> IndexedFile {
        IndexedFile {
            relative_path: path.to_string(),
            language: LanguageId::Rust,
            classification: crate::domain::FileClassification::for_code_path(path),
            content: b"fn test() {}".to_vec(),
            symbols,
            parse_status: status,
            byte_len: 12,
            content_hash: "abc".to_string(),
            references: vec![],
            alias_map: std::collections::HashMap::new(),
        }
    }

    fn make_index(files: Vec<(&str, IndexedFile)>, tripped: bool) -> LiveIndex {
        let cb = CircuitBreakerState::new(0.20);
        if tripped {
            // Force-trip by recording enough failures
            for i in 0..10 {
                cb.record_success();
                if i < 7 {
                    cb.record_success();
                }
            }
            for i in 0..5 {
                cb.record_failure(&format!("f{i}.rs"), "error");
            }
            cb.should_abort();
        }

        let files_map: std::collections::HashMap<String, std::sync::Arc<IndexedFile>> = files
            .into_iter()
            .map(|(p, f)| (p.to_string(), std::sync::Arc::new(f)))
            .collect();
        let trigram_index = crate::live_index::trigram::TrigramIndex::build_from_files(&files_map);
        let mut index = LiveIndex {
            files: files_map,
            loaded_at: Instant::now(),
            loaded_at_system: std::time::SystemTime::now(),
            load_duration: Duration::from_millis(50),
            cb_state: cb,
            is_empty: false,
            load_source: crate::live_index::store::IndexLoadSource::FreshLoad,
            snapshot_verify_state: crate::live_index::store::SnapshotVerifyState::NotNeeded,
            reverse_index: std::collections::HashMap::new(),
            files_by_basename: std::collections::HashMap::new(),
            files_by_dir_component: std::collections::HashMap::new(),
            trigram_index,
        };
        // Rebuild the reverse index so xref query tests work.
        index.rebuild_reverse_index();
        index.rebuild_path_indices();
        index
    }

    // --- xref test helpers ---

    fn make_ref(
        name: &str,
        qualified_name: Option<&str>,
        kind: ReferenceKind,
        enclosing: Option<u32>,
        byte_start: u32,
    ) -> ReferenceRecord {
        ReferenceRecord {
            name: name.to_string(),
            qualified_name: qualified_name.map(|s| s.to_string()),
            kind,
            byte_range: (byte_start, byte_start + 10),
            line_range: (byte_start / 100, byte_start / 100),
            enclosing_symbol_index: enclosing,
        }
    }

    fn make_file_with_refs(
        path: &str,
        refs: Vec<ReferenceRecord>,
        alias_map: HashMap<String, String>,
    ) -> IndexedFile {
        IndexedFile {
            relative_path: path.to_string(),
            language: LanguageId::Rust,
            classification: crate::domain::FileClassification::for_code_path(path),
            content: b"fn test() {}".to_vec(),
            symbols: vec![],
            parse_status: ParseStatus::Parsed,
            byte_len: 12,
            content_hash: "abc".to_string(),
            references: refs,
            alias_map,
        }
    }

    fn make_file_with_refs_and_content(
        path: &str,
        language: LanguageId,
        content: &str,
        refs: Vec<ReferenceRecord>,
        symbols: Vec<SymbolRecord>,
    ) -> IndexedFile {
        IndexedFile {
            relative_path: path.to_string(),
            language,
            classification: crate::domain::FileClassification::for_code_path(path),
            content: content.as_bytes().to_vec(),
            symbols,
            parse_status: ParseStatus::Parsed,
            byte_len: content.len() as u64,
            content_hash: "abc".to_string(),
            references: refs,
            alias_map: HashMap::new(),
        }
    }

    fn make_symbol_with_kind_and_line(name: &str, kind: SymbolKind, line: u32) -> SymbolRecord {
        SymbolRecord {
            name: name.to_string(),
            kind,
            depth: 0,
            sort_order: 0,
            byte_range: (0, 10),
            line_range: (line, line),
            doc_byte_range: None,
        }
    }

    fn make_symbol_with_kind_line_and_bytes(
        name: &str,
        kind: SymbolKind,
        line: u32,
        byte_range: (u32, u32),
    ) -> SymbolRecord {
        SymbolRecord {
            name: name.to_string(),
            kind,
            depth: 0,
            sort_order: 0,
            byte_range,
            line_range: (line, line),
            doc_byte_range: None,
        }
    }

    #[test]
    fn test_get_file_returns_some_for_existing() {
        let f = make_indexed_file(
            "src/main.rs",
            vec![make_symbol("main")],
            ParseStatus::Parsed,
        );
        let index = make_index(vec![("src/main.rs", f)], false);
        assert!(index.get_file("src/main.rs").is_some());
    }

    #[test]
    fn test_get_file_returns_none_for_missing() {
        let index = make_index(vec![], false);
        assert!(index.get_file("nonexistent.rs").is_none());
    }

    #[test]
    fn test_symbols_for_file_returns_slice() {
        let sym = make_symbol("foo");
        let f = make_indexed_file("src/main.rs", vec![sym.clone()], ParseStatus::Parsed);
        let index = make_index(vec![("src/main.rs", f)], false);
        let syms = index.symbols_for_file("src/main.rs");
        assert_eq!(syms.len(), 1);
        assert_eq!(syms[0].name, "foo");
    }

    #[test]
    fn test_symbols_for_file_returns_empty_for_missing() {
        let index = make_index(vec![], false);
        let syms = index.symbols_for_file("nonexistent.rs");
        assert!(syms.is_empty());
    }

    #[test]
    fn test_all_files_returns_all_entries() {
        let f1 = make_indexed_file("a.rs", vec![], ParseStatus::Parsed);
        let f2 = make_indexed_file("b.rs", vec![], ParseStatus::Parsed);
        let index = make_index(vec![("a.rs", f1), ("b.rs", f2)], false);
        let pairs: Vec<_> = index.all_files().collect();
        assert_eq!(pairs.len(), 2);
    }

    #[test]
    fn test_capture_repo_outline_view_sorts_paths_and_counts_symbols() {
        let f1 = make_indexed_file(
            "src/zeta.rs",
            vec![make_symbol("zeta"), make_symbol("helper")],
            ParseStatus::Parsed,
        );
        let f2 = make_indexed_file(
            "src/alpha.rs",
            vec![make_symbol("alpha")],
            ParseStatus::Parsed,
        );
        let index = make_index(vec![("src/zeta.rs", f1), ("src/alpha.rs", f2)], false);

        let view = index.capture_repo_outline_view();

        assert_eq!(view.total_files, 2);
        assert_eq!(view.total_symbols, 3);
        assert_eq!(
            view.files
                .iter()
                .map(|file| file.relative_path.as_str())
                .collect::<Vec<_>>(),
            vec!["src/alpha.rs", "src/zeta.rs"]
        );
        assert_eq!(view.files[0].symbol_count, 1);
        assert_eq!(view.files[1].symbol_count, 2);
    }

    #[test]
    fn test_capture_file_outline_view_clones_path_and_symbols() {
        let f = make_indexed_file(
            "src/main.rs",
            vec![make_symbol("main"), make_symbol("helper")],
            ParseStatus::Parsed,
        );
        let index = make_index(vec![("src/main.rs", f)], false);

        let view = index
            .capture_file_outline_view("src/main.rs")
            .expect("captured outline view");

        assert_eq!(view.relative_path, "src/main.rs");
        assert_eq!(
            view.symbols
                .iter()
                .map(|symbol| symbol.name.as_str())
                .collect::<Vec<_>>(),
            vec!["main", "helper"]
        );
    }

    #[test]
    fn test_capture_symbol_detail_view_clones_content_and_symbols() {
        let f = make_indexed_file(
            "src/lib.rs",
            vec![make_symbol("foo"), make_symbol("bar")],
            ParseStatus::Parsed,
        );
        let index = make_index(vec![("src/lib.rs", f)], false);

        let view = index
            .capture_symbol_detail_view("src/lib.rs")
            .expect("captured symbol detail view");

        assert_eq!(view.relative_path, "src/lib.rs");
        assert_eq!(view.content, b"fn test() {}".to_vec());
        assert_eq!(
            view.symbols
                .iter()
                .map(|symbol| symbol.name.as_str())
                .collect::<Vec<_>>(),
            vec!["foo", "bar"]
        );
    }

    #[test]
    fn test_capture_file_content_view_clones_path_and_content() {
        let f = make_indexed_file("src/lib.rs", vec![], ParseStatus::Parsed);
        let index = make_index(vec![("src/lib.rs", f)], false);

        let view = index
            .capture_file_content_view("src/lib.rs")
            .expect("captured file content view");

        assert_eq!(view.relative_path, "src/lib.rs");
        assert_eq!(view.content, b"fn test() {}".to_vec());
    }

    #[test]
    fn test_capture_shared_file_returns_same_arc_entry() {
        let f = make_indexed_file("src/lib.rs", vec![], ParseStatus::Parsed);
        let index = make_index(vec![("src/lib.rs", f)], false);

        let shared = index
            .capture_shared_file("src/lib.rs")
            .expect("captured shared file");
        let stored = index.files.get("src/lib.rs").expect("stored file");

        assert!(std::sync::Arc::ptr_eq(stored, &shared));
        assert_eq!(shared.relative_path, "src/lib.rs");
        assert_eq!(shared.content, b"fn test() {}".to_vec());
    }

    #[test]
    fn test_capture_shared_file_for_scope_exact_returns_same_arc_entry() {
        let f = make_indexed_file("src/lib.rs", vec![], ParseStatus::Parsed);
        let index = make_index(vec![("src/lib.rs", f)], false);

        let shared = index
            .capture_shared_file_for_scope(&super::PathScope::exact("src/lib.rs"))
            .expect("captured scoped shared file");
        let stored = index.files.get("src/lib.rs").expect("stored file");

        assert!(std::sync::Arc::ptr_eq(stored, &shared));
    }

    #[test]
    fn test_capture_shared_file_for_scope_unique_prefix_returns_only_match() {
        let index = make_index(
            vec![
                (
                    "src/lib.rs",
                    make_indexed_file("src/lib.rs", vec![], ParseStatus::Parsed),
                ),
                (
                    "src/main.rs",
                    make_indexed_file("src/main.rs", vec![], ParseStatus::Parsed),
                ),
                (
                    "tests/lib_test.rs",
                    make_indexed_file("tests/lib_test.rs", vec![], ParseStatus::Parsed),
                ),
            ],
            false,
        );

        let unique = index
            .capture_shared_file_for_scope(&super::PathScope::prefix("tests/"))
            .expect("unique prefix should resolve");
        assert_eq!(unique.relative_path, "tests/lib_test.rs");

        let ambiguous = index.capture_shared_file_for_scope(&super::PathScope::prefix("src/"));
        assert!(ambiguous.is_none(), "ambiguous prefix should not resolve");

        let any = index.capture_shared_file_for_scope(&super::PathScope::any());
        assert!(any.is_none(), "Any scope should not guess a single file");
    }

    #[test]
    fn test_capture_what_changed_timestamp_view_sorts_paths() {
        let f1 = make_indexed_file("src/z.rs", vec![], ParseStatus::Parsed);
        let f2 = make_indexed_file("src/a.rs", vec![], ParseStatus::Parsed);
        let index = make_index(vec![("src/z.rs", f1), ("src/a.rs", f2)], false);

        let view = index.capture_what_changed_timestamp_view();

        assert!(view.loaded_secs >= 0);
        assert_eq!(
            view.paths,
            vec!["src/a.rs".to_string(), "src/z.rs".to_string()]
        );
    }

    #[test]
    fn test_capture_resolve_path_view_returns_exact_path_match() {
        let index = make_index(
            vec![(
                "src/protocol/tools.rs",
                make_indexed_file("src/protocol/tools.rs", vec![], ParseStatus::Parsed),
            )],
            false,
        );

        let view = index.capture_resolve_path_view("./src\\protocol\\tools.rs");

        assert_eq!(
            view,
            ResolvePathView::Resolved {
                path: "src/protocol/tools.rs".to_string()
            }
        );
    }

    #[test]
    fn test_capture_resolve_path_view_uses_basename_and_dir_component_narrowing() {
        let index = make_index(
            vec![
                (
                    "src/protocol/tools.rs",
                    make_indexed_file("src/protocol/tools.rs", vec![], ParseStatus::Parsed),
                ),
                (
                    "src/sidecar/tools.rs",
                    make_indexed_file("src/sidecar/tools.rs", vec![], ParseStatus::Parsed),
                ),
            ],
            false,
        );

        let view = index.capture_resolve_path_view("protocol/tools.rs");

        assert_eq!(
            view,
            ResolvePathView::Resolved {
                path: "src/protocol/tools.rs".to_string()
            }
        );
    }

    #[test]
    fn test_capture_resolve_path_view_falls_back_to_partial_path_match() {
        let index = make_index(
            vec![(
                "src/protocol/tools.rs",
                make_indexed_file("src/protocol/tools.rs", vec![], ParseStatus::Parsed),
            )],
            false,
        );

        let view = index.capture_resolve_path_view("protocol/tools");

        assert_eq!(
            view,
            ResolvePathView::Resolved {
                path: "src/protocol/tools.rs".to_string()
            }
        );
    }

    #[test]
    fn test_capture_resolve_path_view_returns_bounded_ambiguous_matches() {
        let index = make_index(
            vec![
                (
                    "src/lib.rs",
                    make_indexed_file("src/lib.rs", vec![], ParseStatus::Parsed),
                ),
                (
                    "tests/lib.rs",
                    make_indexed_file("tests/lib.rs", vec![], ParseStatus::Parsed),
                ),
            ],
            false,
        );

        let view = index.capture_resolve_path_view("lib.rs");

        assert_eq!(
            view,
            ResolvePathView::Ambiguous {
                hint: "lib.rs".to_string(),
                matches: vec!["src/lib.rs".to_string(), "tests/lib.rs".to_string()],
                overflow_count: 0,
            }
        );
    }

    #[test]
    fn test_capture_search_files_view_groups_tiers_and_caps_results() {
        let index = make_index(
            vec![
                (
                    "src/protocol/tools.rs",
                    make_indexed_file("src/protocol/tools.rs", vec![], ParseStatus::Parsed),
                ),
                (
                    "src/sidecar/tools.rs",
                    make_indexed_file("src/sidecar/tools.rs", vec![], ParseStatus::Parsed),
                ),
                (
                    "src/protocol/tools_helper.rs",
                    make_indexed_file("src/protocol/tools_helper.rs", vec![], ParseStatus::Parsed),
                ),
            ],
            false,
        );

        let view = index.capture_search_files_view("protocol/tools.rs", 2, None);

        assert_eq!(
            view,
            SearchFilesView::Found {
                query: "protocol/tools.rs".to_string(),
                total_matches: 2,
                overflow_count: 0,
                hits: vec![
                    SearchFilesHit {
                        tier: SearchFilesTier::StrongPath,
                        path: "src/protocol/tools.rs".to_string(),
                        coupling_score: None,
                        shared_commits: None,
                    },
                    SearchFilesHit {
                        tier: SearchFilesTier::Basename,
                        path: "src/sidecar/tools.rs".to_string(),
                        coupling_score: None,
                        shared_commits: None,
                    },
                ],
            }
        );
    }

    #[test]
    fn test_capture_search_files_view_returns_loose_path_matches_for_component_query() {
        let index = make_index(
            vec![
                (
                    "src/live_index/query.rs",
                    make_indexed_file("src/live_index/query.rs", vec![], ParseStatus::Parsed),
                ),
                (
                    "src/live_index/store.rs",
                    make_indexed_file("src/live_index/store.rs", vec![], ParseStatus::Parsed),
                ),
            ],
            false,
        );

        let view = index.capture_search_files_view("live_index", 20, None);

        assert_eq!(
            view,
            SearchFilesView::Found {
                query: "live_index".to_string(),
                total_matches: 2,
                overflow_count: 0,
                hits: vec![
                    SearchFilesHit {
                        tier: SearchFilesTier::LoosePath,
                        path: "src/live_index/query.rs".to_string(),
                        coupling_score: None,
                        shared_commits: None,
                    },
                    SearchFilesHit {
                        tier: SearchFilesTier::LoosePath,
                        path: "src/live_index/store.rs".to_string(),
                        coupling_score: None,
                        shared_commits: None,
                    },
                ],
            }
        );
    }

    #[test]
    fn test_capture_search_files_view_prefix_matching() {
        let index = make_index(
            vec![
                (
                    "src/orchestrator.rs",
                    make_indexed_file("src/orchestrator.rs", vec![], ParseStatus::Parsed),
                ),
                (
                    "src/orchestration.rs",
                    make_indexed_file("src/orchestration.rs", vec![], ParseStatus::Parsed),
                ),
                (
                    "src/other.rs",
                    make_indexed_file("src/other.rs", vec![], ParseStatus::Parsed),
                ),
            ],
            false,
        );

        // "orchestrat" should find both orchestrator.rs and orchestration.rs via prefix matching.
        let view = index.capture_search_files_view("orchestrat", 10, None);
        if let SearchFilesView::Found { hits, .. } = view {
            let paths: Vec<&str> = hits.iter().map(|h| h.path.as_str()).collect();
            assert!(
                paths.contains(&"src/orchestrator.rs"),
                "expected orchestrator.rs in results: {paths:?}"
            );
            assert!(
                paths.contains(&"src/orchestration.rs"),
                "expected orchestration.rs in results: {paths:?}"
            );
            assert!(
                !paths.contains(&"src/other.rs"),
                "unexpected other.rs in results: {paths:?}"
            );
        } else {
            panic!("expected found view for prefix query 'orchestrat'");
        }
    }

    #[test]
    fn test_capture_search_files_view_boosts_local_results() {
        let index = make_index(
            vec![
                (
                    "src/client/utils.rs",
                    make_indexed_file("src/client/utils.rs", vec![], ParseStatus::Parsed),
                ),
                (
                    "src/server/utils.rs",
                    make_indexed_file("src/server/utils.rs", vec![], ParseStatus::Parsed),
                ),
            ],
            false,
        );

        // When in server context, server utils should rank first.
        let view_server =
            index.capture_search_files_view("utils.rs", 10, Some("src/server/main.rs"));
        if let SearchFilesView::Found { hits, .. } = view_server {
            assert_eq!(hits[0].path, "src/server/utils.rs");
        } else {
            panic!("expected found view");
        }

        // When in client context, client utils should rank first.
        let view_client =
            index.capture_search_files_view("utils.rs", 10, Some("src/client/main.rs"));
        if let SearchFilesView::Found { hits, .. } = view_client {
            assert_eq!(hits[0].path, "src/client/utils.rs");
        } else {
            panic!("expected found view");
        }
    }

    #[test]
    fn test_capture_find_dependents_view_groups_files_and_lines() {
        let target = make_file_with_refs_and_content(
            "src/db.rs",
            LanguageId::Rust,
            "pub fn connect() {}\n",
            vec![],
            vec![SymbolRecord {
                name: "connect".to_string(),
                kind: SymbolKind::Function,
                depth: 0,
                sort_order: 0,
                byte_range: (0, 18),
                line_range: (0, 0),
                doc_byte_range: None,
            }],
        );
        let importer = make_file_with_refs_and_content(
            "src/app.rs",
            LanguageId::Rust,
            "use crate::db;\nfn run() { connect(); }\n",
            vec![
                make_ref("db", Some("crate::db"), ReferenceKind::Import, None, 0),
                make_ref("connect", None, ReferenceKind::Call, Some(0), 100),
            ],
            vec![SymbolRecord {
                name: "run".to_string(),
                kind: SymbolKind::Function,
                depth: 0,
                sort_order: 0,
                byte_range: (15, 38),
                line_range: (1, 1),
                doc_byte_range: None,
            }],
        );
        let index = make_index(vec![("src/db.rs", target), ("src/app.rs", importer)], false);

        let view = index.capture_find_dependents_view("src/db.rs");

        assert_eq!(view.files.len(), 1);
        assert_eq!(view.files[0].file_path, "src/app.rs");
        assert_eq!(view.files[0].lines[0].line_number, 2);
        assert!(view.files[0].lines[0].line_content.contains("connect"));
    }

    #[test]
    fn test_capture_find_references_view_groups_context_lines() {
        let target = make_file_with_refs_and_content(
            "src/lib.rs",
            LanguageId::Rust,
            "fn process() {}\n",
            vec![],
            vec![SymbolRecord {
                name: "process".to_string(),
                kind: SymbolKind::Function,
                depth: 0,
                sort_order: 0,
                byte_range: (0, 14),
                line_range: (0, 0),
                doc_byte_range: None,
            }],
        );
        let caller = make_file_with_refs_and_content(
            "src/app.rs",
            LanguageId::Rust,
            "fn run() {\n    process();\n}\n",
            vec![make_ref("process", None, ReferenceKind::Call, Some(0), 100)],
            vec![SymbolRecord {
                name: "run".to_string(),
                kind: SymbolKind::Function,
                depth: 0,
                sort_order: 0,
                byte_range: (0, 28),
                line_range: (0, 2),
                doc_byte_range: None,
            }],
        );
        let index = make_index(vec![("src/lib.rs", target), ("src/app.rs", caller)], false);

        let view = index.capture_find_references_view("process", Some("call"), 200);

        assert_eq!(view.total_refs, 1);
        assert_eq!(view.files.len(), 1);
        assert_eq!(view.files[0].file_path, "src/app.rs");
        assert_eq!(view.files[0].hits[0].context_lines[1].line_number, 2);
        assert!(
            view.files[0].hits[0].context_lines[1]
                .text
                .contains("process")
        );
        assert!(
            view.files[0].hits[0].context_lines[1]
                .enclosing_annotation
                .as_deref()
                .unwrap_or("")
                .contains("run")
        );
    }

    #[test]
    fn test_capture_context_bundle_view_collects_owned_sections() {
        let target = make_file_with_refs_and_content(
            "src/db.rs",
            LanguageId::Rust,
            "fn process() {\n    helper();\n}\nfn helper() {}\n",
            vec![make_ref("helper", None, ReferenceKind::Call, Some(0), 100)],
            vec![
                SymbolRecord {
                    name: "process".to_string(),
                    kind: SymbolKind::Function,
                    depth: 0,
                    sort_order: 0,
                    byte_range: (0, 30),
                    line_range: (0, 2),
                    doc_byte_range: None,
                },
                SymbolRecord {
                    name: "helper".to_string(),
                    kind: SymbolKind::Function,
                    depth: 0,
                    sort_order: 1,
                    byte_range: (31, 45),
                    line_range: (3, 3),
                    doc_byte_range: None,
                },
            ],
        );
        let caller = make_file_with_refs_and_content(
            "src/app.rs",
            LanguageId::Rust,
            "use crate::db::process;\nfn run() {\n    process();\n    let value: process;\n}\n",
            vec![
                make_ref("db", Some("crate::db"), ReferenceKind::Import, Some(0), 0),
                make_ref(
                    "process",
                    Some("crate::db::process"),
                    ReferenceKind::Call,
                    Some(0),
                    100,
                ),
                make_ref(
                    "process",
                    Some("crate::db::process"),
                    ReferenceKind::TypeUsage,
                    Some(0),
                    200,
                ),
            ],
            vec![SymbolRecord {
                name: "run".to_string(),
                kind: SymbolKind::Function,
                depth: 0,
                sort_order: 0,
                byte_range: (0, 52),
                line_range: (0, 3),
                doc_byte_range: None,
            }],
        );
        let index = make_index(vec![("src/db.rs", target), ("src/app.rs", caller)], false);

        let view = index.capture_context_bundle_view("src/db.rs", "process", None, None);

        let super::ContextBundleView::Found(found) = view else {
            panic!("expected found context bundle view");
        };

        assert!(found.body.contains("fn process"));
        assert_eq!(found.kind_label, "fn");
        assert_eq!(found.callers.total_count, 1);
        assert_eq!(found.callers.entries[0].file_path, "src/app.rs");
        assert_eq!(found.callers.entries[0].line_number, 2);
        assert!(
            found.callers.entries[0]
                .enclosing
                .as_deref()
                .unwrap_or("")
                .contains("run")
        );
        assert_eq!(found.callees.total_count, 1);
        assert_eq!(found.callees.entries[0].display_name, "helper");
        assert_eq!(found.type_usages.total_count, 1);
    }

    #[test]
    fn test_capture_context_bundle_view_requires_line_for_ambiguous_selector() {
        let content = "fn connect() { first(); }\nfn connect() { second(); }\n";
        let first_body = "fn connect() { first(); }";
        let second_body = "fn connect() { second(); }";
        let second_start = content.find(second_body).unwrap() as u32;
        let target = make_file_with_refs_and_content(
            "src/db.rs",
            LanguageId::Rust,
            content,
            vec![],
            vec![
                make_symbol_with_kind_line_and_bytes(
                    "connect",
                    SymbolKind::Function,
                    1,
                    (0, first_body.len() as u32),
                ),
                make_symbol_with_kind_line_and_bytes(
                    "connect",
                    SymbolKind::Function,
                    2,
                    (second_start, second_start + second_body.len() as u32),
                ),
            ],
        );
        let index = make_index(vec![("src/db.rs", target)], false);

        let view = index.capture_context_bundle_view("src/db.rs", "connect", Some("fn"), None);

        match view {
            ContextBundleView::AmbiguousSymbol {
                path,
                name,
                candidate_lines,
            } => {
                assert_eq!(path, "src/db.rs");
                assert_eq!(name, "connect");
                assert_eq!(candidate_lines, vec![1, 2]);
            }
            _ => panic!("expected ambiguous selector"),
        }
    }

    #[test]
    fn test_capture_context_bundle_view_uses_symbol_line_and_exact_callers() {
        let content = "fn connect() { first(); }\nfn connect() { second(); }\n";
        let first_body = "fn connect() { first(); }";
        let second_body = "fn connect() { second(); }";
        let second_start = content.find(second_body).unwrap() as u32;
        let target = make_file_with_refs_and_content(
            "src/db.rs",
            LanguageId::Rust,
            content,
            vec![],
            vec![
                make_symbol_with_kind_line_and_bytes(
                    "connect",
                    SymbolKind::Function,
                    1,
                    (0, first_body.len() as u32),
                ),
                make_symbol_with_kind_line_and_bytes(
                    "connect",
                    SymbolKind::Function,
                    2,
                    (second_start, second_start + second_body.len() as u32),
                ),
            ],
        );
        let dependent = make_file_with_refs_and_content(
            "src/service.rs",
            LanguageId::Rust,
            "use crate::db::connect;\nfn run() { connect(); }\n",
            vec![
                make_ref("db", Some("crate::db"), ReferenceKind::Import, Some(0), 0),
                make_ref(
                    "connect",
                    Some("crate::db::connect"),
                    ReferenceKind::Call,
                    Some(0),
                    100,
                ),
            ],
            vec![make_symbol("run")],
        );
        let unrelated = make_file_with_refs_and_content(
            "src/other.rs",
            LanguageId::Rust,
            "fn run() { connect(); }\n",
            vec![make_ref("connect", None, ReferenceKind::Call, Some(0), 0)],
            vec![make_symbol("run")],
        );
        let index = make_index(
            vec![
                ("src/db.rs", target),
                ("src/service.rs", dependent),
                ("src/other.rs", unrelated),
            ],
            false,
        );

        let view = index.capture_context_bundle_view("src/db.rs", "connect", Some("fn"), Some(2));

        let ContextBundleView::Found(found) = view else {
            panic!("expected found view");
        };

        assert!(found.body.contains("second();"), "got: {}", found.body);
        assert!(!found.body.contains("first();"), "got: {}", found.body);
        assert_eq!(found.callers.total_count, 1);
        assert_eq!(found.callers.entries.len(), 1);
        assert_eq!(found.callers.entries[0].file_path, "src/service.rs");
    }

    #[test]
    fn test_capture_context_bundle_view_exact_type_usages_exclude_unrelated_same_name_hits() {
        let target = make_file_with_refs_and_content(
            "src/db.rs",
            LanguageId::Rust,
            "pub struct Client;\npub struct Client;\n",
            vec![],
            vec![
                make_symbol_with_kind_line_and_bytes("Client", SymbolKind::Struct, 1, (0, 18)),
                make_symbol_with_kind_line_and_bytes("Client", SymbolKind::Struct, 2, (19, 37)),
            ],
        );
        let dependent = make_file_with_refs_and_content(
            "src/service.rs",
            LanguageId::Rust,
            "use crate::db::Client;\nstruct Holder { client: Client }\n",
            vec![
                make_ref("db", Some("crate::db"), ReferenceKind::Import, Some(0), 0),
                make_ref(
                    "Client",
                    Some("crate::db::Client"),
                    ReferenceKind::TypeUsage,
                    Some(0),
                    100,
                ),
            ],
            vec![make_symbol_with_kind_and_line(
                "Holder",
                SymbolKind::Struct,
                2,
            )],
        );
        let unrelated = make_file_with_refs_and_content(
            "src/other.rs",
            LanguageId::Rust,
            "struct Holder { client: Client }\n",
            vec![make_ref(
                "Client",
                None,
                ReferenceKind::TypeUsage,
                Some(0),
                0,
            )],
            vec![make_symbol_with_kind_and_line(
                "Holder",
                SymbolKind::Struct,
                1,
            )],
        );
        let index = make_index(
            vec![
                ("src/db.rs", target),
                ("src/service.rs", dependent),
                ("src/other.rs", unrelated),
            ],
            false,
        );

        let view =
            index.capture_context_bundle_view("src/db.rs", "Client", Some("struct"), Some(2));

        let ContextBundleView::Found(found) = view else {
            panic!("expected found view");
        };

        assert_eq!(found.type_usages.total_count, 1);
        assert_eq!(found.type_usages.entries.len(), 1);
        assert_eq!(found.type_usages.entries[0].file_path, "src/service.rs");
    }

    #[test]
    fn test_find_files_by_basename_returns_sorted_paths() {
        let f1 = make_indexed_file("src/lib.rs", vec![], ParseStatus::Parsed);
        let f2 = make_indexed_file("tests/lib.rs", vec![], ParseStatus::Parsed);
        let f3 = make_indexed_file("src/main.rs", vec![], ParseStatus::Parsed);
        let index = make_index(
            vec![
                ("tests/lib.rs", f2),
                ("src/main.rs", f3),
                ("src/lib.rs", f1),
            ],
            false,
        );

        assert_eq!(
            index.find_files_by_basename("LIB.RS"),
            vec!["src/lib.rs", "tests/lib.rs"]
        );
    }

    #[test]
    fn test_find_files_by_dir_component_returns_sorted_paths() {
        let f1 = make_indexed_file("src/live_index/mod.rs", vec![], ParseStatus::Parsed);
        let f2 = make_indexed_file("src/live_index/store.rs", vec![], ParseStatus::Parsed);
        let f3 = make_indexed_file("tests/store.rs", vec![], ParseStatus::Parsed);
        let index = make_index(
            vec![
                ("src/live_index/store.rs", f2),
                ("tests/store.rs", f3),
                ("src/live_index/mod.rs", f1),
            ],
            false,
        );

        assert_eq!(
            index.find_files_by_dir_component("live_index"),
            vec!["src/live_index/mod.rs", "src/live_index/store.rs"]
        );
        assert_eq!(
            index.find_files_by_dir_component("SRC"),
            vec!["src/live_index/mod.rs", "src/live_index/store.rs"]
        );
    }

    #[test]
    fn test_file_count_correct() {
        let f1 = make_indexed_file("a.rs", vec![], ParseStatus::Parsed);
        let f2 = make_indexed_file("b.rs", vec![], ParseStatus::Parsed);
        let f3 = make_indexed_file("c.rs", vec![], ParseStatus::Parsed);
        let index = make_index(vec![("a.rs", f1), ("b.rs", f2), ("c.rs", f3)], false);
        assert_eq!(index.file_count(), 3);
    }

    #[test]
    fn test_symbol_count_across_all_files() {
        let f1 = make_indexed_file(
            "a.rs",
            vec![make_symbol("x"), make_symbol("y")],
            ParseStatus::Parsed,
        );
        let f2 = make_indexed_file("b.rs", vec![make_symbol("z")], ParseStatus::Parsed);
        let index = make_index(vec![("a.rs", f1), ("b.rs", f2)], false);
        assert_eq!(index.symbol_count(), 3);
    }

    #[test]
    fn test_health_stats_correct_breakdown() {
        let f1 = make_indexed_file("a.rs", vec![make_symbol("x")], ParseStatus::Parsed);
        let f2 = make_indexed_file(
            "b.rs",
            vec![make_symbol("y")],
            ParseStatus::PartialParse {
                warning: "syntax err".to_string(),
            },
        );
        let f3 = make_indexed_file(
            "c.rs",
            vec![],
            ParseStatus::Failed {
                error: "failed".to_string(),
            },
        );
        let index = make_index(vec![("a.rs", f1), ("b.rs", f2), ("c.rs", f3)], false);

        let stats = index.health_stats();
        assert_eq!(stats.file_count, 3);
        assert_eq!(stats.symbol_count, 2);
        assert_eq!(stats.parsed_count, 1);
        assert_eq!(stats.partial_parse_count, 1);
        assert_eq!(stats.failed_count, 1);
    }

    #[test]
    fn test_is_ready_true_when_not_tripped() {
        let index = make_index(vec![], false);
        assert!(index.is_ready());
    }

    #[test]
    fn test_is_ready_false_when_tripped() {
        // Build a tripped circuit breaker by direct manipulation
        let cb = CircuitBreakerState::new(0.20);
        for _ in 0..7 {
            cb.record_success();
        }
        for i in 0..3 {
            cb.record_failure(&format!("f{i}.rs"), "err");
        }
        cb.should_abort(); // This will trip it

        let index = LiveIndex {
            files: HashMap::new(),
            loaded_at: Instant::now(),
            loaded_at_system: std::time::SystemTime::now(),
            load_duration: Duration::from_millis(10),
            cb_state: cb,
            is_empty: false,
            load_source: crate::live_index::store::IndexLoadSource::FreshLoad,
            snapshot_verify_state: crate::live_index::store::SnapshotVerifyState::NotNeeded,
            reverse_index: std::collections::HashMap::new(),
            files_by_basename: std::collections::HashMap::new(),
            files_by_dir_component: std::collections::HashMap::new(),
            trigram_index: crate::live_index::trigram::TrigramIndex::new(),
        };
        assert!(!index.is_ready());
    }

    #[test]
    fn test_index_state_ready() {
        let index = make_index(vec![], false);
        assert_eq!(index.index_state(), IndexState::Ready);
    }

    #[test]
    fn test_index_state_circuit_breaker_tripped_with_summary() {
        let cb = CircuitBreakerState::new(0.20);
        for _ in 0..7 {
            cb.record_success();
        }
        for i in 0..3 {
            cb.record_failure(&format!("f{i}.rs"), "err");
        }
        cb.should_abort();

        let index = LiveIndex {
            files: HashMap::new(),
            loaded_at: Instant::now(),
            loaded_at_system: std::time::SystemTime::now(),
            load_duration: Duration::from_millis(10),
            cb_state: cb,
            is_empty: false,
            load_source: crate::live_index::store::IndexLoadSource::FreshLoad,
            snapshot_verify_state: crate::live_index::store::SnapshotVerifyState::NotNeeded,
            reverse_index: std::collections::HashMap::new(),
            files_by_basename: std::collections::HashMap::new(),
            files_by_dir_component: std::collections::HashMap::new(),
            trigram_index: crate::live_index::trigram::TrigramIndex::new(),
        };

        match index.index_state() {
            IndexState::CircuitBreakerTripped { summary } => {
                assert!(!summary.is_empty(), "summary should not be empty");
            }
            other => panic!("expected CircuitBreakerTripped, got {:?}", other),
        }
    }

    // --- Extended HealthStats with watcher fields ---

    #[test]
    fn test_health_stats_default_watcher_fields() {
        let index = make_index(vec![], false);
        let stats = index.health_stats();
        assert_eq!(
            stats.watcher_state,
            WatcherState::Off,
            "default watcher state should be Off"
        );
        assert_eq!(
            stats.events_processed, 0,
            "default events_processed should be 0"
        );
        assert!(
            stats.last_event_at.is_none(),
            "default last_event_at should be None"
        );
        assert_eq!(
            stats.debounce_window_ms, 200,
            "default debounce_window_ms should be 200"
        );
    }

    #[test]
    fn test_health_stats_with_watcher_active() {
        let index = make_index(vec![], false);
        let now = SystemTime::now();
        let watcher = WatcherInfo {
            state: WatcherState::Active,
            events_processed: 42,
            last_event_at: Some(now),
            debounce_window_ms: 500,
        };
        let stats = index.health_stats_with_watcher(&watcher);
        assert_eq!(stats.watcher_state, WatcherState::Active);
        assert_eq!(stats.events_processed, 42);
        assert_eq!(stats.last_event_at, Some(now));
        assert_eq!(stats.debounce_window_ms, 500);
    }

    // -----------------------------------------------------------------------
    // Cross-reference query tests (Task 1, Plan 04-02)
    // -----------------------------------------------------------------------

    // --- find_references_for_name: basic ---

    #[test]
    fn test_find_references_for_name_returns_all_matching() {
        // "foo" referenced in two files — both should be returned.
        let refs_a = vec![make_ref("foo", None, ReferenceKind::Call, None, 0)];
        let refs_b = vec![make_ref("foo", None, ReferenceKind::Call, None, 0)];
        let f_a = make_file_with_refs("a.rs", refs_a, HashMap::new());
        let f_b = make_file_with_refs("b.rs", refs_b, HashMap::new());
        let index = make_index(vec![("a.rs", f_a), ("b.rs", f_b)], false);

        let results = index.find_references_for_name("foo", None, false);
        assert_eq!(results.len(), 2, "both files should match");
    }

    #[test]
    fn test_find_references_for_name_kind_filter_call_only() {
        // Two references to "foo" in same file: one Call, one Import. Kind filter returns only Call.
        let refs = vec![
            make_ref("foo", None, ReferenceKind::Call, None, 0),
            make_ref("foo", None, ReferenceKind::Import, None, 100),
        ];
        let f = make_file_with_refs("a.rs", refs, HashMap::new());
        let index = make_index(vec![("a.rs", f)], false);

        let results = index.find_references_for_name("foo", Some(ReferenceKind::Call), false);
        assert_eq!(results.len(), 1, "only Call reference should be returned");
        assert_eq!(results[0].1.kind, ReferenceKind::Call);
    }

    #[test]
    fn test_find_references_for_name_kind_filter_excludes_import() {
        let refs = vec![make_ref("foo", None, ReferenceKind::Import, None, 0)];
        let f = make_file_with_refs("a.rs", refs, HashMap::new());
        let index = make_index(vec![("a.rs", f)], false);

        let results = index.find_references_for_name("foo", Some(ReferenceKind::Call), false);
        assert!(
            results.is_empty(),
            "Import reference should be excluded when filtering for Call"
        );
    }

    // --- Built-in filter (XREF-04 / XREF-06) ---

    #[test]
    fn test_find_references_builtin_string_filtered() {
        // "string" is a JS/TS built-in — should be filtered.
        let refs = vec![make_ref("string", None, ReferenceKind::TypeUsage, None, 0)];
        let f = make_file_with_refs("a.ts", refs, HashMap::new());
        let index = make_index(vec![("a.ts", f)], false);

        let results = index.find_references_for_name("string", None, false);
        assert!(
            results.is_empty(),
            "built-in 'string' should be filtered by default"
        );
    }

    #[test]
    fn test_find_references_builtin_i32_filtered() {
        let refs = vec![make_ref("i32", None, ReferenceKind::TypeUsage, None, 0)];
        let f = make_file_with_refs("a.rs", refs, HashMap::new());
        let index = make_index(vec![("a.rs", f)], false);

        let results = index.find_references_for_name("i32", None, false);
        assert!(results.is_empty(), "Rust built-in 'i32' should be filtered");
    }

    #[test]
    fn test_find_references_mystruct_not_filtered() {
        let refs = vec![make_ref(
            "MyStruct",
            None,
            ReferenceKind::TypeUsage,
            None,
            0,
        )];
        let f = make_file_with_refs("a.rs", refs, HashMap::new());
        let index = make_index(vec![("a.rs", f)], false);

        let results = index.find_references_for_name("MyStruct", None, false);
        assert_eq!(
            results.len(),
            1,
            "user-defined type 'MyStruct' should NOT be filtered"
        );
    }

    #[test]
    fn test_find_references_builtin_include_filtered_bypasses() {
        // include_filtered=true should return even built-in matches.
        let refs = vec![make_ref("i32", None, ReferenceKind::TypeUsage, None, 0)];
        let f = make_file_with_refs("a.rs", refs, HashMap::new());
        let index = make_index(vec![("a.rs", f)], false);

        let results = index.find_references_for_name("i32", None, true);
        assert_eq!(
            results.len(),
            1,
            "include_filtered=true should bypass the filter"
        );
    }

    // --- Generic filter ---

    #[test]
    fn test_find_references_single_letter_t_filtered() {
        let refs = vec![make_ref("T", None, ReferenceKind::TypeUsage, None, 0)];
        let f = make_file_with_refs("a.rs", refs, HashMap::new());
        let index = make_index(vec![("a.rs", f)], false);

        let results = index.find_references_for_name("T", None, false);
        assert!(
            results.is_empty(),
            "single-letter generic 'T' should be filtered"
        );
    }

    #[test]
    fn test_find_references_single_letter_k_filtered() {
        let refs = vec![make_ref("K", None, ReferenceKind::TypeUsage, None, 0)];
        let f = make_file_with_refs("a.rs", refs, HashMap::new());
        let index = make_index(vec![("a.rs", f)], false);

        let results = index.find_references_for_name("K", None, false);
        assert!(
            results.is_empty(),
            "single-letter generic 'K' should be filtered"
        );
    }

    #[test]
    fn test_find_references_multi_letter_key_not_filtered() {
        let refs = vec![make_ref("Key", None, ReferenceKind::TypeUsage, None, 0)];
        let f = make_file_with_refs("a.rs", refs, HashMap::new());
        let index = make_index(vec![("a.rs", f)], false);

        let results = index.find_references_for_name("Key", None, false);
        assert_eq!(
            results.len(),
            1,
            "multi-letter name 'Key' should NOT be filtered"
        );
    }

    // --- Alias resolution (XREF-05) ---

    #[test]
    fn test_find_references_alias_resolution_hashmap_via_map() {
        // File b.rs has a reference to "Map" with alias_map["Map"] = "HashMap".
        // Searching for "HashMap" should also return the "Map" reference.
        let mut alias_map = HashMap::new();
        alias_map.insert("Map".to_string(), "HashMap".to_string());

        let refs_b = vec![make_ref("Map", None, ReferenceKind::Call, None, 0)];
        let f_a = make_file_with_refs("a.rs", vec![], HashMap::new()); // no refs
        let f_b = make_file_with_refs("b.rs", refs_b, alias_map);
        let index = make_index(vec![("a.rs", f_a), ("b.rs", f_b)], false);

        let results = index.find_references_for_name("HashMap", None, false);
        // Should find the "Map" reference from b.rs via alias resolution
        assert!(
            !results.is_empty(),
            "alias resolution should find 'Map' when searching 'HashMap'"
        );
        assert_eq!(results[0].1.name, "Map");
    }

    // --- Qualified name matching ---

    #[test]
    fn test_find_references_qualified_name_vec_new() {
        let refs = vec![make_ref(
            "new",
            Some("Vec::new"),
            ReferenceKind::Call,
            None,
            0,
        )];
        let f = make_file_with_refs("a.rs", refs, HashMap::new());
        let index = make_index(vec![("a.rs", f)], false);

        // Qualified search: "Vec::new" matches against qualified_name field.
        let results = index.find_references_for_name("Vec::new", None, false);
        assert_eq!(
            results.len(),
            1,
            "qualified 'Vec::new' should match via qualified_name field"
        );
    }

    #[test]
    fn test_find_references_qualified_does_not_match_unqualified() {
        // "new" (simple) should not match when searching for qualified "Vec::new".
        let refs = vec![make_ref("new", None, ReferenceKind::Call, None, 0)]; // no qualified_name
        let f = make_file_with_refs("a.rs", refs, HashMap::new());
        let index = make_index(vec![("a.rs", f)], false);

        let results = index.find_references_for_name("Vec::new", None, false);
        assert!(
            results.is_empty(),
            "qualified search should not match reference without qualified_name"
        );
    }

    // --- Result fields ---

    #[test]
    fn test_find_references_result_includes_correct_file_path_and_record() {
        let refs = vec![make_ref("load", None, ReferenceKind::Call, None, 0)];
        let f = make_file_with_refs("src/loader.rs", refs, HashMap::new());
        let index = make_index(vec![("src/loader.rs", f)], false);

        let results = index.find_references_for_name("load", None, false);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].0, "src/loader.rs", "file_path should match");
        assert_eq!(results[0].1.name, "load");
    }

    #[test]
    fn test_capture_find_references_view_for_symbol_scopes_to_dependent_files() {
        let target = make_file_with_refs_and_content(
            "src/db.rs",
            LanguageId::Rust,
            "pub fn connect() {}\n",
            vec![],
            vec![make_symbol_with_kind_and_line(
                "connect",
                SymbolKind::Function,
                1,
            )],
        );
        let dependent = make_file_with_refs_and_content(
            "src/service.rs",
            LanguageId::Rust,
            "use crate::db::connect;\nfn run() { connect(); }\n",
            vec![
                make_ref("db", Some("crate::db"), ReferenceKind::Import, None, 0),
                make_ref(
                    "connect",
                    Some("crate::db::connect"),
                    ReferenceKind::Call,
                    None,
                    100,
                ),
            ],
            vec![make_symbol("run")],
        );
        let unrelated = make_file_with_refs_and_content(
            "src/other.rs",
            LanguageId::Rust,
            "fn run() { connect(); }\n",
            vec![make_ref("connect", None, ReferenceKind::Call, None, 0)],
            vec![make_symbol("run")],
        );
        let index = make_index(
            vec![
                ("src/db.rs", target),
                ("src/service.rs", dependent),
                ("src/other.rs", unrelated),
            ],
            false,
        );

        let view = index
            .capture_find_references_view_for_symbol(
                "src/db.rs",
                "connect",
                Some("fn"),
                Some(1),
                Some("call"),
                200,
            )
            .expect("exact selector should resolve");

        assert_eq!(view.total_refs, 1);
        assert_eq!(view.files.len(), 1);
        assert_eq!(view.files[0].file_path, "src/service.rs");
    }

    #[test]
    fn test_capture_find_references_view_for_symbol_requires_line_for_ambiguous_selector() {
        let target = make_file_with_refs_and_content(
            "src/db.rs",
            LanguageId::Rust,
            "fn connect() {}\nfn connect() {}\n",
            vec![],
            vec![
                make_symbol_with_kind_and_line("connect", SymbolKind::Function, 1),
                make_symbol_with_kind_and_line("connect", SymbolKind::Function, 10),
            ],
        );
        let index = make_index(vec![("src/db.rs", target)], false);

        let error = index
            .capture_find_references_view_for_symbol(
                "src/db.rs",
                "connect",
                Some("fn"),
                None,
                Some("call"),
                200,
            )
            .expect_err("selector without line should be ambiguous");

        assert!(error.contains("Ambiguous symbol selector"), "got: {error}");
        assert!(error.contains("1"), "got: {error}");
        assert!(error.contains("10"), "got: {error}");
    }

    // --- find_dependents_for_file ---

    #[test]
    fn test_find_dependents_for_file_returns_importer() {
        // b.rs imports "db" — should be a dependent of src/db.rs.
        let import_ref = make_ref("db", None, ReferenceKind::Import, None, 0);
        let f_b = make_file_with_refs("src/b.rs", vec![import_ref], HashMap::new());
        let f_db = make_file_with_refs("src/db.rs", vec![], HashMap::new());
        let index = make_index(vec![("src/b.rs", f_b), ("src/db.rs", f_db)], false);

        let deps = index.find_dependents_for_file("src/db.rs");
        assert_eq!(
            deps.len(),
            1,
            "b.rs imports 'db' so it is a dependent of db.rs"
        );
        assert_eq!(deps[0].0, "src/b.rs");
    }

    #[test]
    fn test_find_dependents_no_importers_returns_empty() {
        let f = make_file_with_refs("src/db.rs", vec![], HashMap::new());
        let index = make_index(vec![("src/db.rs", f)], false);

        let deps = index.find_dependents_for_file("src/db.rs");
        assert!(deps.is_empty(), "no importers means empty dependents list");
    }

    #[test]
    fn test_find_dependents_excludes_self() {
        // A file that imports its own stem should not appear as its own dependent.
        let self_import = make_ref("db", None, ReferenceKind::Import, None, 0);
        let f_db = make_file_with_refs("src/db.rs", vec![self_import], HashMap::new());
        let index = make_index(vec![("src/db.rs", f_db)], false);

        let deps = index.find_dependents_for_file("src/db.rs");
        assert!(deps.is_empty(), "a file should not be its own dependent");
    }

    #[test]
    fn test_find_dependents_qualified_import_crate_db() {
        // b.rs has import "crate::db" — should match src/db.rs.
        let import_ref = make_ref("crate::db", None, ReferenceKind::Import, None, 0);
        let f_b = make_file_with_refs("src/b.rs", vec![import_ref], HashMap::new());
        let f_db = make_file_with_refs("src/db.rs", vec![], HashMap::new());
        let index = make_index(vec![("src/b.rs", f_b), ("src/db.rs", f_db)], false);

        let deps = index.find_dependents_for_file("src/db.rs");
        assert_eq!(
            deps.len(),
            1,
            "qualified 'crate::db' should match src/db.rs"
        );
    }

    #[test]
    fn test_find_dependents_workspace_crate_qualified_import() {
        // Workspace layout: crates/core/src/types.rs defines types,
        // crates/api/src/handler.rs imports "crate::types".
        // The module path for "crates/core/src/types.rs" should resolve to
        // "crate::types", matching the import in handler.rs.
        let import_ref = make_ref("crate::types", None, ReferenceKind::Import, None, 0);
        let type_usage = make_ref("MyType", None, ReferenceKind::TypeUsage, None, 5);
        let f_handler = make_file_with_refs(
            "crates/api/src/handler.rs",
            vec![import_ref, type_usage],
            HashMap::new(),
        );
        let my_type_sym = SymbolRecord {
            name: "MyType".to_string(),
            kind: SymbolKind::Struct,
            depth: 0,
            sort_order: 0,
            byte_range: (0, 30),
            line_range: (0, 1),
            doc_byte_range: None,
        };
        let mut f_types = make_file_with_refs("crates/core/src/types.rs", vec![], HashMap::new());
        f_types.symbols.push(my_type_sym);
        let index = make_index(
            vec![
                ("crates/api/src/handler.rs", f_handler),
                ("crates/core/src/types.rs", f_types),
            ],
            false,
        );

        let deps = index.find_dependents_for_file("crates/core/src/types.rs");
        assert!(
            !deps.is_empty(),
            "workspace crate types.rs should have dependents, got: {:?}",
            deps.iter().map(|(p, _)| p).collect::<Vec<_>>()
        );
    }

    // --- find_dependents: module path resolution ---

    #[test]
    fn test_find_dependents_lib_rs_via_module_path() {
        // main.rs imports "crate::error" — lib.rs is the crate root, so it's a dependent.
        let import_ref = make_ref("crate::error", None, ReferenceKind::Import, None, 0);
        let f_main = make_file_with_refs("src/main.rs", vec![import_ref], HashMap::new());
        let f_lib = make_file_with_refs("src/lib.rs", vec![], HashMap::new());
        let index = make_index(vec![("src/main.rs", f_main), ("src/lib.rs", f_lib)], false);

        let deps = index.find_dependents_for_file("src/lib.rs");
        assert_eq!(
            deps.len(),
            1,
            "crate::error starts with 'crate' module path of lib.rs"
        );
        assert_eq!(deps[0].0, "src/main.rs");
    }

    #[test]
    fn test_find_dependents_mod_rs_via_module_path() {
        // store.rs imports "crate::live_index" — mod.rs defines that module.
        let import_ref = make_ref(
            "crate::live_index::store",
            None,
            ReferenceKind::Import,
            None,
            0,
        );
        let f_store = make_file_with_refs("src/store.rs", vec![import_ref], HashMap::new());
        let f_mod = make_file_with_refs("src/live_index/mod.rs", vec![], HashMap::new());
        let index = make_index(
            vec![("src/store.rs", f_store), ("src/live_index/mod.rs", f_mod)],
            false,
        );

        let deps = index.find_dependents_for_file("src/live_index/mod.rs");
        assert_eq!(
            deps.len(),
            1,
            "'crate::live_index::store' starts with module path 'crate::live_index'"
        );
        assert_eq!(deps[0].0, "src/store.rs");
    }

    #[test]
    fn test_find_dependents_rust_returns_symbol_usage_when_module_import_matches() {
        let target = make_file_with_refs_and_content(
            "src/daemon.rs",
            LanguageId::Rust,
            "pub fn connect_or_spawn_session() {}",
            vec![],
            vec![make_symbol("connect_or_spawn_session")],
        );
        let dependent = make_file_with_refs_and_content(
            "src/main.rs",
            LanguageId::Rust,
            "use crate::{daemon, other}; fn main() { daemon::connect_or_spawn_session(); }",
            vec![
                make_ref(
                    "daemon",
                    Some("crate::daemon"),
                    ReferenceKind::Import,
                    None,
                    0,
                ),
                make_ref(
                    "connect_or_spawn_session",
                    Some("daemon::connect_or_spawn_session"),
                    ReferenceKind::Call,
                    Some(0),
                    100,
                ),
            ],
            vec![make_symbol("main")],
        );
        let index = make_index(
            vec![("src/daemon.rs", target), ("src/main.rs", dependent)],
            false,
        );

        let deps = index.find_dependents_for_file("src/daemon.rs");
        assert_eq!(
            deps.len(),
            1,
            "matched Rust module imports should surface actual symbol usage, not just import stubs"
        );
        assert_eq!(deps[0].0, "src/main.rs");
        assert_eq!(deps[0].1.kind, ReferenceKind::Call);
        assert_eq!(deps[0].1.name, "connect_or_spawn_session");
    }

    #[test]
    fn test_find_dependents_python_init_via_module_path() {
        // app.py imports "utils.helpers" — __init__.py defines the utils package.
        let import_ref = make_ref("utils.helpers", None, ReferenceKind::Import, None, 0);
        let mut f_app = make_file_with_refs("src/app.py", vec![import_ref], HashMap::new());
        f_app.language = LanguageId::Python;
        let mut f_init = make_file_with_refs("utils/__init__.py", vec![], HashMap::new());
        f_init.language = LanguageId::Python;
        let index = make_index(
            vec![("src/app.py", f_app), ("utils/__init__.py", f_init)],
            false,
        );

        let deps = index.find_dependents_for_file("utils/__init__.py");
        assert_eq!(
            deps.len(),
            1,
            "'utils.helpers' starts with module path 'utils'"
        );
    }

    #[test]
    fn test_find_dependents_js_index_via_module_path() {
        // app.js imports "src/utils" — index.ts defines that directory module.
        let import_ref = make_ref("src/utils/foo", None, ReferenceKind::Import, None, 0);
        let mut f_app = make_file_with_refs("src/app.js", vec![import_ref], HashMap::new());
        f_app.language = LanguageId::JavaScript;
        let mut f_index = make_file_with_refs("src/utils/index.js", vec![], HashMap::new());
        f_index.language = LanguageId::JavaScript;
        let index = make_index(
            vec![("src/app.js", f_app), ("src/utils/index.js", f_index)],
            false,
        );

        let deps = index.find_dependents_for_file("src/utils/index.js");
        assert_eq!(
            deps.len(),
            1,
            "'src/utils/foo' starts with module path 'src/utils'"
        );
    }

    #[test]
    fn test_find_dependents_csharp_type_usage_with_imported_namespace() {
        let target = make_file_with_refs_and_content(
            "Core/Services/IMinioService.cs",
            LanguageId::CSharp,
            "namespace CeRegistry.Core.Services { public interface IMinioService {} }",
            vec![],
            vec![SymbolRecord {
                name: "IMinioService".to_string(),
                kind: SymbolKind::Interface,
                depth: 0,
                sort_order: 0,
                byte_range: (0, 10),
                line_range: (0, 0),
                doc_byte_range: None,
            }],
        );
        let dependent = make_file_with_refs_and_content(
            "Api/Controllers/PacketsController.cs",
            LanguageId::CSharp,
            r#"using CeRegistry.Core.Services;
namespace CeRegistry.Api.Controllers {
    public class PacketsController {
        private readonly IMinioService _minio;
        public PacketsController(IMinioService minioService) {}
    }
}"#,
            vec![
                make_ref(
                    "Services",
                    Some("CeRegistry.Core.Services"),
                    ReferenceKind::Import,
                    None,
                    0,
                ),
                make_ref(
                    "IMinioService",
                    None,
                    ReferenceKind::TypeUsage,
                    Some(0),
                    100,
                ),
                make_ref(
                    "IMinioService",
                    None,
                    ReferenceKind::TypeUsage,
                    Some(0),
                    200,
                ),
            ],
            vec![make_symbol("PacketsController")],
        );
        let index = make_index(
            vec![
                ("Core/Services/IMinioService.cs", target),
                ("Api/Controllers/PacketsController.cs", dependent),
            ],
            false,
        );

        let deps = index.find_dependents_for_file("Core/Services/IMinioService.cs");
        assert_eq!(
            deps.len(),
            2,
            "constructor and field type usage should both be treated as dependencies"
        );
        assert!(
            deps.iter()
                .all(|(path, _)| *path == "Api/Controllers/PacketsController.cs")
        );
        assert!(deps.iter().all(|(_, r)| r.kind == ReferenceKind::TypeUsage));
    }

    #[test]
    fn test_find_dependents_csharp_type_usage_in_same_namespace_without_import() {
        let target = make_file_with_refs_and_content(
            "Core/Services/IMinioService.cs",
            LanguageId::CSharp,
            "namespace CeRegistry.Core.Services { public interface IMinioService {} }",
            vec![],
            vec![SymbolRecord {
                name: "IMinioService".to_string(),
                kind: SymbolKind::Interface,
                depth: 0,
                sort_order: 0,
                byte_range: (0, 10),
                line_range: (0, 0),
                doc_byte_range: None,
            }],
        );
        let dependent = make_file_with_refs_and_content(
            "Core/Services/MinioServiceConsumer.cs",
            LanguageId::CSharp,
            r#"namespace CeRegistry.Core.Services {
    public class MinioServiceConsumer {
        private readonly IMinioService _minio;
    }
}"#,
            vec![make_ref(
                "IMinioService",
                None,
                ReferenceKind::TypeUsage,
                Some(0),
                100,
            )],
            vec![make_symbol("MinioServiceConsumer")],
        );
        let index = make_index(
            vec![
                ("Core/Services/IMinioService.cs", target),
                ("Core/Services/MinioServiceConsumer.cs", dependent),
            ],
            false,
        );

        let deps = index.find_dependents_for_file("Core/Services/IMinioService.cs");
        assert_eq!(
            deps.len(),
            1,
            "same-namespace C# type usage should count even without a using directive"
        );
        assert_eq!(deps[0].0, "Core/Services/MinioServiceConsumer.cs");
        assert_eq!(deps[0].1.kind, ReferenceKind::TypeUsage);
    }

    #[test]
    fn test_find_dependents_java_type_usage_with_imported_package() {
        let target = make_file_with_refs_and_content(
            "src/main/java/com/acme/storage/MinioService.java",
            LanguageId::Java,
            "package com.acme.storage; public class MinioService {}",
            vec![],
            vec![SymbolRecord {
                name: "MinioService".to_string(),
                kind: SymbolKind::Class,
                depth: 0,
                sort_order: 0,
                byte_range: (0, 10),
                line_range: (0, 0),
                doc_byte_range: None,
            }],
        );
        let dependent = make_file_with_refs_and_content(
            "src/main/java/com/acme/api/PacketsController.java",
            LanguageId::Java,
            r#"package com.acme.api;
import com.acme.storage.MinioService;
public class PacketsController {
    private final MinioService minioService;
}"#,
            vec![
                make_ref(
                    "MinioService",
                    Some("com.acme.storage.MinioService"),
                    ReferenceKind::Import,
                    None,
                    0,
                ),
                make_ref("MinioService", None, ReferenceKind::TypeUsage, Some(0), 100),
            ],
            vec![make_symbol("PacketsController")],
        );
        let index = make_index(
            vec![
                ("src/main/java/com/acme/storage/MinioService.java", target),
                (
                    "src/main/java/com/acme/api/PacketsController.java",
                    dependent,
                ),
            ],
            false,
        );

        let deps =
            index.find_dependents_for_file("src/main/java/com/acme/storage/MinioService.java");
        assert_eq!(
            deps.len(),
            1,
            "Java field type usage should resolve through the imported package"
        );
        assert_eq!(
            deps[0].0,
            "src/main/java/com/acme/api/PacketsController.java"
        );
        assert_eq!(deps[0].1.kind, ReferenceKind::TypeUsage);
    }

    #[test]
    fn test_find_dependents_typescript_prefers_type_usage_when_module_import_matches() {
        let target = make_file_with_refs_and_content(
            "src/service.ts",
            LanguageId::TypeScript,
            "export class Service {}",
            vec![],
            vec![SymbolRecord {
                name: "Service".to_string(),
                kind: SymbolKind::Class,
                depth: 0,
                sort_order: 0,
                byte_range: (0, 10),
                line_range: (0, 0),
                doc_byte_range: None,
            }],
        );
        let dependent = make_file_with_refs_and_content(
            "src/app.ts",
            LanguageId::TypeScript,
            "import { Service } from \"./service\";\nexport class App { constructor(private service: Service) {} }",
            vec![
                make_ref("service", Some("./service"), ReferenceKind::Import, None, 0),
                make_ref("Service", None, ReferenceKind::TypeUsage, Some(0), 100),
            ],
            vec![SymbolRecord {
                name: "App".to_string(),
                kind: SymbolKind::Class,
                depth: 0,
                sort_order: 0,
                byte_range: (0, 10),
                line_range: (0, 0),
                doc_byte_range: None,
            }],
        );
        let index = make_index(
            vec![("src/service.ts", target), ("src/app.ts", dependent)],
            false,
        );

        let deps = index.find_dependents_for_file("src/service.ts");
        assert_eq!(
            deps.len(),
            1,
            "module-backed TypeScript dependents should report type usage, not just the import"
        );
        assert_eq!(deps[0].0, "src/app.ts");
        assert_eq!(deps[0].1.kind, ReferenceKind::TypeUsage);
        assert_eq!(deps[0].1.name, "Service");
    }

    #[test]
    fn test_find_dependents_follows_pub_use_reexport_chain() {
        // src/domain/index.rs defines ReferenceKind
        // src/domain/mod.rs has `pub use index::ReferenceKind;`
        // src/tools.rs has `use crate::domain::ReferenceKind;`
        //
        // find_dependents("src/domain/index.rs") should find src/tools.rs
        // via the re-export chain: index.rs -> mod.rs -> tools.rs

        let target = make_file_with_refs_and_content(
            "src/domain/index.rs",
            LanguageId::Rust,
            "pub enum ReferenceKind { Call, Import }",
            vec![],
            vec![SymbolRecord {
                name: "ReferenceKind".to_string(),
                kind: SymbolKind::Enum,
                depth: 0,
                sort_order: 0,
                byte_range: (0, 38),
                line_range: (0, 0),
                doc_byte_range: None,
            }],
        );

        // mod.rs content: "pub use index::ReferenceKind;\n"
        // The import ref starts at byte 8 ("index::ReferenceKind")
        let mod_content = "pub use index::ReferenceKind;\n";
        let reexporter = make_file_with_refs_and_content(
            "src/domain/mod.rs",
            LanguageId::Rust,
            mod_content,
            vec![make_ref(
                "ReferenceKind",
                Some("index::ReferenceKind"),
                ReferenceKind::Import,
                None,
                8, // byte offset where "index::ReferenceKind" starts
            )],
            vec![],
        );

        // tools.rs imports via crate::domain::ReferenceKind
        let consumer = make_file_with_refs_and_content(
            "src/tools.rs",
            LanguageId::Rust,
            "use crate::domain::ReferenceKind;\nfn check(r: ReferenceKind) {}",
            vec![
                make_ref(
                    "ReferenceKind",
                    Some("crate::domain::ReferenceKind"),
                    ReferenceKind::Import,
                    None,
                    0,
                ),
                make_ref(
                    "ReferenceKind",
                    None,
                    ReferenceKind::TypeUsage,
                    Some(0),
                    100,
                ),
            ],
            vec![make_symbol("check")],
        );

        let index = make_index(
            vec![
                ("src/domain/index.rs", target),
                ("src/domain/mod.rs", reexporter),
                ("src/tools.rs", consumer),
            ],
            false,
        );

        let deps = index.find_dependents_for_file("src/domain/index.rs");
        // mod.rs is a direct dependent (it imports from index.rs)
        // tools.rs is a transitive dependent (it imports from mod.rs which re-exports from index.rs)
        let dep_files: Vec<&str> = deps.iter().map(|(p, _)| *p).collect();
        assert!(
            dep_files.contains(&"src/domain/mod.rs"),
            "mod.rs should be a direct dependent, got: {dep_files:?}"
        );
        assert!(
            dep_files.contains(&"src/tools.rs"),
            "tools.rs should be found via pub use re-export chain, got: {dep_files:?}"
        );
    }

    #[test]
    fn test_find_dependents_lib_rs_reexport_finds_transitive() {
        // src/lib.rs has `pub use error::AppError;`
        // src/error.rs defines AppError
        // src/main.rs has `use crate::AppError;`
        //
        // find_dependents("src/error.rs") should find main.rs via the
        // lib.rs re-export chain.

        let target = make_file_with_refs_and_content(
            "src/error.rs",
            LanguageId::Rust,
            "pub struct AppError { msg: String }",
            vec![],
            vec![SymbolRecord {
                name: "AppError".to_string(),
                kind: SymbolKind::Struct,
                depth: 0,
                sort_order: 0,
                byte_range: (0, 35),
                line_range: (0, 0),
                doc_byte_range: None,
            }],
        );

        // lib.rs re-exports from error module
        let lib_content = "pub mod error;\npub use error::AppError;\n";
        let lib_file = make_file_with_refs_and_content(
            "src/lib.rs",
            LanguageId::Rust,
            lib_content,
            vec![make_ref(
                "AppError",
                Some("error::AppError"),
                ReferenceKind::Import,
                None,
                24, // byte offset of "error::AppError" in the content
            )],
            vec![],
        );

        // main.rs imports AppError from crate root (lib.rs)
        let consumer = make_file_with_refs_and_content(
            "src/main.rs",
            LanguageId::Rust,
            "use crate::AppError;\nfn main() { let _e = AppError { msg: String::new() }; }",
            vec![
                make_ref(
                    "AppError",
                    Some("crate::AppError"),
                    ReferenceKind::Import,
                    None,
                    0,
                ),
                make_ref("AppError", None, ReferenceKind::TypeUsage, Some(0), 100),
            ],
            vec![make_symbol("main")],
        );

        let index = make_index(
            vec![
                ("src/error.rs", target),
                ("src/lib.rs", lib_file),
                ("src/main.rs", consumer),
            ],
            false,
        );

        let deps = index.find_dependents_for_file("src/error.rs");
        let dep_files: Vec<&str> = deps.iter().map(|(p, _)| *p).collect();
        assert!(
            dep_files.contains(&"src/lib.rs"),
            "lib.rs should be a direct dependent (it imports from error.rs), got: {dep_files:?}"
        );
        assert!(
            dep_files.contains(&"src/main.rs"),
            "main.rs should be found via lib.rs pub use re-export, got: {dep_files:?}"
        );
    }

    #[test]
    fn test_find_dependents_non_pub_use_does_not_create_reexport_chain() {
        // Ensure that a normal `use` (not `pub use`) does NOT trigger
        // re-export chain resolution.

        let target = make_file_with_refs_and_content(
            "src/domain/index.rs",
            LanguageId::Rust,
            "pub struct Record {}",
            vec![],
            vec![SymbolRecord {
                name: "Record".to_string(),
                kind: SymbolKind::Struct,
                depth: 0,
                sort_order: 0,
                byte_range: (0, 20),
                line_range: (0, 0),
                doc_byte_range: None,
            }],
        );

        // mod.rs has private `use index::Record;` (not pub)
        let mod_content = "use index::Record;\nfn internal() {}";
        let private_importer = make_file_with_refs_and_content(
            "src/domain/mod.rs",
            LanguageId::Rust,
            mod_content,
            vec![make_ref(
                "Record",
                Some("index::Record"),
                ReferenceKind::Import,
                None,
                4, // byte offset of "index::Record" in "use index::Record;\n"
            )],
            vec![make_symbol("internal")],
        );

        // tools.rs imports from crate::domain but only mod.rs is the module root
        let consumer = make_file_with_refs_and_content(
            "src/tools.rs",
            LanguageId::Rust,
            "use crate::domain::Record;\n",
            vec![make_ref(
                "Record",
                Some("crate::domain::Record"),
                ReferenceKind::Import,
                None,
                0,
            )],
            vec![],
        );

        let index = make_index(
            vec![
                ("src/domain/index.rs", target),
                ("src/domain/mod.rs", private_importer),
                ("src/tools.rs", consumer),
            ],
            false,
        );

        let deps = index.find_dependents_for_file("src/domain/index.rs");
        let dep_files: Vec<&str> = deps.iter().map(|(p, _)| *p).collect();
        assert!(
            dep_files.contains(&"src/domain/mod.rs"),
            "mod.rs is a direct dependent, got: {dep_files:?}"
        );
        assert!(
            !dep_files.contains(&"src/tools.rs"),
            "tools.rs should NOT be found when mod.rs uses private `use` (not pub use), got: {dep_files:?}"
        );
    }

    #[test]
    fn test_resolve_module_path_rust_cases() {
        use super::resolve_module_path;
        assert_eq!(
            resolve_module_path("src/lib.rs", &LanguageId::Rust),
            Some("crate".to_string())
        );
        assert_eq!(
            resolve_module_path("src/main.rs", &LanguageId::Rust),
            Some("crate".to_string())
        );
        assert_eq!(
            resolve_module_path("src/error.rs", &LanguageId::Rust),
            Some("crate::error".to_string())
        );
        assert_eq!(
            resolve_module_path("src/live_index/mod.rs", &LanguageId::Rust),
            Some("crate::live_index".to_string())
        );
        assert_eq!(
            resolve_module_path("src/live_index/store.rs", &LanguageId::Rust),
            Some("crate::live_index::store".to_string())
        );
        // Files outside src/ have no module path
        assert_eq!(resolve_module_path("tests/foo.rs", &LanguageId::Rust), None);
        // Workspace crates: "crates/my-crate/src/types.rs" → "crate::types"
        assert_eq!(
            resolve_module_path("crates/aap-core/src/types.rs", &LanguageId::Rust),
            Some("crate::types".to_string())
        );
        assert_eq!(
            resolve_module_path("crates/aap-core/src/lib.rs", &LanguageId::Rust),
            Some("crate".to_string())
        );
        assert_eq!(
            resolve_module_path("crates/aap-core/src/domain/mod.rs", &LanguageId::Rust),
            Some("crate::domain".to_string())
        );
        // Boundary: no false positives on non-src paths
        assert_eq!(
            resolve_module_path("benches/foo.rs", &LanguageId::Rust),
            None,
            "benches/ should not resolve"
        );
        assert_eq!(
            resolve_module_path("my-src/lib.rs", &LanguageId::Rust),
            None,
            "my-src/ should not match /src/ component"
        );
    }

    #[test]
    fn test_resolve_module_path_python_cases() {
        use super::resolve_module_path;
        assert_eq!(
            resolve_module_path("utils/__init__.py", &LanguageId::Python),
            Some("utils".to_string())
        );
        assert_eq!(
            resolve_module_path("utils/helpers.py", &LanguageId::Python),
            Some("utils.helpers".to_string())
        );
    }

    #[test]
    fn test_resolve_module_path_js_cases() {
        use super::resolve_module_path;
        assert_eq!(
            resolve_module_path("src/utils/index.js", &LanguageId::JavaScript),
            Some("src/utils".to_string())
        );
        assert_eq!(
            resolve_module_path("src/utils/index.ts", &LanguageId::TypeScript),
            Some("src/utils".to_string())
        );
        assert_eq!(
            resolve_module_path("src/foo.js", &LanguageId::JavaScript),
            Some("src/foo".to_string())
        );
    }

    // --- callees_for_symbol ---

    #[test]
    fn test_callees_for_symbol_returns_enclosed_calls() {
        let refs = vec![
            make_ref("helper", None, ReferenceKind::Call, Some(0), 0),
            make_ref("other", None, ReferenceKind::Call, Some(1), 100), // different enclosing
            make_ref("imported", None, ReferenceKind::Import, Some(0), 200), // not a Call
        ];
        let f = make_file_with_refs("src/main.rs", refs, HashMap::new());
        let index = make_index(vec![("src/main.rs", f)], false);

        let callees = index.callees_for_symbol("src/main.rs", 0);
        assert_eq!(
            callees.len(),
            1,
            "only the Call reference with enclosing=0 should be returned"
        );
        assert_eq!(callees[0].name, "helper");
    }

    #[test]
    fn test_callees_for_symbol_includes_calls_inside_nested_methods() {
        let file = make_file_with_refs_and_content(
            "src/service.rs",
            LanguageId::CSharp,
            r#"public class MinioService {
    public async Task UploadAsync() {
        _minioClient.BucketExistsAsync();
        _logger.LogInformation(""upload"");
    }
}"#,
            vec![
                make_ref("BucketExistsAsync", None, ReferenceKind::Call, Some(1), 100),
                make_ref("LogInformation", None, ReferenceKind::Call, Some(1), 200),
            ],
            vec![
                SymbolRecord {
                    name: "MinioService".to_string(),
                    kind: SymbolKind::Class,
                    depth: 0,
                    sort_order: 0,
                    byte_range: (0, 500),
                    line_range: (0, 4),
                    doc_byte_range: None,
                },
                SymbolRecord {
                    name: "UploadAsync".to_string(),
                    kind: SymbolKind::Method,
                    depth: 1,
                    sort_order: 1,
                    byte_range: (50, 400),
                    line_range: (1, 3),
                    doc_byte_range: None,
                },
            ],
        );
        let index = make_index(vec![("src/service.rs", file)], false);

        let callees = index.callees_for_symbol("src/service.rs", 0);
        assert_eq!(
            callees.len(),
            2,
            "class bundles should surface calls made inside enclosed methods"
        );
        assert_eq!(callees[0].name, "BucketExistsAsync");
        assert_eq!(callees[1].name, "LogInformation");
    }

    #[test]
    fn test_callees_for_symbol_empty_for_nonexistent_file() {
        let index = make_index(vec![], false);
        let callees = index.callees_for_symbol("nonexistent.rs", 0);
        assert!(callees.is_empty(), "nonexistent file returns empty callees");
    }

    #[test]
    fn test_callees_for_symbol_excludes_non_call_kinds() {
        let refs = vec![
            make_ref("T", None, ReferenceKind::TypeUsage, Some(0), 0),
            make_ref("my_macro", None, ReferenceKind::MacroUse, Some(0), 50),
        ];
        let f = make_file_with_refs("src/lib.rs", refs, HashMap::new());
        let index = make_index(vec![("src/lib.rs", f)], false);

        let callees = index.callees_for_symbol("src/lib.rs", 0);
        assert!(
            callees.is_empty(),
            "TypeUsage and MacroUse should not appear in callees"
        );
    }

    // --- is_filtered_name (unit coverage) ---

    #[test]
    fn test_is_filtered_name_rust_builtins() {
        use super::is_filtered_name;
        assert!(is_filtered_name("i32"), "i32 is a Rust built-in");
        assert!(is_filtered_name("bool"), "bool is a Rust built-in");
        assert!(is_filtered_name("String"), "String is a Rust built-in");
        assert!(!is_filtered_name("MyString"), "MyString is not a built-in");
    }

    #[test]
    fn test_is_filtered_name_single_letter_generics() {
        use super::is_filtered_name;
        assert!(is_filtered_name("T"), "T is a single-letter generic");
        assert!(is_filtered_name("K"), "K is a single-letter generic");
        assert!(is_filtered_name("V"), "V is a single-letter generic");
        assert!(
            !is_filtered_name("Key"),
            "Key is not a single-letter generic"
        );
    }

    #[test]
    fn test_type_refs_for_symbol_returns_type_usages_within_symbol() {
        let refs = vec![
            make_ref("UserConfig", None, ReferenceKind::TypeUsage, Some(0), 0),
            make_ref("helper", None, ReferenceKind::Call, Some(0), 50),
            make_ref("Address", None, ReferenceKind::TypeUsage, Some(1), 100),
        ];
        let file = make_file_with_refs_and_content(
            "src/main.rs",
            LanguageId::Rust,
            "fn process(cfg: UserConfig) { helper(); }\nfn other(a: Address) {}",
            refs,
            vec![
                SymbolRecord {
                    name: "process".to_string(),
                    kind: SymbolKind::Function,
                    depth: 0,
                    sort_order: 0,
                    byte_range: (0, 40),
                    line_range: (0, 0),
                    doc_byte_range: None,
                },
                SymbolRecord {
                    name: "other".to_string(),
                    kind: SymbolKind::Function,
                    depth: 0,
                    sort_order: 1,
                    byte_range: (41, 65),
                    line_range: (1, 1),
                    doc_byte_range: None,
                },
            ],
        );
        let index = make_index(vec![("src/main.rs", file)], false);

        let type_refs = index.type_refs_for_symbol("src/main.rs", 0);
        assert_eq!(type_refs.len(), 1, "only TypeUsage within symbol 0");
        assert_eq!(type_refs[0].name, "UserConfig");
    }

    #[test]
    fn test_resolve_type_dependencies_finds_struct_definitions() {
        let config_body = "pub struct UserConfig {\n    pub name: String,\n}";
        let config_file = make_file_with_refs_and_content(
            "src/config.rs",
            LanguageId::Rust,
            config_body,
            vec![],
            vec![SymbolRecord {
                name: "UserConfig".to_string(),
                kind: SymbolKind::Struct,
                depth: 0,
                sort_order: 0,
                byte_range: (0, config_body.len() as u32),
                line_range: (0, 2),
                doc_byte_range: None,
            }],
        );
        let index = make_index(vec![("src/config.rs", config_file)], false);

        let deps = index.resolve_type_dependencies(&["UserConfig", "String", "T"], 0);
        assert_eq!(deps.len(), 1, "String and T should be filtered out");
        assert_eq!(deps[0].name, "UserConfig");
        assert_eq!(deps[0].kind_label, "struct");
        assert_eq!(deps[0].file_path, "src/config.rs");
        assert!(deps[0].body.contains("pub struct UserConfig"));
    }

    #[test]
    fn test_resolve_type_dependencies_recurses_to_depth() {
        let addr_body = "pub struct Address {\n    pub city: String,\n}";
        let config_body = "pub struct UserConfig {\n    pub addr: Address,\n}";
        let config_file = make_file_with_refs_and_content(
            "src/config.rs",
            LanguageId::Rust,
            config_body,
            vec![ReferenceRecord {
                name: "Address".to_string(),
                qualified_name: None,
                kind: ReferenceKind::TypeUsage,
                byte_range: (30, 37),
                line_range: (1, 1),
                enclosing_symbol_index: Some(0),
            }],
            vec![SymbolRecord {
                name: "UserConfig".to_string(),
                kind: SymbolKind::Struct,
                depth: 0,
                sort_order: 0,
                byte_range: (0, config_body.len() as u32),
                line_range: (0, 1),
                doc_byte_range: None,
            }],
        );
        let addr_file = make_file_with_refs_and_content(
            "src/address.rs",
            LanguageId::Rust,
            addr_body,
            vec![],
            vec![SymbolRecord {
                name: "Address".to_string(),
                kind: SymbolKind::Struct,
                depth: 0,
                sort_order: 0,
                byte_range: (0, addr_body.len() as u32),
                line_range: (0, 1),
                doc_byte_range: None,
            }],
        );
        let index = make_index(
            vec![
                ("src/config.rs", config_file),
                ("src/address.rs", addr_file),
            ],
            false,
        );

        // Depth 0: only UserConfig, no recursion.
        let deps_d0 = index.resolve_type_dependencies(&["UserConfig"], 0);
        assert_eq!(deps_d0.len(), 1, "depth 0 should only find UserConfig");

        // Depth 1: UserConfig + Address (found transitively).
        let deps_d1 = index.resolve_type_dependencies(&["UserConfig"], 1);
        assert_eq!(
            deps_d1.len(),
            2,
            "depth 1 should find UserConfig and Address"
        );
        let names: Vec<&str> = deps_d1.iter().map(|d| d.name.as_str()).collect();
        assert!(names.contains(&"UserConfig"));
        assert!(names.contains(&"Address"));
        // Verify depth markers.
        let uc = deps_d1.iter().find(|d| d.name == "UserConfig").unwrap();
        let ad = deps_d1.iter().find(|d| d.name == "Address").unwrap();
        assert_eq!(uc.depth, 0);
        assert_eq!(ad.depth, 1);
    }

    #[test]
    fn test_resolve_type_dependencies_respects_max_cap() {
        // Create 20 distinct struct types — should be capped at 15.
        let files: Vec<(&str, IndexedFile)> = (0..20)
            .map(|i| {
                let name = format!("Type{i}");
                let body = format!("pub struct {name} {{}}");
                let leaked_path: &'static str = Box::leak(format!("src/t{i}.rs").into_boxed_str());
                let leaked_body: &'static str = Box::leak(body.into_boxed_str());
                let f = make_file_with_refs_and_content(
                    leaked_path,
                    LanguageId::Rust,
                    leaked_body,
                    vec![],
                    vec![SymbolRecord {
                        name: name.clone(),
                        kind: SymbolKind::Struct,
                        depth: 0,
                        sort_order: 0,
                        byte_range: (0, leaked_body.len() as u32),
                        line_range: (0, 0),
                        doc_byte_range: None,
                    }],
                );
                (leaked_path, f)
            })
            .collect();
        let index = make_index(files, false);

        let type_names: Vec<&str> = (0..20)
            .map(|i| {
                let s: &'static str = Box::leak(format!("Type{i}").into_boxed_str());
                s
            })
            .collect();
        let deps = index.resolve_type_dependencies(&type_names, 0);
        assert!(
            deps.len() <= 15,
            "should be capped at MAX_DEPENDENCIES=15, got {}",
            deps.len()
        );
    }
}
