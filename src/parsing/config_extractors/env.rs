use crate::domain::{SymbolKind, SymbolRecord};
use super::{ConfigExtractor, EditCapability, ExtractionOutcome, ExtractionResult};

pub struct EnvExtractor;

impl ConfigExtractor for EnvExtractor {
    fn extract(&self, content: &[u8]) -> ExtractionResult {
        let text = match std::str::from_utf8(content) {
            Ok(s) => s,
            Err(e) => {
                return ExtractionResult {
                    symbols: vec![],
                    outcome: ExtractionOutcome::Failed(format!("Invalid UTF-8: {e}")),
                }
            }
        };

        // Build line-start index table for accurate byte ranges (handles LF and CRLF)
        let mut line_starts: Vec<u32> = vec![0];
        for (i, byte) in content.iter().enumerate() {
            if *byte == b'\n' {
                line_starts.push((i + 1) as u32);
            }
        }

        let mut symbols: Vec<SymbolRecord> = Vec::new();
        let mut sort_order: u32 = 0;

        for (line_idx, line) in text.lines().enumerate() {
            let trimmed = line.trim_end();

            // Skip blank lines and comment lines
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }

            // Must contain '=' to be a valid KEY=value pair
            let eq_pos = match trimmed.find('=') {
                Some(pos) => pos,
                None => continue,
            };

            // Key is everything before '=', trimmed of surrounding spaces
            let key = trimmed[..eq_pos].trim();
            if key.is_empty() {
                continue;
            }

            // Compute byte range for this line (excluding line ending)
            let line_start = line_starts[line_idx];
            let line_byte_len = trimmed.len() as u32;
            let line_end = line_start + line_byte_len;

            symbols.push(SymbolRecord {
                name: key.to_string(),
                kind: SymbolKind::Variable,
                depth: 0,
                sort_order,
                byte_range: (line_start, line_end),
                line_range: (line_idx as u32, line_idx as u32),
                doc_byte_range: None,
            });

            sort_order += 1;
        }

        ExtractionResult {
            symbols,
            outcome: ExtractionOutcome::Ok,
        }
    }

    fn edit_capability(&self) -> EditCapability {
        EditCapability::StructuralEditSafe
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::SymbolKind;

    fn extractor() -> EnvExtractor {
        EnvExtractor
    }

    #[test]
    fn test_basic_key_value() {
        let content = b"DATABASE_URL=postgres://localhost/db\nPORT=3000\n";
        let result = extractor().extract(content);
        assert_eq!(result.symbols.len(), 2);
        assert_eq!(result.symbols[0].name, "DATABASE_URL");
        assert_eq!(result.symbols[1].name, "PORT");
        assert!(matches!(result.symbols[0].kind, SymbolKind::Variable));
        assert!(matches!(result.symbols[1].kind, SymbolKind::Variable));
    }

    #[test]
    fn test_comments_and_blanks_skipped() {
        let content = b"# comment\n\nKEY=value\n";
        let result = extractor().extract(content);
        assert_eq!(result.symbols.len(), 1);
        assert_eq!(result.symbols[0].name, "KEY");
    }

    #[test]
    fn test_no_value_key() {
        let content = b"EMPTY_KEY=\n";
        let result = extractor().extract(content);
        assert_eq!(result.symbols.len(), 1);
        assert_eq!(result.symbols[0].name, "EMPTY_KEY");
    }

    #[test]
    fn test_quoted_value() {
        let content = b"SECRET=\"hello world\"\n";
        let result = extractor().extract(content);
        assert_eq!(result.symbols.len(), 1);
        assert_eq!(result.symbols[0].name, "SECRET");
    }

    #[test]
    fn test_empty_file() {
        let content = b"";
        let result = extractor().extract(content);
        assert_eq!(result.symbols.len(), 0);
    }

    #[test]
    fn test_byte_ranges_cover_full_line() {
        let content = b"A=1\nB=2\n";
        let result = extractor().extract(content);
        assert_eq!(result.symbols.len(), 2);
        let (start, end) = result.symbols[0].byte_range;
        let line_text = std::str::from_utf8(&content[start as usize..end as usize]).unwrap();
        assert_eq!(line_text, "A=1");
    }

    #[test]
    fn test_edit_capability() {
        assert!(matches!(extractor().edit_capability(), EditCapability::StructuralEditSafe));
    }
}
