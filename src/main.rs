use std::sync::{Arc, Mutex};

use clap::Parser;
use tokenizor_agentic_mcp::{cli, discovery, live_index, observability, protocol, sidecar, watcher};
use tokenizor_agentic_mcp::live_index::persist;
use rmcp::{serve_server, transport};

fn main() -> anyhow::Result<()> {
    let cli = cli::Cli::parse();
    match cli.command {
        Some(cli::Commands::Init) => cli::init::run_init(),
        Some(cli::Commands::Hook { subcommand }) => cli::hook::run_hook(subcommand.as_ref()),
        None => run_mcp_server(),
    }
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

    let (index, project_name, watcher_root) = if let Some(root) = resolved_root {
        tracing::info!(root = %root.display(), "auto-indexing from project root");

        // Try loading from persisted snapshot first (fast path: no re-parsing).
        let index = if let Some(snapshot) = persist::load_snapshot(&root) {
            let file_count = snapshot.files.len();
            // Extract mtime map before consuming snapshot
            let snapshot_mtimes: std::collections::HashMap<String, i64> = snapshot.files.iter()
                .map(|(k, v)| (k.clone(), v.mtime_secs))
                .collect();

            tracing::info!(files = file_count, "loaded serialized index from .tokenizor/index.bin");
            let live = persist::snapshot_to_live_index(snapshot);
            let shared: live_index::SharedIndex = std::sync::Arc::new(std::sync::RwLock::new(live));

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

        let guard = index.read().expect("lock poisoned");
        match guard.index_state() {
            live_index::IndexState::Ready => {
                let stats = guard.health_stats();
                tracing::info!(
                    files = stats.file_count,
                    symbols = stats.symbol_count,
                    parsed = stats.parsed_count,
                    partial = stats.partial_parse_count,
                    failed = stats.failed_count,
                    duration_ms = stats.load_duration.as_millis() as u64,
                    "LiveIndex ready"
                );
            }
            live_index::IndexState::CircuitBreakerTripped { ref summary } => {
                tracing::error!(%summary, "circuit breaker tripped — index degraded");
            }
            _ => {}
        }
        drop(guard);

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
    let bind_host = std::env::var("TOKENIZOR_SIDECAR_BIND")
        .unwrap_or_else(|_| "127.0.0.1".to_string());
    let sidecar_handle = sidecar::spawn_sidecar(Arc::clone(&index), &bind_host).await?;
    tracing::info!(port = sidecar_handle.port, "HTTP sidecar started");

    // Share the sidecar's TokenStats Arc with the MCP server so the health tool
    // can display token savings without an HTTP round-trip.
    let token_stats = Some(sidecar_handle.token_stats);

    // Create MCP server and serve on stdio transport.
    let server = protocol::TokenizorServer::new(Arc::clone(&index), project_name, watcher_info, watcher_root.clone(), token_stats);
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
        let guard = index.read().expect("lock not poisoned");
        match persist::serialize_index(&guard, root) {
            Ok(()) => tracing::info!("index serialized to .tokenizor/index.bin"),
            Err(e) => tracing::warn!("failed to serialize index on shutdown: {e}"),
        }
    }

    // Shutdown the sidecar now that the MCP server has exited.
    let _ = sidecar_handle.shutdown_tx.send(());
    tracing::info!("sidecar shutdown signal sent");

    Ok(())
}
