use tokenizor_agentic_mcp::{discovery, live_index, observability};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    observability::init_tracing()?;

    let root = discovery::find_git_root();
    tracing::info!(root = %root.display(), "discovered project root");

    let index = live_index::LiveIndex::load(&root)?;
    let guard = index.read().expect("LiveIndex lock poisoned");

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
        live_index::IndexState::CircuitBreakerTripped { summary } => {
            tracing::error!(%summary, "circuit breaker tripped — index degraded");
            // Phase 2 will return this as health tool response
            // For now, log and exit with error code
            std::process::exit(1);
        }
        live_index::IndexState::Loading => {
            unreachable!("load() is synchronous — cannot be in Loading state after return");
        }
        live_index::IndexState::Empty => {
            unreachable!("LiveIndex::load() always sets is_empty=false — cannot be Empty after load");
        }
    }

    // Phase 2 adds: MCP server startup here
    // For now, the binary just loads and exits — sufficient for Phase 1 validation
    Ok(())
}
