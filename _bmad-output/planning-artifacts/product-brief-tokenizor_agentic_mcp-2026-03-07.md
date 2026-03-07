---
stepsCompleted:
  - 1
  - 2
  - 3
  - 4
  - 5
inputDocuments:
  - docs/project-overview.md
  - docs/tokenizor_project_direction.md
  - docs/architecture.md
  - docs/provider_cli_integration_research.md
  - docs/provider_cli_runtime_architecture.md
  - docs/source-tree-analysis.md
  - docs/data-models.md
  - docs/api-contracts.md
  - docs/development-guide.md
  - docs/index.md
date: 2026-03-07
author: Sir
---

# Product Brief: tokenizor_agentic_mcp

## Executive Summary

Tokenizor is a trusted retrieval substrate for AI coding workflows.

It is being built as a Rust-native successor to `jcodemunch-mcp` for developers who use AI coding agents heavily on real repositories and need something more dependable than repeated file reads, broad grep, and fragile session memory. The core problem is that current AI coding workflows still explore codebases too naively: they repeatedly re-read files, lose project context across sessions, depend on incomplete or stale retrieval, and often lack strong trust in symbol or source grounding. This wastes tokens, increases latency, and reduces confidence in agent output.

Tokenizor addresses this by providing a correctness-first indexing and retrieval layer built around verified byte-exact retrieval, durable project and workspace identity, persistent local index state, resumable indexing, and explicit operational health and repair. The minimum meaningful product is not the full long-term platform, but a fast, trustworthy system that already makes repository search, symbol lookup, file outlines, and verified source retrieval materially better than brute-force exploration through existing AI coding CLIs.

The long-term ambition is larger than "another MCP tool." Tokenizor is intended to become a dependable local retrieval engine that AI coding tools rely on routinely, with MCP as the universal compatibility surface and deeper provider-native integrations added where they improve real workflow usage. The runtime and provider-adapter story should remain target-state direction, not immediate product promise.

---

## Core Vision

### Problem Statement

AI coding agents still explore repositories too naively. In practice, they rely on a patchwork workflow of raw file reads, grep/ripgrep, manual path hunting, partial repo docs, memory files, and occasional MCP tools. This is not a trusted retrieval layer. It is stitched-together exploration that frequently has to be repeated across sessions, worktrees, and longer-running coding tasks.

### Problem Impact

This creates several concrete problems for serious AI-assisted coding workflows:

- wrong, stale, or unverified context reduces trust in agent output
- repeated exploration wastes tokens and increases latency
- project understanding must often be rebuilt from scratch each session
- refactors and larger code changes become less safe when retrieval is weak or unverifiable
- there is a practical ceiling on how much users trust AI on medium and large real-world repositories

The people who feel this pain most acutely are solo power users working in medium-to-large repositories, often across worktrees or multi-session implementation/refactor flows, and already relying heavily on AI coding CLIs such as Codex and Claude Code.

### Why Existing Solutions Fall Short

Current alternatives do not provide AI coding agents with a trusted, durable, verified retrieval layer that is both operationally reliable and natural to use across repeated sessions.

Plain AI CLI workflows rely on brute-force exploration. Existing MCP tools may help, but are often not the default path and do not always establish strong retrieval trust. Repo docs and memory files help with guidance, but they are advisory rather than authoritative. IDE-native navigation and local indexing tools can be useful for humans, but they are not designed as a persistent retrieval substrate for AI coding agents. The result is that users still operate in a fragmented workflow rather than through one dependable retrieval system.

### Proposed Solution

Tokenizor provides a local, coding-first indexing and retrieval engine designed specifically for AI coding workflows.

Its minimum meaningful version should provide:

- repository or folder indexing
- durable project and workspace identity
- persistent local index state across sessions
- fast `search_text`
- fast `search_symbols`
- `get_file_outline`
- verified `get_symbol` or equivalent source retrieval
- byte-exact raw storage behind retrieval
- basic re-index and invalidate flows
- MCP usability across major AI coding CLIs

The broader product direction may evolve this into a more durable runtime with explicit operational state, resumable indexing, recovery and repair behaviors, and later provider-native integrations that increase how often the system is actually used.

### Key Differentiators

- **Trusted retrieval foundation:** Verified byte-exact retrieval is the core trust wedge, not just faster search.
- **Correctness that persists:** Durable project and workspace state, plus dependable indexing, recovery, and repair, make trust survive across sessions.
- **Engine-first product design:** The product is the retrieval engine itself; MCP, hooks, prompts, skills, and adapters are delivery surfaces.
- **Built for repeated AI workflow use:** The goal is to become the default retrieval substrate for the session, not an occasional utility invoked at the margins.

## Target Users

### Primary Users

The primary user for Tokenizor is a solo power user or highly independent senior individual contributor working in a medium-to-large repository and already relying heavily on AI coding CLIs such as Codex or Claude Code.

This user typically works in environments that involve:

- repeated implementation sessions in the same repository
- multi-file changes and refactors
- jumping across worktrees or long-running branches
- frequent code search, symbol lookup, dependency tracing, and repo navigation
- using AI agents as a serious coding partner rather than for occasional one-off prompts

Their pain is not simply lack of file access. Their real pain is that the AI repeatedly has to rediscover the repository, reread files, and operate on context that may be incomplete, stale, or weakly grounded.

What this user wants is straightforward:

- faster grounding in real code
- less repeated exploration
- more trust in retrieved symbols and source
- context that persists across sessions and worktrees
- an AI coding workflow that feels dependable instead of fragile

This is the sharpest early user because they are highly sensitive to wasted tokens, repeated exploration, and incorrect retrieval, and they generate strong product signal quickly.

### Secondary Users

Secondary users include several adjacent groups who benefit from the same core capability, even if they are not the initial design center:

- small AI-heavy teams that want more consistent retrieval and less repeated exploration across team workflows
- tooling-minded senior engineers who want repeatable, dependable AI coding workflows
- team leads who want safer and more effective AI-assisted refactors
- platform or developer tooling engineers who may standardize, deploy, or support the system for a team
- individual contributors who benefit indirectly from a more reliable team AI workflow
- later, organizational adopters who care about operational trust, repeatability, and integration hygiene more than daily hands-on usage

The key difference from the primary segment is that these users care not only about personal speed and trust, but also about repeatability, consistency, and broader workflow hygiene.

### User Journey

#### Discovery

The primary user notices that AI coding sessions in real repositories are too repetitive, too expensive, and not trustworthy enough. They may discover Tokenizor while looking for better repo indexing, stronger MCP support, or a more dependable retrieval layer for Codex- or Claude-based workflows.

#### Onboarding

They install Tokenizor, connect it to their AI coding CLI workflow, and index a repository they work in often. The onboarding experience needs to feel like enabling a serious retrieval capability for an AI workflow, not setting up infrastructure for its own sake.

#### Core Usage

In a real coding session, the user begins relying on Tokenizor for:

- repository and file outlines
- symbol lookup
- text search
- verified source retrieval

instead of relying mainly on repeated file reads and broad grep.

#### Aha Moment

The aha moment happens when the AI stops behaving like it is rediscovering the repository from scratch and starts behaving like it has a retrieval layer it can actually trust. The AI stops feeling forgetful and starts feeling grounded.

In practical terms, the user sees:

- less repeated rereading
- faster grounding
- better symbol and file targeting
- more confidence in retrieved code

#### Long-Term Routine

Tokenizor becomes routine when starting a session without it feels noticeably worse. The user begins to expect persistent project state, fast retrieval, and stronger grounding every time they work with an AI coding agent.

#### “This Is Exactly What I Needed” Moment

The primary user reaches this moment when Tokenizor makes the AI feel materially more competent on a real repository.

That means:

- the agent finds the right code faster
- there is less re-exploration
- there is better targeting
- there is stronger trust in retrieved code
- there is continuity across sessions and worktrees
- the user feels they finally have a retrieval layer built for AI coding, not a patchwork of grep, rereads, docs, and memory files

## Success Metrics

The first phase of Tokenizor should be judged primarily by whether it creates a trusted retrieval layer that users return to and rely on in real coding workflows.

### User Success Metrics

The most important user outcomes in the first phase are:

1. **Trusted retrieval**
   - users can rely on retrieved source and symbol results as correct and verifiable
2. **Less repeated exploration**
   - users and agents spend less time re-reading files and re-discovering repository structure
3. **Faster grounding**
   - the agent can find relevant files, symbols, and source slices more quickly
4. **Continuity across sessions and worktrees**
   - project and workspace state persists in a way that reduces repeated setup and rediscovery
5. **Lower wasted tokens and latency**
   - efficiency improves as a result of better retrieval and less brute-force exploration

The most meaningful behavioral signals of success are:

- users keep Tokenizor enabled across repeated sessions
- they routinely use search and retrieval flows instead of starting with brute-force file exploration
- they rely on file outline, symbol search, and verified retrieval during normal work
- they re-index and keep project state current because the system is worth maintaining
- they return to the same indexed repositories and workspaces repeatedly
- they report that sessions without Tokenizor feel noticeably worse

The strongest behavioral proof is that Tokenizor becomes part of normal session setup rather than an optional extra tool.

### Business Objectives

#### 3-Month Objectives

At 3 months, success should mean credible evidence of real workflow value for the sharp early user segment.

That includes:

- a small but real group of heavy users using Tokenizor on real medium-to-large repositories
- repeated usage across the same repositories and workspaces, not just one-time trials
- the minimum meaningful product working reliably:
  - indexing
  - persistent project/workspace identity
  - `search_text`
  - `search_symbols`
  - `get_file_outline`
  - verified source or symbol retrieval
- strong retrieval trust signals
- evidence that users are using Tokenizor before broad raw file re-exploration
- basic recovery and resume flows working reliably enough to avoid fragile session behavior

At this stage, broad adoption is not the goal. The goal is proof that Tokenizor materially improves serious AI coding workflows.

#### 12-Month Objectives

At 12 months, success should look like a genuinely dependable retrieval layer with visible workflow dependence.

That includes:

- a stable core engine that users rely on routinely
- strong repeat usage across many indexed repositories and workspaces
- mature retrieval trust and recovery behavior
- broader provider coverage where it clearly improves real workflow usage
- deeper workflow integration where it increases usage frequency
- clear signs that users see Tokenizor as session infrastructure rather than as a niche MCP utility

The 12-month objective is not just more users. It is stronger habit, deeper workflow dependence, and higher confidence in the system as a retrieval substrate.

### Key Performance Indicators

The core KPI set for the brief should stay small and measurable.

#### Primary KPIs

1. **Repeat session retention across indexed projects/workspaces**
   - percentage of indexed projects or workspaces that return to active use in later sessions after first indexing

2. **Verified retrieval success rate**
   - percentage of retrieval requests that pass verification and are served without integrity failure

3. **Search and retrieval latency**
   - median and p95 latency for `search_text`, `search_symbols`, `get_file_outline`, and verified retrieval flows

4. **Index resume / recovery success**
   - percentage of interrupted or restart-required indexing flows that recover successfully

#### Supporting KPIs

- **Active indexed repos/workspaces**
  - number of repositories or workspaces with repeated active use, not just one-time indexing

- **Reduction in brute-force exploration behavior**
  - where directly measurable, evidence that Tokenizor-backed lookup replaces repeated broad raw exploration
  - where direct measurement is difficult, use a proxy such as Tokenizor lookup actions per active session

### Strategic Metric Guidance

Token reduction and latency improvement matter, but they should be treated as secondary outcomes rather than the headline proof of success.

In the first phase, the strongest proof points are:

- indexed projects and workspaces come back into active use
- retrieval is trusted
- the system is fast enough to become habit-forming
- indexing and recovery are operationally dependable

## Baseline Product Scope

### Core Features

The minimum baseline for Tokenizor is not a narrow utility MVP. The minimum baseline is a proper Rust-native successor to `jcodemunch-mcp`, rebuilt with stronger correctness, recovery, and operational state.

That baseline must include:

- full repository or folder indexing for real codebases
- `search_text`
- `search_symbols`
- `get_file_outline`
- `get_symbol` or equivalent verified source retrieval
- `get_repo_outline`
- invalidate and re-index flows
- durable project and workspace identity across sessions
- local byte-exact CAS for raw file bytes
- resumable and recoverable indexing behavior
- full SpacetimeDB-backed operational state for:
  - repositories
  - runs
  - checkpoints
  - leases
  - health
  - repair
  - idempotency
  - file metadata
  - symbol metadata
- usable MCP access in the primary AI coding CLI workflows
- at least one concrete retrieval-adoption mechanism in a primary AI coding CLI workflow that increases the likelihood of Tokenizor being used before brute-force exploration

This is the floor, not the stretch goal.

Tokenizor’s product definition also includes a second layer beyond the baseline engine:

- making AI coding workflows actually use trusted retrieval routinely rather than treating it as an occasional MCP tool

So the product is not just a retrieval engine. It is also an adoption and routing problem: how to make AI coding clients prefer trusted retrieval by default.

### Out of Scope for Baseline Delivery

The following may remain out of scope for the initial delivery phase, even though they matter later:

- broad or provider-complete native integration coverage such as full hook/extension/plugin/custom-agent support across multiple CLIs
- broad provider coverage beyond the initial MCP-based workflows
- advanced prompts and resources ecosystem work beyond what supports baseline usage
- broad language coverage beyond the initial language set needed for early users
- semantic or embedding-based retrieval
- full enterprise or team administration features
- rich multi-user collaboration features
- overbuilt install and distribution layers beyond what early users need to get real value

What is not out of scope is the product problem of retrieval adoption. Even if deeper native integrations are phased later, the brief must treat “getting the model to use trusted retrieval routinely” as part of the product from the start.

### Baseline Success Criteria

The baseline is worth pushing forward if it demonstrates both of the following:

#### 1. Engine Baseline Proven

- Tokenizor provides full `jcodemunch-mcp` retrieval-style functionality in a Rust-native implementation
- retrieval is verified and trusted
- project and workspace continuity materially reduce rediscovery
- indexing can resume and recover reliably enough to avoid brittleness
- SpacetimeDB-backed operational state is real, durable, and useful in practice

#### 2. Adoption Problem Is Being Solved

- users rely on Tokenizor search, outline, and retrieval during normal work, not just isolated trials
- Tokenizor is used before broad brute-force file exploration often enough to change workflow behavior
- sessions without Tokenizor feel noticeably worse
- the system starts behaving like session infrastructure rather than an occasional MCP utility

The baseline has not crossed the threshold if:

- retrieval parity is incomplete
- SpacetimeDB-backed operational state is still mostly conceptual
- users still fall back mainly to raw file rereads and broad grep
- trusted retrieval exists but does not get used often enough to matter
- Tokenizor remains a niche tool rather than a default retrieval path

The baseline product must prove trusted retrieval, not the entire platform vision.

### Future Vision

If Tokenizor is highly successful over the next 2-3 years, it may evolve into a dependable local retrieval platform for AI coding workflows.

That future state may include:

- a long-lived local runtime
- a mature project and workspace registry with stronger continuity
- stronger repair and health workflows
- broader language support
- richer MCP tools, resources, and prompts
- provider-native adapters where they clearly improve usage frequency
- deeper integration into repeated AI coding sessions across multiple provider CLIs
- more team-friendly deployment and operational maturity

But that future should build on one proven baseline:

Tokenizor is a proper Rust-native successor to `jcodemunch-mcp`, with trusted retrieval, full operational state, and a credible path to routine usage in AI coding workflows.
