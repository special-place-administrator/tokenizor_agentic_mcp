//! Smart query routing: natural-language entry point that classifies intent
//! and dispatches to the right specialized tool internally.

/// Classified intent from a natural-language query.
#[derive(Debug)]
pub enum QueryIntent {
    /// "who calls X", "callers of X", "references to X"
    FindCallers {
        symbol: String,
        path: Option<String>,
    },
    /// "where is X defined", "find symbol X"
    FindSymbol { name: String, kind: Option<String> },
    /// "find file X", "where is file X", "path to X"
    FindFile { hint: String },
    /// "what changed", "recent changes", "uncommitted"
    FindChanges,
    /// "how does X work", "explain X", "understand X"
    Understand { concept: String },
    /// Broad explanation query upgraded to a direct symbol walkthrough.
    UnderstandSymbol { symbol: String },
    /// Broad explanation query upgraded to implementation discovery.
    UnderstandImplementations { name: String },
    /// "search for X in code", "grep X", code pattern
    SearchCode { pattern: String },
    /// "what depends on X", "dependents of X"
    FindDependents { target: String },
    /// "implementations of X", "who implements X"
    FindImplementations { name: String },
    /// Fallback: explore the concept
    Explore { query: String },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RouteConfidence {
    Exact,
    Inferred,
    Fallback,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RouteAssessment {
    pub confidence: RouteConfidence,
    pub rationale: &'static str,
    pub suggested_next_step: Option<&'static str>,
}

/// Classify a natural-language query into a routable intent.
pub fn classify_intent(query: &str) -> QueryIntent {
    classify_intent_with_match(query).0
}

/// Strip leading articles ("the", "a", "an") from a query for cleaner routing.
pub(crate) fn strip_leading_articles(q: &str) -> &str {
    let lower = q.as_bytes();
    for (article, len) in &[
        (&b"the "[..], 4),
        (&b"The "[..], 4),
        (&b"a "[..], 2),
        (&b"A "[..], 2),
        (&b"an "[..], 3),
        (&b"An "[..], 3),
    ] {
        if lower.starts_with(article) {
            return &q[*len..];
        }
    }
    q
}

/// Classify a query and report whether it matched an explicit routing phrase.
pub(crate) fn classify_intent_with_match(query: &str) -> (QueryIntent, bool) {
    let q = strip_leading_articles(query.trim());
    let lower = q.to_ascii_lowercase();

    // --- Pattern: "who/what calls X" or "callers of X" or "references to X" ---
    if let Some(sym) = strip_prefix_phrase(
        &lower,
        &[
            "who calls ",
            "what calls ",
            "callers of ",
            "callers for ",
            "references to ",
            "references for ",
            "find references ",
            "usages of ",
            "who uses ",
        ],
    ) {
        let (symbol, path) = clean_symbol_and_optional_path(sym, q);
        return (QueryIntent::FindCallers { symbol, path }, true);
    }

    // --- Pattern: "what depends on X" or "dependents of X" ---
    if let Some(target) = strip_prefix_phrase(
        &lower,
        &[
            "what depends on ",
            "depends on ",
            "dependents of ",
            "dependents for ",
            "who imports ",
            "what imports ",
        ],
    ) {
        return (
            QueryIntent::FindDependents {
                target: clean_symbol_name(target, q),
            },
            true,
        );
    }

    // --- Pattern: "implementations of X" or "who implements X" ---
    if let Some(name) = strip_prefix_phrase(
        &lower,
        &[
            "implementations of ",
            "implementors of ",
            "who implements ",
            "what implements ",
            "implementations for ",
        ],
    ) {
        return (
            QueryIntent::FindImplementations {
                name: clean_symbol_name(name, q),
            },
            true,
        );
    }

    // --- Pattern: "where is X defined" or "find symbol X" or "definition of X" ---
    if let Some(name) = strip_prefix_phrase(
        &lower,
        &[
            "where is ",
            "find symbol ",
            "definition of ",
            "show me ",
            "go to ",
            "jump to ",
            "locate ",
            "find definition ",
            "find function ",
            "find struct ",
            "find class ",
            "find type ",
            "find method ",
            "find enum ",
            "find trait ",
            "find interface ",
        ],
    ) {
        let name = name
            .trim_end_matches(" defined")
            .trim_end_matches(" declaration");
        let (kind, clean_name) = extract_kind_hint(name);
        return (
            QueryIntent::FindSymbol {
                name: clean_symbol_name(clean_name, q),
                kind,
            },
            true,
        );
    }

    // --- Pattern: "find file X" or "path to X" ---
    if let Some(hint) = strip_prefix_phrase(
        &lower,
        &[
            "find file ",
            "path to ",
            "where is file ",
            "locate file ",
            "which file ",
        ],
    ) {
        return (
            QueryIntent::FindFile {
                hint: hint.to_string(),
            },
            true,
        );
    }

    // --- Pattern: "what changed" or "recent changes" ---
    if lower.starts_with("what changed")
        || lower.starts_with("recent changes")
        || lower.starts_with("uncommitted")
        || lower == "changes"
        || lower.starts_with("what's changed")
        || lower.starts_with("show changes")
        || lower.starts_with("git status")
        || lower.starts_with("what did i change")
    {
        return (QueryIntent::FindChanges, true);
    }

    // --- Pattern: "how does X work" or "explain X" or "understand X" ---
    if let Some(concept) = strip_prefix_phrase(
        &lower,
        &[
            "how does ",
            "how do ",
            "explain ",
            "understand ",
            "what is ",
            "what are ",
            "describe ",
            "tell me about ",
            "help me understand ",
            "walk me through ",
        ],
    ) {
        let concept = concept
            .trim_end_matches(" work")
            .trim_end_matches(" works")
            .trim_end_matches("?");
        return (
            QueryIntent::Understand {
                concept: concept.to_string(),
            },
            true,
        );
    }

    // --- Pattern: "search for X" or "grep X" or "find X in code" ---
    if let Some(pattern) = strip_prefix_phrase(
        &lower,
        &[
            "search for ",
            "search ",
            "grep ",
            "find in code ",
            "look for ",
            "find text ",
            "find string ",
        ],
    ) {
        return (
            QueryIntent::SearchCode {
                pattern: pattern.trim_matches('"').trim_matches('\'').to_string(),
            },
            true,
        );
    }

    // --- Heuristic: looks like a file path (contains / or common extensions) ---
    if looks_like_path(q) {
        return (
            QueryIntent::FindFile {
                hint: q.to_string(),
            },
            false,
        );
    }

    // --- Heuristic: looks like a symbol name (CamelCase, snake_case, no spaces) ---
    if looks_like_symbol(q) {
        return (
            QueryIntent::FindSymbol {
                name: q.to_string(),
                kind: None,
            },
            false,
        );
    }

    // --- Heuristic: looks like a code pattern (operators, keywords, brackets) ---
    if looks_like_code_pattern(q) {
        return (
            QueryIntent::SearchCode {
                pattern: q.to_string(),
            },
            false,
        );
    }

    // --- Default: explore the concept ---
    (
        QueryIntent::Explore {
            query: q.to_string(),
        },
        false,
    )
}

pub fn assess_route(intent: &QueryIntent, matched_prefix: bool) -> RouteAssessment {
    match intent {
        QueryIntent::FindCallers { .. } => {
            if matched_prefix {
                RouteAssessment {
                    confidence: RouteConfidence::Exact,
                    rationale: "matched explicit caller/reference phrasing",
                    suggested_next_step: None,
                }
            } else {
                RouteAssessment {
                    confidence: RouteConfidence::Inferred,
                    rationale: "inferred a symbol-centric caller query from the input shape",
                    suggested_next_step: Some(
                        "If this route looks wrong, ask with explicit phrasing like `who calls X` or `references to X`.",
                    ),
                }
            }
        }
        QueryIntent::FindDependents { .. } => {
            if matched_prefix {
                RouteAssessment {
                    confidence: RouteConfidence::Exact,
                    rationale: "matched explicit dependent/import phrasing",
                    suggested_next_step: None,
                }
            } else {
                RouteAssessment {
                    confidence: RouteConfidence::Inferred,
                    rationale: "inferred a dependency-path question from the input shape",
                    suggested_next_step: Some(
                        "If this route looks wrong, ask with explicit phrasing like `what depends on X` or call `find_dependents` directly.",
                    ),
                }
            }
        }
        QueryIntent::FindImplementations { .. } => {
            if matched_prefix {
                RouteAssessment {
                    confidence: RouteConfidence::Exact,
                    rationale: "matched explicit implementation phrasing",
                    suggested_next_step: None,
                }
            } else {
                RouteAssessment {
                    confidence: RouteConfidence::Inferred,
                    rationale: "inferred an implementation query from the symbol-like input",
                    suggested_next_step: Some(
                        "If this route looks wrong, ask with explicit phrasing like `implementations of X`.",
                    ),
                }
            }
        }
        QueryIntent::FindSymbol { .. } => {
            if matched_prefix {
                RouteAssessment {
                    confidence: RouteConfidence::Exact,
                    rationale: "matched explicit definition/lookup phrasing",
                    suggested_next_step: None,
                }
            } else {
                RouteAssessment {
                    confidence: RouteConfidence::Inferred,
                    rationale: "inferred a symbol lookup from a symbol-like query",
                    suggested_next_step: Some(
                        "If this route looks wrong, call `search_files` for paths or `search_text` for literal text instead.",
                    ),
                }
            }
        }
        QueryIntent::FindFile { .. } => {
            if matched_prefix {
                RouteAssessment {
                    confidence: RouteConfidence::Exact,
                    rationale: "matched explicit file/path phrasing",
                    suggested_next_step: None,
                }
            } else {
                RouteAssessment {
                    confidence: RouteConfidence::Inferred,
                    rationale: "inferred a file lookup from path-like input",
                    suggested_next_step: Some(
                        "If this route looks wrong, call `search_text` for literal content or `search_symbols` for code names.",
                    ),
                }
            }
        }
        QueryIntent::FindChanges => RouteAssessment {
            confidence: RouteConfidence::Exact,
            rationale: "matched explicit change/status phrasing",
            suggested_next_step: None,
        },
        QueryIntent::Understand { .. } => {
            if matched_prefix {
                RouteAssessment {
                    confidence: RouteConfidence::Exact,
                    rationale: "matched explicit explanation/understanding phrasing",
                    suggested_next_step: None,
                }
            } else {
                RouteAssessment {
                    confidence: RouteConfidence::Inferred,
                    rationale: "inferred a conceptual exploration from the query wording",
                    suggested_next_step: Some(
                        "If this route looks too broad, ask for a specific symbol, file, or caller relationship.",
                    ),
                }
            }
        }
        QueryIntent::UnderstandSymbol { .. } => RouteAssessment {
            confidence: RouteConfidence::Inferred,
            rationale: "detected an exact indexed symbol inside a broad explanation query",
            suggested_next_step: Some(
                "If this route is too narrow, ask about the wider concept explicitly or call `explore` directly.",
            ),
        },
        QueryIntent::UnderstandImplementations { .. } => RouteAssessment {
            confidence: RouteConfidence::Inferred,
            rationale: "detected a distinctive trait-like symbol inside a broad explanation query about implementations or types",
            suggested_next_step: Some(
                "If this route is too narrow, ask about the wider concept explicitly or call `explore` directly.",
            ),
        },
        QueryIntent::SearchCode { .. } => {
            if matched_prefix {
                RouteAssessment {
                    confidence: RouteConfidence::Exact,
                    rationale: "matched explicit code-search phrasing",
                    suggested_next_step: None,
                }
            } else {
                RouteAssessment {
                    confidence: RouteConfidence::Inferred,
                    rationale: "inferred literal or pattern search from code-like syntax",
                    suggested_next_step: Some(
                        "If this route looks wrong, call `search_symbols` for names or rephrase with `search for ...`.",
                    ),
                }
            }
        }
        QueryIntent::Explore { .. } => RouteAssessment {
            confidence: RouteConfidence::Fallback,
            rationale: "no stronger route matched, so SymForge fell back to conceptual exploration",
            suggested_next_step: Some(
                "If this is too broad, rephrase with a direct intent like `who calls`, `find symbol`, `find file`, or `search for`.",
            ),
        },
    }
}

pub fn route_confidence_label(confidence: RouteConfidence) -> &'static str {
    match confidence {
        RouteConfidence::Exact => "exact",
        RouteConfidence::Inferred => "inferred",
        RouteConfidence::Fallback => "fallback",
    }
}

pub fn route_invocation(intent: &QueryIntent) -> String {
    match intent {
        QueryIntent::FindCallers { symbol, path } => {
            if let Some(path) = path {
                format!("find_references(name=\"{symbol}\", path=\"{path}\")")
            } else {
                format!("find_references(name=\"{symbol}\")")
            }
        }
        QueryIntent::FindSymbol { name, kind } => {
            if let Some(k) = kind {
                format!("search_symbols(query=\"{name}\", kind=\"{k}\")")
            } else {
                format!("search_symbols(query=\"{name}\")")
            }
        }
        QueryIntent::FindFile { hint } => {
            format!("search_files(query=\"{hint}\")")
        }
        QueryIntent::FindChanges => "what_changed(uncommitted=true)".to_string(),
        QueryIntent::Understand { concept } => {
            format!("explore(query=\"{concept}\", depth=2)")
        }
        QueryIntent::UnderstandSymbol { symbol } => {
            format!("get_symbol_context(name=\"{symbol}\")")
        }
        QueryIntent::UnderstandImplementations { name } => {
            format!("find_references(name=\"{name}\", mode=\"implementations\")")
        }
        QueryIntent::SearchCode { pattern } => {
            format!("search_text(query=\"{pattern}\")")
        }
        QueryIntent::FindDependents { target } => {
            format!("find_dependents(path=\"{target}\")")
        }
        QueryIntent::FindImplementations { name } => {
            format!("find_references(name=\"{name}\", mode=\"implementations\")")
        }
        QueryIntent::Explore { query } => {
            format!("explore(query=\"{query}\")")
        }
    }
}

pub fn route_tool_name(intent: &QueryIntent) -> &'static str {
    match intent {
        QueryIntent::FindCallers { .. } => "find_references",
        QueryIntent::FindSymbol { .. } => "search_symbols",
        QueryIntent::FindFile { .. } => "search_files",
        QueryIntent::FindChanges => "what_changed",
        QueryIntent::Understand { .. } => "explore",
        QueryIntent::UnderstandSymbol { .. } => "get_symbol_context",
        QueryIntent::UnderstandImplementations { .. } => "find_references",
        QueryIntent::SearchCode { .. } => "search_text",
        QueryIntent::FindDependents { .. } => "find_dependents",
        QueryIntent::FindImplementations { .. } => "find_references",
        QueryIntent::Explore { .. } => "explore",
    }
}

/// Try each prefix phrase; return the remainder if one matches.
fn strip_prefix_phrase<'a>(lower: &'a str, prefixes: &[&str]) -> Option<&'a str> {
    for prefix in prefixes {
        if let Some(rest) = lower.strip_prefix(prefix) {
            let rest = rest.trim();
            if !rest.is_empty() {
                return Some(rest);
            }
        }
    }
    None
}

/// Extract a kind hint from phrases like "function foo" or "struct Bar".
fn extract_kind_hint(name: &str) -> (Option<String>, &str) {
    let kind_prefixes = [
        ("function ", "fn"),
        ("fn ", "fn"),
        ("struct ", "struct"),
        ("class ", "class"),
        ("type ", "type"),
        ("method ", "method"),
        ("enum ", "enum"),
        ("trait ", "trait"),
        ("interface ", "interface"),
        ("constant ", "constant"),
        ("const ", "constant"),
        ("variable ", "variable"),
        ("var ", "variable"),
    ];
    for (prefix, kind) in &kind_prefixes {
        if let Some(stripped) = name.strip_prefix(prefix) {
            return (Some(kind.to_string()), stripped);
        }
    }
    (None, name)
}

/// Preserve original casing from the user's query for the matched portion.
fn clean_symbol_name(lower_match: &str, original: &str) -> String {
    clean_symbol_and_optional_path(lower_match, original).0
}

fn clean_symbol_and_optional_path(lower_match: &str, original: &str) -> (String, Option<String>) {
    // Try to find the original-cased version in the user's query
    let lower_original = original.to_ascii_lowercase();
    if let Some(pos) = lower_original.find(lower_match) {
        return split_trailing_scope_hint(original[pos..pos + lower_match.len()].trim());
    }
    split_trailing_scope_hint(lower_match.trim())
}

fn split_trailing_scope_hint(value: &str) -> (String, Option<String>) {
    for needle in [" within ", " inside ", " under ", " in "] {
        if let Some((head, tail)) = value.rsplit_once(needle) {
            let head = head.trim();
            let tail = tail.trim();
            if !head.is_empty() && looks_like_scope_hint(tail) {
                return (head.to_string(), Some(tail.to_string()));
            }
        }
    }
    (value.trim().to_string(), None)
}

fn looks_like_scope_hint(value: &str) -> bool {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return false;
    }

    looks_like_path(trimmed)
        || trimmed.contains("::")
        || ((trimmed.contains('/') || trimmed.contains('\\'))
            && trimmed
                .chars()
                .all(|c| c.is_alphanumeric() || matches!(c, '/' | '\\' | '_' | '-' | '.' | ':')))
}

fn looks_like_path(q: &str) -> bool {
    // Contains path separators or file extensions
    (q.contains('/') || q.contains('\\'))
        || q.ends_with(".rs")
        || q.ends_with(".py")
        || q.ends_with(".ts")
        || q.ends_with(".js")
        || q.ends_with(".go")
        || q.ends_with(".java")
        || q.ends_with(".toml")
        || q.ends_with(".yaml")
        || q.ends_with(".yml")
        || q.ends_with(".json")
        || q.ends_with(".md")
}

fn looks_like_symbol(q: &str) -> bool {
    if q.len() < 3 || q.contains(' ') {
        return false;
    }
    if q.chars().all(|c| c == '_') {
        return false;
    }
    // CamelCase: has uppercase not at start, or has underscore (snake_case)
    let has_camel = q.chars().skip(1).any(|c| c.is_uppercase());
    let has_snake = q.contains('_');
    let has_colons = q.contains("::");
    let all_alnum = q
        .chars()
        .all(|c| c.is_alphanumeric() || c == '_' || c == ':');
    all_alnum && (has_camel || has_snake || has_colons)
}

fn looks_like_code_pattern(q: &str) -> bool {
    // Contains operators, brackets, or obvious code syntax
    q.contains("==")
        || q.contains("!=")
        || q.contains("->")
        || q.contains("=>")
        || q.contains("fn ")
        || q.contains("pub ")
        || q.contains("let ")
        || q.contains("def ")
        || q.contains("class ")
        || q.contains("impl ")
        || q.contains("struct ")
        || (q.contains('(') && q.contains(')'))
        || (q.contains('{') && q.contains('}'))
}

/// Describe which tool was routed to, for the LLM to learn the mapping.
pub fn route_description(intent: &QueryIntent) -> String {
    format!("[Routed to: {}]", route_invocation(intent))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_classify_callers() {
        match classify_intent("who calls optimize_deterministic") {
            QueryIntent::FindCallers { symbol, path } => {
                assert_eq!(symbol, "optimize_deterministic");
                assert_eq!(path, None);
            }
            other => panic!("Expected FindCallers, got {:?}", other),
        }
    }

    #[test]
    fn test_classify_callers_references() {
        match classify_intent("references to LiveIndex") {
            QueryIntent::FindCallers { symbol, path } => {
                assert_eq!(symbol, "LiveIndex");
                assert_eq!(path, None);
            }
            other => panic!("Expected FindCallers, got {:?}", other),
        }
    }

    #[test]
    fn test_classify_callers_preserves_trailing_path_scope_hint() {
        match classify_intent("who calls AddCoreServices in src/protocol/tools.rs") {
            QueryIntent::FindCallers { symbol, path } => {
                assert_eq!(symbol, "AddCoreServices");
                assert_eq!(path.as_deref(), Some("src/protocol/tools.rs"));
            }
            other => panic!("Expected FindCallers, got {:?}", other),
        }
    }

    #[test]
    fn test_classify_callers_path_scope_does_not_capture_plain_language() {
        match classify_intent("who calls actor in production") {
            QueryIntent::FindCallers { symbol, path } => {
                assert_eq!(symbol, "actor in production");
                assert_eq!(path, None);
            }
            other => panic!("Expected FindCallers, got {:?}", other),
        }
    }

    #[test]
    fn test_classify_find_symbol() {
        match classify_intent("where is optimize_deterministic defined") {
            QueryIntent::FindSymbol { name, kind } => {
                assert_eq!(name, "optimize_deterministic");
                assert!(kind.is_none());
            }
            other => panic!("Expected FindSymbol, got {:?}", other),
        }
    }

    #[test]
    fn test_classify_find_symbol_with_kind() {
        match classify_intent("find struct LiveIndex") {
            QueryIntent::FindSymbol { name, kind } => {
                assert_eq!(name, "LiveIndex");
                assert_eq!(kind, None);
            }
            other => panic!("Expected FindSymbol, got {:?}", other),
        }
    }

    #[test]
    fn test_classify_find_file() {
        match classify_intent("find file tools.rs") {
            QueryIntent::FindFile { hint } => assert_eq!(hint, "tools.rs"),
            other => panic!("Expected FindFile, got {:?}", other),
        }
    }

    #[test]
    fn test_classify_path_heuristic() {
        match classify_intent("src/protocol/mod.rs") {
            QueryIntent::FindFile { hint } => assert_eq!(hint, "src/protocol/mod.rs"),
            other => panic!("Expected FindFile, got {:?}", other),
        }
    }

    #[test]
    fn test_classify_symbol_heuristic() {
        match classify_intent("LiveIndex") {
            QueryIntent::FindSymbol { name, .. } => assert_eq!(name, "LiveIndex"),
            other => panic!("Expected FindSymbol, got {:?}", other),
        }
    }

    #[test]
    fn test_classify_snake_case_heuristic() {
        match classify_intent("search_symbols_with_options") {
            QueryIntent::FindSymbol { name, .. } => assert_eq!(name, "search_symbols_with_options"),
            other => panic!("Expected FindSymbol, got {:?}", other),
        }
    }

    #[test]
    fn test_classify_changes() {
        match classify_intent("what changed") {
            QueryIntent::FindChanges => {}
            other => panic!("Expected FindChanges, got {:?}", other),
        }
    }

    #[test]
    fn test_classify_understand() {
        match classify_intent("how does the synergy pipeline work") {
            QueryIntent::Understand { concept } => assert_eq!(concept, "the synergy pipeline"),
            other => panic!("Expected Understand, got {:?}", other),
        }
    }

    #[test]
    fn test_classify_search_code() {
        match classify_intent("search for TODO") {
            QueryIntent::SearchCode { pattern } => assert_eq!(pattern, "todo"),
            other => panic!("Expected SearchCode, got {:?}", other),
        }
    }

    #[test]
    fn test_classify_dependents() {
        match classify_intent("what depends on src/protocol/mod.rs") {
            QueryIntent::FindDependents { target } => assert_eq!(target, "src/protocol/mod.rs"),
            other => panic!("Expected FindDependents, got {:?}", other),
        }
    }

    #[test]
    fn test_classify_implementations() {
        match classify_intent("implementations of LlmClient") {
            QueryIntent::FindImplementations { name } => assert_eq!(name, "LlmClient"),
            other => panic!("Expected FindImplementations, got {:?}", other),
        }
    }

    #[test]
    fn test_classify_explore_fallback() {
        match classify_intent("error handling patterns") {
            QueryIntent::Explore { query } => assert_eq!(query, "error handling patterns"),
            other => panic!("Expected Explore, got {:?}", other),
        }
    }

    #[test]
    fn test_route_description() {
        let intent = classify_intent("who calls optimize_deterministic");
        let desc = route_description(&intent);
        assert!(desc.contains("find_references"));
        assert!(desc.contains("optimize_deterministic"));
    }

    #[test]
    fn test_assess_route_exact() {
        let (intent, matched_prefix) =
            classify_intent_with_match("who calls optimize_deterministic");
        let assessment = assess_route(&intent, matched_prefix);
        assert_eq!(assessment.confidence, RouteConfidence::Exact);
        assert_eq!(assessment.suggested_next_step, None);
    }

    #[test]
    fn test_assess_route_inferred() {
        let (intent, matched_prefix) = classify_intent_with_match("LiveIndex");
        let assessment = assess_route(&intent, matched_prefix);
        assert_eq!(assessment.confidence, RouteConfidence::Inferred);
        assert!(assessment.suggested_next_step.is_some());
    }

    #[test]
    fn test_assess_route_fallback() {
        let (intent, matched_prefix) = classify_intent_with_match("error handling patterns");
        let assessment = assess_route(&intent, matched_prefix);
        assert_eq!(assessment.confidence, RouteConfidence::Fallback);
        assert!(assessment.suggested_next_step.is_some());
    }

    #[test]
    fn test_route_invocation() {
        let intent = classify_intent("src/protocol/mod.rs");
        let invocation = route_invocation(&intent);
        assert!(invocation.contains("search_files"));
        assert!(invocation.contains("src/protocol/mod.rs"));
    }

    #[test]
    fn test_assess_route_understand_implementations() {
        let intent = QueryIntent::UnderstandImplementations {
            name: "Actor".to_string(),
        };
        let assessment = assess_route(&intent, false);
        assert_eq!(assessment.confidence, RouteConfidence::Inferred);
        assert!(assessment.rationale.contains("trait-like symbol"));
        assert!(assessment.suggested_next_step.is_some());
    }

    #[test]
    fn test_classify_intent_with_match_reports_explicit_prefix() {
        let (intent, matched_prefix) =
            classify_intent_with_match("who calls optimize_deterministic");
        assert!(matched_prefix);
        assert!(matches!(intent, QueryIntent::FindCallers { .. }));
    }

    #[test]
    fn test_classify_intent_with_match_reports_inferred_symbol() {
        let (intent, matched_prefix) = classify_intent_with_match("LiveIndex");
        assert!(!matched_prefix);
        assert!(matches!(intent, QueryIntent::FindSymbol { .. }));
    }

    #[test]
    fn test_route_invocation_understand_implementations() {
        let intent = QueryIntent::UnderstandImplementations {
            name: "Actor".to_string(),
        };
        let invocation = route_invocation(&intent);
        assert_eq!(
            invocation,
            "find_references(name=\"Actor\", mode=\"implementations\")"
        );
        assert_eq!(route_tool_name(&intent), "find_references");
    }

    #[test]
    fn test_strip_leading_articles() {
        assert_eq!(
            strip_leading_articles("the error handling"),
            "error handling"
        );
        assert_eq!(
            strip_leading_articles("The LiveIndex struct"),
            "LiveIndex struct"
        );
        assert_eq!(strip_leading_articles("a config file"), "config file");
        assert_eq!(strip_leading_articles("an async handler"), "async handler");
        // Should NOT strip when not followed by space
        assert_eq!(strip_leading_articles("theorem"), "theorem");
        assert_eq!(strip_leading_articles("ankle"), "ankle");
    }

    #[test]
    fn test_classify_strips_article_before_routing() {
        // "the error handling" should route the same as "error handling"
        let (intent, _) = classify_intent_with_match("the error handling");
        assert!(matches!(
            intent,
            QueryIntent::Understand { .. } | QueryIntent::Explore { .. }
        ));
    }
}
