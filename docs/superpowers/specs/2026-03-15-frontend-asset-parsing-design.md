# Frontend Asset Parsing — Design Spec

**Date**: 2026-03-15
**Sprint**: 12
**Status**: Draft
**Approach**: Tree-sitter grammars via existing parsing pipeline

Validated against Angular 21 template features and PrimeNG 21 theming direction; no design changes required.

## Problem

Tokenizor indexes 16 source languages and 5 config formats, but does not support frontend asset files: `.html` (Angular templates), `.css`, and `.scss`. These are core to Angular/PrimeNG development — Angular templates contain component structure and control flow, CSS/SCSS contain theming via design tokens and custom properties. Without indexing, LLMs fall back to raw file reads for all frontend styling and template work.

## Goal

Add tree-sitter-based parsing for HTML (via Angular-capable grammar), CSS, and SCSS using the existing parsing pipeline. All existing tools (search, navigation, edit) work on these files. Edit capability starts at `TextEditSafe` for all three — no structural edit claims until grammars are validated on real project files.

## Architecture

Same tree-sitter pipeline as existing 16 languages. No new abstractions. Each new language gets:

1. A `LanguageId` variant + extension mapping in `src/domain/index.rs`
2. A grammar crate in `Cargo.toml`
3. An extractor file in `src/parsing/languages/`
4. Match arms in `parse_source()` and `extract_symbols()` dispatch

**Note:** `.html` files are parsed with an Angular-capable grammar (`tree-sitter-angular`) for both plain HTML and Angular templates. The Angular grammar is a superset of standard HTML — plain HTML parses correctly through it. This sprint assumes host `tree-sitter` (0.26) compatibility with the selected grammar versions; if validation fails, grammar versions may be pinned accordingly.

### Languages

| Language | LanguageId | Extensions | Grammar crate | Version |
|----------|-----------|------------|---------------|---------|
| HTML/Angular | `Html` | `.html` | `tree-sitter-angular` | 0.8.4 |
| CSS | `Css` | `.css` | `tree-sitter-css` | 0.25.0 |
| SCSS | `Scss` | `.scss` | `tree-sitter-scss` | 1.0.0 |

Grammar dependencies:
- `tree-sitter-angular` 0.8.4 depends on `tree-sitter ~0.25`, `tree-sitter-html ~0.23`
- `tree-sitter-css` 0.25.0 is the official tree-sitter CSS grammar
- `tree-sitter-scss` 1.0.0 is community-maintained (serenadeai)

**Known ABI risk:** `tree-sitter-angular` declares `tree-sitter ~0.25` but the host is `tree-sitter = "0.26"`. This may cause a compilation failure if the `~0.25` semver range excludes 0.26. Fallback: pin an older compatible version of `tree-sitter-angular`, or fork and update its dependency to accept `~0.26`. The ABI smoke tests (see Testing section) will catch this before any extractor work begins.

**Out of scope:** `.less` (Less CSS) and `.sass` (indented Sass syntax) are not included — no mature tree-sitter grammars available. Can be added in a future sprint if needed.

## Symbol Extraction

### HTML/Angular

| Node type | Symbol name | SymbolKind | Extracted? |
|-----------|------------|------------|------------|
| Top-level elements | Tag name (e.g., `app-header`) | `Other` | Always |
| Custom elements (tag contains `-`) | Tag name | `Other` | Always, regardless of depth |
| `<ng-template>` | `ng-template` | `Other` | Always |
| Control-flow (`@if`, `@for`, `@switch`, `@defer`) | `@if`, `@for`, etc. | `Module` | Always |
| Template ref (`#myRef`) | `myRef` | `Variable` | Always |
| `@let user = expr` | `user` | `Variable` | Current Angular local template vars |
| `@else`, `@else if`, `@empty` | — | — | Not separate symbols; belong to parent block |
| Generic nested HTML (`div`, `span`, `p`, `li`) | — | — | Skip unless top-level |
| Interpolation (`{{ expr }}`) | — | — | Skip (attribute-level, too granular) |
| Bindings (`[prop]`, `(event)`, `[(model)]`) | — | — | Skip (attribute-level) |

**Extraction rule:** Extract top-level elements, custom elements (tag contains `-`), `ng-template`, control-flow blocks, template refs, and `@let` declarations. Skip generic nested HTML tags, subordinate control-flow branches (`@else`, `@empty`), interpolation, and bindings.

### CSS

| Node type | Symbol name | SymbolKind |
|-----------|------------|------------|
| Rule set (`.btn`, `#sidebar`, `:host`) | Full selector text | `Other` |
| Custom property (`--primary-color: ...`) | `--primary-color` | `Variable` |
| `@media (...)` | `@media (...)` | `Module` |
| `@keyframes fade-in` | `@keyframes fade-in` | `Module` |
| `@layer utilities` | `@layer utilities` | `Module` |
| Inner `@keyframes` steps (`0%`, `100%`) | — | — | Skip |

**One symbol per rule block** with full selector list as name. Selector lists like `.btn, .btn-primary, :host { ... }` become a single symbol. Inner `@keyframes` steps are not extracted.

**Note:** `@layer` is a recent CSS feature (2022). Verify that `tree-sitter-css` 0.25.0 includes a node type for `@layer` during ABI smoke testing. If absent, skip `@layer` extraction and add it when the grammar is updated.

### SCSS (extends CSS)

All CSS symbols above, plus:

| Node type | Symbol name | SymbolKind |
|-----------|------------|------------|
| `$variable: value` | `$variable` | `Variable` |
| `@mixin button-base` | `button-base` | `Function` |
| `@function darken-color` | `darken-color` | `Function` |
| `@include` | — | Skip (call site, not definition) |
| `@use` / `@forward` | — | Skip (imports, not definitions) |

## Edit Capability

All three languages start at `TextEditSafe`:

| Language | Capability | Rationale |
|----------|-----------|-----------|
| Html | `TextEditSafe` | Angular grammar is new to the project, needs validation |
| Css | `TextEditSafe` | Selectors safe for scoped edits; structural delete could break cascade |
| Scss | `TextEditSafe` | Community grammar, lower confidence on span accuracy |

**Implementation:** Add a new function `edit_capability_for_language` in `src/parsing/config_extractors/mod.rs` that unifies both config and source-language capability checks:

```rust
pub fn edit_capability_for_language(language: &LanguageId) -> Option<EditCapability> {
    // Config languages — delegate to their extractor
    if let Some(cap) = edit_capability_for(language) {
        return Some(cap);
    }
    // Source languages with restricted editing
    match language {
        LanguageId::Html | LanguageId::Css | LanguageId::Scss => Some(EditCapability::TextEditSafe),
        // All other source languages → None (unrestricted).
        // None means "no capability restriction" — these are mature tree-sitter
        // languages with proven span accuracy and existing edit tool support.
        _ => None,
    }
}
```

This function lives in `config_extractors/mod.rs` because it already owns `EditCapability`. The name `edit_capability_for_language` distinguishes it from the existing `edit_capability_for` (config-only).

The existing `check_config_edit_capability` in `tools.rs` (line ~2476) is renamed to `check_edit_capability` and updated to call `edit_capability_for_language` instead of `edit_capability_for`. The three call sites (in `replace_symbol_body`, `delete_symbol`, `edit_within_symbol`) and the function's internal comment ("Non-config files → no restriction") must all be updated. After the change, Html/Css/Scss source files will be gated at `TextEditSafe`, while all other source languages remain unrestricted.

**Cross-reference extraction:** The `xref::extract_references` function in `src/parsing/xref.rs` has an exhaustive match on `LanguageId` for tree-sitter grammar selection. Html/Css/Scss need `unreachable!()` arms there (same pattern as config languages), since cross-reference extraction is not implemented for these languages in v1. The xref pass will return empty references for these file types.

## ABI Compatibility Validation

**Approach:** "Try it" — add all three grammar crates, run `cargo check`, then run parser smoke tests.

Smoke test per grammar:
1. Create `Parser`
2. `set_language(grammar::LANGUAGE)`
3. Parse a trivial snippet (e.g., `<div></div>`, `.a { }`, `$x: 1;`)
4. Assert parse succeeds and root node is not an error

If any grammar fails to compile or parse, pin a compatible version. This replaces the "dedicated spike" from the original PLAN.md.

## Dependencies

| Crate | Version | Status | Notes |
|-------|---------|--------|-------|
| `tree-sitter-angular` | 0.8.4 | **New** | Depends on `tree-sitter ~0.25`, `tree-sitter-html ~0.23` |
| `tree-sitter-css` | 0.25.0 | **New** | Official grammar |
| `tree-sitter-scss` | 1.0.0 | **New** | Community (serenadeai) |

Transitive: `tree-sitter-html` pulled in by `tree-sitter-angular`.

## Testing

### Unit tests per extractor

**HTML/Angular** (`src/parsing/languages/html.rs`):
- Top-level element extracted
- Custom element (contains `-`) extracted at any depth
- `ng-template` extracted
- `@if` / `@for` control-flow blocks extracted as `Module`
- `@else` / `@empty` NOT extracted as separate symbols
- Template ref `#myRef` extracted as `Variable`
- `@let user = expr` extracted as `Variable`
- Generic nested `div`/`span` skipped
- Interpolation and bindings skipped
- Empty file → zero symbols

**CSS** (`src/parsing/languages/css.rs`):
- Selector block extracted with full selector text as name
- Selector list (multiple selectors) → one symbol
- Custom property `--var` extracted as `Variable`
- `@media` extracted as `Module`
- `@keyframes` outer block extracted, inner steps skipped
- Empty file → zero symbols

**SCSS** (`src/parsing/languages/scss.rs`):
- All CSS tests pass on SCSS content
- `$variable` extracted as `Variable`
- `@mixin` extracted as `Function`
- `@function` extracted as `Function`
- `@include` / `@use` / `@forward` NOT extracted
- Empty file → zero symbols

### ABI smoke tests

One test per grammar: create parser, set language, parse trivial snippet, assert success.

### Integration tests

- Index temp directory with `.html`, `.css`, `.scss` files
- `search_symbols(kind="variable")` finds CSS custom properties and SCSS variables
- `get_file_context(path="styles.scss")` returns structured outline
- `get_file_context(path="app.component.html")` returns Angular template structure
- Edit tools gated at `TextEditSafe` for all three

### Regression

- All existing source-code tests unchanged (932+ lib tests)
- Config file parsing (Sprint 11) unaffected

## Acceptance Criteria

### Indexing
- [ ] `.html` files indexed with Angular-capable grammar
- [ ] `.css` files indexed with official CSS grammar
- [ ] `.scss` files indexed with SCSS grammar
- [ ] `search_symbols(kind="variable")` finds CSS custom properties and SCSS variables
- [ ] `get_file_context` returns structured outline for all three file types

### Angular Template Intelligence
- [ ] Custom elements (tag contains `-`) extracted at any depth
- [ ] Control-flow blocks (`@if`, `@for`, `@switch`, `@defer`) extracted
- [ ] Template refs and `@let` declarations extracted
- [ ] `@else`/`@empty` not separate symbols

### Edit Safety
- [ ] All three languages gated at `TextEditSafe`
- [ ] `replace_symbol_body` returns capability warning for Html/Css/Scss
- [ ] `edit_within_symbol` works on all three

### Compatibility
- [ ] ABI smoke tests pass for all three grammars
- [ ] All existing tests unchanged
- [ ] Config file parsing (Sprint 11) unaffected
