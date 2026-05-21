use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use symforge::domain::{FileOutcome, LanguageId, SymbolRecord};
use symforge::parsing::process_file;

const DECISION: &str = "CHOOSE_CI_YAML";
const CORPUS_ROOT: &str = "tests/fixtures/repository_intelligence/ci_yaml";

const NORMAL_FIXTURE: &str = "normal/github_ci.yml";
const MALFORMED_FIXTURE: &str = "malformed/unclosed_step.yml";
const LARGE_FIXTURE: &str = "large/matrix_release.yml";
const EMPTY_FIXTURE: &str = "edge/empty.yml";
const COMMENTS_ONLY_FIXTURE: &str = "edge/comments_only.yml";

const REQUIRED_FIXTURES: &[(&str, &str)] = &[
    ("normal", NORMAL_FIXTURE),
    ("malformed", MALFORMED_FIXTURE),
    ("large", LARGE_FIXTURE),
    ("empty", EMPTY_FIXTURE),
    ("edge", COMMENTS_ONLY_FIXTURE),
];

fn corpus_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(CORPUS_ROOT)
}

fn fixture_path(relative: &str) -> PathBuf {
    corpus_root().join(relative)
}

fn read_fixture(relative: &str) -> Vec<u8> {
    let path = fixture_path(relative);
    fs::read(&path).unwrap_or_else(|err| panic!("failed to read {}: {err}", path.display()))
}

fn has_symbol(symbols: &[SymbolRecord], name: &str) -> bool {
    symbols.iter().any(|symbol| symbol.name == name)
}

#[test]
fn sfb27_decision_and_fixture_roles_are_pinned() {
    assert_eq!(DECISION, "CHOOSE_CI_YAML");
    assert!(
        corpus_root().join("CORPUS.md").is_file(),
        "corpus manifest should record the SFB27 decision and rationale"
    );

    let roles: BTreeSet<&str> = REQUIRED_FIXTURES
        .iter()
        .map(|(role, _path)| *role)
        .collect();
    for required in ["normal", "malformed", "large", "empty", "edge"] {
        assert!(roles.contains(required), "missing {required} fixture role");
    }

    for (role, relative) in REQUIRED_FIXTURES {
        assert!(
            fixture_path(relative).is_file(),
            "missing {role} CI/YAML fixture at {relative}"
        );
    }
}

#[test]
fn normal_ci_yaml_fixture_uses_existing_yaml_extractor_shape() {
    let bytes = read_fixture(NORMAL_FIXTURE);
    let result = process_file(NORMAL_FIXTURE, &bytes, LanguageId::Yaml);

    assert_eq!(result.outcome, FileOutcome::Processed);
    for expected in [
        "name",
        "env",
        "jobs",
        "jobs.conventional_commits",
        "jobs.conventional_commits.steps",
        "jobs.conventional_commits.steps[0]",
        "jobs.conventional_commits.steps[0].uses",
        "jobs.rust",
        "jobs.rust.steps[2].run",
        "jobs.npm.steps[2].working-directory",
    ] {
        assert!(
            has_symbol(&result.symbols, expected),
            "missing expected CI/YAML symbol {expected}; got {:?}",
            result
                .symbols
                .iter()
                .map(|symbol| symbol.name.as_str())
                .collect::<Vec<_>>()
        );
    }
}

#[test]
fn malformed_ci_yaml_fixture_fails_loudly() {
    let bytes = read_fixture(MALFORMED_FIXTURE);
    let result = process_file(MALFORMED_FIXTURE, &bytes, LanguageId::Yaml);

    assert!(
        matches!(result.outcome, FileOutcome::Failed { .. }),
        "malformed workflow must not become success: {:?}",
        result.outcome
    );
    assert!(
        result.symbols.is_empty(),
        "malformed workflow should not expose partial fake symbols"
    );
}

#[test]
fn large_ci_yaml_fixture_exercises_broad_workflow_shape() {
    let bytes = read_fixture(LARGE_FIXTURE);
    assert!(
        bytes.len() > 2 * 1024,
        "large fixture should stay large enough for SFB28 breadth checks"
    );

    let result = process_file(LARGE_FIXTURE, &bytes, LanguageId::Yaml);
    assert_eq!(result.outcome, FileOutcome::Processed);
    assert!(
        result.symbols.len() >= 40,
        "large workflow should produce broad structural symbols, got {}",
        result.symbols.len()
    );
    for expected in [
        "jobs.matrix_release",
        "jobs.matrix_release.strategy.matrix.include",
        "jobs.matrix_release.strategy.matrix.include[0]",
        "jobs.matrix_release.strategy.matrix.include[19]",
        "jobs.matrix_release.steps[0].uses",
        "jobs.matrix_release.steps[2].run",
    ] {
        assert!(
            has_symbol(&result.symbols, expected),
            "missing expected large CI/YAML symbol {expected}"
        );
    }
}

#[test]
fn empty_and_comments_only_yaml_fixtures_are_edge_cases_not_failures() {
    for relative in [EMPTY_FIXTURE, COMMENTS_ONLY_FIXTURE] {
        let bytes = read_fixture(relative);
        let result = process_file(relative, &bytes, LanguageId::Yaml);

        assert_eq!(
            result.outcome,
            FileOutcome::Processed,
            "{relative} should be a valid empty/edge workflow fixture"
        );
        assert!(
            result.symbols.is_empty(),
            "{relative} should not invent symbols"
        );
    }
}
