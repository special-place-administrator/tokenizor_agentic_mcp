# Config File Parsing — Design Spec

**Date**: 2026-03-14
**Sprint**: 11 (Tier 1)
**Status**: Draft
**Approach**: Native Rust parsers (Approach A)

## Problem

Tokenizor only indexes source code files (16 languages via tree-sitter). Config and doc files (JSON, TOML, YAML, Markdown, .env) are silently skipped at discovery time. LLMs must fall back to built-in Read/Edit tools for these files, losing all Tokenizor benefits (symbol navigation, structured search, targeted edits, token savings).

These files are everywhere in every project — `package.json`, `Cargo.toml`, `docker-compose.yaml`, `README.md`, `.env` — and are read/edited constantly during development sessions.

## Goal

Make JSON, TOML, YAML, Markdown, and .env files first-class citizens in the LiveIndex by producing pseudo-symbols from their structure. All existing tools (search, navigation, edit) work on these files without modification.

## Domain Model Changes

### LanguageId (src/domain/index.rs)

Add variants to the existing enum:

```
Json, Toml, Yaml, Markdown, Env
```

Update `from_extension()`:
- `.json` → Json
- `.toml` → Toml
- `.yaml`, `.yml` → Yaml
- `.md` → Markdown
- `.env` → Env (note: `from_extension` extracts text after the last dot, so `.env` yields `"env"`. Dotenv variants like `.env.local`, `.env.production` yield `"local"`/`"production"` and are **out of scope for v1**. A future enhancement could match by filename prefix instead of extension.)

### SymbolKind (src/domain/index.rs)

Add variants:

```
Key       — JSON/TOML/YAML key-value pairs (name = dot-joined path)
Section   — Markdown headers (name = header text)
```

Reuse existing `Variable` kind for `.env` entries.

### FileClassification (src/domain/index.rs)

`FileClassification` is a struct with fields `class: FileClass`, `is_generated: bool`, `is_test: bool`, `is_vendor: bool`. `FileClass` is an enum with `Code`, `Text`, `Binary`. Add `is_config: bool` field to the struct to distinguish config files from source when needed. Config files get `FileClass::Text` with `is_config: true`.

## Extractor Architecture

### New file: src/parsing/config_extractors.rs

Public dispatch function with five internal extractors:

```rust
pub fn extract_config_symbols(content: &[u8], language: LanguageId) -> Vec<SymbolRecord> {
    match language {
        LanguageId::Json => extract_json(content),
        LanguageId::Toml => extract_toml(content),
        LanguageId::Yaml => extract_yaml(content),
        LanguageId::Markdown => extract_markdown(content),
        LanguageId::Env => extract_env(content),
        _ => vec![],
    }
}
```

### Integration point: src/parsing/mod.rs

In the public entry points `process_file` / `process_file_with_classification` (not the private `parse_source`): if the language is a config type, call `extract_config_symbols` instead of entering the tree-sitter pipeline. This branch must happen before `parse_source` is called, since `parse_source` immediately creates a tree-sitter `Parser`.

### Byte Range Strategy

| Format | Parser | Byte range covers |
|--------|--------|-------------------|
| JSON | `serde_json` + manual byte offset tracking | Key start to value end (including quotes/braces) |
| TOML | `toml_edit` (spans via `Item::span()` → `Option<Range<usize>>`, handle `None`) | Key-value pair including inline comment |
| YAML | `serde_yml` + line-based offset calculation | Key-value line(s) |
| Markdown | Regex line scan for `^#{1,6} ` | Header line to next same-or-higher-level header |
| .env | Line scan for `KEY=value` | Full line |

### Symbol Naming Convention

Dot-joined key paths for structured formats:

```
# JSON/TOML/YAML:
scripts.test          → kind: Key
dependencies.serde    → kind: Key
services.api.ports    → kind: Key

# Arrays:
items[0]              → kind: Key
items[1]              → kind: Key

# Markdown:
Installation          → kind: Section
Installation.Prerequisites → kind: Section (nested via header level)

# .env:
DATABASE_URL          → kind: Variable
```

### Depth and Size Limits

- **Depth cap**: 6 levels for JSON/TOML/YAML (prevents explosion on pathological files)
- **Array cap**: 20 items per array (emit `key[0]` through `key[19]`, skip rest)
- **No cross-references**: Config files produce no `ReferenceRecord` entries. The `references` field on `IndexedFile` is empty for config files.

## Discovery and Watcher

**Zero changes needed.** Both `discover_files` (src/discovery/mod.rs) and `supported_language` (src/watcher/mod.rs) gate on `LanguageId::from_extension()`. Adding the new enum variants and extension mappings is sufficient — config files will be discovered, indexed, and watched automatically.

## Tool Impact

**Zero tool code changes needed.** Tools are extension-agnostic at query time:

- `search_symbols` — works with any `SymbolRecord`. Filter by `kind="key"` or `kind="section"`.
- `get_symbol` / `get_symbol_context` — resolves by name + path. `get_symbol(path="Cargo.toml", name="dependencies.serde")` works.
- `get_file_context` — produces outline from indexed symbols. TOML files show key hierarchy.
- `search_text` — searches raw content with enclosing symbol context.
- `get_file_content` — `around_symbol` resolves to byte range.

### Edit Tools

Expected to work since they operate on byte ranges, not language-specific logic:

- `replace_symbol_body` — resolves symbol, splices bytes, rewrites file. LLM is responsible for valid replacement content (e.g. proper JSON syntax).
- `edit_within_symbol` — scoped find-and-replace within byte range.
- `delete_symbol` — removes byte range. **Known limitation**: deleting a JSON key may leave trailing commas, producing invalid JSON. The tool output should warn the user about this. Acceptable for v1; a future enhancement could add JSON-aware comma cleanup.

### PreToolUse Hook Update

After shipping, update `is_non_source_path` in `src/cli/hook.rs` to remove `.json`, `.toml`, `.yaml`, `.yml`, `.md`, `.env` from the skip list so the PreToolUse hook starts suggesting Tokenizor for these files.

## Dependencies

| Crate | Status | Purpose |
|-------|--------|---------|
| `serde_json` | Already in deps | JSON parsing |
| `toml_edit` | Already in deps | TOML parsing with span preservation |
| `serde_yml` | **New** (~50KB) | YAML parsing (`serde_yaml` is deprecated; `serde_yml` is the maintained successor) |

No new deps for Markdown or .env (regex/line scan).

## Testing Strategy

### Unit tests (in config_extractors.rs)

Per extractor:
- **JSON**: nested objects → correct dot-paths, byte ranges. Depth limit at 6. Array indexing `[0]`..`[19]`, cap at 20.
- **TOML**: tables, inline tables, arrays of tables.
- **YAML**: mappings, sequences, multi-line values.
- **Markdown**: ATX headers levels 1-6, nesting, consecutive headers. Frontmatter (lines between opening and closing `---`) is ignored entirely — not parsed as YAML, not emitted as symbols.
- **.env**: KEY=value, quoted values, comments, blank lines, no-value keys.

### Integration tests (tests/config_files.rs)

- Index temp directory with config files, verify `search_symbols` finds keys.
- `get_symbol` on JSON key path returns correct content.
- `get_file_context` on TOML file returns structured outline.
- `replace_symbol_body` on YAML key writes correct file.
- File watcher picks up config file changes.
- **Update existing test**: `test_discover_files_ignores_json_md_toml` in `src/discovery/mod.rs` explicitly asserts these files are NOT discovered. This test must be updated to expect discovery of config files.

### Edge cases

- Empty files → zero symbols, no crash.
- Malformed JSON/TOML/YAML → `FileOutcome::Failed { error }`, zero symbols, fail-open.
- Deeply nested (>6 levels) → symbols stop at depth 6.
- Large arrays (>20 items) → capped.
- Binary files with `.json` extension → detect and skip.

## Performance

No concern. Config files are tiny compared to source code. A project with 50 config files adds ~500 symbols to an index already containing 3000+. Parsing is sub-millisecond per file.

## Acceptance Criteria

- [ ] `search_symbols(name="dependencies")` finds TOML/JSON dependency keys
- [ ] `get_file_context(path="Cargo.toml")` returns structured outline of keys
- [ ] `get_file_content(path="README.md", around_symbol="Installation")` works
- [ ] `get_symbol(path="package.json", name="scripts.build")` returns the value
- [ ] File watcher re-indexes config files on change
- [ ] PreToolUse hook intercepts config files after this ships
- [ ] All edit tools work on config file symbols (byte-range accuracy)
- [ ] Malformed files fail-open with FileOutcome::Failed, zero symbols
- [ ] Existing discovery test updated to expect config file discovery
