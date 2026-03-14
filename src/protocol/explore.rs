//! Concept → pattern mapping for the `explore` tool.

/// A set of search patterns associated with a programming concept.
pub struct ConceptPattern {
    pub label: &'static str,
    pub symbol_queries: &'static [&'static str],
    pub text_queries: &'static [&'static str],
    pub kind_filters: &'static [&'static str],
}

// Sorted by key length descending so longer/more-specific keys match first.
pub const CONCEPT_MAP: &[(&str, ConceptPattern)] = &[
    (
        "error handling",
        ConceptPattern {
            label: "Error Handling",
            symbol_queries: &["Error", "Result", "anyhow", "bail", "catch"],
            text_queries: &["unwrap()", ".expect(", "return Err(", "try {", "catch"],
            kind_filters: &["struct", "enum", "fn"],
        },
    ),
    (
        "file watching",
        ConceptPattern {
            label: "File Watching",
            symbol_queries: &["watcher", "notify", "debounce", "event", "burst"],
            text_queries: &["notify::Event", "DebouncedEvent", "file_event", "inotify"],
            kind_filters: &[],
        },
    ),
    (
        "serialization",
        ConceptPattern {
            label: "Serialization",
            symbol_queries: &["serialize", "deserialize", "serde", "json", "postcard"],
            text_queries: &[
                "#[derive(Serialize",
                "#[derive(Deserialize",
                "serde_json",
                "postcard::",
            ],
            kind_filters: &[],
        },
    ),
    (
        "authentication",
        ConceptPattern {
            label: "Authentication",
            symbol_queries: &[
                "auth",
                "login",
                "session",
                "token",
                "credential",
                "password",
            ],
            text_queries: &["Bearer", "JWT", "OAuth", "verify_token", "authenticate"],
            kind_filters: &[],
        },
    ),
    (
        "configuration",
        ConceptPattern {
            label: "Configuration",
            symbol_queries: &["config", "settings", "env", "options", "params"],
            text_queries: &["dotenv", "env::var", "process.env", "serde", "toml", "yaml"],
            kind_filters: &["struct"],
        },
    ),
    (
        "concurrency",
        ConceptPattern {
            label: "Concurrency",
            symbol_queries: &["Mutex", "RwLock", "Atomic", "channel", "spawn", "async"],
            text_queries: &[
                "tokio::spawn",
                "thread::spawn",
                ".lock()",
                ".read()",
                ".write()",
            ],
            kind_filters: &[],
        },
    ),
    (
        "permissions",
        ConceptPattern {
            label: "Permissions / Authorization",
            symbol_queries: &["permission", "role", "policy", "acl", "authorize"],
            text_queries: &["forbidden", "unauthorized", "access_control", "RBAC"],
            kind_filters: &[],
        },
    ),
    (
        "deployment",
        ConceptPattern {
            label: "Deployment / Release",
            symbol_queries: &["release", "deploy", "version", "publish", "migrate"],
            text_queries: &[
                "npm publish",
                "cargo publish",
                "release-please",
                "changelog",
            ],
            kind_filters: &[],
        },
    ),
    (
        "networking",
        ConceptPattern {
            label: "Networking",
            symbol_queries: &["socket", "listener", "bind", "connect", "server"],
            text_queries: &["TcpListener", "hyper", "axum", "reqwest", "tonic"],
            kind_filters: &[],
        },
    ),
    (
        "database",
        ConceptPattern {
            label: "Database",
            symbol_queries: &[
                "query",
                "migrate",
                "schema",
                "pool",
                "connection",
                "transaction",
            ],
            text_queries: &[
                "SELECT",
                "INSERT",
                "CREATE TABLE",
                "sqlx",
                "diesel",
                "TypeORM",
            ],
            kind_filters: &[],
        },
    ),
    (
        "indexing",
        ConceptPattern {
            label: "Indexing",
            symbol_queries: &["index", "reindex", "snapshot", "persist"],
            text_queries: &["LiveIndex", "index.bin", "reindex", "rebuild_reverse"],
            kind_filters: &[],
        },
    ),
    (
        "testing",
        ConceptPattern {
            label: "Testing",
            symbol_queries: &["test", "mock", "fixture", "assert", "expect"],
            text_queries: &["#[test]", "#[tokio::test]", "describe(", "it(", "pytest"],
            kind_filters: &["fn"],
        },
    ),
    (
        "parsing",
        ConceptPattern {
            label: "Parsing",
            symbol_queries: &["parse", "parser", "ast", "node", "tree_sitter"],
            text_queries: &["tree_sitter::", ".parse(", "syntax tree", "grammar"],
            kind_filters: &[],
        },
    ),
    (
        "caching",
        ConceptPattern {
            label: "Caching",
            symbol_queries: &["cache", "lru", "memoize", "ttl", "expire"],
            text_queries: &["LruCache", "cache.get(", "cached::", "moka::"],
            kind_filters: &[],
        },
    ),
    (
        "logging",
        ConceptPattern {
            label: "Logging / Observability",
            symbol_queries: &["log", "trace", "span", "metric", "telemetry"],
            text_queries: &["tracing::", "log::", "debug!", "warn!", "info!"],
            kind_filters: &[],
        },
    ),
    (
        "watcher",
        ConceptPattern {
            label: "File Watching",
            symbol_queries: &["watcher", "notify", "debounce", "event", "burst"],
            text_queries: &["notify::Event", "DebouncedEvent", "file_event", "inotify"],
            kind_filters: &[],
        },
    ),
    (
        "parser",
        ConceptPattern {
            label: "Parsing",
            symbol_queries: &["parse", "parser", "ast", "node", "tree_sitter"],
            text_queries: &["tree_sitter::", ".parse(", "syntax tree", "grammar"],
            kind_filters: &[],
        },
    ),
    (
        "api",
        ConceptPattern {
            label: "API / HTTP",
            symbol_queries: &[
                "handler",
                "route",
                "endpoint",
                "controller",
                "request",
                "response",
            ],
            text_queries: &[
                "GET", "POST", "PUT", "DELETE", "Router", "axum", "actix", "express",
            ],
            kind_filters: &["fn"],
        },
    ),
    (
        "cli",
        ConceptPattern {
            label: "CLI / Command Line",
            symbol_queries: &["cli", "args", "command", "subcommand"],
            text_queries: &["clap", "structopt", "Arg::", "Command::new"],
            kind_filters: &[],
        },
    ),
];

/// Find the best matching concept for a query.
/// Returns the matched key and the corresponding pattern, or `None` if no concept matches.
/// Uses word-boundary matching to avoid substring collisions (e.g. "clinical" should not match "cli").
pub fn match_concept(query: &str) -> Option<(&'static str, &'static ConceptPattern)> {
    let query_words: Vec<&str> = query.split_whitespace().collect();
    CONCEPT_MAP
        .iter()
        .find(|(key, _)| {
            let key_words: Vec<&str> = key.split_whitespace().collect();
            query_words.windows(key_words.len()).any(|window| {
                window
                    .iter()
                    .zip(key_words.iter())
                    .all(|(qw, kw)| qw.eq_ignore_ascii_case(kw))
            })
        })
        .map(|(key, pattern)| (*key, pattern))
}

/// For queries that don't match a concept, split into search terms.
pub fn fallback_terms(query: &str) -> Vec<String> {
    query
        .split_whitespace()
        .map(|w| w.to_lowercase())
        .filter(|w| w.len() >= 2)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_match_concept_finds_error_handling() {
        let concept = match_concept("error handling patterns");
        assert!(concept.is_some());
        assert_eq!(concept.unwrap().1.label, "Error Handling");
    }

    #[test]
    fn test_match_concept_case_insensitive() {
        let concept = match_concept("Error Handling");
        assert!(concept.is_some());
        assert_eq!(concept.unwrap().1.label, "Error Handling");
    }

    #[test]
    fn test_match_concept_returns_none_for_unknown() {
        let concept = match_concept("quantum entanglement");
        assert!(concept.is_none());
    }

    #[test]
    fn test_fallback_terms_splits_query() {
        let terms = fallback_terms("process data handler");
        assert_eq!(terms, vec!["process", "data", "handler"]);
    }

    #[test]
    fn test_fallback_terms_filters_short_words() {
        let terms = fallback_terms("a bb ccc");
        assert_eq!(terms, vec!["bb", "ccc"]);
    }

    #[test]
    fn test_match_concept_word_boundary_no_substring() {
        assert!(match_concept("clinical trial data").is_none());
        assert!(match_concept("capital investment").is_none());
        assert!(match_concept("cli tools").is_some());
        assert!(match_concept("api endpoints").is_some());
    }
}
