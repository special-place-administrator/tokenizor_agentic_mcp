use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use rmcp::{
    ErrorData as McpError, ServerHandler,
    model::{CallToolRequestParams, CallToolResult, ErrorCode, NumberOrString},
    service::{RequestContext, serve_directly},
};
use tokenizor_agentic_mcp::application::{ApplicationContext, run_manager::RunManager};
use tokenizor_agentic_mcp::config::{BlobStoreConfig, ControlPlaneBackend, ServerConfig};
use tokenizor_agentic_mcp::domain::{
    IndexRunMode, IndexRunStatus, RepoOutlineResponse, Repository, RepositoryKind,
    RepositoryStatus, ResultEnvelope, RetrievalOutcome, SearchResultItem, TrustLevel,
};
use tokenizor_agentic_mcp::storage::{BlobStore, LocalCasBlobStore};

fn setup_application_env() -> (tempfile::TempDir, ApplicationContext, Arc<dyn BlobStore>) {
    let dir = tempfile::tempdir().unwrap();
    let mut config = ServerConfig::default();
    config.blob_store.root_dir = dir.path().join(".tokenizor");
    config.control_plane.backend = ControlPlaneBackend::InMemory;
    config.runtime.require_ready_control_plane = false;

    let application = ApplicationContext::from_config(config.clone()).unwrap();
    application.initialize_local_storage().unwrap();

    let cas: Arc<dyn BlobStore> = Arc::new(LocalCasBlobStore::new(BlobStoreConfig {
        root_dir: config.blob_store.root_dir.clone(),
    }));
    cas.initialize().unwrap();

    (dir, application, cas)
}

fn register_repo(manager: &RunManager, repo_id: &str, status: RepositoryStatus) {
    let (quarantined_at_unix_ms, quarantine_reason) = if status == RepositoryStatus::Quarantined {
        (
            Some(1_709_827_200_000),
            Some("retrieval trust suspended".to_string()),
        )
    } else {
        (None, None)
    };
    let repo = Repository {
        repo_id: repo_id.to_string(),
        kind: RepositoryKind::Local,
        root_uri: "/tmp/test".to_string(),
        project_identity: "test-project".to_string(),
        project_identity_kind: Default::default(),
        default_branch: None,
        last_known_revision: None,
        status,
        invalidated_at_unix_ms: None,
        invalidation_reason: None,
        quarantined_at_unix_ms,
        quarantine_reason,
    };
    manager.persistence().save_repository(&repo).unwrap();
}

async fn wait_for_run_success(manager: &RunManager, run_id: &str, timeout_ms: u64) {
    let deadline = tokio::time::Instant::now() + tokio::time::Duration::from_millis(timeout_ms);
    loop {
        if tokio::time::Instant::now() >= deadline {
            panic!("timed out waiting for run '{run_id}' to succeed after {timeout_ms}ms");
        }
        let report = manager
            .inspect_run(run_id)
            .unwrap_or_else(|err| panic!("failed to inspect run '{run_id}': {err}"));
        if report.run.status == IndexRunStatus::Succeeded {
            return;
        }
        if report.run.status.is_terminal() {
            panic!(
                "run '{run_id}' terminated with status {:?}: {:?}",
                report.run.status, report.run.error_summary
            );
        }
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    }
}

fn tool_text(result: &CallToolResult) -> &str {
    result
        .content
        .first()
        .and_then(|content| content.raw.as_text())
        .map(|text| text.text.as_str())
        .expect("expected text content")
}

async fn call_tool_via_server_handler(
    server: tokenizor_agentic_mcp::TokenizorServer,
    name: &'static str,
    arguments: serde_json::Map<String, serde_json::Value>,
) -> Result<CallToolResult, McpError> {
    let (server_transport, _client_transport) = tokio::io::duplex(4096);
    let mut running = serve_directly(server, server_transport, None);
    let context = RequestContext::new(NumberOrString::Number(1), running.peer().clone());
    let request = CallToolRequestParams::new(name).with_arguments(arguments);
    let result = ServerHandler::call_tool(running.service(), request, context).await;
    let _ = running.close().await;
    result
}

#[tokio::test]
async fn test_server_handler_call_tool_search_text_success_path() {
    let (_dir, application, cas) = setup_application_env();

    let repo_dir = tempfile::tempdir().unwrap();
    fs::write(
        repo_dir.path().join("main.rs"),
        "fn main() {\n    println!(\"call tool success\");\n}\n",
    )
    .unwrap();

    register_repo(
        application.run_manager().as_ref(),
        "call-tool-success",
        RepositoryStatus::Ready,
    );

    let (run, _progress) = application
        .run_manager()
        .launch_run(
            "call-tool-success",
            IndexRunMode::Full,
            repo_dir.path().to_path_buf(),
            cas,
        )
        .unwrap();

    wait_for_run_success(application.run_manager().as_ref(), &run.run_id, 10_000).await;

    let server = tokenizor_agentic_mcp::TokenizorServer::new(application);
    let result = call_tool_via_server_handler(
        server,
        "search_text",
        serde_json::json!({
            "repo_id": "call-tool-success",
            "query": "println"
        })
        .as_object()
        .unwrap()
        .clone(),
    )
    .await
    .unwrap();

    let payload = tool_text(&result);
    let parsed: ResultEnvelope<Vec<SearchResultItem>> = serde_json::from_str(payload).unwrap();

    assert_eq!(parsed.outcome, RetrievalOutcome::Success);
    assert_eq!(parsed.trust, TrustLevel::Verified);
    assert!(parsed.provenance.is_some(), "missing provenance");
    assert_eq!(
        parsed.provenance.as_ref().unwrap().repo_id,
        "call-tool-success"
    );
    assert!(
        parsed
            .data
            .as_ref()
            .expect("missing data")
            .iter()
            .any(|item| item.line_content.contains("println")),
        "search_text payload should include the indexed line"
    );
}

#[tokio::test]
async fn test_server_handler_call_tool_request_gated_failure_path() {
    let (_dir, application, cas) = setup_application_env();

    let repo_dir = tempfile::tempdir().unwrap();
    fs::write(repo_dir.path().join("main.rs"), "fn main() {}\n").unwrap();

    register_repo(
        application.run_manager().as_ref(),
        "call-tool-gated",
        RepositoryStatus::Ready,
    );

    let (run, _progress) = application
        .run_manager()
        .launch_run(
            "call-tool-gated",
            IndexRunMode::Full,
            repo_dir.path().to_path_buf(),
            cas,
        )
        .unwrap();

    wait_for_run_success(application.run_manager().as_ref(), &run.run_id, 10_000).await;

    application
        .run_manager()
        .invalidate_repository("call-tool-gated", None, Some("forced invalidation"))
        .unwrap();

    let server = tokenizor_agentic_mcp::TokenizorServer::new(application);
    let err = call_tool_via_server_handler(
        server,
        "search_text",
        serde_json::json!({
            "repo_id": "call-tool-gated",
            "query": "main"
        })
        .as_object()
        .unwrap()
        .clone(),
    )
    .await
    .unwrap_err();

    assert_eq!(err.code, ErrorCode::INVALID_PARAMS);
    assert!(
        err.message.contains("request gated"),
        "unexpected error message: {}",
        err.message
    );
    assert!(
        err.message.contains("invalidated"),
        "gate error should explain why the request was rejected: {}",
        err.message
    );
}

struct BenchmarkFixture {
    text_queries: Vec<String>,
    symbol_queries: Vec<String>,
}

fn write_registry_benchmark_fixture(root: &Path, files_per_language: usize) -> BenchmarkFixture {
    let mut text_queries = Vec::new();
    let mut symbol_queries = Vec::new();
    let languages = [
        ("rust", "rs"),
        ("python", "py"),
        ("typescript", "ts"),
        ("go", "go"),
    ];

    for (language, ext) in languages {
        let dir = root.join(language);
        fs::create_dir_all(&dir).unwrap();

        for index in 0..files_per_language {
            let text_token = format!("shared_text_{language}_{index:03}");
            let symbol_name = match ext {
                "rs" => format!("rust_function_{index:03}"),
                "py" => format!("python_function_{index:03}"),
                "ts" => format!("ts_function_{index:03}"),
                "go" => format!("go_function_{index:03}"),
                _ => unreachable!(),
            };

            let source = match ext {
                "rs" => {
                    format!("pub fn {symbol_name}() -> &'static str {{\n    \"{text_token}\"\n}}\n")
                }
                "py" => format!("def {symbol_name}():\n    return \"{text_token}\"\n"),
                "ts" => format!(
                    "export function {symbol_name}(): string {{\n  return \"{text_token}\";\n}}\n"
                ),
                "go" => format!(
                    "package main\n\nfunc {symbol_name}() string {{\n    return \"{text_token}\"\n}}\n"
                ),
                _ => unreachable!(),
            };

            fs::write(dir.join(format!("file_{index:03}.{ext}")), source).unwrap();

            if text_queries.len() < 10 {
                text_queries.push(text_token.clone());
            }
            if symbol_queries.len() < 10 {
                symbol_queries.push(symbol_name.clone());
            }
        }
    }

    BenchmarkFixture {
        text_queries,
        symbol_queries,
    }
}

fn median_ms(values: &[f64]) -> f64 {
    let mut sorted = values.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let mid = sorted.len() / 2;
    if sorted.len() % 2 == 0 {
        (sorted[mid - 1] + sorted[mid]) / 2.0
    } else {
        sorted[mid]
    }
}

fn format_timings(values: &[f64]) -> String {
    values
        .iter()
        .map(|value| format!("{value:.3}"))
        .collect::<Vec<_>>()
        .join(", ")
}

#[tokio::test]
#[ignore = "benchmark evidence for Epic 4.0 gate; run manually when refreshing recorded timings"]
async fn benchmark_registry_read_performance_and_write_report() {
    let (_dir, application, cas) = setup_application_env();
    let repo_dir = tempfile::tempdir().unwrap();
    let fixture = write_registry_benchmark_fixture(repo_dir.path(), 125);

    register_repo(
        application.run_manager().as_ref(),
        "registry-benchmark",
        RepositoryStatus::Ready,
    );

    let (run, _progress) = application
        .run_manager()
        .launch_run(
            "registry-benchmark",
            IndexRunMode::Full,
            repo_dir.path().to_path_buf(),
            cas,
        )
        .unwrap();

    wait_for_run_success(application.run_manager().as_ref(), &run.run_id, 60_000).await;

    // Warm the index before recording timings.
    let _ = application
        .search_text("registry-benchmark", &fixture.text_queries[0])
        .unwrap();
    let _ = application
        .search_symbols("registry-benchmark", &fixture.symbol_queries[0], None)
        .unwrap();
    let _ = application.get_repo_outline("registry-benchmark").unwrap();

    let mut text_timings = Vec::new();
    for query in &fixture.text_queries {
        let started = Instant::now();
        let result = application
            .search_text("registry-benchmark", query)
            .unwrap();
        text_timings.push(started.elapsed().as_secs_f64() * 1000.0);
        assert_eq!(result.outcome, RetrievalOutcome::Success);
        assert!(
            !result.data.as_ref().unwrap().is_empty(),
            "text query '{query}' returned no matches"
        );
    }

    let mut symbol_timings = Vec::new();
    for query in &fixture.symbol_queries {
        let started = Instant::now();
        let result = application
            .search_symbols("registry-benchmark", query, None)
            .unwrap();
        symbol_timings.push(started.elapsed().as_secs_f64() * 1000.0);
        assert_eq!(result.outcome, RetrievalOutcome::Success);
        assert!(
            !result.data.as_ref().unwrap().matches.is_empty(),
            "symbol query '{query}' returned no matches"
        );
    }

    let mut repo_outline_timings = Vec::new();
    for _ in 0..5 {
        let started = Instant::now();
        let result = application.get_repo_outline("registry-benchmark").unwrap();
        repo_outline_timings.push(started.elapsed().as_secs_f64() * 1000.0);
        assert_eq!(result.outcome, RetrievalOutcome::Success);
        let data: &RepoOutlineResponse = result.data.as_ref().unwrap();
        assert_eq!(data.coverage.total_files, 500);
    }

    let text_p50 = median_ms(&text_timings);
    let symbol_p50 = median_ms(&symbol_timings);
    let repo_outline_p50 = median_ms(&repo_outline_timings);
    let text_pass = text_p50 <= 150.0;
    let symbol_pass = symbol_p50 <= 100.0;

    let report_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("_bmad-output/implementation-artifacts/epic3-registry-benchmark.md");
    fs::create_dir_all(report_path.parent().unwrap()).unwrap();
    fs::write(
        &report_path,
        format!(
            "# Epic 3 Registry Benchmark\n\n\
**Date:** 2026-03-08\n\
**Command:** `cargo test --test epic4_hardening benchmark_registry_read_performance_and_write_report -- --ignored --exact --nocapture`\n\n\
## Fixture\n\n\
- Repository id: `registry-benchmark`\n\
- File count: 500\n\
- Language mix: 125 Rust, 125 Python, 125 TypeScript, 125 Go\n\
- Query mix: 10 `search_text`, 10 `search_symbols`, 5 `get_repo_outline`\n\
- Warm-up: one query per operation before timings were recorded\n\n\
## Thresholds\n\n\
- `search_text` p50 <= 150 ms\n\
- `search_symbols` p50 <= 100 ms\n\n\
## Results\n\n\
| Operation | Raw timings (ms) | p50 (ms) | Threshold | Status |\n\
|---|---|---:|---:|---|\n\
| `search_text` | {text_raw} | {text_p50:.3} | 150.000 | {text_status} |\n\
| `search_symbols` | {symbol_raw} | {symbol_p50:.3} | 100.000 | {symbol_status} |\n\
| `get_repo_outline` | {repo_raw} | {repo_p50:.3} | n/a | recorded |\n\n\
## Notes\n\n\
- Timings were collected on a warm local index after the indexing run completed successfully.\n\
- The benchmark records application-layer registry read performance on the generated 500-file mixed-language fixture.\n\
- `get_repo_outline` timings are recorded for visibility even though the Epic 4 gate only enforces thresholds for `search_text` and `search_symbols`.\n",
            text_raw = format_timings(&text_timings),
            text_p50 = text_p50,
            text_status = if text_pass { "pass" } else { "fail" },
            symbol_raw = format_timings(&symbol_timings),
            symbol_p50 = symbol_p50,
            symbol_status = if symbol_pass { "pass" } else { "fail" },
            repo_raw = format_timings(&repo_outline_timings),
            repo_p50 = repo_outline_p50,
        ),
    )
    .unwrap();

    assert!(
        text_pass,
        "search_text p50 exceeded threshold: {text_p50:.3}ms"
    );
    assert!(
        symbol_pass,
        "search_symbols p50 exceeded threshold: {symbol_p50:.3}ms"
    );
}
