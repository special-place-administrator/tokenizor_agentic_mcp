//! CLI module — clap Parser types and subcommand dispatch for the `tokenizor` binary.
//!
//! Subcommands:
//!   `tokenizor init`               — install hooks into ~/.claude/settings.json
//!   `tokenizor hook <subcommand>`  — hook scripts called by Claude Code
//!
//! Plan 03 wires these into main.rs and handles the top-level dispatch.

pub mod hook;
pub mod init;

use clap::{Parser, Subcommand};

/// Top-level CLI parser for the `tokenizor` binary.
#[derive(Parser)]
#[command(name = "tokenizor", about = "Tokenizor MCP server and hook system")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,
}

/// Top-level subcommands.
#[derive(Subcommand)]
pub enum Commands {
    /// Install hooks into ~/.claude/settings.json
    Init,
    /// Hook subcommands called by Claude Code (PostToolUse / SessionStart)
    Hook {
        #[command(subcommand)]
        subcommand: Option<HookSubcommand>,
    },
}

/// Hook subcommands — one per Claude Code tool event type.
#[derive(Subcommand, Debug, Clone)]
pub enum HookSubcommand {
    /// PostToolUse hook for the Read tool — returns outline for the read file
    Read,
    /// PostToolUse hook for Edit/Write tools — returns impact (dependents) for the edited file
    Edit,
    /// PostToolUse hook for the Write tool — confirms indexing of new file
    Write,
    /// PostToolUse hook for the Grep tool — returns symbol-context for the search query
    Grep,
    /// SessionStart hook — returns repo map for the project
    #[command(name = "session-start")]
    SessionStart,
}
