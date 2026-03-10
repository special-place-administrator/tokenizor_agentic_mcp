#!/usr/bin/env bash
# ============================================================================
# Tokenizor MCP — Launch Wrapper
# ============================================================================
# Single-command launcher that ensures SpacetimeDB is running, then starts
# the Tokenizor MCP server. Use this as the "command" in MCP client configs
# for a fully hands-off experience.
#
# Usage: bash scripts/tokenizor-mcp.sh
#
# MCP client config example:
#   "command": "bash",
#   "args": ["C:/path/to/tokenizor_agentic_mcp/scripts/tokenizor-mcp.sh"]
# ============================================================================

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# -- Resolve binary ----------------------------------------------------------
case "$(uname -s)" in
    MINGW*|MSYS*|CYGWIN*|Windows_NT) BIN_EXT=".exe" ;;
    *)                                BIN_EXT=""     ;;
esac

BINARY="$PROJECT_ROOT/target/release/tokenizor_agentic_mcp${BIN_EXT}"

if [[ ! -f "$BINARY" ]]; then
    echo "ERROR: Binary not found at $BINARY" >&2
    echo "       Run 'bash scripts/setup.sh' first." >&2
    exit 1
fi

# -- Set defaults if not already in environment --------------------------------
export TOKENIZOR_CONTROL_PLANE_BACKEND="${TOKENIZOR_CONTROL_PLANE_BACKEND:-spacetimedb}"
export TOKENIZOR_SPACETIMEDB_ENDPOINT="${TOKENIZOR_SPACETIMEDB_ENDPOINT:-http://127.0.0.1:3007}"
export TOKENIZOR_SPACETIMEDB_DATABASE="${TOKENIZOR_SPACETIMEDB_DATABASE:-tokenizor}"
export TOKENIZOR_SPACETIMEDB_MODULE_PATH="${TOKENIZOR_SPACETIMEDB_MODULE_PATH:-$PROJECT_ROOT/spacetime/tokenizor}"
export TOKENIZOR_SPACETIMEDB_SCHEMA_VERSION="${TOKENIZOR_SPACETIMEDB_SCHEMA_VERSION:-2}"

# -- Ensure SpacetimeDB is running --------------------------------------------
if ! curl -s --connect-timeout 2 "$TOKENIZOR_SPACETIMEDB_ENDPOINT" &>/dev/null; then
    spacetime start --edition standalone &>/dev/null &

    RETRIES=15
    while (( RETRIES > 0 )); do
        if curl -s --connect-timeout 1 "$TOKENIZOR_SPACETIMEDB_ENDPOINT" &>/dev/null; then
            break
        fi
        sleep 1
        (( RETRIES-- ))
    done

    if ! curl -s --connect-timeout 2 "$TOKENIZOR_SPACETIMEDB_ENDPOINT" &>/dev/null; then
        echo "ERROR: SpacetimeDB failed to start at $TOKENIZOR_SPACETIMEDB_ENDPOINT" >&2
        exit 1
    fi
fi

# -- Launch MCP server (stdio) ------------------------------------------------
exec "$BINARY" run
