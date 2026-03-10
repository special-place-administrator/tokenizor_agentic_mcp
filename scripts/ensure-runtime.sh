#!/usr/bin/env bash
# ============================================================================
# Tokenizor MCP — Ensure Runtime
# ============================================================================
# Lightweight script that ensures SpacetimeDB is running before the MCP
# server starts. Designed to be called as a wrapper in MCP client configs.
#
# Usage (direct):    bash scripts/ensure-runtime.sh
# Usage (as wrapper): scripts/ensure-runtime.sh && target/release/tokenizor_agentic_mcp run
# ============================================================================

set -euo pipefail

SPACETIMEDB_ENDPOINT="${TOKENIZOR_SPACETIMEDB_ENDPOINT:-http://127.0.0.1:3007}"

# Check if SpacetimeDB is already running
if curl -s --connect-timeout 2 "$SPACETIMEDB_ENDPOINT" &>/dev/null; then
    exit 0
fi

# Not running — start it
spacetime start --edition standalone &>/dev/null &

# Wait up to 15 seconds
RETRIES=15
while (( RETRIES > 0 )); do
    if curl -s --connect-timeout 1 "$SPACETIMEDB_ENDPOINT" &>/dev/null; then
        exit 0
    fi
    sleep 1
    (( RETRIES-- ))
done

echo "ERROR: SpacetimeDB failed to start at $SPACETIMEDB_ENDPOINT" >&2
exit 1
