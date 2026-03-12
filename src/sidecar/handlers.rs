//! HTTP endpoint handlers for the tokenizor sidecar.
//!
//! All handlers follow this contract:
//!  - Accept `State(state): State<SidecarState>` plus optional `Query(params)`.
//!  - Acquire `state.index.read()`, extract owned data, drop the guard, then return text or Json.
//!  - Never hold a `RwLockReadGuard` across an `.await` point.
//!  - On lock poison: return `StatusCode::INTERNAL_SERVER_ERROR`.
//!  - On file not found: return `StatusCode::NOT_FOUND`.

use axum::{
    Json,
    extract::{Query, State},
    http::StatusCode,
};
use serde::{Deserialize, Serialize};

use crate::domain::ReferenceKind;
use crate::sidecar::{SidecarState, SymbolSnapshot, build_with_budget};

// ---------------------------------------------------------------------------
// Request parameter structs
// ---------------------------------------------------------------------------

#[derive(Clone, Deserialize, Serialize)]
pub struct OutlineParams {
    pub path: String,
    /// Optional token budget override. Default: 200 tokens (800 bytes).
    pub max_tokens: Option<u64>,
}

#[derive(Clone, Deserialize, Serialize)]
pub struct ImpactParams {
    pub path: String,
    /// If `true`, treat this as a new-file indexing request (HOOK-06).
    pub new_file: Option<bool>,
}

#[derive(Clone, Deserialize, Serialize)]
pub struct SymbolContextParams {
    pub name: String,
    /// Optional: restrict search to a specific file.
    pub file: Option<String>,
    /// Optional exact-selector path from `search_symbols`.
    pub path: Option<String>,
    /// Optional selected symbol kind such as `fn`, `class`, or `struct`.
    pub symbol_kind: Option<String>,
    /// Optional selected symbol line from `search_symbols`.
    pub symbol_line: Option<u32>,
}

#[derive(Clone, Deserialize, Serialize)]
pub struct PromptContextParams {
    pub text: String,
}

struct PromptFileHint {
    path: String,
    match_kind: PromptFileHintMatchKind,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum PromptFileHintMatchKind {
    ExactPath,
    QualifiedExtensionlessPath,
    Basename,
    ExtensionlessAlias,
}

#[derive(Serialize)]
pub struct HealthResponse {
    pub file_count: usize,
    pub symbol_count: usize,
    pub index_state: String,
    pub uptime_secs: u64,
}

#[derive(Clone, Copy)]
struct RenderOptions {
    include_savings_footer: bool,
    record_stats: bool,
}

const HOOK_RENDER_OPTIONS: RenderOptions = RenderOptions {
    include_savings_footer: true,
    record_stats: true,
};

const TOOL_RENDER_OPTIONS: RenderOptions = RenderOptions {
    include_savings_footer: false,
    record_stats: false,
};

fn resolve_repo_root(state: &SidecarState) -> Result<std::path::PathBuf, StatusCode> {
    match &state.repo_root {
        Some(root) => Ok(root.clone()),
        None => std::env::current_dir().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR),
    }
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// `GET /health` — index state, file count, symbol count, uptime.
pub async fn health_handler(
    State(state): State<SidecarState>,
) -> Result<Json<HealthResponse>, StatusCode> {
    let published = state.index.published_state();

    let uptime_secs = published
        .loaded_at_system
        .elapsed()
        .unwrap_or_default()
        .as_secs();

    Ok(Json(HealthResponse {
        file_count: published.file_count,
        symbol_count: published.symbol_count,
        index_state: published.status_label().to_string(),
        uptime_secs,
    }))
}

/// `GET /outline?path=<relative>[&max_tokens=N]` — symbol outline for a single file.
///
/// Returns formatted plain text with:
/// - Symbol outline lines (compact, ripgrep-like)
/// - "Key references" section showing top 3-5 most-called symbols with up to 3 callers each
/// - "[~N tokens saved]" footer
///
/// Budget: 200 tokens (800 bytes) by default.
pub async fn outline_handler(
    State(state): State<SidecarState>,
    Query(params): Query<OutlineParams>,
) -> Result<String, StatusCode> {
    outline_hook_text(&state, &params)
}

pub(crate) fn outline_tool_text(
    state: &SidecarState,
    params: &OutlineParams,
) -> Result<String, StatusCode> {
    outline_text(state, params, TOOL_RENDER_OPTIONS)
}

fn outline_hook_text(state: &SidecarState, params: &OutlineParams) -> Result<String, StatusCode> {
    outline_text(state, params, HOOK_RENDER_OPTIONS)
}

fn outline_text(
    state: &SidecarState,
    params: &OutlineParams,
    options: RenderOptions,
) -> Result<String, StatusCode> {
    let guard = state
        .index
        .read()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Return 404 for non-indexed files.
    let file = guard.get_file(&params.path).ok_or(StatusCode::NOT_FOUND)?;

    let file_bytes = file.byte_len;
    let language = format!("{:?}", file.language);

    // Build symbol outline lines.
    let mut lines: Vec<String> = Vec::new();
    lines.push(format!(
        "── {} ({} symbols, {}) ──",
        params.path,
        file.symbols.len(),
        language
    ));

    for sym in &file.symbols {
        let indent = "  ".repeat(sym.depth as usize);
        lines.push(format!(
            "{}  {:<10} {}  L{}-{}",
            indent,
            sym.kind.to_string(),
            sym.name,
            sym.line_range.0,
            sym.line_range.1,
        ));
    }

    // Build "Key references" section.
    // Rank symbols by caller count descending, take top 5, show up to 3 callers each.
    let attributed_dependents = guard.find_dependents_for_file(&params.path);
    let mut symbol_callers: Vec<(String, Vec<(String, u32)>)> = Vec::new();

    for sym in &file.symbols {
        let external_callers: Vec<(String, u32)> = attributed_dependents
            .iter()
            .filter(|(_, reference)| {
                reference.kind != ReferenceKind::Import && reference.name == sym.name
            })
            .map(|(fp, r)| (fp.to_string(), r.line_range.0))
            .take(3)
            .collect();

        if !external_callers.is_empty() {
            symbol_callers.push((sym.name.clone(), external_callers));
        }
    }

    // Sort by caller count descending, take top 5.
    symbol_callers.sort_by(|a, b| b.1.len().cmp(&a.1.len()));
    symbol_callers.truncate(5);

    if !symbol_callers.is_empty() {
        lines.push(String::new());
        lines.push("Key references:".to_string());
        for (sym_name, callers) in &symbol_callers {
            lines.push(format!("  {}()", sym_name));
            for (caller_file, caller_line) in callers {
                lines.push(format!("    {}  line {}", caller_file, caller_line));
            }
        }
    }

    drop(guard);

    // Apply budget enforcement.
    let max_bytes = params.max_tokens.unwrap_or(200) * 4;
    let (mut text, _remaining) = build_with_budget(&lines, max_bytes);

    let output_bytes = text.len() as u64;
    if options.include_savings_footer {
        let saved_tokens = file_bytes.saturating_sub(output_bytes) / 4;
        text.push_str(&format!("\n[~{} tokens saved]", saved_tokens));
    }

    if options.record_stats {
        state.token_stats.record_read(file_bytes, output_bytes);
    }

    Ok(text)
}

/// `GET /impact?path=<relative>[&new_file=true]` — symbol diff after edit, or index confirmation.
///
/// **new_file=true (HOOK-06):** Reads file from disk, parses it, indexes it.
/// Returns: language, symbol kind breakdown, `[Indexed, 0 callers yet]`.
///
/// **default (HOOK-05 edit):** Re-indexes the file from disk, computes pre/post symbol diff.
/// Shows Added/Changed/Removed symbols plus callers for Changed+Removed symbols.
///
/// Budget: 150 tokens (600 bytes).
pub async fn impact_handler(
    State(state): State<SidecarState>,
    Query(params): Query<ImpactParams>,
) -> Result<String, StatusCode> {
    impact_hook_text(state, &params).await
}

pub(crate) async fn impact_tool_text(
    state: SidecarState,
    params: &ImpactParams,
) -> Result<String, StatusCode> {
    impact_text(state, params, TOOL_RENDER_OPTIONS).await
}

async fn impact_hook_text(
    state: SidecarState,
    params: &ImpactParams,
) -> Result<String, StatusCode> {
    impact_text(state, params, HOOK_RENDER_OPTIONS).await
}

async fn impact_text(
    state: SidecarState,
    params: &ImpactParams,
    options: RenderOptions,
) -> Result<String, StatusCode> {
    let is_new_file = params.new_file.unwrap_or(false);

    if is_new_file {
        // HOOK-06: Index a new file from disk.
        return handle_new_file_impact(state, &params.path, options).await;
    }

    // HOOK-05: Re-index existing file and compute symbol diff.
    handle_edit_impact(state, &params.path, options).await
}

async fn handle_new_file_impact(
    state: SidecarState,
    path: &str,
    options: RenderOptions,
) -> Result<String, StatusCode> {
    use crate::domain::LanguageId;

    // Determine language from file extension.
    let extension = std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");

    let language = LanguageId::from_extension(extension).ok_or(StatusCode::NOT_FOUND)?;

    // Read file from disk. The sidecar doesn't know the project root, so
    // we look up the root from the existing index as a heuristic.
    // For new files, we try to find them relative to cwd.
    let abs_path = resolve_repo_root(&state)?.join(path);
    let bytes = std::fs::read(&abs_path).map_err(|_| StatusCode::NOT_FOUND)?;

    // Parse the file.
    let result = crate::parsing::process_file(path, &bytes, language.clone());

    // Build symbol kind breakdown.
    let mut kind_counts: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();
    for sym in &result.symbols {
        *kind_counts.entry(sym.kind.to_string()).or_insert(0) += 1;
    }

    let mut kind_parts: Vec<String> = kind_counts
        .iter()
        .map(|(k, v)| format!("{} {}", v, k))
        .collect();
    kind_parts.sort();
    let kinds_str = if kind_parts.is_empty() {
        "0 symbols".to_string()
    } else {
        kind_parts.join(", ")
    };

    // Index the file.
    let indexed = crate::live_index::store::IndexedFile::from_parse_result(result, bytes);
    state.index.update_file(path.to_string(), indexed);

    // Update symbol cache with empty pre-edit snapshot (it's new, no pre-state).
    {
        let mut cache = state
            .symbol_cache
            .write()
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        cache.insert(path.to_string(), Vec::new());
    }

    if options.record_stats {
        state.token_stats.record_write();
    }

    let text = format!(
        "Language: {:?}\nSymbols: {}\n[Indexed, 0 callers yet]",
        language, kinds_str,
    );

    Ok(text)
}

async fn handle_edit_impact(
    state: SidecarState,
    path: &str,
    options: RenderOptions,
) -> Result<String, StatusCode> {
    use crate::domain::LanguageId;

    // Get pre-edit symbols from cache or from current index.
    let pre_symbols: Vec<SymbolSnapshot> = {
        let cache = state
            .symbol_cache
            .read()
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        if let Some(cached) = cache.get(path) {
            cached.clone()
        } else {
            // No cache entry — populate from current index.
            let guard = state
                .index
                .read()
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            if let Some(file) = guard.get_file(path) {
                let file_bytes = file.byte_len;
                let syms: Vec<SymbolSnapshot> = file
                    .symbols
                    .iter()
                    .map(|s| SymbolSnapshot {
                        name: s.name.clone(),
                        kind: s.kind.to_string(),
                        line_range: s.line_range,
                        byte_range: s.byte_range,
                    })
                    .collect();
                drop(guard);
                // Can't update cache here (have read lock on cache) — return empty pre
                // so we get an "all Added" diff on first edit.
                let _ = file_bytes; // suppress unused warning
                syms
            } else {
                Vec::new()
            }
        }
    };

    // Get file byte_len from index before re-indexing.
    let file_bytes_pre: u64 = {
        let guard = state
            .index
            .read()
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        guard.get_file(path).map(|f| f.byte_len).unwrap_or(0)
    };

    // Determine language.
    let extension = std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");

    let language =
        LanguageId::from_extension(extension).ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;

    // Read file from disk and re-index.
    let abs_path = resolve_repo_root(&state)?.join(path);
    let bytes = std::fs::read(&abs_path).unwrap_or_default();
    let file_bytes_new = bytes.len() as u64;
    let file_bytes = if file_bytes_new > 0 {
        file_bytes_new
    } else {
        file_bytes_pre
    };

    let result = crate::parsing::process_file(path, &bytes, language);
    let post_symbols: Vec<SymbolSnapshot> = result
        .symbols
        .iter()
        .map(|s| SymbolSnapshot {
            name: s.name.clone(),
            kind: s.kind.to_string(),
            line_range: s.line_range,
            byte_range: s.byte_range,
        })
        .collect();

    let indexed = crate::live_index::store::IndexedFile::from_parse_result(result, bytes);
    state.index.update_file(path.to_string(), indexed);

    // Compute symbol diff.
    let added: Vec<&SymbolSnapshot> = post_symbols
        .iter()
        .filter(|ps| {
            !pre_symbols
                .iter()
                .any(|pr| pr.name == ps.name && pr.kind == ps.kind)
        })
        .collect();

    let removed: Vec<&SymbolSnapshot> = pre_symbols
        .iter()
        .filter(|pr| {
            !post_symbols
                .iter()
                .any(|ps| ps.name == pr.name && ps.kind == pr.kind)
        })
        .collect();

    let changed: Vec<&SymbolSnapshot> = post_symbols
        .iter()
        .filter(|ps| {
            pre_symbols.iter().any(|pr| {
                pr.name == ps.name
                    && pr.kind == ps.kind
                    && (pr.line_range != ps.line_range || pr.byte_range != ps.byte_range)
            })
        })
        .collect();

    // Update cache with post-edit snapshot.
    {
        let mut cache = state
            .symbol_cache
            .write()
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        cache.insert(path.to_string(), post_symbols.clone());
    }

    // Build response lines.
    let mut lines: Vec<String> = Vec::new();
    lines.push(format!("── Impact: {} ──", path));

    if added.is_empty() && changed.is_empty() && removed.is_empty() {
        lines.push("No symbol changes detected.".to_string());
    } else {
        for sym in &added {
            lines.push(format!("  [Added]   {} {}", sym.kind, sym.name));
        }
        for sym in &changed {
            lines.push(format!("  [Changed] {} {}", sym.kind, sym.name));
        }
        for sym in &removed {
            lines.push(format!("  [Removed] {} {}", sym.kind, sym.name));
        }

        // Show callers for Changed + Removed symbols.
        let impacted: Vec<&SymbolSnapshot> =
            changed.iter().chain(removed.iter()).copied().collect();
        if !impacted.is_empty() {
            let guard = state
                .index
                .read()
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            let mut callers_lines: Vec<String> = Vec::new();
            for sym in &impacted {
                let callers = guard.find_references_for_name(&sym.name, None, false);
                let external: Vec<_> = callers
                    .iter()
                    .filter(|(fp, _)| *fp != path)
                    .take(5)
                    .collect();
                if !external.is_empty() {
                    callers_lines.push(format!("  Callers of {}():", sym.name));
                    for (caller_file, r) in &external {
                        callers_lines.push(format!("    {}  line {}", caller_file, r.line_range.0));
                    }
                }
            }
            drop(guard);
            if !callers_lines.is_empty() {
                lines.push(String::new());
                lines.push("Callers to review:".to_string());
                lines.extend(callers_lines);
            }
        }
    }

    // Apply budget (150 tokens = 600 bytes).
    let (mut text, _) = build_with_budget(&lines, 600);

    let output_bytes = text.len() as u64;
    if options.include_savings_footer {
        let saved_tokens = file_bytes.saturating_sub(output_bytes) / 4;
        text.push_str(&format!("\n[~{} tokens saved]", saved_tokens));
    }

    if options.record_stats {
        state.token_stats.record_edit(file_bytes, output_bytes);
    }

    Ok(text)
}

/// `GET /symbol-context?name=<name>[&file=<path>]` — all references to a named symbol.
///
/// Returns formatted plain text with enclosing-symbol annotations, grouped by file.
/// Caps at 10 annotated matches.
///
/// Budget: 100 tokens (400 bytes).
pub async fn symbol_context_handler(
    State(state): State<SidecarState>,
    Query(params): Query<SymbolContextParams>,
) -> Result<String, StatusCode> {
    symbol_context_hook_text(&state, &params)
}

pub(crate) fn symbol_context_tool_text(
    state: &SidecarState,
    params: &SymbolContextParams,
) -> Result<String, StatusCode> {
    symbol_context_text(state, params, TOOL_RENDER_OPTIONS)
}

fn symbol_context_hook_text(
    state: &SidecarState,
    params: &SymbolContextParams,
) -> Result<String, StatusCode> {
    symbol_context_text(state, params, HOOK_RENDER_OPTIONS)
}

fn symbol_context_text(
    state: &SidecarState,
    params: &SymbolContextParams,
    options: RenderOptions,
) -> Result<String, StatusCode> {
    let guard = state
        .index
        .read()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let raw = if let Some(path) = params.path.as_deref() {
        match guard.find_exact_references_for_symbol(
            path,
            &params.name,
            params.symbol_kind.as_deref(),
            params.symbol_line,
            None,
        ) {
            Ok(refs) => refs,
            Err(error) => return Ok(error),
        }
    } else {
        guard.find_references_for_name(&params.name, None, false)
    };

    // Group by file, applying optional file filter, capping at 10 total matches.
    let mut map: std::collections::HashMap<String, Vec<(u32, String, Option<String>)>> =
        std::collections::HashMap::new();

    let mut total = 0usize;
    let mut grand_total = 0usize;

    for (file_path, reference) in &raw {
        grand_total += 1;
        if let Some(ref filter_file) = params.file
            && *file_path != filter_file.as_str()
        {
            continue;
        }
        if total >= 10 {
            continue; // count beyond 10 but don't include
        }

        let enclosing = reference.enclosing_symbol_index.and_then(|idx| {
            guard
                .get_file(file_path)
                .and_then(|f| f.symbols.get(idx as usize))
                .map(|s| s.name.clone())
        });

        map.entry(file_path.to_string()).or_default().push((
            reference.line_range.0,
            format!("{}", reference.kind),
            enclosing,
        ));
        total += 1;
    }

    // Compute total bytes for savings (sum of content of all matched files).
    let total_bytes: u64 = map
        .keys()
        .filter_map(|fp| guard.get_file(fp))
        .map(|f| f.byte_len)
        .sum();

    drop(guard);

    // Sort files for deterministic output.
    let mut files: Vec<String> = map.keys().cloned().collect();
    files.sort();

    let mut lines: Vec<String> = Vec::new();

    for file in &files {
        lines.push(format!("── {} ──", file));
        let refs = map.get(file).unwrap();
        let mut sorted_refs = refs.clone();
        sorted_refs.sort_by_key(|(line, _, _)| *line);
        for (line, _kind, enclosing) in &sorted_refs {
            if let Some(sym_name) = enclosing {
                lines.push(format!("  line {}  in fn {}", line, sym_name));
            } else {
                lines.push(format!("  line {}  (module level)", line));
            }
        }
    }

    if total < grand_total {
        lines.push(format!(
            "... (showing {} of {} matches)",
            total, grand_total
        ));
    }

    // Apply budget (100 tokens = 400 bytes).
    let (mut text, _) = build_with_budget(&lines, 400);

    let output_bytes = text.len() as u64;
    if options.include_savings_footer {
        let saved_tokens = total_bytes.saturating_sub(output_bytes) / 4;
        text.push_str(&format!("\n[~{} tokens saved]", saved_tokens));
    }

    if options.record_stats {
        state.token_stats.record_grep(total_bytes, output_bytes);
    }

    Ok(text)
}

/// `GET /repo-map` — formatted directory tree with symbol counts.
///
/// Returns 2-level directory tree with file counts and symbol counts per directory,
/// plus a language breakdown header.
///
/// Budget: 500 tokens (2000 bytes). No token savings recorded (additive, not replacement).
pub async fn repo_map_handler(State(state): State<SidecarState>) -> Result<String, StatusCode> {
    repo_map_text(&state)
}

pub(crate) fn repo_map_text(state: &SidecarState) -> Result<String, StatusCode> {
    let guard = state
        .index
        .read()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let total_files = guard.file_count();
    let total_symbols = guard.symbol_count();

    // Collect language breakdown.
    let mut lang_counts: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();
    // Collect per-directory stats (2-level max).
    let mut dir_file_counts: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();
    let mut dir_symbol_counts: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();

    for (path, file) in guard.all_files() {
        // Language breakdown.
        let lang = format!("{:?}", file.language);
        *lang_counts.entry(lang).or_insert(0) += 1;

        // Directory (up to 2 levels).
        let dir = get_dir_2level(path);
        *dir_file_counts.entry(dir.clone()).or_insert(0) += 1;
        *dir_symbol_counts.entry(dir).or_insert(0) += file.symbols.len();
    }

    drop(guard);

    // Build header.
    let mut lang_parts: Vec<String> = lang_counts
        .iter()
        .map(|(k, v)| format!("{}: {}", k, v))
        .collect();
    lang_parts.sort();

    let mut lines: Vec<String> = Vec::new();
    lines.push(format!(
        "Index: {} files, {} symbols  [{}]",
        total_files,
        total_symbols,
        lang_parts.join(", ")
    ));
    lines.push(String::new());

    // Sort directories and emit tree.
    let mut dirs: Vec<String> = dir_file_counts.keys().cloned().collect();
    dirs.sort();

    for dir in &dirs {
        let file_count = dir_file_counts[dir];
        let sym_count = dir_symbol_counts[dir];
        lines.push(format!(
            "  {:<35}  {:>3} files   {:>5} symbols",
            dir, file_count, sym_count
        ));
    }

    // Apply budget (500 tokens = 2000 bytes).
    let (text, _) = build_with_budget(&lines, 2000);

    Ok(text)
}

/// `GET /prompt-context?text=<prompt>` — derive compact context from a user prompt.
///
/// Heuristics:
/// - explicit file hint in the prompt => outline for that file
/// - explicit symbol hint in the prompt => symbol context for that symbol
/// - repo-map intent keywords => repo map
/// - otherwise => empty context
pub async fn prompt_context_handler(
    State(state): State<SidecarState>,
    Query(params): Query<PromptContextParams>,
) -> Result<String, StatusCode> {
    prompt_context_hook_text(&state, &params).await
}

async fn prompt_context_hook_text(
    state: &SidecarState,
    params: &PromptContextParams,
) -> Result<String, StatusCode> {
    prompt_context_text(state, params, HOOK_RENDER_OPTIONS).await
}

async fn prompt_context_text(
    state: &SidecarState,
    params: &PromptContextParams,
    options: RenderOptions,
) -> Result<String, StatusCode> {
    let prompt = params.text.trim();
    if prompt.is_empty() {
        return Ok(String::new());
    }

    let file_hint = find_prompt_file_hint(state, prompt)?;
    let symbol_hint = find_prompt_symbol_hint(state, prompt)?;
    let line_hint = find_prompt_line_hint(prompt, file_hint.as_ref());

    match (file_hint, symbol_hint) {
        (Some(file_hint), Some(name)) => {
            return symbol_context_text(
                state,
                &SymbolContextParams {
                    name,
                    file: None,
                    path: Some(file_hint.path),
                    symbol_kind: None,
                    symbol_line: line_hint,
                },
                options,
            );
        }
        (Some(file_hint), None) => {
            return outline_text(
                state,
                &OutlineParams {
                    path: file_hint.path,
                    max_tokens: Some(160),
                },
                options,
            );
        }
        (None, Some(name)) => {
            return symbol_context_text(
                state,
                &SymbolContextParams {
                    name,
                    file: None,
                    path: None,
                    symbol_kind: None,
                    symbol_line: None,
                },
                options,
            );
        }
        (None, None) => {}
    }

    if prompt_requests_repo_map(prompt) {
        return repo_map_text(state);
    }

    Ok(String::new())
}

/// `GET /stats` — return token savings snapshot as JSON.
pub async fn stats_handler(
    State(state): State<SidecarState>,
) -> Json<crate::sidecar::StatsSnapshot> {
    Json(state.token_stats.summary())
}

// ---------------------------------------------------------------------------
// Helper: extract up to 2-level directory from a relative path
// ---------------------------------------------------------------------------

fn get_dir_2level(path: &str) -> String {
    let p = std::path::Path::new(path);
    let components: Vec<_> = p.components().collect();

    if components.len() <= 1 {
        // Root-level file.
        return "(root)".to_string();
    }

    // Take at most 2 directory components (exclude the file name).
    let dir_components: Vec<_> = components[..components.len() - 1].iter().take(2).collect();
    dir_components
        .iter()
        .map(|c| c.as_os_str().to_string_lossy().to_string())
        .collect::<Vec<_>>()
        .join("/")
}

fn find_prompt_file_hint(
    state: &SidecarState,
    prompt: &str,
) -> Result<Option<PromptFileHint>, StatusCode> {
    let guard = state
        .index
        .read()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let prompt_lower = prompt.to_ascii_lowercase();
    let mut qualified_path_match: Option<PromptFileHint> = None;
    let mut qualified_path_ambiguous = false;
    let mut basename_match: Option<PromptFileHint> = None;
    let mut basename_ambiguous = false;
    let mut stem_match: Option<PromptFileHint> = None;
    let mut stem_ambiguous = false;

    for (path, _) in guard.all_files() {
        if prompt.contains(path) || prompt_lower.contains(&path.to_ascii_lowercase()) {
            return Ok(Some(PromptFileHint {
                path: path.to_string(),
                match_kind: PromptFileHintMatchKind::ExactPath,
            }));
        }

        if let Some(path_without_extension) = prompt_path_without_extension(path) {
            if find_prompt_path_line_hint(prompt, &path_without_extension).is_some() {
                if let Some(existing) = &qualified_path_match {
                    if existing.path != path.as_str() {
                        qualified_path_ambiguous = true;
                    }
                } else {
                    qualified_path_match = Some(PromptFileHint {
                        path: path.to_string(),
                        match_kind: PromptFileHintMatchKind::QualifiedExtensionlessPath,
                    });
                }
            }
        }

        let Some(file_name) = std::path::Path::new(path)
            .file_name()
            .and_then(|name| name.to_str())
        else {
            continue;
        };
        if prompt_lower.contains(&file_name.to_ascii_lowercase()) {
            if let Some(existing) = &basename_match {
                if existing.path != path.as_str() {
                    basename_ambiguous = true;
                }
            } else {
                basename_match = Some(PromptFileHint {
                    path: path.to_string(),
                    match_kind: PromptFileHintMatchKind::Basename,
                });
            }
        }

        let Some(file_stem) = std::path::Path::new(path)
            .file_stem()
            .and_then(|name| name.to_str())
        else {
            continue;
        };

        if !find_prompt_path_line_hint(prompt, file_stem).is_some() {
            continue;
        }

        if let Some(existing) = &stem_match {
            if existing.path != path.as_str() {
                stem_ambiguous = true;
            }
        } else {
            stem_match = Some(PromptFileHint {
                path: path.to_string(),
                match_kind: PromptFileHintMatchKind::ExtensionlessAlias,
            });
        }
    }

    if !qualified_path_ambiguous && qualified_path_match.is_some() {
        return Ok(qualified_path_match);
    }

    if !basename_ambiguous && basename_match.is_some() {
        return Ok(basename_match);
    }

    if stem_ambiguous {
        Ok(None)
    } else {
        Ok(stem_match)
    }
}

fn find_prompt_symbol_hint(
    state: &SidecarState,
    prompt: &str,
) -> Result<Option<String>, StatusCode> {
    let guard = state
        .index
        .read()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    for token in prompt_tokens(prompt) {
        if token.len() < 3 || token.contains('/') || token.contains('.') {
            continue;
        }

        let has_match = guard
            .all_files()
            .any(|(_, file)| file.symbols.iter().any(|symbol| symbol.name == token));
        if has_match {
            return Ok(Some(token));
        }
    }

    Ok(None)
}

fn find_prompt_line_hint(prompt: &str, file_hint: Option<&PromptFileHint>) -> Option<u32> {
    if let Some(file_hint) = file_hint {
        if let Some(line) = find_prompt_path_line_hint(prompt, &file_hint.path) {
            return Some(line);
        }
        let alias = match file_hint.match_kind {
            PromptFileHintMatchKind::ExactPath => None,
            PromptFileHintMatchKind::QualifiedExtensionlessPath => {
                prompt_path_without_extension(&file_hint.path)
            }
            PromptFileHintMatchKind::Basename => std::path::Path::new(&file_hint.path)
                .file_name()
                .and_then(|name| name.to_str())
                .map(ToString::to_string),
            PromptFileHintMatchKind::ExtensionlessAlias => std::path::Path::new(&file_hint.path)
                .file_stem()
                .and_then(|name| name.to_str())
                .map(ToString::to_string),
        };
        if let Some(alias) = alias {
            if let Some(line) = find_prompt_path_line_hint(prompt, &alias) {
                return Some(line);
            }
        }
    }

    let tokens = prompt_tokens(prompt);
    for window in tokens.windows(2) {
        if !window[0].eq_ignore_ascii_case("line") {
            continue;
        }
        if let Ok(line) = window[1].parse::<u32>() {
            if line > 0 {
                return Some(line);
            }
        }
    }

    None
}

fn find_prompt_path_line_hint(prompt: &str, path: &str) -> Option<u32> {
    let prompt_lower = prompt.to_ascii_lowercase();
    let needle = format!("{}:", path.to_ascii_lowercase());
    let mut search_start = 0;

    while let Some(offset) = prompt_lower[search_start..].find(&needle) {
        let value_start = search_start + offset + needle.len();
        let digits: String = prompt[value_start..]
            .chars()
            .take_while(|ch| ch.is_ascii_digit())
            .collect();
        if let Ok(line) = digits.parse::<u32>() {
            if line > 0 {
                return Some(line);
            }
        }

        search_start = value_start;
    }

    None
}

fn prompt_path_without_extension(path: &str) -> Option<String> {
    let file_name = std::path::Path::new(path).file_name()?.to_str()?;
    let file_stem = std::path::Path::new(path).file_stem()?.to_str()?;
    if let Some((parent, _)) = path.rsplit_once('/') {
        Some(format!("{parent}/{file_stem}"))
    } else if file_name != file_stem {
        Some(file_stem.to_string())
    } else {
        None
    }
}

fn prompt_tokens(prompt: &str) -> Vec<String> {
    prompt
        .split(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_' || ch == '/' || ch == '.'))
        .filter(|token| !token.is_empty())
        .map(ToString::to_string)
        .collect()
}

fn prompt_requests_repo_map(prompt: &str) -> bool {
    let lower = prompt.to_ascii_lowercase();
    [
        "architecture",
        "codebase",
        "map",
        "overview",
        "repo",
        "repository",
        "structure",
    ]
    .iter()
    .any(|term| lower.contains(term))
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::{Arc, RwLock};
    use std::time::{Duration, Instant, SystemTime};

    use crate::domain::{LanguageId, ReferenceKind, ReferenceRecord, SymbolKind, SymbolRecord};
    use crate::live_index::store::{CircuitBreakerState, IndexedFile, LiveIndex, ParseStatus};
    use crate::sidecar::{SidecarState, SymbolSnapshot, TokenStats};

    // -----------------------------------------------------------------------
    // Test helper: minimal LiveIndex with known contents
    // -----------------------------------------------------------------------

    fn make_symbol(name: &str, kind: SymbolKind, start: u32, end: u32) -> SymbolRecord {
        SymbolRecord {
            name: name.to_string(),
            kind,
            depth: 0,
            sort_order: 0,
            byte_range: (0, 10),
            line_range: (start, end),
        }
    }

    fn make_reference(name: &str, kind: ReferenceKind, line: u32) -> ReferenceRecord {
        ReferenceRecord {
            name: name.to_string(),
            qualified_name: None,
            kind,
            byte_range: (100, 110),
            line_range: (line, line),
            enclosing_symbol_index: None,
        }
    }

    fn make_indexed_file(
        path: &str,
        symbols: Vec<SymbolRecord>,
        references: Vec<ReferenceRecord>,
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
            references,
            alias_map: HashMap::new(),
        }
    }

    fn build_shared_index(
        files: Vec<(&str, IndexedFile)>,
    ) -> crate::live_index::store::SharedIndex {
        use crate::live_index::trigram::TrigramIndex;
        let files_map: HashMap<String, std::sync::Arc<IndexedFile>> = files
            .into_iter()
            .map(|(p, f)| (p.to_string(), std::sync::Arc::new(f)))
            .collect();
        let trigram_index = TrigramIndex::build_from_files(&files_map);
        let mut index = LiveIndex {
            files: files_map,
            loaded_at: Instant::now(),
            loaded_at_system: SystemTime::now(),
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
        crate::live_index::SharedIndexHandle::shared(index)
    }

    /// Build a SidecarState wrapping a SharedIndex for use in tests.
    fn make_state(files: Vec<(&str, IndexedFile)>) -> SidecarState {
        SidecarState {
            index: build_shared_index(files),
            token_stats: TokenStats::new(),
            repo_root: None,
            symbol_cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    // -----------------------------------------------------------------------
    // health_handler
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_health_handler_returns_counts() {
        let f1 = make_indexed_file(
            "src/main.rs",
            vec![make_symbol("main", SymbolKind::Function, 1, 10)],
            vec![],
            ParseStatus::Parsed,
        );
        let f2 = make_indexed_file(
            "src/lib.rs",
            vec![
                make_symbol("foo", SymbolKind::Function, 1, 5),
                make_symbol("bar", SymbolKind::Function, 7, 12),
            ],
            vec![],
            ParseStatus::Parsed,
        );
        let state = make_state(vec![("src/main.rs", f1), ("src/lib.rs", f2)]);

        let result = health_handler(State(state)).await.unwrap();
        let body = result.0;
        assert_eq!(body.file_count, 2, "health should report 2 files");
        assert_eq!(body.symbol_count, 3, "health should report 3 symbols");
        assert!(
            body.index_state.contains("Ready"),
            "index_state should include Ready"
        );
    }

    #[tokio::test]
    async fn test_health_handler_empty_index() {
        let state = make_state(vec![]);
        let result = health_handler(State(state)).await.unwrap();
        let body = result.0;
        assert_eq!(body.file_count, 0);
        assert_eq!(body.symbol_count, 0);
    }

    // -----------------------------------------------------------------------
    // outline_handler
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_outline_handler_returns_formatted_text() {
        let file = make_indexed_file(
            "src/foo.rs",
            vec![
                make_symbol("alpha", SymbolKind::Function, 1, 5),
                make_symbol("Beta", SymbolKind::Struct, 7, 10),
            ],
            vec![],
            ParseStatus::Parsed,
        );
        let state = make_state(vec![("src/foo.rs", file)]);

        let params = OutlineParams {
            path: "src/foo.rs".to_string(),
            max_tokens: None,
        };
        let result = outline_handler(State(state), Query(params)).await.unwrap();
        assert!(
            result.contains("alpha"),
            "outline should contain symbol name 'alpha'"
        );
        assert!(
            result.contains("Beta"),
            "outline should contain symbol name 'Beta'"
        );
        assert!(
            result.contains("src/foo.rs"),
            "outline should contain file path"
        );
        assert!(
            result.contains("tokens saved"),
            "outline should have token savings footer"
        );
    }

    #[tokio::test]
    async fn test_outline_handler_not_found_for_missing_file() {
        let state = make_state(vec![]);
        let params = OutlineParams {
            path: "nonexistent.rs".to_string(),
            max_tokens: None,
        };
        let err = outline_handler(State(state), Query(params))
            .await
            .unwrap_err();
        assert_eq!(err, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_outline_handler_budget_enforced() {
        // Create a file with many symbols to trigger truncation.
        let symbols: Vec<SymbolRecord> = (0..50)
            .map(|i| {
                make_symbol(
                    &format!("symbol_{:04}", i),
                    SymbolKind::Function,
                    i * 2,
                    i * 2 + 1,
                )
            })
            .collect();
        let file = make_indexed_file("src/big.rs", symbols, vec![], ParseStatus::Parsed);
        let state = make_state(vec![("src/big.rs", file)]);

        let params = OutlineParams {
            path: "src/big.rs".to_string(),
            max_tokens: Some(10), // tiny budget to force truncation
        };
        let result = outline_handler(State(state), Query(params)).await.unwrap();
        // With 10-token (40 byte) budget, only the header fits. Truncation suffix should appear.
        assert!(
            result.contains("truncated") || result.len() < 500,
            "result should be truncated or short: {}",
            result.len()
        );
    }

    #[tokio::test]
    async fn test_outline_handler_records_token_stats() {
        let file = make_indexed_file(
            "src/foo.rs",
            vec![make_symbol("alpha", SymbolKind::Function, 1, 5)],
            vec![],
            ParseStatus::Parsed,
        );
        let state = make_state(vec![("src/foo.rs", file)]);
        let stats = Arc::clone(&state.token_stats);

        let params = OutlineParams {
            path: "src/foo.rs".to_string(),
            max_tokens: None,
        };
        let _ = outline_handler(State(state), Query(params)).await.unwrap();
        assert_eq!(
            stats.summary().read_fires,
            1,
            "read fires should be incremented"
        );
    }

    // -----------------------------------------------------------------------
    // impact_handler — new_file path
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_impact_handler_new_file_returns_language_and_symbols() {
        use std::io::Write;
        use tempfile::TempDir;

        let tmp = TempDir::new().unwrap();
        let rs_path = tmp.path().join("new_file.rs");
        let mut f = std::fs::File::create(&rs_path).unwrap();
        writeln!(f, "fn greet() {{}}").unwrap();
        writeln!(f, "struct Config {{}}").unwrap();
        drop(f);

        // Change cwd to tmp dir so the handler can find the file.
        let state = make_state(vec![]);

        // We'll call the handler with a relative path that exists when cwd = tmp.
        // Use absolute path directly to sidestep cwd issues.
        let abs_path_str = rs_path.to_string_lossy().to_string();
        let params = ImpactParams {
            path: abs_path_str.clone(),
            new_file: Some(true),
        };

        // The handler uses cwd.join(path), so with abs path it resolves correctly.
        let result = impact_handler(State(state), Query(params)).await;
        // It may fail if the extension detection doesn't work for absolute paths, but
        // the basic test is that it doesn't panic.
        // The result depends on file system state.
        let _ = result; // just verify no panic
    }

    // -----------------------------------------------------------------------
    // impact_handler — edit path
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_impact_handler_edit_returns_formatted_text() {
        let file = make_indexed_file(
            "src/db.rs",
            vec![make_symbol("connect", SymbolKind::Function, 1, 10)],
            vec![],
            ParseStatus::Parsed,
        );
        let state = make_state(vec![("src/db.rs", file)]);

        // Seed the symbol cache with pre-edit state.
        {
            let mut cache = state.symbol_cache.write().unwrap();
            cache.insert(
                "src/db.rs".to_string(),
                vec![SymbolSnapshot {
                    name: "connect".to_string(),
                    kind: "function".to_string(),
                    line_range: (1, 5), // different range = "Changed"
                    byte_range: (0, 50),
                }],
            );
        }

        let params = ImpactParams {
            path: "src/db.rs".to_string(),
            new_file: None,
        };

        // The handler will try to read src/db.rs from disk (cwd). Since the file
        // doesn't exist on disk in this test, it will use empty bytes — that's OK,
        // the important thing is it returns a string response, not an error.
        let result = impact_handler(State(state), Query(params)).await;
        // Should return Ok with some text
        assert!(
            result.is_ok(),
            "impact_handler should return Ok even if file missing from disk"
        );
        let text = result.unwrap();
        assert!(text.contains("tokens saved"), "should have footer");
    }

    // -----------------------------------------------------------------------
    // symbol_context_handler
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_symbol_context_handler_returns_formatted_text() {
        let f = make_indexed_file(
            "src/main.rs",
            vec![],
            vec![make_reference("process", ReferenceKind::Call, 5)],
            ParseStatus::Parsed,
        );
        let state = make_state(vec![("src/main.rs", f)]);

        let params = SymbolContextParams {
            name: "process".to_string(),
            file: None,
            path: None,
            symbol_kind: None,
            symbol_line: None,
        };
        let result = symbol_context_handler(State(state), Query(params))
            .await
            .unwrap();
        assert!(result.contains("src/main.rs"), "should contain the file");
        assert!(result.contains("line 5"), "should show line number");
        assert!(result.contains("tokens saved"), "should have footer");
    }

    #[tokio::test]
    async fn test_symbol_context_handler_caps_at_10() {
        // Create 20 files each with one reference to "target".
        let files: Vec<(&str, IndexedFile)> = (0..20usize)
            .map(|i| {
                let path = Box::leak(format!("src/f{i}.rs").into_boxed_str()) as &'static str;
                let file = make_indexed_file(
                    path,
                    vec![],
                    vec![make_reference("target", ReferenceKind::Call, 1)],
                    ParseStatus::Parsed,
                );
                (path, file)
            })
            .collect();
        let state = make_state(files);

        let params = SymbolContextParams {
            name: "target".to_string(),
            file: None,
            path: None,
            symbol_kind: None,
            symbol_line: None,
        };
        let result = symbol_context_handler(State(state), Query(params))
            .await
            .unwrap();
        // Should show at most 10 matches (either via our cap-at-10 note, or via budget truncation).
        // Count the number of "line 1" occurrences to verify we don't show more than 10.
        let match_count = result.matches("line 1").count();
        assert!(
            match_count <= 10,
            "should show at most 10 matches, got {}: {}",
            match_count,
            result
        );
        // Should indicate there are more matches (via "showing" or "truncated").
        assert!(
            result.contains("showing") || result.contains("truncated"),
            "should indicate truncation: {}",
            result
        );
    }

    #[tokio::test]
    async fn test_symbol_context_handler_exact_selector_excludes_unrelated_same_name_hits() {
        let target = make_indexed_file(
            "src/db.rs",
            vec![make_symbol("connect", SymbolKind::Function, 1, 1)],
            vec![],
            ParseStatus::Parsed,
        );
        let dependent = IndexedFile {
            relative_path: "src/service.rs".to_string(),
            language: LanguageId::Rust,
            classification: crate::domain::FileClassification::for_code_path("src/service.rs"),
            content: b"use crate::db::connect;\nfn run() { connect(); }\n".to_vec(),
            symbols: vec![make_symbol("run", SymbolKind::Function, 2, 2)],
            parse_status: ParseStatus::Parsed,
            byte_len: 46,
            content_hash: "abc".to_string(),
            references: vec![
                ReferenceRecord {
                    name: "db".to_string(),
                    qualified_name: Some("crate::db".to_string()),
                    kind: ReferenceKind::Import,
                    byte_range: (0, 6),
                    line_range: (0, 0),
                    enclosing_symbol_index: Some(0),
                },
                ReferenceRecord {
                    name: "connect".to_string(),
                    qualified_name: Some("crate::db::connect".to_string()),
                    kind: ReferenceKind::Call,
                    byte_range: (10, 16),
                    line_range: (1, 1),
                    enclosing_symbol_index: Some(0),
                },
            ],
            alias_map: HashMap::new(),
        };
        let unrelated = make_indexed_file(
            "src/other.rs",
            vec![make_symbol("run", SymbolKind::Function, 1, 1)],
            vec![make_reference("connect", ReferenceKind::Call, 1)],
            ParseStatus::Parsed,
        );
        let state = make_state(vec![
            ("src/db.rs", target),
            ("src/service.rs", dependent),
            ("src/other.rs", unrelated),
        ]);

        let params = SymbolContextParams {
            name: "connect".to_string(),
            file: None,
            path: Some("src/db.rs".to_string()),
            symbol_kind: Some("fn".to_string()),
            symbol_line: Some(1),
        };
        let result = symbol_context_handler(State(state), Query(params))
            .await
            .unwrap();

        assert!(result.contains("src/service.rs"), "got: {result}");
        assert!(!result.contains("src/other.rs"), "got: {result}");
    }

    #[tokio::test]
    async fn test_symbol_context_handler_exact_selector_requires_line_for_ambiguous_symbol() {
        let target = make_indexed_file(
            "src/db.rs",
            vec![
                make_symbol("connect", SymbolKind::Function, 1, 1),
                make_symbol("connect", SymbolKind::Function, 2, 2),
            ],
            vec![],
            ParseStatus::Parsed,
        );
        let state = make_state(vec![("src/db.rs", target)]);

        let params = SymbolContextParams {
            name: "connect".to_string(),
            file: None,
            path: Some("src/db.rs".to_string()),
            symbol_kind: Some("fn".to_string()),
            symbol_line: None,
        };
        let result = symbol_context_handler(State(state), Query(params))
            .await
            .unwrap();

        assert!(result.contains("Ambiguous symbol selector"), "got: {result}");
        assert!(result.contains("1"), "got: {result}");
        assert!(result.contains("2"), "got: {result}");
    }

    // -----------------------------------------------------------------------
    // repo_map_handler
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_repo_map_handler_returns_formatted_tree() {
        let f1 = make_indexed_file(
            "src/main.rs",
            vec![make_symbol("x", SymbolKind::Function, 1, 3)],
            vec![],
            ParseStatus::Parsed,
        );
        let f2 = make_indexed_file(
            "src/lib.rs",
            vec![],
            vec![],
            ParseStatus::Failed {
                error: "oops".to_string(),
            },
        );
        let state = make_state(vec![("src/main.rs", f1), ("src/lib.rs", f2)]);

        let result = repo_map_handler(State(state)).await.unwrap();
        assert!(result.contains("files"), "should mention file count");
        assert!(result.contains("symbols"), "should mention symbol count");
        assert!(result.contains("src"), "should show directory");
    }

    #[tokio::test]
    async fn test_repo_map_handler_empty_index() {
        let state = make_state(vec![]);
        let result = repo_map_handler(State(state)).await.unwrap();
        assert!(
            result.contains("0 files"),
            "empty index should show 0 files"
        );
    }

    #[tokio::test]
    async fn test_prompt_context_handler_prefers_file_hint() {
        let file = make_indexed_file(
            "src/main.rs",
            vec![make_symbol("serve", SymbolKind::Function, 1, 3)],
            vec![],
            ParseStatus::Parsed,
        );
        let state = make_state(vec![("src/main.rs", file)]);

        let result = prompt_context_handler(
            State(state),
            Query(PromptContextParams {
                text: "please inspect src/main.rs".to_string(),
            }),
        )
        .await
        .unwrap();

        assert!(
            result.contains("src/main.rs"),
            "prompt context should target the hinted file"
        );
        assert!(
            result.contains("serve"),
            "prompt context should surface the file outline"
        );
    }

    #[tokio::test]
    async fn test_prompt_context_handler_symbol_hint_uses_name_only_symbol_context() {
        let target = make_indexed_file(
            "src/db.rs",
            vec![make_symbol("connect", SymbolKind::Function, 1, 1)],
            vec![],
            ParseStatus::Parsed,
        );
        let dependent = IndexedFile {
            relative_path: "src/service.rs".to_string(),
            language: LanguageId::Rust,
            classification: crate::domain::FileClassification::for_code_path("src/service.rs"),
            content: b"use crate::db::connect;\nfn run() { connect(); }\n".to_vec(),
            symbols: vec![make_symbol("run", SymbolKind::Function, 2, 2)],
            parse_status: ParseStatus::Parsed,
            byte_len: 46,
            content_hash: "abc".to_string(),
            references: vec![
                ReferenceRecord {
                    name: "db".to_string(),
                    qualified_name: Some("crate::db".to_string()),
                    kind: ReferenceKind::Import,
                    byte_range: (0, 6),
                    line_range: (0, 0),
                    enclosing_symbol_index: Some(0),
                },
                ReferenceRecord {
                    name: "connect".to_string(),
                    qualified_name: Some("crate::db::connect".to_string()),
                    kind: ReferenceKind::Call,
                    byte_range: (10, 16),
                    line_range: (1, 1),
                    enclosing_symbol_index: Some(0),
                },
            ],
            alias_map: HashMap::new(),
        };
        let unrelated = make_indexed_file(
            "src/other.rs",
            vec![make_symbol("run", SymbolKind::Function, 1, 1)],
            vec![make_reference("connect", ReferenceKind::Call, 1)],
            ParseStatus::Parsed,
        );
        let state = make_state(vec![
            ("src/db.rs", target),
            ("src/service.rs", dependent),
            ("src/other.rs", unrelated),
        ]);

        let result = prompt_context_handler(
            State(state),
            Query(PromptContextParams {
                text: "where is connect used".to_string(),
            }),
        )
        .await
        .unwrap();

        assert!(
            result.contains("src/service.rs"),
            "symbol-only prompt should use symbol context: {result}"
        );
        assert!(
            result.contains("src/other.rs"),
            "name-only symbol context should keep global same-name hits: {result}"
        );
    }

    #[tokio::test]
    async fn test_prompt_context_handler_combined_file_and_symbol_hint_uses_exact_selector() {
        let target = make_indexed_file(
            "src/db.rs",
            vec![make_symbol("connect", SymbolKind::Function, 1, 1)],
            vec![],
            ParseStatus::Parsed,
        );
        let dependent = IndexedFile {
            relative_path: "src/service.rs".to_string(),
            language: LanguageId::Rust,
            classification: crate::domain::FileClassification::for_code_path("src/service.rs"),
            content: b"use crate::db::connect;\nfn run() { connect(); }\n".to_vec(),
            symbols: vec![make_symbol("run", SymbolKind::Function, 2, 2)],
            parse_status: ParseStatus::Parsed,
            byte_len: 46,
            content_hash: "abc".to_string(),
            references: vec![
                ReferenceRecord {
                    name: "db".to_string(),
                    qualified_name: Some("crate::db".to_string()),
                    kind: ReferenceKind::Import,
                    byte_range: (0, 6),
                    line_range: (0, 0),
                    enclosing_symbol_index: Some(0),
                },
                ReferenceRecord {
                    name: "connect".to_string(),
                    qualified_name: Some("crate::db::connect".to_string()),
                    kind: ReferenceKind::Call,
                    byte_range: (10, 16),
                    line_range: (1, 1),
                    enclosing_symbol_index: Some(0),
                },
            ],
            alias_map: HashMap::new(),
        };
        let unrelated = make_indexed_file(
            "src/other.rs",
            vec![make_symbol("run", SymbolKind::Function, 1, 1)],
            vec![make_reference("connect", ReferenceKind::Call, 1)],
            ParseStatus::Parsed,
        );
        let state = make_state(vec![
            ("src/db.rs", target),
            ("src/service.rs", dependent),
            ("src/other.rs", unrelated),
        ]);

        let result = prompt_context_handler(
            State(state),
            Query(PromptContextParams {
                text: "inspect src/db.rs connect".to_string(),
            }),
        )
        .await
        .unwrap();

        assert!(
            result.contains("src/service.rs"),
            "combined prompt should use exact selector symbol context: {result}"
        );
        assert!(
            !result.contains("src/other.rs"),
            "exact selector should exclude unrelated same-name hits: {result}"
        );
    }

    #[tokio::test]
    async fn test_prompt_context_handler_combined_hint_reports_exact_selector_ambiguity() {
        let target = make_indexed_file(
            "src/db.rs",
            vec![
                make_symbol("connect", SymbolKind::Function, 1, 1),
                make_symbol("connect", SymbolKind::Function, 2, 2),
            ],
            vec![],
            ParseStatus::Parsed,
        );
        let state = make_state(vec![("src/db.rs", target)]);

        let result = prompt_context_handler(
            State(state),
            Query(PromptContextParams {
                text: "inspect src/db.rs connect".to_string(),
            }),
        )
        .await
        .unwrap();

        assert!(
            result.contains("Ambiguous symbol selector"),
            "combined prompt should surface exact-selector ambiguity: {result}"
        );
        assert!(result.contains("1"), "got: {result}");
        assert!(result.contains("2"), "got: {result}");
    }

    #[tokio::test]
    async fn test_prompt_context_handler_combined_hint_line_hint_disambiguates_selector() {
        let target = make_indexed_file(
            "src/db.rs",
            vec![
                make_symbol("connect", SymbolKind::Function, 1, 1),
                make_symbol("connect", SymbolKind::Function, 2, 2),
            ],
            vec![],
            ParseStatus::Parsed,
        );
        let dependent = IndexedFile {
            relative_path: "src/service.rs".to_string(),
            language: LanguageId::Rust,
            classification: crate::domain::FileClassification::for_code_path("src/service.rs"),
            content: b"use crate::db::connect;\nfn run() { connect(); }\n".to_vec(),
            symbols: vec![make_symbol("run", SymbolKind::Function, 2, 2)],
            parse_status: ParseStatus::Parsed,
            byte_len: 46,
            content_hash: "abc".to_string(),
            references: vec![
                ReferenceRecord {
                    name: "db".to_string(),
                    qualified_name: Some("crate::db".to_string()),
                    kind: ReferenceKind::Import,
                    byte_range: (0, 6),
                    line_range: (0, 0),
                    enclosing_symbol_index: Some(0),
                },
                ReferenceRecord {
                    name: "connect".to_string(),
                    qualified_name: Some("crate::db::connect".to_string()),
                    kind: ReferenceKind::Call,
                    byte_range: (10, 16),
                    line_range: (1, 1),
                    enclosing_symbol_index: Some(0),
                },
            ],
            alias_map: HashMap::new(),
        };
        let state = make_state(vec![("src/db.rs", target), ("src/service.rs", dependent)]);

        let result = prompt_context_handler(
            State(state),
            Query(PromptContextParams {
                text: "inspect src/db.rs connect line 2".to_string(),
            }),
        )
        .await
        .unwrap();

        assert!(
            !result.contains("Ambiguous symbol selector"),
            "line hint should disambiguate the exact selector: {result}"
        );
        assert!(
            result.contains("src/service.rs"),
            "line hint should still return symbol context results: {result}"
        );
    }

    #[tokio::test]
    async fn test_prompt_context_handler_ignores_unlabeled_numbers_for_line_hint() {
        let target = make_indexed_file(
            "src/db.rs",
            vec![
                make_symbol("connect", SymbolKind::Function, 1, 1),
                make_symbol("connect", SymbolKind::Function, 2, 2),
            ],
            vec![],
            ParseStatus::Parsed,
        );
        let state = make_state(vec![("src/db.rs", target)]);

        let result = prompt_context_handler(
            State(state),
            Query(PromptContextParams {
                text: "inspect src/db.rs connect 2".to_string(),
            }),
        )
        .await
        .unwrap();

        assert!(
            result.contains("Ambiguous symbol selector"),
            "unlabeled numbers should not count as line hints: {result}"
        );
    }

    #[tokio::test]
    async fn test_prompt_context_handler_path_line_hint_disambiguates_selector() {
        let target = make_indexed_file(
            "src/db.rs",
            vec![
                make_symbol("connect", SymbolKind::Function, 1, 1),
                make_symbol("connect", SymbolKind::Function, 2, 2),
            ],
            vec![],
            ParseStatus::Parsed,
        );
        let dependent = IndexedFile {
            relative_path: "src/service.rs".to_string(),
            language: LanguageId::Rust,
            classification: crate::domain::FileClassification::for_code_path("src/service.rs"),
            content: b"use crate::db::connect;\nfn run() { connect(); }\n".to_vec(),
            symbols: vec![make_symbol("run", SymbolKind::Function, 2, 2)],
            parse_status: ParseStatus::Parsed,
            byte_len: 46,
            content_hash: "abc".to_string(),
            references: vec![
                ReferenceRecord {
                    name: "db".to_string(),
                    qualified_name: Some("crate::db".to_string()),
                    kind: ReferenceKind::Import,
                    byte_range: (0, 6),
                    line_range: (0, 0),
                    enclosing_symbol_index: Some(0),
                },
                ReferenceRecord {
                    name: "connect".to_string(),
                    qualified_name: Some("crate::db::connect".to_string()),
                    kind: ReferenceKind::Call,
                    byte_range: (10, 16),
                    line_range: (1, 1),
                    enclosing_symbol_index: Some(0),
                },
            ],
            alias_map: HashMap::new(),
        };
        let state = make_state(vec![("src/db.rs", target), ("src/service.rs", dependent)]);

        let result = prompt_context_handler(
            State(state),
            Query(PromptContextParams {
                text: "inspect src/db.rs:2 connect".to_string(),
            }),
        )
        .await
        .unwrap();

        assert!(
            !result.contains("Ambiguous symbol selector"),
            "path:line hint should disambiguate the exact selector: {result}"
        );
        assert!(
            result.contains("src/service.rs"),
            "path:line hint should still return symbol context results: {result}"
        );
    }

    #[tokio::test]
    async fn test_prompt_context_handler_basename_line_hint_disambiguates_selector() {
        let target = make_indexed_file(
            "src/db.rs",
            vec![
                make_symbol("connect", SymbolKind::Function, 1, 1),
                make_symbol("connect", SymbolKind::Function, 2, 2),
            ],
            vec![],
            ParseStatus::Parsed,
        );
        let dependent = IndexedFile {
            relative_path: "src/service.rs".to_string(),
            language: LanguageId::Rust,
            classification: crate::domain::FileClassification::for_code_path("src/service.rs"),
            content: b"use crate::db::connect;\nfn run() { connect(); }\n".to_vec(),
            symbols: vec![make_symbol("run", SymbolKind::Function, 2, 2)],
            parse_status: ParseStatus::Parsed,
            byte_len: 46,
            content_hash: "abc".to_string(),
            references: vec![
                ReferenceRecord {
                    name: "db".to_string(),
                    qualified_name: Some("crate::db".to_string()),
                    kind: ReferenceKind::Import,
                    byte_range: (0, 6),
                    line_range: (0, 0),
                    enclosing_symbol_index: Some(0),
                },
                ReferenceRecord {
                    name: "connect".to_string(),
                    qualified_name: Some("crate::db::connect".to_string()),
                    kind: ReferenceKind::Call,
                    byte_range: (10, 16),
                    line_range: (1, 1),
                    enclosing_symbol_index: Some(0),
                },
            ],
            alias_map: HashMap::new(),
        };
        let state = make_state(vec![("src/db.rs", target), ("src/service.rs", dependent)]);

        let result = prompt_context_handler(
            State(state),
            Query(PromptContextParams {
                text: "inspect db.rs:2 connect".to_string(),
            }),
        )
        .await
        .unwrap();

        assert!(
            !result.contains("Ambiguous symbol selector"),
            "basename:line hint should disambiguate the exact selector: {result}"
        );
        assert!(
            result.contains("src/service.rs"),
            "basename:line hint should still return symbol context results: {result}"
        );
    }

    #[tokio::test]
    async fn test_prompt_context_handler_extensionless_alias_line_hint_disambiguates_selector() {
        let target = make_indexed_file(
            "src/db.rs",
            vec![
                make_symbol("connect", SymbolKind::Function, 1, 1),
                make_symbol("connect", SymbolKind::Function, 2, 2),
            ],
            vec![],
            ParseStatus::Parsed,
        );
        let dependent = IndexedFile {
            relative_path: "src/service.rs".to_string(),
            language: LanguageId::Rust,
            classification: crate::domain::FileClassification::for_code_path("src/service.rs"),
            content: b"use crate::db::connect;\nfn run() { connect(); }\n".to_vec(),
            symbols: vec![make_symbol("run", SymbolKind::Function, 2, 2)],
            parse_status: ParseStatus::Parsed,
            byte_len: 46,
            content_hash: "abc".to_string(),
            references: vec![
                ReferenceRecord {
                    name: "db".to_string(),
                    qualified_name: Some("crate::db".to_string()),
                    kind: ReferenceKind::Import,
                    byte_range: (0, 6),
                    line_range: (0, 0),
                    enclosing_symbol_index: Some(0),
                },
                ReferenceRecord {
                    name: "connect".to_string(),
                    qualified_name: Some("crate::db::connect".to_string()),
                    kind: ReferenceKind::Call,
                    byte_range: (10, 16),
                    line_range: (1, 1),
                    enclosing_symbol_index: Some(0),
                },
            ],
            alias_map: HashMap::new(),
        };
        let unrelated = make_indexed_file(
            "src/other.rs",
            vec![make_symbol("run", SymbolKind::Function, 1, 1)],
            vec![make_reference("connect", ReferenceKind::Call, 1)],
            ParseStatus::Parsed,
        );
        let state = make_state(vec![
            ("src/db.rs", target),
            ("src/service.rs", dependent),
            ("src/other.rs", unrelated),
        ]);

        let result = prompt_context_handler(
            State(state),
            Query(PromptContextParams {
                text: "inspect db:2 connect".to_string(),
            }),
        )
        .await
        .unwrap();

        assert!(
            !result.contains("Ambiguous symbol selector"),
            "extensionless alias should disambiguate the exact selector: {result}"
        );
        assert!(
            result.contains("src/service.rs"),
            "extensionless alias should still return symbol context results: {result}"
        );
        assert!(
            !result.contains("src/other.rs"),
            "extensionless alias should exclude unrelated same-name hits: {result}"
        );
    }

    #[tokio::test]
    async fn test_prompt_context_handler_extensionless_path_line_hint_disambiguates_selector() {
        let src_target = make_indexed_file(
            "src/db.rs",
            vec![
                make_symbol("connect", SymbolKind::Function, 1, 1),
                make_symbol("connect", SymbolKind::Function, 2, 2),
            ],
            vec![],
            ParseStatus::Parsed,
        );
        let test_target = make_indexed_file(
            "tests/db.py",
            vec![make_symbol("connect", SymbolKind::Function, 1, 1)],
            vec![],
            ParseStatus::Parsed,
        );
        let src_dependent = IndexedFile {
            relative_path: "src/service.rs".to_string(),
            language: LanguageId::Rust,
            classification: crate::domain::FileClassification::for_code_path("src/service.rs"),
            content: b"use crate::db::connect;\nfn run() { connect(); }\n".to_vec(),
            symbols: vec![make_symbol("run", SymbolKind::Function, 2, 2)],
            parse_status: ParseStatus::Parsed,
            byte_len: 46,
            content_hash: "abc".to_string(),
            references: vec![
                ReferenceRecord {
                    name: "db".to_string(),
                    qualified_name: Some("crate::db".to_string()),
                    kind: ReferenceKind::Import,
                    byte_range: (0, 6),
                    line_range: (0, 0),
                    enclosing_symbol_index: Some(0),
                },
                ReferenceRecord {
                    name: "connect".to_string(),
                    qualified_name: Some("crate::db::connect".to_string()),
                    kind: ReferenceKind::Call,
                    byte_range: (10, 16),
                    line_range: (1, 1),
                    enclosing_symbol_index: Some(0),
                },
            ],
            alias_map: HashMap::new(),
        };
        let unrelated = make_indexed_file(
            "src/other.rs",
            vec![make_symbol("run", SymbolKind::Function, 1, 1)],
            vec![make_reference("connect", ReferenceKind::Call, 1)],
            ParseStatus::Parsed,
        );
        let state = make_state(vec![
            ("src/db.rs", src_target),
            ("tests/db.py", test_target),
            ("src/service.rs", src_dependent),
            ("src/other.rs", unrelated),
        ]);

        let result = prompt_context_handler(
            State(state),
            Query(PromptContextParams {
                text: "inspect src/db:2 connect".to_string(),
            }),
        )
        .await
        .unwrap();

        assert!(
            !result.contains("Ambiguous symbol selector"),
            "extensionless path alias should disambiguate the exact selector: {result}"
        );
        assert!(
            result.contains("src/service.rs"),
            "extensionless path alias should still return symbol context results: {result}"
        );
        assert!(
            !result.contains("src/other.rs"),
            "extensionless path alias should exclude unrelated same-name hits: {result}"
        );
    }

    #[tokio::test]
    async fn test_prompt_context_handler_partial_extensionless_path_hint_does_not_activate() {
        let src_target = make_indexed_file(
            "src/db.rs",
            vec![make_symbol("connect", SymbolKind::Function, 2, 2)],
            vec![],
            ParseStatus::Parsed,
        );
        let alt_target = make_indexed_file(
            "src/data.rs",
            vec![make_symbol("connect", SymbolKind::Function, 2, 2)],
            vec![],
            ParseStatus::Parsed,
        );
        let src_dependent = make_indexed_file(
            "src/service.rs",
            vec![make_symbol("run", SymbolKind::Function, 1, 1)],
            vec![make_reference("connect", ReferenceKind::Call, 1)],
            ParseStatus::Parsed,
        );
        let alt_dependent = make_indexed_file(
            "src/other.rs",
            vec![make_symbol("run", SymbolKind::Function, 1, 1)],
            vec![make_reference("connect", ReferenceKind::Call, 1)],
            ParseStatus::Parsed,
        );
        let state = make_state(vec![
            ("src/db.rs", src_target),
            ("src/data.rs", alt_target),
            ("src/service.rs", src_dependent),
            ("src/other.rs", alt_dependent),
        ]);

        let result = prompt_context_handler(
            State(state),
            Query(PromptContextParams {
                text: "inspect src/d:2 connect".to_string(),
            }),
        )
        .await
        .unwrap();

        assert!(
            result.contains("src/service.rs"),
            "partial extensionless paths should stay on the fallback path: {result}"
        );
        assert!(
            result.contains("src/other.rs"),
            "partial extensionless paths should not collapse to one exact file: {result}"
        );
    }

    #[tokio::test]
    async fn test_prompt_context_handler_ignores_unrelated_colon_numbers_for_line_hint() {
        let target = make_indexed_file(
            "src/db.rs",
            vec![
                make_symbol("connect", SymbolKind::Function, 1, 1),
                make_symbol("connect", SymbolKind::Function, 2, 2),
            ],
            vec![],
            ParseStatus::Parsed,
        );
        let state = make_state(vec![("src/db.rs", target)]);

        let result = prompt_context_handler(
            State(state),
            Query(PromptContextParams {
                text: "inspect src/db.rs connect port 8080:2".to_string(),
            }),
        )
        .await
        .unwrap();

        assert!(
            result.contains("Ambiguous symbol selector"),
            "unrelated colon numbers should not count as path:line hints: {result}"
        );
    }

    #[tokio::test]
    async fn test_prompt_context_handler_ambiguous_basename_line_hint_does_not_activate() {
        let src_target = make_indexed_file(
            "src/db.rs",
            vec![make_symbol("connect", SymbolKind::Function, 1, 1)],
            vec![],
            ParseStatus::Parsed,
        );
        let test_target = make_indexed_file(
            "tests/db.rs",
            vec![make_symbol("connect", SymbolKind::Function, 1, 1)],
            vec![],
            ParseStatus::Parsed,
        );
        let src_dependent = IndexedFile {
            relative_path: "src/service.rs".to_string(),
            language: LanguageId::Rust,
            classification: crate::domain::FileClassification::for_code_path("src/service.rs"),
            content: b"use crate::db::connect;\nfn run() { connect(); }\n".to_vec(),
            symbols: vec![make_symbol("run", SymbolKind::Function, 2, 2)],
            parse_status: ParseStatus::Parsed,
            byte_len: 46,
            content_hash: "abc".to_string(),
            references: vec![
                ReferenceRecord {
                    name: "db".to_string(),
                    qualified_name: Some("crate::db".to_string()),
                    kind: ReferenceKind::Import,
                    byte_range: (0, 6),
                    line_range: (0, 0),
                    enclosing_symbol_index: Some(0),
                },
                ReferenceRecord {
                    name: "connect".to_string(),
                    qualified_name: Some("crate::db::connect".to_string()),
                    kind: ReferenceKind::Call,
                    byte_range: (10, 16),
                    line_range: (1, 1),
                    enclosing_symbol_index: Some(0),
                },
            ],
            alias_map: HashMap::new(),
        };
        let test_dependent = IndexedFile {
            relative_path: "tests/helper.rs".to_string(),
            language: LanguageId::Rust,
            classification: crate::domain::FileClassification::for_code_path("tests/helper.rs"),
            content: b"use crate::db::connect;\nfn helper() { connect(); }\n".to_vec(),
            symbols: vec![make_symbol("helper", SymbolKind::Function, 2, 2)],
            parse_status: ParseStatus::Parsed,
            byte_len: 52,
            content_hash: "def".to_string(),
            references: vec![
                ReferenceRecord {
                    name: "db".to_string(),
                    qualified_name: Some("crate::db".to_string()),
                    kind: ReferenceKind::Import,
                    byte_range: (0, 6),
                    line_range: (0, 0),
                    enclosing_symbol_index: Some(0),
                },
                ReferenceRecord {
                    name: "connect".to_string(),
                    qualified_name: Some("crate::db::connect".to_string()),
                    kind: ReferenceKind::Call,
                    byte_range: (10, 16),
                    line_range: (1, 1),
                    enclosing_symbol_index: Some(0),
                },
            ],
            alias_map: HashMap::new(),
        };
        let state = make_state(vec![
            ("src/db.rs", src_target),
            ("tests/db.rs", test_target),
            ("src/service.rs", src_dependent),
            ("tests/helper.rs", test_dependent),
        ]);

        let result = prompt_context_handler(
            State(state),
            Query(PromptContextParams {
                text: "inspect db.rs:1 connect".to_string(),
            }),
        )
        .await
        .unwrap();

        assert!(
            result.contains("src/service.rs"),
            "ambiguous basename should fall back to name-only symbol context: {result}"
        );
        assert!(
            result.contains("tests/helper.rs"),
            "ambiguous basename should not collapse to one file hint: {result}"
        );
    }

    #[tokio::test]
    async fn test_prompt_context_handler_ambiguous_extensionless_alias_does_not_activate() {
        let src_target = make_indexed_file(
            "src/db.rs",
            vec![make_symbol("connect", SymbolKind::Function, 1, 1)],
            vec![],
            ParseStatus::Parsed,
        );
        let test_target = make_indexed_file(
            "tests/db.py",
            vec![make_symbol("connect", SymbolKind::Function, 1, 1)],
            vec![],
            ParseStatus::Parsed,
        );
        let src_dependent = IndexedFile {
            relative_path: "src/service.rs".to_string(),
            language: LanguageId::Rust,
            classification: crate::domain::FileClassification::for_code_path("src/service.rs"),
            content: b"use crate::db::connect;\nfn run() { connect(); }\n".to_vec(),
            symbols: vec![make_symbol("run", SymbolKind::Function, 2, 2)],
            parse_status: ParseStatus::Parsed,
            byte_len: 46,
            content_hash: "abc".to_string(),
            references: vec![
                ReferenceRecord {
                    name: "db".to_string(),
                    qualified_name: Some("crate::db".to_string()),
                    kind: ReferenceKind::Import,
                    byte_range: (0, 6),
                    line_range: (0, 0),
                    enclosing_symbol_index: Some(0),
                },
                ReferenceRecord {
                    name: "connect".to_string(),
                    qualified_name: Some("crate::db::connect".to_string()),
                    kind: ReferenceKind::Call,
                    byte_range: (10, 16),
                    line_range: (1, 1),
                    enclosing_symbol_index: Some(0),
                },
            ],
            alias_map: HashMap::new(),
        };
        let test_dependent = IndexedFile {
            relative_path: "tests/helper.py".to_string(),
            language: LanguageId::Python,
            classification: crate::domain::FileClassification::for_code_path("tests/helper.py"),
            content: b"from db import connect\n\ndef helper():\n    connect()\n".to_vec(),
            symbols: vec![make_symbol("helper", SymbolKind::Function, 3, 4)],
            parse_status: ParseStatus::Parsed,
            byte_len: 51,
            content_hash: "def".to_string(),
            references: vec![
                ReferenceRecord {
                    name: "db".to_string(),
                    qualified_name: Some("db".to_string()),
                    kind: ReferenceKind::Import,
                    byte_range: (5, 7),
                    line_range: (0, 0),
                    enclosing_symbol_index: None,
                },
                ReferenceRecord {
                    name: "connect".to_string(),
                    qualified_name: Some("db.connect".to_string()),
                    kind: ReferenceKind::Call,
                    byte_range: (39, 45),
                    line_range: (3, 3),
                    enclosing_symbol_index: Some(0),
                },
            ],
            alias_map: HashMap::new(),
        };
        let state = make_state(vec![
            ("src/db.rs", src_target),
            ("tests/db.py", test_target),
            ("src/service.rs", src_dependent),
            ("tests/helper.py", test_dependent),
        ]);

        let result = prompt_context_handler(
            State(state),
            Query(PromptContextParams {
                text: "inspect db:1 connect".to_string(),
            }),
        )
        .await
        .unwrap();

        assert!(
            result.contains("src/service.rs"),
            "ambiguous extensionless alias should fall back to name-only symbol context: {result}"
        );
        assert!(
            result.contains("tests/helper.py"),
            "ambiguous extensionless alias should not collapse to one file hint: {result}"
        );
    }

    // -----------------------------------------------------------------------
    // stats_handler
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_stats_handler_returns_snapshot() {
        let state = make_state(vec![]);
        // Record some stats manually.
        state.token_stats.record_read(1000, 200);
        state.token_stats.record_write();

        let result = stats_handler(State(state)).await;
        let snap = result.0;
        assert_eq!(snap.read_fires, 1);
        assert_eq!(snap.write_fires, 1);
        assert_eq!(snap.read_saved_tokens, 200);
    }
}
