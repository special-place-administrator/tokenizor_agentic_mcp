//! Concept → pattern mapping for the `explore` tool.

/// A set of search patterns associated with a programming concept.
pub struct ConceptPattern {
    pub label: &'static str,
    pub symbol_queries: &'static [&'static str],
    pub text_queries: &'static [&'static str],
    pub kind_filters: &'static [&'static str],
}

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
        "testing",
        ConceptPattern {
            label: "Testing",
            symbol_queries: &["test", "mock", "fixture", "assert", "expect"],
            text_queries: &["#[test]", "#[tokio::test]", "describe(", "it(", "pytest"],
            kind_filters: &["fn"],
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
        "configuration",
        ConceptPattern {
            label: "Configuration",
            symbol_queries: &["config", "settings", "env", "options", "params"],
            text_queries: &["dotenv", "env::var", "process.env", "serde", "toml", "yaml"],
            kind_filters: &["struct"],
        },
    ),
];

/// Find the best matching concept for a query.
pub fn match_concept(query: &str) -> Option<&'static ConceptPattern> {
    let lower = query.to_ascii_lowercase();
    CONCEPT_MAP
        .iter()
        .find(|(key, _)| lower.contains(key))
        .map(|(_, pattern)| pattern)
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
        assert_eq!(concept.unwrap().label, "Error Handling");
    }

    #[test]
    fn test_match_concept_case_insensitive() {
        let concept = match_concept("Error Handling");
        assert!(concept.is_some());
        assert_eq!(concept.unwrap().label, "Error Handling");
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
}
