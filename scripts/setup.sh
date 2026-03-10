#!/usr/bin/env bash
# ============================================================================
# Tokenizor MCP — Fully Automated Setup
# ============================================================================
# Usage: bash scripts/setup.sh
#
# This script handles everything:
#   1. Checks prerequisites (Rust, SpacetimeDB CLI)
#   2. Installs SpacetimeDB CLI if missing
#   3. Builds Tokenizor in release mode
#   4. Starts SpacetimeDB local runtime
#   5. Publishes the SpacetimeDB module
#   6. Runs `doctor` to verify readiness
#   7. Prints MCP config for Claude Code / Claude Desktop / Cursor
#
# After running this script, just add the printed config to your MCP client.
# ============================================================================

set -euo pipefail

# -- Colors ------------------------------------------------------------------
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
BOLD='\033[1m'
NC='\033[0m'

info()  { echo -e "${BLUE}[INFO]${NC}  $*"; }
ok()    { echo -e "${GREEN}[OK]${NC}    $*"; }
warn()  { echo -e "${YELLOW}[WARN]${NC}  $*"; }
err()   { echo -e "${RED}[ERROR]${NC} $*"; }
step()  { echo -e "\n${BOLD}${CYAN}==> $*${NC}"; }

# -- Resolve project root ----------------------------------------------------
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$PROJECT_ROOT"

# -- Platform detection -------------------------------------------------------
detect_platform() {
    case "$(uname -s)" in
        MINGW*|MSYS*|CYGWIN*|Windows_NT) PLATFORM="windows" ;;
        Darwin*)                          PLATFORM="macos"   ;;
        Linux*)                           PLATFORM="linux"   ;;
        *)                                PLATFORM="unknown" ;;
    esac

    if [[ "$PLATFORM" == "windows" ]]; then
        BIN_EXT=".exe"
    else
        BIN_EXT=""
    fi
}

detect_platform

# -- Configuration -----------------------------------------------------------
DB_NAME="tokenizor"
SPACETIMEDB_ENDPOINT="http://127.0.0.1:3007"
MODULE_PATH="spacetime/tokenizor"
BINARY_NAME="tokenizor_agentic_mcp${BIN_EXT}"
RELEASE_BINARY="target/release/${BINARY_NAME}"

# ============================================================================
# Step 1: Check Rust toolchain
# ============================================================================
step "Checking Rust toolchain"

if command -v rustc &>/dev/null && command -v cargo &>/dev/null; then
    RUST_VERSION=$(rustc --version)
    ok "Rust found: $RUST_VERSION"
else
    err "Rust toolchain not found."
    echo "    Install from: https://rustup.rs"
    echo "    Run: curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"
    exit 1
fi

# ============================================================================
# Step 2: Check / Install SpacetimeDB CLI
# ============================================================================
step "Checking SpacetimeDB CLI"

if command -v spacetime &>/dev/null; then
    SPACETIME_VERSION=$(spacetime --version 2>&1 | head -1)
    ok "SpacetimeDB CLI found: $SPACETIME_VERSION"
else
    warn "SpacetimeDB CLI not found. Installing..."

    if [[ "$PLATFORM" == "windows" ]]; then
        # Windows: use the official installer
        info "Downloading SpacetimeDB installer for Windows..."
        INSTALLER_URL="https://install.spacetimedb.com/install.sh"
        if command -v curl &>/dev/null; then
            curl -sSf "$INSTALLER_URL" | bash
        elif command -v wget &>/dev/null; then
            wget -qO- "$INSTALLER_URL" | bash
        else
            err "Neither curl nor wget available. Install SpacetimeDB manually:"
            echo "    https://spacetimedb.com/install"
            exit 1
        fi
    elif [[ "$PLATFORM" == "macos" ]]; then
        if command -v brew &>/dev/null; then
            brew install clockworklabs/tap/spacetimedb
        else
            curl -sSf https://install.spacetimedb.com/install.sh | bash
        fi
    else
        curl -sSf https://install.spacetimedb.com/install.sh | bash
    fi

    # Refresh PATH
    export PATH="$HOME/.spacetime/bin:$PATH"
    if [[ "$PLATFORM" == "windows" ]]; then
        export PATH="$LOCALAPPDATA/SpacetimeDB/bin/current:$PATH"
    fi

    if command -v spacetime &>/dev/null; then
        ok "SpacetimeDB CLI installed successfully"
    else
        err "SpacetimeDB CLI installation failed. Install manually:"
        echo "    https://spacetimedb.com/install"
        exit 1
    fi
fi

# ============================================================================
# Step 3: Build Tokenizor in release mode
# ============================================================================
step "Building Tokenizor (release mode)"

info "This may take a few minutes on first build..."
cargo build --release 2>&1 | tail -3

if [[ -f "$RELEASE_BINARY" ]]; then
    ok "Binary built: $RELEASE_BINARY"
else
    err "Build failed — binary not found at $RELEASE_BINARY"
    exit 1
fi

# Resolve absolute path for MCP config
BINARY_ABS_PATH="$(cd "$(dirname "$RELEASE_BINARY")" && pwd)/$(basename "$RELEASE_BINARY")"

# ============================================================================
# Step 4: Start SpacetimeDB local runtime
# ============================================================================
step "Starting SpacetimeDB local runtime"

# Check if already running
if curl -s --connect-timeout 2 "$SPACETIMEDB_ENDPOINT" &>/dev/null; then
    ok "SpacetimeDB runtime already running at $SPACETIMEDB_ENDPOINT"
else
    info "Starting SpacetimeDB standalone runtime..."

    # Start in background
    spacetime start --edition standalone &>/dev/null &
    SPACETIME_PID=$!

    # Wait for it to become ready (up to 30 seconds)
    RETRIES=30
    while (( RETRIES > 0 )); do
        if curl -s --connect-timeout 1 "$SPACETIMEDB_ENDPOINT" &>/dev/null; then
            break
        fi
        sleep 1
        (( RETRIES-- ))
    done

    if curl -s --connect-timeout 2 "$SPACETIMEDB_ENDPOINT" &>/dev/null; then
        ok "SpacetimeDB runtime started (PID: $SPACETIME_PID)"
    else
        err "SpacetimeDB runtime failed to start within 30 seconds."
        echo "    Try manually: spacetime start"
        exit 1
    fi
fi

# ============================================================================
# Step 5: Publish SpacetimeDB module
# ============================================================================
step "Publishing SpacetimeDB module"

if [[ ! -d "$MODULE_PATH" ]]; then
    err "SpacetimeDB module not found at $MODULE_PATH"
    exit 1
fi

info "Publishing module to database '$DB_NAME'..."
# --yes for non-interactive, --delete-data=on-conflict to handle schema changes
spacetime publish "$DB_NAME" \
    --module-path "$MODULE_PATH" \
    --server local \
    --yes \
    --delete-data=on-conflict 2>&1 | tail -5

ok "Module published to '$DB_NAME'"

# ============================================================================
# Step 6: Run doctor to verify readiness
# ============================================================================
step "Verifying deployment readiness"

info "Running: tokenizor_agentic_mcp doctor"

DOCTOR_OUTPUT=$("$RELEASE_BINARY" doctor 2>&1) || true
echo "$DOCTOR_OUTPUT"

if echo "$DOCTOR_OUTPUT" | grep -qi "blocked\|error\|fatal"; then
    warn "Doctor reported issues — review output above."
    warn "The MCP server may not start until these are resolved."
else
    ok "All readiness checks passed"
fi

# ============================================================================
# Step 7: Create .env file
# ============================================================================
step "Creating environment configuration"

ENV_FILE="$PROJECT_ROOT/.env"
if [[ -f "$ENV_FILE" ]]; then
    warn ".env file already exists — not overwriting"
else
    cat > "$ENV_FILE" <<ENVEOF
# Tokenizor MCP — Environment Configuration
# Generated by setup.sh on $(date -u +"%Y-%m-%dT%H:%M:%SZ")

# Control plane backend: spacetimedb | local_registry | in_memory
TOKENIZOR_CONTROL_PLANE_BACKEND=spacetimedb

# SpacetimeDB settings
TOKENIZOR_SPACETIMEDB_ENDPOINT=http://127.0.0.1:3007
TOKENIZOR_SPACETIMEDB_DATABASE=tokenizor
TOKENIZOR_SPACETIMEDB_MODULE_PATH=spacetime/tokenizor
TOKENIZOR_SPACETIMEDB_SCHEMA_VERSION=2

# Local CAS root (default: .tokenizor in current working directory)
# TOKENIZOR_BLOB_ROOT=.tokenizor

# Require all readiness checks to pass before serving MCP (default: true)
# TOKENIZOR_REQUIRE_READY_CONTROL_PLANE=true
ENVEOF
    ok "Created .env"
fi

# ============================================================================
# Step 8: Print MCP client configuration
# ============================================================================
step "MCP Client Configuration"

# Convert path for JSON (escape backslashes on Windows)
if [[ "$PLATFORM" == "windows" ]]; then
    JSON_BINARY_PATH=$(echo "$BINARY_ABS_PATH" | sed 's|/|\\\\|g')
    # Also try to convert MSYS paths like /c/Users to C:\\Users
    JSON_BINARY_PATH=$(echo "$JSON_BINARY_PATH" | sed 's|^\\\\c\\\\|C:\\\\|')
    JSON_BINARY_PATH=$(echo "$JSON_BINARY_PATH" | sed 's|^\\\\d\\\\|D:\\\\|')
else
    JSON_BINARY_PATH="$BINARY_ABS_PATH"
fi

echo ""
echo -e "${BOLD}Choose your MCP client and add the config below:${NC}"

# --- Claude Code ---
echo ""
echo -e "${CYAN}--- Claude Code (.claude.json or .mcp.json) ---${NC}"
cat <<CLAUDECODE
{
  "mcpServers": {
    "tokenizor": {
      "command": "${JSON_BINARY_PATH}",
      "args": ["run"],
      "env": {
        "TOKENIZOR_CONTROL_PLANE_BACKEND": "spacetimedb",
        "TOKENIZOR_SPACETIMEDB_ENDPOINT": "http://127.0.0.1:3007",
        "TOKENIZOR_SPACETIMEDB_DATABASE": "tokenizor",
        "TOKENIZOR_SPACETIMEDB_MODULE_PATH": "spacetime/tokenizor",
        "TOKENIZOR_SPACETIMEDB_SCHEMA_VERSION": "2"
      }
    }
  }
}
CLAUDECODE

# --- Claude Desktop ---
echo ""
echo -e "${CYAN}--- Claude Desktop (claude_desktop_config.json) ---${NC}"
cat <<CLAUDEDESKTOP
Add to your claude_desktop_config.json under "mcpServers":

"tokenizor": {
  "command": "${JSON_BINARY_PATH}",
  "args": ["run"],
  "env": {
    "TOKENIZOR_CONTROL_PLANE_BACKEND": "spacetimedb",
    "TOKENIZOR_SPACETIMEDB_ENDPOINT": "http://127.0.0.1:3007",
    "TOKENIZOR_SPACETIMEDB_DATABASE": "tokenizor",
    "TOKENIZOR_SPACETIMEDB_MODULE_PATH": "spacetime/tokenizor",
    "TOKENIZOR_SPACETIMEDB_SCHEMA_VERSION": "2"
  }
}
CLAUDEDESKTOP

# --- Cursor ---
echo ""
echo -e "${CYAN}--- Cursor (.cursor/mcp.json) ---${NC}"
cat <<CURSOR
{
  "mcpServers": {
    "tokenizor": {
      "command": "${JSON_BINARY_PATH}",
      "args": ["run"],
      "env": {
        "TOKENIZOR_CONTROL_PLANE_BACKEND": "spacetimedb",
        "TOKENIZOR_SPACETIMEDB_ENDPOINT": "http://127.0.0.1:3007",
        "TOKENIZOR_SPACETIMEDB_DATABASE": "tokenizor",
        "TOKENIZOR_SPACETIMEDB_MODULE_PATH": "spacetime/tokenizor",
        "TOKENIZOR_SPACETIMEDB_SCHEMA_VERSION": "2"
      }
    }
  }
}
CURSOR

# ============================================================================
# Done
# ============================================================================
echo ""
echo -e "${BOLD}${GREEN}============================================================================${NC}"
echo -e "${BOLD}${GREEN}  Tokenizor MCP setup complete!${NC}"
echo -e "${BOLD}${GREEN}============================================================================${NC}"
echo ""
echo -e "  Binary:     ${BOLD}$BINARY_ABS_PATH${NC}"
echo -e "  Database:   ${BOLD}$DB_NAME${NC} at ${BOLD}$SPACETIMEDB_ENDPOINT${NC}"
echo -e "  Module:     ${BOLD}$MODULE_PATH${NC}"
echo ""
echo -e "  ${YELLOW}IMPORTANT:${NC} SpacetimeDB must be running before the MCP server starts."
echo -e "  If your machine reboots, run: ${BOLD}spacetime start${NC}"
echo ""
echo -e "  To verify anytime:  ${BOLD}$RELEASE_BINARY doctor${NC}"
echo -e "  To start manually:  ${BOLD}$RELEASE_BINARY run${NC}"
echo ""
