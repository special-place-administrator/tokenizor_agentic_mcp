// TODO: Add inline_test! cases for the remaining language extractors tracked in docs/live-code-backlog.md.
macro_rules! inline_test {
    (
        $name:ident,
        $language:expr,
        $source:expr,
        [$(($kind:expr, $symbol_name:expr)),* $(,)?]
    ) => {
        #[test]
        fn $name() {
            let language: $crate::domain::LanguageId = $language;
            let source: &str = $source;
            let (symbols, has_error, diagnostic, _, _) =
                $crate::parsing::parse_source(source, &language)
                    .expect("inline language test source should parse");

            assert!(
                !has_error,
                "inline language test for {language} reported parse errors: {diagnostic:?}"
            );

            let actual: Vec<($crate::domain::SymbolKind, &str)> = symbols
                .iter()
                .map(|symbol| (symbol.kind, symbol.name.as_str()))
                .collect();
            let expected: Vec<($crate::domain::SymbolKind, &str)> = vec![
                $(($kind, $symbol_name)),*
            ];

            assert_eq!(actual, expected, "symbols extracted for {language}");
        }
    };
}

pub(crate) use inline_test;

#[cfg(test)]
mod systems_backend_tests {
    use crate::domain::{LanguageId, SymbolKind};

    inline_test!(
        go_inline_test_extracts_function,
        LanguageId::Go,
        r#"
package main

func InlineGoProbe() {}
"#,
        [(SymbolKind::Function, "InlineGoProbe")]
    );

    inline_test!(
        java_inline_test_extracts_class,
        LanguageId::Java,
        r#"
public class InlineJavaProbe {}
"#,
        [(SymbolKind::Class, "InlineJavaProbe")]
    );

    inline_test!(
        c_inline_test_extracts_function,
        LanguageId::C,
        r#"
int inline_c_probe(void) { return 0; }
"#,
        [(SymbolKind::Function, "inline_c_probe")]
    );

    inline_test!(
        cpp_inline_test_extracts_function,
        LanguageId::Cpp,
        r#"
int inline_cpp_probe() { return 0; }
"#,
        [(SymbolKind::Function, "inline_cpp_probe")]
    );

    inline_test!(
        csharp_inline_test_extracts_class,
        LanguageId::CSharp,
        r#"
public class InlineCSharpProbe {}
"#,
        [(SymbolKind::Class, "InlineCSharpProbe")]
    );

    inline_test!(
        swift_inline_test_extracts_class,
        LanguageId::Swift,
        r#"
class InlineSwiftProbe {}
"#,
        [(SymbolKind::Class, "InlineSwiftProbe")]
    );
}
