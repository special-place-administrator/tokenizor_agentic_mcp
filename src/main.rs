use std::sync::{Arc, Mutex};

use clap::Parser;
use rmcp::{serve_server, transport};
use tokenizor_agentic_mcp::live_index::persist;
use tokenizor_agentic_mcp::{
    cli, daemon, discovery, live_index, observability, protocol, sidecar, watcher,
};

#[derive(Debug, Clone, PartialEq, Eq)]
enum StartupIndexLogView {
    Ready {
        file_count: usize,
        symbol_count: usize,
        parsed_count: usize,
        partial_parse_count: usize,
        failed_count: usize,
        duration_ms: u64,
    },
    Degraded {
        summary: String,
    },
}

fn startup_index_log_view(
    published: &live_index::PublishedIndexState,
) -> Option<StartupIndexLogView> {
    match published.status {
        live_index::PublishedIndexStatus::Ready => Some(StartupIndexLogView::Ready {
            file_count: published.file_count,
            symbol_count: published.symbol_count,
            parsed_count: published.parsed_count,
            partial_parse_count: published.partial_parse_count,
            failed_count: published.failed_count,
            duration_ms: published.load_duration.as_millis() as u64,
        }),
        live_index::PublishedIndexStatus::Degraded => Some(StartupIndexLogView::Degraded {
            summary: published
                .degraded_summary
                .clone()
                .unwrap_or_else(|| "circuit breaker tripped".to_string()),
        }),
        live_index::PublishedIndexStatus::Empty | live_index::PublishedIndexStatus::Loading => None,
    }
}

fn main() -> anyhow::Result<()> {
    let cli = cli::Cli::parse();
    match cli.command {
        Some(cli::Commands::Init { client }) => cli::init::run_init(client),
        Some(cli::Commands::Daemon) => run_daemon(),
        Some(cli::Commands::Hook { subcommand }) => cli::hook::run_hook(subcommand.as_ref()),
        None => run_mcp_server(),
    }
}

fn run_daemon() -> anyhow::Result<()> {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?
        .block_on(async {
            observability::init_tracing()?;
            daemon::run_daemon_until_shutdown("127.0.0.1").await
        })
}

fn run_mcp_server() -> anyhow::Result<()> {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?
        .block_on(async { run_mcp_server_async().await })
}

async fn run_mcp_server_async() -> anyhow::Result<()> {
    observability::init_tracing()?;

    // INFR-02: Auto-index on startup (configurable via TOKENIZOR_AUTO_INDEX)
    let should_auto_index = std::env::var("TOKENIZOR_AUTO_INDEX")
        .map(|v| v != "false")
        .unwrap_or(true);

    let resolved_root = if should_auto_index {
        discovery::find_project_root()
    } else {
        None
    };

    if let Some(root) = resolved_root.clone() {
        match daemon::connect_or_spawn_session(&root, "mcp-stdio", Some(std::process::id())).await {
            Ok(session) => return run_remote_mcp_server_async(session).await,
            Err(error) => {
                tracing::warn!(
                    root = %root.display(),
                    "daemon-backed startup failed, falling back to local mode: {error}"
                );
            }
        }
    }

    run_local_mcp_server_async(should_auto_index, resolved_root).await
}

async fn run_remote_mcp_server_async(session: daemon::DaemonSessionClient) -> anyhow::Result<()> {
    if let Some(port) = session.port() {
        sidecar::port_file::write_port_file(port)?;
        sidecar::port_file::write_pid_file(std::process::id())?;
        sidecar::port_file::write_session_file(session.session_id())?;
    }

    let heartbeat_client = session.clone();
    let heartbeat_task = tokio::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(15)).await;
            let _ = heartbeat_client.heartbeat().await;
        }
    });

    let server = protocol::TokenizorServer::new_daemon_proxy(session.clone());
    tracing::info!(
        project_id = %session.project_id(),
        session_id = %session.session_id(),
        "starting daemon-backed MCP server on stdio transport"
    );
    let service = serve_server(server, transport::stdio()).await?;

    tokio::select! {
        result = service.waiting() => { result?; }
        _ = tokio::signal::ctrl_c() => {
            tracing::info!("Ctrl+C received, shutting down");
        }
    }

    heartbeat_task.abort();
    let _ = session.close().await;
    sidecar::port_file::cleanup_files();
    tracing::info!("daemon-backed MCP server shut down cleanly");
    Ok(())
}

async fn run_local_mcp_server_async(
    should_auto_index: bool,
    resolved_root: Option<std::path::PathBuf>,
) -> anyhow::Result<()> {
    let (index, project_name, watcher_root) = if let Some(root) = resolved_root {
        tracing::info!(root = %root.display(), "auto-indexing from project root");

        // Try loading from persisted snapshot first (fast path: no re-parsing).
        let index = if let Some(snapshot) = persist::load_snapshot(&root) {
            let file_count = snapshot.files.len();
            // Extract mtime map before consuming snapshot
            let snapshot_mtimes: std::collections::HashMap<String, i64> = snapshot
                .files
                .iter()
                .map(|(k, v)| (k.clone(), v.mtime_secs))
                .collect();

            let live = persist::snapshot_to_live_index(snapshot);
            tracing::info!(
                files = file_count,
                load_source = ?live.load_source(),
                snapshot_verify_state = ?live.snapshot_verify_state(),
                "loaded serialized index from .tokenizor/index.bin"
            );
            let shared: live_index::SharedIndex = live_index::SharedIndexHandle::shared(live);

            // Spawn background verification to reconcile against current disk state.
            let bg_index = shared.clone();
            let bg_root = root.clone();
            tokio::spawn(async move {
                persist::background_verify(bg_index, bg_root, snapshot_mtimes).await;
            });

            shared
        } else {
            // Fall back to full re-index (existing behavior).
            live_index::LiveIndex::load(&root)?
        };

        let published = index.published_state();
        match startup_index_log_view(&published) {
            Some(StartupIndexLogView::Ready {
                file_count,
                symbol_count,
                parsed_count,
                partial_parse_count,
                failed_count,
                duration_ms,
            }) => {
                tracing::info!(
                    files = file_count,
                    symbols = symbol_count,
                    parsed = parsed_count,
                    partial = partial_parse_count,
                    failed = failed_count,
                    duration_ms,
                    "LiveIndex ready"
                );
            }
            Some(StartupIndexLogView::Degraded { summary }) => {
                tracing::error!(%summary, "circuit breaker tripped — index degraded");
            }
            None => {}
        }

        let name = root
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("project")
            .to_string();

        (index, name, Some(root))
    } else {
        if !should_auto_index {
            tracing::info!("TOKENIZOR_AUTO_INDEX=false — starting with empty index");
        } else {
            tracing::info!("no safe project root found — starting with empty index");
        }
        (live_index::LiveIndex::empty(), "project".to_string(), None)
    };

    // Spawn file watcher after initial load (only when auto-index is enabled).
    let watcher_info = Arc::new(Mutex::new(watcher::WatcherInfo::default()));

    if let Some(ref root) = watcher_root {
        let watcher_index = Arc::clone(&index);
        let watcher_root_clone = root.clone();
        let watcher_info_clone = Arc::clone(&watcher_info);
        tokio::spawn(async move {
            watcher::run_watcher(watcher_root_clone, watcher_index, watcher_info_clone).await;
        });
        tracing::info!("file watcher started");
    }

    // Spawn HTTP sidecar after watcher, before MCP serve.
    // The sidecar shares the same Arc<LiveIndex> so mutations are immediately visible.
    let bind_host =
        std::env::var("TOKENIZOR_SIDECAR_BIND").unwrap_or_else(|_| "127.0.0.1".to_string());
    let sidecar_handle = sidecar::spawn_sidecar(Arc::clone(&index), &bind_host).await?;
    tracing::info!(port = sidecar_handle.port, "HTTP sidecar started");

    // Share the sidecar's TokenStats Arc with the MCP server so the health tool
    // can display token savings without an HTTP round-trip.
    let token_stats = Some(sidecar_handle.token_stats);

    // Create MCP server and serve on stdio transport.
    let server = protocol::TokenizorServer::new(
        Arc::clone(&index),
        project_name,
        watcher_info,
        watcher_root.clone(),
        token_stats,
    );
    tracing::info!("starting MCP server on stdio transport");
    let service = serve_server(server, transport::stdio()).await?;

    // Wait for either MCP server shutdown (stdin EOF) or Ctrl+C/SIGTERM.
    tokio::select! {
        result = service.waiting() => { result?; }
        _ = tokio::signal::ctrl_c() => {
            tracing::info!("Ctrl+C received, shutting down");
        }
    }

    tracing::info!("MCP server shut down cleanly");

    // Serialize index to disk on clean shutdown.
    // Only serialize when auto-index is enabled (i.e., we have a real project root).
    if let Some(ref root) = watcher_root {
        match persist::serialize_shared_index(&index, root) {
            Ok(()) => tracing::info!("index serialized to .tokenizor/index.bin"),
            Err(e) => tracing::warn!("failed to serialize index on shutdown: {e}"),
        }
    }

    // Shutdown the sidecar now that the MCP server has exited.
    let _ = sidecar_handle.shutdown_tx.send(());
    tracing::info!("sidecar shutdown signal sent");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{StartupIndexLogView, startup_index_log_view};
    use std::time::{Duration, SystemTime};
    use tokenizor_agentic_mcp::live_index::{
        IndexLoadSource, PublishedIndexState, PublishedIndexStatus, SnapshotVerifyState,
    };

    fn published_state(status: PublishedIndexStatus) -> PublishedIndexState {
        PublishedIndexState {
            generation: 7,
            status,
            degraded_summary: None,
            file_count: 12,
            parsed_count: 10,
            partial_parse_count: 1,
            failed_count: 1,
            symbol_count: 34,
            loaded_at_system: SystemTime::now(),
            load_duration: Duration::from_millis(42),
            load_source: IndexLoadSource::FreshLoad,
            snapshot_verify_state: SnapshotVerifyState::NotNeeded,
            is_empty: false,
        }
    }

    #[test]
    fn test_startup_index_log_view_uses_published_ready_counts() {
        let published = published_state(PublishedIndexStatus::Ready);

        assert_eq!(
            startup_index_log_view(&published),
            Some(StartupIndexLogView::Ready {
                file_count: 12,
                symbol_count: 34,
                parsed_count: 10,
                partial_parse_count: 1,
                failed_count: 1,
                duration_ms: 42,
            })
        );
    }

    #[test]
    fn test_startup_index_log_view_uses_published_degraded_summary() {
        let mut published = published_state(PublishedIndexStatus::Degraded);
        published.degraded_summary = Some("circuit breaker tripped: 3/10 files failed".to_string());

        assert_eq!(
            startup_index_log_view(&published),
            Some(StartupIndexLogView::Degraded {
                summary: "circuit breaker tripped: 3/10 files failed".to_string(),
            })
        );
    }
}
