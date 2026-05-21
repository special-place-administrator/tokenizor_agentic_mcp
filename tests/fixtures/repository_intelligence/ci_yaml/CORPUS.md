# SFB27 CI/YAML Repository Intelligence Corpus

Decision: CHOOSE_CI_YAML

## Rationale

CI/YAML is the first non-code repository-intelligence family because the current repository has real GitHub workflow files at `.github/workflows/ci.yml` and `.github/workflows/release.yml`, and `execution/release_ops.py` names `.github/workflows/release.yml` as release source of truth. The implementation path can reuse `src/parsing/config_extractors::ConfigExtractor` and `YamlExtractor`, which already provide key-path symbols, sequence symbols, malformed-file failure diagnostics, empty-file behavior, array caps, and `TextEditSafe` edit capability.

Existing surfaces also already recognize the workflow shape: `src/live_index/rank_signals.rs` treats `.github/workflows/*.yml` and `.github/workflows/*.yaml` as chore anchors for co-change ranking, and `src/protocol/smart_query.rs` recognizes `.yml` and `.yaml` as path-like query targets. By contrast, the current worktree has no `.sql` migration files, no `LanguageId::Sql`, and no SQL fixture corpus. Choosing SQL/migrations first would require a larger parser/language decision before SFB28 could start.

## Pattern To Reuse

Reuse the existing config-extractor path: `LanguageId::Yaml` -> `extractor_for` -> `YamlExtractor` -> `ExtractionOutcome` -> `process_file_with_classification`. Malformed workflows must remain failed or partial with diagnostics; they must not be converted into success. Oversized YAML should reuse admission/degraded metadata-only behavior rather than a bespoke fallback.

## Fixture Targets

- `tests/fixtures/repository_intelligence/ci_yaml/normal/github_ci.yml` - normal GitHub Actions workflow with triggers, env, dependent jobs, checkout/setup actions, working-directory, and cargo/npm commands.
- `tests/fixtures/repository_intelligence/ci_yaml/malformed/unclosed_step.yml` - malformed workflow with an unclosed flow sequence inside a step; expected to fail loudly.
- `tests/fixtures/repository_intelligence/ci_yaml/large/matrix_release.yml` - large but reviewable workflow with a matrix include list beyond the YAML extractor array cap and release-style build/publish steps.
- `tests/fixtures/repository_intelligence/ci_yaml/edge/empty.yml` - empty YAML file; expected to parse successfully with no symbols.
- `tests/fixtures/repository_intelligence/ci_yaml/edge/comments_only.yml` - comments-only YAML file; expected to parse successfully with no symbols.

## SFB28 Entry Point

SFB28 should implement CI/YAML repository intelligence through existing search, outline/context, explanation, and edit-dry-run surfaces before adding public tools. Start by making these fixtures searchable and explainable as workflow facts: workflow name, triggers, permissions, env, jobs, needs, strategy matrix, runs-on, steps, action uses, run commands, and working-directory.
