//! CLI module — clap Parser types and subcommand dispatch for the `tokenizor` binary.
//!
//! Subcommands:
//!   `tokenizor init`               — configure Claude, Codex, or both
//!   `tokenizor hook <subcommand>`  — hook scripts called by Claude Code
//!   `tokenizor daemon`             — shared project/session backend
//!
//! Plan 03 wires these into main.rs and handles the top-level dispatch.

pub mod hook;
pub mod init;

use clap::{Parser, Subcommand, ValueEnum};

/// Top-level CLI parser for the `tokenizor` binary.
#[derive(Parser)]
#[command(
    name = "tokenizor",
    about = "Tokenizor MCP server and hook system",
    version
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,
}

/// Top-level subcommands.
#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Install Tokenizor integration for Claude, Codex, Gemini, or all
    Init {
        /// Client to configure
        #[arg(long, value_enum, default_value_t = InitClient::All)]
        client: InitClient,
    },
    /// Run the shared local daemon that tracks project and session state
    Daemon,
    /// Hook subcommands called by Claude Code (PostToolUse / SessionStart / UserPromptSubmit)
    Hook {
        #[command(subcommand)]
        subcommand: Option<HookSubcommand>,
    },
}

/// Supported `tokenizor init` targets.
#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum InitClient {
    Claude,
    Codex,
    Gemini,
    All,
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
    /// UserPromptSubmit hook — injects targeted context from file/symbol hints in the prompt
    #[command(name = "prompt-submit")]
    PromptSubmit,
    /// PreToolUse hook — suggests Tokenizor alternatives before built-in tools execute
    #[command(name = "pre-tool")]
    PreTool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_init_defaults_to_all_clients() {
        let cli = Cli::parse_from(["tokenizor", "init"]);

        match cli.command {
            Some(Commands::Init { client }) => assert_eq!(client, InitClient::All),
            _ => panic!("expected init command"),
        }
    }

    #[test]
    fn test_init_accepts_codex_client() {
        let cli = Cli::parse_from(["tokenizor", "init", "--client", "codex"]);

        match cli.command {
            Some(Commands::Init { client }) => assert_eq!(client, InitClient::Codex),
            _ => panic!("expected init command"),
        }
    }

    #[test]
    fn test_init_accepts_gemini_client() {
        let cli = Cli::parse_from(["tokenizor", "init", "--client", "gemini"]);

        match cli.command {
            Some(Commands::Init { client }) => assert_eq!(client, InitClient::Gemini),
            _ => panic!("expected init command"),
        }
    }

    #[test]
    fn test_daemon_command_parses() {
        let cli = Cli::parse_from(["tokenizor", "daemon"]);

        match cli.command {
            Some(Commands::Daemon) => {}
            _ => panic!("expected daemon command"),
        }
    }

    #[test]
    fn test_hook_prompt_submit_command_parses() {
        let cli = Cli::parse_from(["tokenizor", "hook", "prompt-submit"]);

        match cli.command {
            Some(Commands::Hook {
                subcommand: Some(HookSubcommand::PromptSubmit),
            }) => {}
            _ => panic!("expected prompt-submit hook command"),
        }
    }

    #[test]
    fn test_hook_pre_tool_command_parses() {
        let cli = Cli::parse_from(["tokenizor", "hook", "pre-tool"]);

        match cli.command {
            Some(Commands::Hook {
                subcommand: Some(HookSubcommand::PreTool),
            }) => {}
            _ => panic!("expected pre-tool hook command"),
        }
    }
}
