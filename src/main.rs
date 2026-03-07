use std::future::Future;

use anyhow::{Result, bail};
use rmcp::{ServiceExt, transport::stdio};
use tokenizor_agentic_mcp::{
    ApplicationContext, ServerConfig, TokenizorServer, domain::DeploymentReport, observability,
};

#[tokio::main]
async fn main() -> Result<()> {
    observability::init_tracing()?;

    match std::env::args().nth(1).as_deref() {
        None | Some("run") => run().await,
        Some("doctor") => doctor(),
        Some("init") => init(),
        Some("attach") => attach(),
        Some("migrate") => migrate(),
        Some("inspect") => inspect(),
        Some("resolve") => resolve(),
        Some(command) => {
            bail!(
                "unknown command `{command}`; expected `run`, `doctor`, `init`, `attach`, `migrate`, `inspect`, or `resolve`"
            )
        }
    }
}

async fn run() -> Result<()> {
    let config = ServerConfig::from_env()?;
    let application = ApplicationContext::from_config(config)?;
    let readiness_application = application.clone();

    guard_and_serve(
        move || Ok(readiness_application.ensure_runtime_ready()?),
        move |_report| async move {
            tracing::info!("starting tokenizor_agentic_mcp");

            let service = TokenizorServer::new(application)
                .serve(stdio())
                .await
                .inspect_err(|error| tracing::error!(?error, "failed to start MCP server"))?;

            service.waiting().await?;
            Ok(())
        },
    )
    .await
}

fn doctor() -> Result<()> {
    let config = ServerConfig::from_env()?;
    let application = ApplicationContext::from_config(config)?;
    let report = application.deployment_report()?;

    println!("{}", serde_json::to_string_pretty(&report)?);

    if report.is_ready() {
        Ok(())
    } else {
        bail!("{}", doctor_failure_message(&report))
    }
}

fn init() -> Result<()> {
    let target_path = std::env::args().nth(2).map(std::path::PathBuf::from);
    let config = ServerConfig::from_env()?;
    let application = ApplicationContext::from_config(config)?;
    let report = application.initialize_repository(target_path)?;

    println!("{}", serde_json::to_string_pretty(&report)?);

    if report.is_ready() {
        Ok(())
    } else {
        bail!("initialization completed with remaining required actions")
    }
}

fn attach() -> Result<()> {
    let target_path = std::env::args().nth(2).map(std::path::PathBuf::from);
    let config = ServerConfig::from_env()?;
    let application = ApplicationContext::from_config(config)?;
    let report = application.attach_workspace(target_path)?;

    println!("{}", serde_json::to_string_pretty(&report)?);

    if report.is_ready() {
        Ok(())
    } else {
        bail!("workspace attachment completed with remaining required actions")
    }
}

fn migrate() -> Result<()> {
    let args = std::env::args().skip(2).collect::<Vec<_>>();
    let (source_path, target_path) = match args.as_slice() {
        [] => (None, None),
        [source_path, target_path] => (
            Some(std::path::PathBuf::from(source_path)),
            Some(std::path::PathBuf::from(target_path)),
        ),
        _ => {
            bail!(
                "migrate expects either no path arguments or an explicit `<from-path> <to-path>` pair"
            )
        }
    };
    let config = ServerConfig::from_env()?;
    let application = ApplicationContext::from_config(config)?;
    let report = application.migrate_registry(source_path, target_path)?;

    println!("{}", serde_json::to_string_pretty(&report)?);

    if report.is_successful() {
        Ok(())
    } else {
        bail!("migration completed with unresolved records; review the JSON report for guidance")
    }
}

fn inspect() -> Result<()> {
    let config = ServerConfig::from_env()?;
    let application = ApplicationContext::from_config(config)?;
    let report = application.inspect_registry()?;

    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

fn resolve() -> Result<()> {
    let target_path = std::env::args().nth(2).map(std::path::PathBuf::from);
    let config = ServerConfig::from_env()?;
    let application = ApplicationContext::from_config(config)?;
    let report = application.resolve_active_context(target_path)?;

    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

fn doctor_failure_message(report: &tokenizor_agentic_mcp::domain::DeploymentReport) -> String {
    format!(
        "deployment readiness is blocked: {}",
        report.blocking_summary()
    )
}

async fn guard_and_serve<R, S, Fut>(readiness_check: R, serve: S) -> Result<()>
where
    R: FnOnce() -> Result<DeploymentReport>,
    S: FnOnce(DeploymentReport) -> Fut,
    Fut: Future<Output = Result<()>>,
{
    let report = readiness_check()?;

    if report.is_ready() {
        tracing::info!(
            control_plane_backend = %report.control_plane_backend,
            check_count = report.checks.len(),
            "runtime readiness confirmed"
        );
    } else {
        tracing::warn!(
            control_plane_backend = %report.control_plane_backend,
            summary = %report.blocking_summary(),
            "runtime readiness enforcement disabled; continuing startup despite blockers"
        );
    }

    serve(report).await
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use tokenizor_agentic_mcp::domain::{ComponentHealth, DeploymentReport, HealthIssueCategory};

    use super::{doctor_failure_message, guard_and_serve};

    #[test]
    fn doctor_failure_message_lists_remediation() {
        let report = DeploymentReport::new(
            "spacetimedb",
            PathBuf::from(".tokenizor"),
            vec![ComponentHealth::error(
                "spacetimedb_cli",
                HealthIssueCategory::Dependency,
                "`spacetimedb` is not installed",
                "Install the SpacetimeDB CLI and ensure it is on PATH.",
            )],
        );

        let message = doctor_failure_message(&report);

        assert!(message.contains("deployment readiness is blocked"));
        assert!(message.contains("spacetimedb_cli"));
        assert!(message.contains("Install the SpacetimeDB CLI"));
    }

    #[tokio::test]
    async fn guard_and_serve_stops_before_serving_when_readiness_fails() {
        let serve_calls = Arc::new(AtomicUsize::new(0));
        let serve_calls_for_closure = serve_calls.clone();

        let result = guard_and_serve(
            || anyhow::bail!("runtime readiness is blocked: spacetimedb_cli"),
            move |_report| async move {
                serve_calls_for_closure.fetch_add(1, Ordering::SeqCst);
                Ok(())
            },
        )
        .await;

        assert!(result.is_err());
        assert_eq!(serve_calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn guard_and_serve_reaches_serve_path_when_readiness_succeeds() {
        let serve_calls = Arc::new(AtomicUsize::new(0));
        let serve_calls_for_closure = serve_calls.clone();

        let result = guard_and_serve(
            || {
                Ok(DeploymentReport::new(
                    "spacetimedb",
                    PathBuf::from(".tokenizor"),
                    vec![ComponentHealth::ok(
                        "spacetimedb_endpoint",
                        HealthIssueCategory::Dependency,
                        "endpoint is reachable",
                    )],
                ))
            },
            move |_report| async move {
                serve_calls_for_closure.fetch_add(1, Ordering::SeqCst);
                Ok(())
            },
        )
        .await;

        assert!(result.is_ok());
        assert_eq!(serve_calls.load(Ordering::SeqCst), 1);
    }
}
