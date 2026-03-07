pub mod languages;

use std::panic;

use tree_sitter::Parser;

use crate::domain::{FileOutcome, FileProcessingResult, LanguageId, SymbolRecord};
use crate::storage::digest_hex;

pub fn process_file(
    relative_path: &str,
    bytes: &[u8],
    language: LanguageId,
) -> FileProcessingResult {
    let byte_len = bytes.len() as u64;
    let content_hash = digest_hex(bytes);
    let source = String::from_utf8_lossy(bytes);

    let parse_result = panic::catch_unwind(|| parse_source(&source, &language));

    match parse_result {
        Ok(Ok((symbols, has_error))) => {
            let outcome = if has_error {
                FileOutcome::PartialParse {
                    warning: "tree-sitter reported syntax errors in the parse tree".to_string(),
                }
            } else {
                FileOutcome::Processed
            };
            FileProcessingResult {
                relative_path: relative_path.to_string(),
                language,
                outcome,
                symbols,
                byte_len,
                content_hash,
            }
        }
        Ok(Err(err)) => FileProcessingResult {
            relative_path: relative_path.to_string(),
            language,
            outcome: FileOutcome::Failed {
                error: err.to_string(),
            },
            symbols: vec![],
            byte_len,
            content_hash,
        },
        Err(_panic) => FileProcessingResult {
            relative_path: relative_path.to_string(),
            language,
            outcome: FileOutcome::Failed {
                error: "tree-sitter parser panicked during parsing".to_string(),
            },
            symbols: vec![],
            byte_len,
            content_hash,
        },
    }
}

fn parse_source(
    source: &str,
    language: &LanguageId,
) -> Result<(Vec<SymbolRecord>, bool), String> {
    let mut parser = Parser::new();

    let ts_language = match language {
        LanguageId::Rust => tree_sitter_rust::LANGUAGE.into(),
        LanguageId::Python => tree_sitter_python::LANGUAGE.into(),
        LanguageId::JavaScript => tree_sitter_javascript::LANGUAGE.into(),
        LanguageId::TypeScript => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
        LanguageId::Go => tree_sitter_go::LANGUAGE.into(),
        LanguageId::Java => tree_sitter_java::LANGUAGE.into(),
        _ => return Err(format!("language not yet onboarded for parsing: {language:?}")),
    };

    parser
        .set_language(&ts_language)
        .map_err(|e| format!("failed to set language: {e}"))?;

    let tree = parser
        .parse(source, None)
        .ok_or_else(|| "tree-sitter parse returned None".to_string())?;

    let root = tree.root_node();
    let has_error = root.has_error();
    let symbols = languages::extract_symbols(&root, source, language);

    Ok((symbols, has_error))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{FileOutcome, LanguageId, SymbolKind};

    #[test]
    fn test_process_file_rust_extracts_function() {
        let source = b"fn hello() { }";
        let result = process_file("test.rs", source, LanguageId::Rust);
        assert_eq!(result.outcome, FileOutcome::Processed);
        assert!(!result.symbols.is_empty());
        assert_eq!(result.symbols[0].name, "hello");
        assert_eq!(result.symbols[0].kind, SymbolKind::Function);
    }

    #[test]
    fn test_process_file_python_extracts_function() {
        let source = b"def greet():\n    pass";
        let result = process_file("test.py", source, LanguageId::Python);
        assert_eq!(result.outcome, FileOutcome::Processed);
        assert!(!result.symbols.is_empty());
        assert_eq!(result.symbols[0].name, "greet");
        assert_eq!(result.symbols[0].kind, SymbolKind::Function);
    }

    #[test]
    fn test_process_file_javascript_extracts_function() {
        let source = b"function doStuff() { }";
        let result = process_file("test.js", source, LanguageId::JavaScript);
        assert_eq!(result.outcome, FileOutcome::Processed);
        assert!(!result.symbols.is_empty());
        assert_eq!(result.symbols[0].name, "doStuff");
        assert_eq!(result.symbols[0].kind, SymbolKind::Function);
    }

    #[test]
    fn test_process_file_typescript_extracts_interface() {
        let source = b"interface Greeter { greet(): void; }";
        let result = process_file("test.ts", source, LanguageId::TypeScript);
        assert_eq!(result.outcome, FileOutcome::Processed);
        let interface = result.symbols.iter().find(|s| s.kind == SymbolKind::Interface);
        assert!(interface.is_some());
        assert_eq!(interface.unwrap().name, "Greeter");
    }

    #[test]
    fn test_process_file_go_extracts_function() {
        let source = b"package main\nfunc main() { }";
        let result = process_file("test.go", source, LanguageId::Go);
        assert_eq!(result.outcome, FileOutcome::Processed);
        assert!(!result.symbols.is_empty());
        assert_eq!(result.symbols[0].name, "main");
        assert_eq!(result.symbols[0].kind, SymbolKind::Function);
    }

    #[test]
    fn test_process_file_partial_parse() {
        let source = b"fn broken( { }";
        let result = process_file("bad.rs", source, LanguageId::Rust);
        assert!(matches!(result.outcome, FileOutcome::PartialParse { .. }));
    }

    #[test]
    fn test_process_file_computes_content_hash() {
        let source = b"fn foo() {}";
        let result = process_file("hash_test.rs", source, LanguageId::Rust);
        assert!(!result.content_hash.is_empty());
        assert_eq!(result.content_hash, digest_hex(source));
    }

    #[test]
    fn test_process_file_byte_len() {
        let source = b"fn bar() {}";
        let result = process_file("len.rs", source, LanguageId::Rust);
        assert_eq!(result.byte_len, source.len() as u64);
    }

    #[test]
    fn test_process_file_preserves_relative_path() {
        let result = process_file("src/lib.rs", b"fn x() {}", LanguageId::Rust);
        assert_eq!(result.relative_path, "src/lib.rs");
    }

    #[test]
    fn test_process_file_never_panics_on_adversarial_input() {
        // Verifies the catch_unwind safety net: process_file must ALWAYS
        // return a FileProcessingResult regardless of input, never propagate a panic.
        let cases: &[(&[u8], &str, LanguageId)] = &[
            (b"\xff\xfe\x00\x01", "binary.rs", LanguageId::Rust),
            (b"", "empty.py", LanguageId::Python),
            (&[0u8; 10000], "zeros.js", LanguageId::JavaScript),
            (b"\n\n\n\n\n", "newlines.ts", LanguageId::TypeScript),
            ("\u{200b}\u{200b}".as_bytes(), "zwsp.go", LanguageId::Go),
            (b"\0\0\0fn main() {}\0\0", "null_padded.rs", LanguageId::Rust),
        ];

        for &(source, path, ref lang) in cases {
            let result = process_file(path, source, lang.clone());
            assert_eq!(result.relative_path, path);
            assert_eq!(result.byte_len, source.len() as u64);
            assert!(!result.content_hash.is_empty());
        }
    }
}
