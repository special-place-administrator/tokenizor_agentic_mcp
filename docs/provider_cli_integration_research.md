# Provider CLI Integration Research

Status: research baseline  
Date: 2026-03-07

## Purpose

This document captures the current, externally verified integration surfaces for AI provider CLIs that are relevant to Tokenizor.

The goal is not to prove a final architecture. The goal is to remove hand-waving and constrain the design to what the clients actually expose today.

This pass is focused on:

- OpenAI Codex
- Anthropic Claude Code
- Gemini CLI
- GitHub Copilot CLI
- Amazon Q Developer CLI

These are the practical targets because they are coding-oriented, local-first enough to matter, and already expose MCP or adjacent agent integration mechanisms.

## Design Question

Tokenizor should not be "just another MCP server" that the model might occasionally use.

It should become a durable local code-intelligence runtime that provider CLIs can consume through the deepest supported integration surface:

- MCP where that is the only public surface
- native hooks, agents, skills, prompts, or extensions where available

The central architectural question is:

How do we make Tokenizor get used continuously for lookup, search, repo understanding, and verified retrieval, instead of waiting for the model to discover it by chance?

## Research Summary

### High-confidence conclusion

Different clients offer very different control surfaces.

There is no universal native plugin standard across provider CLIs today.

Therefore:

- a plain MCP-only strategy is too weak
- a plugin-only strategy is impossible across providers
- the most plausible cross-provider architecture is a local Tokenizor runtime plus provider-specific adapters

### What "constant usage" can realistically mean

Tokenizor should become the default path for:

- repository outline
- symbol search
- text search
- verified source retrieval
- codebase exploration
- project/session context injection

It does **not** need to replace the client's native edit/write tools.

Trying to make every model edit files through Tokenizor is the wrong optimization target. The right target is to make models consult Tokenizor before they search or reason about code.

## Verified Provider Surfaces

### OpenAI Codex

Verified in this pass:

- Codex can connect to MCP servers through the CLI or IDE, with shared configuration across both.
- Codex can be configured through `~/.codex/config.toml` and project-scoped `.codex/config.toml` in trusted projects.
- OpenAI's own MCP docs explicitly recommend adding an `AGENTS.md` instruction to get more reliable MCP usage.
- OpenAI product docs confirm that Codex uses `AGENTS.md` for persistent repository guidance.
- Codex has first-class Skills support in the CLI, IDE extension, and Codex app.
- Codex discovers skills from repository, user, admin, and system locations.
- Codex supports Automations for scheduled background work in the Codex app.
- Codex exposes a Codex SDK for programmatic control of local Codex agents.
- Codex exposes an open app-server protocol for deep custom client integrations and rich clients.
- Codex can also run as its own MCP server for orchestration by other agents.

What we did **not** confirm in public docs during this pass:

- a public third-party Codex plugin marketplace or extension packaging model analogous to Gemini extensions
- a public hook system comparable to Claude hooks or Gemini hooks
- a public documented mechanism to force tool-routing decisions inside Codex in the same way hook-capable clients can

Implication:

- Codex is stronger than "MCP plus AGENTS only".
- Codex already exposes several meaningful native surfaces:
  - shared and project-scoped MCP config
  - layered `AGENTS.md`
  - skills
  - automations
  - SDK
  - app-server
- Codex is still weaker than Claude on workflow interception because we did not confirm an equivalent public hook system.
- Codex likely needs an "instruction + MCP + skills" adapter in the short term, with app-server and SDK remaining strategic deeper options.

Most useful levers:

- global MCP registration
- project-scoped `.codex/config.toml` where appropriate
- project `AGENTS.md` augmentation
- repository and user skill installation
- Codex SDK for internal orchestration use cases
- app-server for future deep custom integration
- high-signal MCP tool/prompt/resource naming

### Claude Code

Verified in this pass:

- Claude Code supports MCP server configuration at local, project, and user scopes.
- Claude Code supports hierarchical settings files at `~/.claude/settings.json`, `.claude/settings.json`, and `.claude/settings.local.json`.
- Claude Code supports plugins and plugin-provided MCP servers.
- Claude Code hooks include `UserPromptSubmit`, `SessionStart`, `PreToolUse`, `PostToolUse`, `Stop`, and related lifecycle events.
- Claude hooks can target MCP tools using `mcp__<server>__<tool>` match patterns.
- Claude Code supports subagents that can inherit or restrict tool access, including MCP tools.
- MCP prompts can appear as slash commands in Claude Code.
- Claude has explicit project initialization and memory surfaces around `CLAUDE.md` and slash commands like `/init` and `/memory`.

Implication:

- Claude Code is the strongest immediate native integration target.
- Tokenizor can inject context automatically, bias tool selection, expose MCP prompts as slash commands, and provide dedicated analysis subagents.
- Claude is the best first provider for near-"always on" behavior.

Most useful levers:

- user/project MCP registration
- plugin packaging where a shared install unit is valuable
- `SessionStart` context injection
- `UserPromptSubmit` context refresh
- `PreToolUse` steering before naive read/grep workflows
- Tokenizor-focused subagents
- MCP prompts as slash commands

### Gemini CLI

Verified in this pass:

- Gemini CLI supports MCP.
- Gemini CLI supports a first-class extension packaging system.
- Gemini extensions can bundle MCP servers, custom commands, hooks, sub-agents, and agent skills.
- Gemini CLI supports hooks at both lifecycle and tool boundaries.
- Gemini CLI supports user and workspace settings in `~/.gemini/settings.json` and `.gemini/settings.json`.
- Gemini CLI supports hierarchical `GEMINI.md` context loading.
- Gemini CLI supports extension installation through `gemini extensions install`.
- Gemini's extension format is the cleanest documented packaging surface found in this pass.

Implication:

- Gemini is the best target for a packaged native distribution.
- Tokenizor should ship an official Gemini extension.
- Gemini can support both "discoverable MCP" and "bundled workflow behavior" from the same install unit.

Most useful levers:

- official Gemini extension
- bundled MCP server config
- lifecycle and tool hooks
- bundled subagents
- bundled skills
- `GEMINI.md` or compatible context-file strategy

### GitHub Copilot CLI

Verified in this pass:

- Copilot CLI supports MCP server configuration.
- Copilot CLI stores MCP config under `~/.copilot` by default.
- Copilot CLI supports repository custom instructions, path-specific instructions, and `AGENTS.md`.
- Copilot CLI supports custom agents.
- Copilot CLI supports hooks.
- Copilot CLI supports skills.
- Copilot CLI exposes plugin documentation and a CLI plugin reference.
- Copilot CLI includes built-in specialized agents and can auto-infer custom agent usage.
- Copilot CLI has Copilot Memory as a separate persistent memory feature.

Implication:

- Copilot CLI is more extensible than a plain MCP-only client.
- Tokenizor can be integrated through MCP plus repo instructions plus custom agents plus hooks plus skills.
- This makes Copilot a serious second-wave target, even if the initial product focus remains Codex and Claude.

Most useful levers:

- global MCP registration
- repo custom instructions and `AGENTS.md`
- Tokenizor-specific custom agent profiles
- hook-based context injection and steering
- skill bundles
- plugin packaging once the adapter matures

### Amazon Q Developer CLI

Verified in this pass:

- Amazon Q Developer CLI supports MCP.
- Q CLI supports globally defined MCP configuration and both local process and remote HTTP MCP servers.
- AWS documentation describes custom agents for Q CLI.
- Q custom agents support hooks such as `agentSpawn` and `userPromptSubmit`.
- Q supports a prompt system that includes local prompts, global prompts, and MCP prompts.
- Admin control exists for enabling or disabling MCP functionality in organizations.

Implication:

- Amazon Q is not just an MCP target.
- Q has enough agent and hook surface to support a deeper adapter.
- It is a valid third-wave target after Claude and Gemini, and possibly after Copilot depending on user demand.

Most useful levers:

- MCP registration
- custom agent profiles
- `agentSpawn` and `userPromptSubmit` hooks
- reusable prompt bundles

## Integration Theory Matrix

### Theory A: Plain generic MCP server only

Description:

- Ship Tokenizor as a standard MCP server
- Tell users to add it to their provider CLI
- Hope the model learns to use it

Advantages:

- simplest implementation
- broadest protocol compatibility
- low maintenance

Disadvantages:

- too passive
- poor discoverability in clients with many tools
- weak control over routing
- no reliable "always use this first" behavior

Assessment:

- necessary
- insufficient as the product strategy

### Theory B: MCP plus repo instruction files only

Description:

- MCP registration
- generate `AGENTS.md`, `CLAUDE.md`, `GEMINI.md`, and provider-specific instruction files

Advantages:

- works in many environments
- lightweight
- improves adoption over plain MCP

Disadvantages:

- still advisory in many clients
- no lifecycle automation
- no guaranteed context refresh

Assessment:

- useful baseline
- still insufficient alone

### Theory C: Local Tokenizor runtime plus provider adapters

Description:

- Tokenizor runs as a local service/runtime
- provider-specific adapters register MCP and native integrations
- adapters inject context, prompts, hooks, agents, and commands where supported

Advantages:

- strongest cross-provider story
- keeps indexing and project state out of per-session stdio startup
- allows one warm local cache and one project registry
- lets each provider use its deepest supported integration surface

Disadvantages:

- higher implementation complexity
- adapter maintenance burden
- requires disciplined compatibility management

Assessment:

- most plausible architecture
- recommended

### Theory D: Wrapper launcher around every provider CLI

Description:

- users invoke `tokenizor codex`, `tokenizor claude`, `tokenizor gemini`, etc.
- Tokenizor starts the client with injected config and environment

Advantages:

- strong control at process start
- easier bootstrapping of runtime dependencies

Disadvantages:

- brittle
- fights normal user workflows
- weaker UX when users launch the provider CLI directly
- cannot cover cloud and IDE surfaces cleanly

Assessment:

- useful fallback
- should not be the primary architecture

### Theory E: Full client-specific plugin or fork strategy

Description:

- build custom plugin or modified client implementation for each provider

Advantages:

- maximal control where possible

Disadvantages:

- impossible or unsupported for several providers
- high maintenance
- distribution and trust problems

Assessment:

- not practical as the core strategy

## Most Plausible Strategic Position

The most plausible position after this research pass is:

- Tokenizor should be a local runtime first
- MCP remains mandatory, but only as one transport surface
- provider-native adapters should be first-class product components
- project instruction/context files remain important, but only as one control layer

In short:

**Tokenizor should become a local code-intelligence platform with MCP transport and provider-native adapters, not just an MCP server binary.**

## Provider Priority Recommendation

### Tier 1

- Claude Code
- Codex

Reason:

- these are the immediate target users
- Claude has the strongest current hook/agent integration surface
- Codex is strategically important even though its public native surface is thinner

### Tier 2

- Gemini CLI
- GitHub Copilot CLI

Reason:

- both have strong extension/customization surfaces
- Gemini has the cleanest extension packaging model
- Copilot has strong repo-native customization and MCP support

### Tier 3

- Amazon Q Developer CLI

Reason:

- valid integration target
- likely lower immediate demand than Claude/Codex/Gemini/Copilot
- still worth designing for now to avoid architecture paint traps

## Hard Constraints Exposed by Research

### Constraint 1: No universal plugin model

We cannot design once and assume every provider will load a "Tokenizor plugin" the same way.

### Constraint 2: MCP remains the universal minimum

Every serious target in this pass has MCP support or meaningful MCP-adjacent support.

### Constraint 3: Context files still matter

Instruction files remain a reliable steering mechanism across multiple clients:

- `AGENTS.md`
- `CLAUDE.md`
- `GEMINI.md`
- provider-specific instruction files where relevant

### Constraint 4: Hooks are the strongest enforcement layer when available

Claude, Gemini, Copilot, and Q expose hook surfaces that can bias or block naive tool usage.

Codex currently appears to rely more on layered instructions, MCP, skills, and custom client surfaces than on a public hook system.

### Constraint 5: Project tracking must live in Tokenizor, not in the client

Clients differ too much.

Project identity, workspace tracking, index state, and retrieval policy should live inside Tokenizor itself.

## Recommended Research Follow-ups

Still worth deeper verification later:

- whether Codex will expose a broader public plugin or hook surface in future official docs
- how far Codex app-server can realistically serve as a Tokenizor-aware custom client surface without overreaching the current product scope
- exact GitHub Copilot CLI plugin distribution story beyond skills/agents/hooks
- Amazon Q file layout and packaging conventions for team-shared agent bundles
- security/trust prompts and approval models for provider-installed hooks

## Sources

- OpenAI Docs MCP: <https://platform.openai.com/docs/docs-mcp>
- OpenAI Codex AGENTS.md guide: <https://developers.openai.com/codex/guides/agents-md>
- OpenAI Codex MCP guide: <https://developers.openai.com/codex/mcp>
- OpenAI Codex Skills guide: <https://developers.openai.com/codex/skills>
- OpenAI Codex Automations: <https://developers.openai.com/codex/app/automations>
- OpenAI Codex SDK: <https://developers.openai.com/codex/sdk>
- OpenAI Codex App Server: <https://developers.openai.com/codex/app-server>
- OpenAI Codex launch post: <https://openai.com/index/introducing-codex/>
- OpenAI Codex product page: <https://openai.com/codex/>
- OpenAI Codex app launch: <https://openai.com/index/introducing-the-codex-app/>
- OpenAI usage guidance mentioning `AGENTS.md`: <https://openai.com/business/guides-and-resources/how-openai-uses-codex/>
- Anthropic Claude Code MCP docs: <https://docs.anthropic.com/en/docs/claude-code/mcp>
- Anthropic Claude Code hooks: <https://docs.anthropic.com/en/docs/claude-code/hooks>
- Anthropic Claude Code subagents: <https://docs.anthropic.com/en/docs/claude-code/subagents>
- Anthropic Claude Code slash commands: <https://docs.anthropic.com/en/docs/claude-code/slash-commands>
- Anthropic Claude Code settings: <https://docs.anthropic.com/en/docs/claude-code/settings>
- Gemini CLI docs home: <https://geminicli.com/docs/>
- Gemini CLI extensions: <https://geminicli.com/docs/extensions/>
- Gemini CLI extensions reference: <https://geminicli.com/docs/extensions/reference/>
- Gemini CLI hooks: <https://geminicli.com/docs/hooks/>
- Gemini CLI `GEMINI.md`: <https://geminicli.com/docs/cli/gemini-md>
- Gemini CLI settings: <https://geminicli.com/docs/cli/settings/>
- GitHub Copilot CLI overview: <https://docs.github.com/en/copilot/concepts/agents/about-copilot-cli>
- GitHub Copilot CLI usage: <https://docs.github.com/copilot/how-tos/use-copilot-agents/use-copilot-cli>
- GitHub Copilot CLI MCP: <https://docs.github.com/en/copilot/how-tos/copilot-cli/customize-copilot/add-mcp-servers>
- GitHub Copilot CLI hooks: <https://docs.github.com/en/copilot/how-tos/copilot-cli/use-hooks>
- GitHub Copilot CLI skills: <https://docs.github.com/copilot/how-tos/copilot-cli/customize-copilot/create-skills>
- GitHub Copilot CLI custom agents: <https://docs.github.com/en/enterprise-cloud%40latest/copilot/how-tos/copilot-cli/customize-copilot/create-custom-agents-for-cli>
- Amazon Q MCP docs: <https://docs.aws.amazon.com/amazonq/latest/qdeveloper-ug/command-line-mcp.html>
- Amazon Q custom agents: <https://docs.aws.amazon.com/amazonq/latest/qdeveloper-ug/command-line-custom-agents.html>
- Amazon Q custom agent configuration: <https://docs.aws.amazon.com/amazonq/latest/qdeveloper-ug/command-line-custom-agents-configuration.html>
