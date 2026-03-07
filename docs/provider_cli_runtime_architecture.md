# Provider CLI Runtime Architecture

Status: probable architecture  
Date: 2026-03-07

## Purpose

This document takes the verified integration research and turns it into a buildable architecture and source scaffold for Tokenizor.

This is not the final architecture.

It is the current **probable architecture**: the design that best fits the verified provider surfaces, the product direction, and the operational constraints we already know about.

## Product Position

Tokenizor should become:

- a local code-intelligence runtime
- a project and workspace registry
- a byte-exact retrieval system
- a SpacetimeDB-backed operational control plane
- an MCP server
- a provider-adapter platform

In short:

**Tokenizor is not just a server process. It is the local code-intelligence substrate that provider CLIs consume.**

## Probable Architecture

### Layer 1: Core runtime

Core local daemon:

- `tokenizord`

Responsibilities:

- project and workspace tracking
- SpacetimeDB lifecycle management
- local CAS lifecycle management
- indexing orchestration
- file watching and change detection
- repo outline, symbol, and text query services
- prompt/resource generation support
- provider policy evaluation
- local telemetry and health

This daemon should be long-lived and warm.

It should not be recreated from scratch on every MCP stdio session if we want low-latency, consistent usage.

### Layer 2: Public transport surfaces

Primary transports:

- stdio MCP
- local RPC between adapters and daemon

Likely local RPC options:

- named pipe on Windows
- Unix domain socket on macOS/Linux
- optional localhost HTTP control API later if needed

Initial recommendation:

- implement local daemon control over named pipe / Unix socket
- keep the public AI-facing surface as MCP
- do not make provider adapters talk directly to SpacetimeDB or CAS

### Layer 3: Provider adapters

Provider-specific packages/config generators:

- Codex adapter
- Claude adapter
- Gemini adapter
- Copilot adapter
- Amazon Q adapter

Responsibilities:

- install MCP registration
- install provider-specific instruction/context files
- install hooks when supported
- install custom agents/subagents when supported
- install prompts/commands when supported
- point all integrations back to the same local Tokenizor runtime

### Layer 4: Repo-local integration assets

Generated or managed per project:

- `AGENTS.md`
- `CLAUDE.md`
- `GEMINI.md`
- `.claude/settings.json`
- `.claude/agents/*`
- `.gemini/settings.json`
- `.gemini/agents/*`
- `.github/copilot-instructions.md`
- `.github/agents/*`
- `.github/hooks/*`
- `.tokenizor/project.toml`

Important:

- only a small subset should be mandatory
- repo-local files should be additive and easy to remove
- large durable state must remain outside the repo

## Why a daemon is the probable answer

### Problem with pure stdio MCP

If Tokenizor lives only as a stdio MCP process:

- indexing state is cold too often
- project discovery repeats on every session
- background indexing is awkward
- provider-native hooks and commands have no stable local service to talk to
- "always on" behavior is weaker

### Problem with per-provider custom logic in the MCP process

If we stuff provider logic directly into the MCP binary:

- responsibilities blur
- provider-specific behavior leaks into core retrieval logic
- testability gets worse
- distribution becomes harder

### Why daemon plus adapters wins

- one warm runtime
- one authoritative project registry
- one place for SpacetimeDB and CAS management
- multiple lightweight provider adapters
- easier to test and evolve

## Project Tracking Model

Project tracking is required.

### Core entities

#### Project

Logical codebase identity.

Fields:

- `project_id`
- `canonical_root`
- `git_remote_url`
- `default_branch`
- `provider_bindings`
- `created_at`
- `last_seen_at`

#### Workspace

Concrete local checkout or worktree.

Fields:

- `workspace_id`
- `project_id`
- `root_path`
- `kind` (`git_checkout`, `git_worktree`, `plain_folder`)
- `current_branch`
- `is_primary`
- `last_seen_at`

#### ClientBinding

Installed provider integration state.

Fields:

- `binding_id`
- `project_id`
- `provider`
- `scope` (`user`, `project`, `workspace`)
- `status`
- `config_path`
- `last_verified_at`

#### SessionLease

Active client/runtime association.

Fields:

- `session_id`
- `provider`
- `workspace_id`
- `started_at`
- `last_heartbeat_at`

### Resolution algorithm

When a provider launches Tokenizor:

1. use explicit project root if passed
2. else look for `.tokenizor/project.toml`
3. else walk upward for `.git`
4. else treat current directory as unmanaged workspace
5. map workspace to existing project if remote/root fingerprint matches
6. otherwise create a new project registration

### Important behavior

- one project can have many workspaces
- git worktrees should map to one logical project
- monorepo subtree pinning should be supported through `.tokenizor/project.toml`

## Provider Adapter Strategy

### Codex adapter

Current strategy:

- global MCP registration
- optional project-scoped `.codex/config.toml`
- repo `AGENTS.md` augmentation
- repository and user skill installation
- optional startup helper to verify Tokenizor runtime is reachable

Current limitation:

- public docs in this pass do not show a hook system equivalent to Claude or Gemini
- public docs in this pass do not show a packaged extension model equivalent to Gemini extensions

Therefore:

- Codex adapter is instruction-first, MCP-native, and skills-aware
- Codex should not be the source of truth for enforcement policy

Strategic deeper options:

- Codex SDK for internal orchestration workflows
- Codex app-server for future deep custom client integration if that becomes product-worthy

### Claude adapter

Current strategy:

- user or project MCP registration
- optional Claude plugin packaging for bundled distribution
- `.claude/settings.json` hook installation
- Tokenizor subagent definitions
- slash-command-oriented MCP prompts
- session-start and prompt-submit context injection

This is the strongest adapter for early implementation.

### Gemini adapter

Current strategy:

- official Gemini extension package
- bundled MCP registration
- bundled hooks
- bundled subagents
- bundled skills
- compatible `GEMINI.md` strategy

This is the cleanest packaging story.

### Copilot adapter

Current strategy:

- MCP registration
- `.github/copilot-instructions.md` and `AGENTS.md`
- `.github/agents/*`
- `.github/hooks/*`
- optional Tokenizor skill pack
- optional Copilot plugin package

This is a strong candidate after Claude and Gemini.

### Amazon Q adapter

Current strategy:

- MCP registration
- Q custom agent profile
- `agentSpawn` and `userPromptSubmit` hooks
- reusable local and MCP prompts

This is a reasonable later adapter.

## Behavioral Goal by Provider

### Baseline goal for all clients

When the model needs to:

- search code
- find symbols
- understand repo structure
- retrieve exact source spans

it should prefer Tokenizor over naive raw file scanning whenever Tokenizor is ready.

### Strong goal for hook-capable clients

Hook-capable clients should:

- inject repo/index context before the model plans
- bias the agent toward Tokenizor lookup
- optionally warn or steer when the model tries expensive naive grep/read flows first

### Non-goal

Do not try to force all file edits through Tokenizor.

The native file edit/write tools in each client should remain the primary mutation path.

## Installation and Deployment Shape

### User-facing binaries

- `tokenizor`
- `tokenizord`
- `tokenizor-mcp`

### Responsibilities

`tokenizor`

- install
- integrate with provider CLIs
- init project
- doctor
- migrate
- runtime status

`tokenizord`

- background runtime
- project registry
- SpacetimeDB manager
- CAS manager
- indexing coordinator

`tokenizor-mcp`

- thin MCP transport shim
- forwards requests to `tokenizord`

### Why split the binaries

- keeps MCP startup cheap
- gives hooks and adapters a stable local target
- avoids embedding provider logic into the daemon

## Probable Source Scaffold

This is the source scaffold most worth moving toward next.

```text
src/
  application/
    services/
      project_registry.rs
      workspace_resolution.rs
      runtime_health.rs
      integration_installer.rs
    use_cases/
      install_runtime.rs
      install_provider_adapter.rs
      init_project.rs
      doctor.rs
      migrate.rs
  control_plane/
    spacetimedb/
      client.rs
      schema.rs
      migrations.rs
      bootstrap.rs
  daemon/
    server.rs
    ipc.rs
    session.rs
    supervisor.rs
  domain/
    project.rs
    workspace.rs
    provider.rs
    binding.rs
    session.rs
    health.rs
  indexing/
    coordinator.rs
    watcher.rs
    discovery.rs
    hashing.rs
    checkpoints.rs
  integration/
    mod.rs
    codex/
      install.rs
      doctor.rs
      assets.rs
    claude/
      install.rs
      doctor.rs
      assets.rs
    gemini/
      install.rs
      doctor.rs
      assets.rs
    copilot/
      install.rs
      doctor.rs
      assets.rs
    amazon_q/
      install.rs
      doctor.rs
      assets.rs
  ipc/
    protocol.rs
    pipe.rs
  protocol/
    mcp/
      server.rs
      tools/
      resources/
      prompts/
  storage/
    cas/
    repositories/
  templates/
    codex/
      AGENTS.md
    claude/
      CLAUDE.md
      settings.json
      agents/
    gemini/
      GEMINI.md
      settings.json
      extension/
    copilot/
      copilot-instructions.md
      agents/
      hooks/
    amazon_q/
      agent.json
      prompts/
```

## Runtime Lifecycle

### Startup

1. provider client launches MCP shim or helper
2. shim resolves current workspace
3. shim connects to `tokenizord`
4. if daemon is absent, start it
5. daemon verifies SpacetimeDB runtime
6. daemon loads project and workspace context
7. daemon returns readiness and context summary
8. provider session continues with Tokenizor available

### Steady state

- daemon keeps project metadata warm
- background indexing updates state incrementally
- provider adapters consult the daemon, not raw repo scans

### Shutdown

- MCP shim exits with provider client
- daemon may remain running briefly with idle timeout
- durable state stays in SpacetimeDB and CAS

## Policy Model

Tokenizor should have a provider-agnostic policy engine.

Policy decisions:

- when Tokenizor is ready enough to be preferred
- whether stale index state should be served, degraded, or blocked
- whether hooks should warn vs block naive reads
- what context summary to inject at session start

This policy should live in Tokenizor, not in per-provider assets.

Provider assets should call back into Tokenizor for decisions whenever possible.

## Recommended First Implementation Sequence

### Phase 1

- document the adapter architecture
- add provider/domain models for project, workspace, provider binding, and session
- split `tokenizord` from the MCP shim

### Phase 2

- implement project/workspace registry
- add local IPC
- move current health logic behind daemon services

### Phase 3

- implement Claude adapter first
- install MCP registration plus project/user hook assets
- validate session-start and prompt-submit flows

### Phase 4

- implement Codex adapter
- focus on MCP registration plus `AGENTS.md` strategy

### Phase 5

- implement Gemini extension package

### Phase 6

- add Copilot and Amazon Q adapters

## Open Questions

These questions should remain open until the next research or implementation pass:

- whether Codex exposes a public third-party skill packaging format beyond what is visible in current official docs
- how much provider-specific config Tokenizor should write automatically versus suggest for review
- whether repo-local generated integration assets should be opt-in or default
- how aggressively hook-capable clients should steer versus block non-Tokenizor lookups
- whether `tokenizord` should be single-user and global, or one runtime per project root

## Working Decision

Unless new evidence contradicts it, the working decision should be:

**Build Tokenizor as a long-lived local runtime with MCP transport and provider-specific adapters.**

That is the most plausible architecture supported by the verified provider surfaces in this pass.
