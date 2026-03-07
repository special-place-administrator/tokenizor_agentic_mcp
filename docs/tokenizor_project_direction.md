# Tokenizor Project Direction

Status: working direction baseline  
Date: 2026-03-07

## Purpose

This document consolidates the current architectural direction for `tokenizor_agentic_mcp` into one place.

It is intended to serve as the pre-BMAD baseline:

- what this project is
- what it is not
- what architecture is currently most plausible
- how to phase the work without losing the original mission

This is not the final architecture.

It is the current best working direction given:

- the original mission of building a Rust-native successor to `jcodemunch-mcp`
- the need for stronger correctness, durability, and recovery
- the desire to make provider CLIs use Tokenizor frequently instead of only occasionally

## Core Thesis

Tokenizor should be built as:

- a Rust-native successor to `jcodemunch-mcp`
- a coding-first indexing and retrieval engine
- an MCP-compatible product by default
- a provider-adapter platform by design

The engine is the product.

MCP, plugins, hooks, skills, prompts, and extensions are delivery surfaces.

That distinction matters.

If the engine is weak, no integration surface will save the product.
If the engine is strong, multiple integration surfaces become worth building.

## Product Statement

Tokenizor is not meant to be a random new code tool.

It is meant to become the proper Rust-native evolution of `jcodemunch-mcp` with:

- better byte correctness
- better recovery
- better determinism
- better operational structure
- better long-term extensibility

It should preserve the useful core of `jcodemunch`:

- indexing repositories
- symbol extraction
- text search
- file outlines
- symbol retrieval
- repo outlines
- cache invalidation

And it should improve on it through:

- byte-exact local storage
- stronger idempotency
- resumable runs
- health and repair flows
- explicit operational state
- better native integration into AI provider CLIs

## What Problem We Are Actually Solving

The immediate technical problem is not "how do we make a plugin."

It is:

How do we build a code-indexing and retrieval system that AI coding agents can rely on because it is:

- fast
- correct
- resumable
- deterministic
- cheap in token usage compared with repeated naive file exploration

The product problem is:

How do we make provider CLIs use this system often enough that it changes real workflow behavior?

That leads to two parallel design truths:

1. the indexing/retrieval engine must become genuinely good
2. the integration story must go beyond "optional MCP tool"

## Working Conclusions

### Conclusion 1: This should remain a Rust-first `jcodemunch` successor

The project should keep its original center of gravity:

- Rust everywhere practical
- tree-sitter parsing in Rust
- local byte-exact CAS in Rust
- deterministic retrieval in Rust
- MCP protocol surface in Rust

This project should not drift into being "an integration shell around a mediocre engine."

### Conclusion 2: MCP is necessary but not sufficient

MCP is the universal floor because it is the one integration mechanism that many provider CLIs already support.

But MCP alone is too passive.

If Tokenizor is only exposed as "one more tool":

- models may ignore it
- clients may not prefer it
- tool usage will remain sporadic

Therefore:

- Tokenizor must support MCP
- Tokenizor must not stop at MCP

### Conclusion 3: Skills alone are not the answer

Skills, prompts, and instruction files are useful.

They help:

- discovery
- consistency
- steering
- project memory

But they are advisory.

They do not create a reliable default retrieval plane by themselves.

### Conclusion 4: Native provider adapters are worth doing, but only on top of a real engine

Plugins, hooks, extensions, subagents, and custom agents are valuable where supported.

But they should be built on top of a strong engine, not used as a substitute for one.

The correct order is:

- engine first
- MCP second
- provider-native adapters third

### Conclusion 5: Project tracking is required

If Tokenizor is going to act like a local intelligence layer for AI coding CLIs, it must understand:

- what project the user is in
- which local workspace or worktree is active
- what index state exists
- what provider integrations are installed

This cannot be delegated to the clients.

It must live inside Tokenizor.

## Recommended Product Model

The current most plausible product model is:

### Layer 1: Core engine

The engine does the real work:

- discovery
- hashing
- byte-exact storage
- parsing
- symbol extraction
- verified retrieval
- search
- checkpoints
- repair
- health

This is the true successor to `jcodemunch-mcp`.

### Layer 2: Control plane

SpacetimeDB acts as the authoritative control plane for:

- repositories
- workspaces
- index runs
- checkpoints
- leases
- health events
- repair actions
- idempotency records
- file and symbol metadata

Raw bytes should remain in local CAS, not be forced into SpacetimeDB.

### Layer 3: MCP surface

MCP remains the universal compatibility layer.

This gives Tokenizor:

- interoperability across provider CLIs
- parity with the old `jcodemunch-mcp` idea
- a clean universal tool/resource/prompt surface

### Layer 4: Provider adapters

Native provider adapters should be added where clients support them.

These adapters should:

- register Tokenizor automatically
- install provider-specific instruction files
- install hooks where available
- install subagents/custom agents where available
- install prompts/commands where available

The purpose is to make Tokenizor more likely to be used constantly, not just occasionally.

## Most Plausible Runtime Architecture

The most plausible architecture today is:

- `tokenizord` as a long-lived local runtime
- `tokenizor-mcp` as a thin MCP shim
- provider-specific adapters layered on top

### Why this is probably right

If Tokenizor stays only as a stdio MCP process:

- startup is colder than it should be
- project discovery repeats too often
- background indexing is awkward
- provider hooks have nothing stable to talk to

If Tokenizor becomes a long-lived local runtime:

- project state can stay warm
- indexing can continue outside a single session
- integrations can target one local service
- the MCP layer stays thin and replaceable

## What "frequent use" should mean

The goal is not to replace native file editing tools.

The goal is for AI coding models to prefer Tokenizor for:

- repo outline
- file outline
- symbol search
- text search
- exact source retrieval
- project/session context

This is where the token savings and speed gains come from.

Native client edit tools should still perform the final file mutations.

That is the pragmatic split.

## Provider Strategy

### Claude Code

Claude is the best early native-integration target because it exposes strong hook and agent surfaces.

Use:

- MCP registration
- project or user settings
- hooks
- subagents
- prompt/command integration

### Codex

Codex is strategically important, but the currently visible public native integration surface is thinner.

So the current likely Codex strategy is:

- MCP registration
- `AGENTS.md`
- skills
- strong tool/prompt/resource design

Codex should still be supported early because it is a primary target workflow, but it should not dictate the entire architecture.

Important refinement from later research:

Codex is not limited to MCP plus `AGENTS.md`.

OpenAI now documents:

- project-scoped `.codex/config.toml`
- repository/user/admin/system skills
- automations
- the Codex SDK
- an open app-server protocol for deep custom client integrations

What is still not clearly confirmed in the public docs is a Claude-style public hook layer for intercepting workflow decisions inside Codex itself.

### Gemini CLI

Gemini has a strong extension story and should be a major target after the core engine is real enough.

### Copilot CLI and Amazon Q

Both appear worth supporting later through their own adapter layers once the engine and first adapters are stable.

## Non-Goals

The following should not drive early architecture:

- exact UI/UX parity with every existing client
- forcing all edits through Tokenizor
- embedding-first or vector-first design
- storing all raw file data in SpacetimeDB
- building plugins before core retrieval parity exists
- chasing provider-specific packaging before the engine is worth preferring

## Probable Build Order

### Phase 1: Rust parity foundation

Goal:

Build the minimum credible Rust-native `jcodemunch` successor.

Needed outcomes:

- domain models
- config and error model
- local byte-exact CAS
- SpacetimeDB boundary
- indexing skeleton
- `search_text`
- `get_file_outline`
- `get_symbol`
- `search_symbols`
- `get_repo_outline`

This phase is about core usefulness.

### Phase 2: Durable runtime model

Goal:

Evolve the engine into a local runtime that can support deeper integrations.

Needed outcomes:

- project/workspace tracking
- local IPC
- long-lived runtime process
- health and readiness reporting
- resumable background indexing

This is where `tokenizord` becomes justified.

### Phase 3: MCP maturity

Goal:

Make the MCP surface complete and production-worthy.

Needed outcomes:

- tool parity with the original core concept
- useful resources
- useful prompts
- repair and health surfaces
- stable idempotent mutations

### Phase 4: First native adapter

Goal:

Make one provider use Tokenizor much more often than plain MCP alone would achieve.

Recommended first adapter:

- Claude Code

Reason:

- strongest documented hook and agent surface

### Phase 5: Codex adapter

Goal:

Make Codex consume Tokenizor reliably through the public surfaces available today.

Likely scope:

- MCP registration
- `AGENTS.md` strategy
- install/doctor integration

### Phase 6: Gemini extension

Goal:

Ship a stronger packaged native integration where extension packaging is clearly supported.

### Phase 7: Broader adapters

- Copilot CLI
- Amazon Q

## Source Scaffold Direction

The codebase should be allowed to evolve toward a structure like this:

```text
src/
  application/
  control_plane/
  daemon/
  domain/
  indexing/
  integration/
  ipc/
  parsing/
  protocol/
  storage/
  observability/
```

And specifically:

- the engine should not depend on provider-specific code
- provider adapters should depend on the engine/runtime, not the other way around
- the MCP layer should be a transport surface, not the center of the product

## Working Decision

The current working decision should be:

**Build Tokenizor first as the Rust-native successor to `jcodemunch-mcp`, keep MCP as the default universal surface, and design the internals so provider-native adapters can later make Tokenizor a default retrieval plane rather than an occasionally used tool.**

That is the cleanest balance between:

- practicality
- correctness
- strategic flexibility
- compatibility with current provider ecosystems

## Immediate Next Documentation Set

If this direction is accepted, the next BMAD-friendly documents should probably be:

- product brief
- technical architecture
- epics and stories
- deployment/install architecture
- provider adapter architecture
- project/workspace identity model

## Related Documents

- [Architecture](architecture.md)
- [Provider CLI Integration Research](provider_cli_integration_research.md)
- [Provider CLI Runtime Architecture](provider_cli_runtime_architecture.md)
