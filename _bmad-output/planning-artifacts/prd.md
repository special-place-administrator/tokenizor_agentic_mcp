---
stepsCompleted:
  - step-01-init
  - step-02-discovery
  - step-02b-vision
  - step-02c-executive-summary
  - step-03-success
  - step-04-journeys
  - step-05-domain
  - step-06-innovation
  - step-07-project-type
  - step-08-scoping
  - step-09-functional
  - step-10-nonfunctional
  - step-11-polish
  - step-12-complete
inputDocuments:
  - _bmad-output/planning-artifacts/product-brief-tokenizor_agentic_mcp-2026-03-07.md
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
workflowType: 'prd'
documentCounts:
  briefCount: 1
  researchCount: 1
  brainstormingCount: 0
  projectDocsCount: 9
classification:
  projectType: developer_tool (hybrid with cli_tool secondary surface)
  domain: developer tooling / AI coding workflow infrastructure
  complexity: high
  projectContext: brownfield
---

# Product Requirements Document - tokenizor_agentic_mcp

**Author:** Sir
**Date:** 2026-03-07

## Executive Summary

Tokenizor provides dependable retrieval infrastructure for AI coding workflows. The product is not a conventional CLI utility, even though CLI and MCP are important delivery surfaces. The core product is the retrieval, indexing, orchestration, and recovery engine behind those surfaces.

The product exists because current AI coding workflows still rely too heavily on stitched-together file exploration: repeated file reads, broad grep, weak symbol grounding, fragile session continuity, and incomplete or unverified retrieval. That workflow is costly in tokens and latency, but more importantly it reduces trust. AI coding agents do not yet have dependable retrieval infrastructure that is trusted, durable across sessions, operationally recoverable, and natural to use as part of normal work.

Tokenizor addresses this by establishing a trusted retrieval substrate for AI coding agents. Its baseline product scope is full `jcodemunch-mcp` functional parity, rebuilt properly in Rust, with full SpacetimeDB integration as the authoritative control plane and local byte-exact content-addressed storage for raw file bytes. That baseline includes repository indexing, symbol and text search, file and repository outlines, verified source retrieval, durable project and workspace identity, resumable indexing, checkpoints, repair, health, leases, idempotency, and the operational metadata needed to make retrieval dependable rather than best-effort.

The target user is a solo power user or highly independent senior IC working in medium-to-large repositories and relying heavily on AI coding CLIs for repeated implementation and refactor workflows. For this user, success means the AI stops behaving like it is rediscovering the repository from scratch and starts behaving like it has a retrieval layer it can actually rely on. The workflow becomes more cumulative, more grounded, and more dependable across sessions and worktrees.

A core product requirement is not only to build the retrieval engine, but to make AI coding workflows use it routinely. Existing MCP-style retrieval tools often remain optional utilities instead of becoming the default path before brute-force exploration. Tokenizor therefore includes an explicit adoption layer in its product definition: usable MCP access in primary AI coding workflows plus at least one concrete mechanism that increases the likelihood of trusted retrieval being used before repeated raw file exploration. This adoption/routing requirement is part of the baseline product, not a later optimization.

### What Makes This Special

Tokenizor is differentiated by correctness that persists. Verified retrieval is the trust foundation. Durable project and workspace identity make that trust survive across sessions. SpacetimeDB-backed runs, checkpoints, leases, health, repair, and idempotency make the system operationally dependable. Local byte-exact CAS ensures that raw source storage and retrieval remain correct even in edge cases where newline translation, stale spans, or corrupted derived state would otherwise undermine trust.

The core insight is that the market gap is not simply better code search. AI coding workflows need dependable retrieval infrastructure: trusted, reusable, recoverable, and integrated into behavior. Tokenizor is therefore not positioned as another search tool or another MCP utility. It is a retrieval substrate that AI coding agents can rely on routinely, with CLI and MCP as delivery surfaces and future deeper integrations added only where they improve real workflow usage.

This is also why the product timing is strong now. AI coding CLI usage is frequent enough that repeated repository rediscovery is a real bottleneck. MCP has matured into a practical baseline across major environments. Some clients now expose deeper integration surfaces that make retrieval adoption a real product question. The prior `jcodemunch-mcp` investigation and known byte-correctness failures provide concrete design lessons. The Rust-native path is now sufficiently clear to build a more durable and correct successor rather than another experimental wrapper.

## Project Classification

- **Project Type:** Developer tooling product with CLI/MCP delivery surfaces; effectively a hybrid where `developer_tool` is primary and `cli_tool` is secondary.
- **Domain:** Developer tooling / AI coding workflow infrastructure.
- **Complexity:** High, due to correctness-critical retrieval, durable operational state, resumable indexing, repair and health flows, project/workspace identity, and retrieval adoption/routing requirements.
- **Project Context:** Brownfield, docs-led, dual-state. The intended system direction is defined primarily in the product brief and project docs, while the current codebase reflects current implementation maturity rather than final product completeness.

## Success Criteria

### User Success

Tokenizor succeeds for users when it becomes part of normal session infrastructure rather than an optional utility. The key threshold is reached when starting a session without Tokenizor feels materially worse.

User success requires all of the following:

- retrieval is trusted enough that users do not second-guess it by default
- the AI does less repeated rereading and less broad brute-force exploration
- relevant symbols, files, and source are found faster than in the prior stitched-together workflow
- project and workspace continuity reduce session-to-session rediscovery enough to be felt in normal work

In practical terms, user success means the AI feels more grounded, more cumulative, and less forgetful across repeated sessions and worktrees.

### Business Success

For the baseline release, business and product success should be judged primarily by workflow behavior change in the sharp early-user segment, not by broad adoption alone.

**3-month success** means:

- a real group of serious early users is using Tokenizor on medium-to-large repositories
- indexed projects and workspaces are being revisited across later sessions
- retrieval-first behavior is visible in real usage rather than isolated trials
- Tokenizor is proving that trusted retrieval can change normal AI coding behavior

**12-month success** means:

- Tokenizor has become routine infrastructure for a larger set of serious users
- repeat usage and dependence across active repositories and workspaces are stronger
- retrieval-first behavior is more deeply established in normal workflows
- workflow coverage has expanded where it clearly improves real usage
- broader provider coverage is added only where it strengthens actual workflow dependence

Cross-provider expansion matters, but it is not the primary early proof point. The first business proof is that Tokenizor changes workflow behavior for serious users.

### Technical Success

Technical success for the baseline release is defined by trusted retrieval, dependable continuity, and operational recoverability.

The baseline is technically successful when:

- verified retrieval is consistently enforced before source is served
- indexing can resume and recover reliably enough for real repeated use
- project and workspace continuity materially reduce session forgetfulness
- SpacetimeDB-backed operational state is real, durable, and useful in practice
- at least one retrieval-adoption mechanism materially increases retrieval-first usage behavior

The following failures are unacceptable in the baseline release:

- unverified or incorrect retrieval being served as trustworthy
- indexing that cannot resume or recover reliably enough to support real use
- project/workspace continuity breaking often enough that sessions still feel forgetful
- SpacetimeDB operational state being mostly conceptual instead of operationally useful
- adoption or routing mechanisms failing to materially influence retrieval-first usage behavior

### Measurable Outcomes

The primary measurable outcomes for the PRD are:

1. **Repeat active projects/workspaces**
   - number or percentage of indexed repositories/workspaces with repeated active use across later sessions

2. **Verified retrieval success rate**
   - percentage of retrieval responses that pass verification and are served without integrity failure

3. **Retrieval-first usage rate**
   - percentage of active sessions where Tokenizor retrieval/search flows are used early enough to replace or reduce brute-force exploration
   - if direct measurement is difficult, use a proxy tied to lookup usage before broad raw exploration

4. **Index resume/recovery success rate**
   - percentage of interrupted indexing flows that recover successfully and remain usable

Supporting metric:

- **Search/retrieval latency**
  - median and p95 latency for key lookup and retrieval flows

These metrics are intended to capture the four core proofs:
- users come back
- retrieval is trusted
- the product is used in the intended retrieval-first way
- the system is operationally dependable

## Product Scope

### MVP - Baseline Product Floor

For this project, `MVP` means the minimum baseline product floor, not a narrow utility release.

The baseline floor includes:

- full `jcodemunch-mcp` functional parity rebuilt properly in Rust
- full SpacetimeDB integration as authoritative control plane
- local byte-exact CAS for raw file bytes
- repository/folder indexing
- `search_text`
- `search_symbols`
- `get_file_outline`
- `get_repo_outline`
- verified `get_symbol` or equivalent source retrieval
- invalidate and re-index flows
- durable project/workspace identity
- resumable/recoverable indexing with checkpoints, repair, health, leases, and idempotency
- usable MCP access in primary AI coding CLI workflows
- at least one concrete retrieval-adoption mechanism that increases retrieval-first usage in a primary workflow

The baseline floor is crossed only if Tokenizor proves both engine parity and real workflow adoption.

### Growth Features (Post-Baseline)

Growth scope begins after baseline parity and adoption proof are established.

Growth features may include:

- broader workflow coverage where it clearly improves real usage
- broader provider coverage beyond the initial workflow focus
- richer MCP tools, resources, and prompts
- stronger adoption/routing mechanisms across more client paths
- broader language support
- more polished installation and distribution paths
- team-oriented operational features where they improve repeatability and usage

### Vision (Future)

The longer-term vision is a dependable local retrieval platform for AI coding workflows.

That future direction may include:

- a long-lived local runtime where justified by engine needs
- stronger project/workspace registry and continuity models
- mature repair, health, and recovery workflows
- provider-native adapters where they clearly increase usage frequency
- deeper integration into repeated AI coding sessions across multiple clients
- more team-friendly and organizationally mature deployment models

The future vision matters only if the baseline product first proves trusted retrieval, durable operational state, and retrieval-first workflow adoption.

## User Journeys

### Journey 1: Primary User Success Path - The AI Feels Grounded

We meet a solo power user in the middle of real implementation work in a medium-to-large repository. They already use AI coding CLIs heavily, but every serious session still starts the same way: repeated file reads, broad grep, manual path hunting, and rebuilding context the AI should already have. The AI is capable, but it feels forgetful.

They install Tokenizor, connect it to their normal workflow, and index a repository they return to often. In the next real session, the agent begins using repository outlines, symbol search, file outlines, and verified source retrieval instead of repeatedly brute-forcing the codebase. The user notices that the AI targets the right files and symbols faster, rereads less, and grounds its answers in source that feels trustworthy.

The climax of the journey is not simply that search works. It is that the AI stops behaving like it is rediscovering the repository from scratch and starts behaving like it has retrieval infrastructure it can rely on. The user’s workflow becomes more cumulative across sessions and worktrees. Starting a session without Tokenizor begins to feel materially worse.

**Capabilities this journey reveals:**
- repository and folder indexing
- persistent project/workspace identity
- `search_text`, `search_symbols`, `get_file_outline`, `get_repo_outline`
- verified `get_symbol` or equivalent source retrieval
- fast retrieval performance
- session-to-session continuity across repeated work

### Journey 2: Primary User Recovery Path - The System Recovers Without Losing Trust

The same user is in the middle of a long-running repository update when indexing is interrupted by a crash, machine restart, or aborted session. In a weaker tool, this would mean stale state, uncertain coverage, or starting over. That kind of fragility would quickly turn a promising tool back into an occasional utility.

Instead, the user returns and finds that the system recognizes interrupted work as an operational condition, not an unrecoverable surprise. Runs, checkpoints, and recovery state are visible. The system can resume, repair, or re-index deterministically without forcing a full reset unless one is truly needed. The user does not need to guess whether the system is safe to trust again.

The critical moment is when recovery feels explicit and dependable instead of hidden or magical. Tokenizor demonstrates that long-running indexing work can survive interruption without collapsing trust in the system. The user keeps the project current because maintenance no longer feels brittle.

**Capabilities this journey reveals:**
- durable runs and checkpoints
- resume/recovery flows for interrupted indexing
- health visibility around run state
- deterministic repair behavior
- trustworthy re-index and invalidate flows
- user-facing signals that recovery has succeeded or failed safely

### Journey 3: Operator Path - The User Keeps the System Healthy and Current

Early in the product lifecycle, the primary user is often also the operator. They are not delegating system management to a platform team; they are the person who installs Tokenizor, keeps repositories indexed, watches for stale state, and decides when to re-index or repair.

Over time, the user wants the system to feel maintainable, not like a fragile science project. They need to understand which repositories and workspaces are registered, whether indexing is healthy, whether runs are stuck, and what repair or re-index action is needed. A tooling-minded senior engineer supporting a small team may play the same role soon after.

The important shift is that operational visibility is built into the product, not buried. The user can inspect health, understand run state, and take deterministic recovery actions without guessing. Tokenizor remains usable because keeping it healthy is part of the designed experience.

**Capabilities this journey reveals:**
- repository/workspace registration and visibility
- run health and status inspection
- lease, checkpoint, and repair visibility
- explicit repair and maintenance actions
- re-index and invalidate controls
- operational feedback that is understandable to an advanced end user, not just to system developers

### Journey 4: Troubleshooting / Investigation Path - Suspect Retrieval Is Diagnosed, Not Silently Served

A user asks the AI for a symbol or source slice and gets back something that does not feel right. In a less trustworthy system, the failure might stay hidden: a bad span, stale metadata, newline corruption, or other integrity problem could be served as if it were correct. That is the most dangerous failure because it attacks the reason the product exists.

In Tokenizor, suspect retrieval is treated as a diagnosable event rather than a silent degradation. Verification checks fail explicitly. The retrieval is quarantined, blocked, or marked as untrustworthy rather than presented as trustworthy output. The user or operator can inspect the state, understand what failed, and trigger the appropriate repair or re-index path.

The critical moment is that trust is preserved by refusing to fake correctness. The product protects the user from silent corruption and provides a path back to healthy state.

**Capabilities this journey reveals:**
- verification before serving source
- quarantine or safe-fail behavior for suspect retrieval
- diagnosable integrity and health signals
- repair pathways tied to retrieval failures
- operational history that helps explain why a retrieval was blocked or marked suspect

### Journey 5: Integration / Adoption Path - The Workflow Reaches for Tokenizor First

Even useful retrieval tools often fail because the model does not use them often enough. The user may have Tokenizor installed and indexed, but if the workflow still defaults to repeated file reads and broad exploration, the product never becomes real session infrastructure.

In this journey, Tokenizor is not merely present. It is surfaced in a way that makes retrieval-first behavior more likely in an actual AI coding session. Through usable MCP access and at least one concrete retrieval-adoption mechanism in a primary workflow, the session begins by consulting Tokenizor early enough to change behavior. The model reaches for outline, symbol, search, or verified retrieval before falling back to brute-force exploration.

The climax is behavioral, not purely technical: the user notices that Tokenizor is being used early and often enough to matter. This is the point where the product stops being an optional MCP add-on and starts functioning as workflow infrastructure.

**Capabilities this journey reveals:**
- usable MCP access in primary AI coding workflows
- at least one concrete retrieval-adoption mechanism in a primary workflow
- workflow surfaces that encourage early retrieval use
- signals or instrumentation showing retrieval-first usage behavior
- integration patterns that reduce fallback to brute-force exploration

### Journey Requirements Summary

Taken together, these journeys define Tokenizor as more than a retrieval feature set. They require the product to support five connected capability areas:

- **Trusted retrieval:** verified source serving, symbol/file lookup, repository navigation, and safe failure when trust cannot be established
- **Continuity and recovery:** durable project/workspace identity, resumable indexing, checkpoints, and explicit recovery paths
- **Operational usability:** health, run, repair, and maintenance visibility for early users acting as their own operators
- **Troubleshooting and trust protection:** quarantine, diagnosable failures, and repair flows that preserve trust instead of hiding corruption
- **Adoption and workflow routing:** mechanisms that make Tokenizor retrieval more likely to be used before brute-force exploration

These journey-derived capabilities should drive the later functional requirements. If any of these five areas is missing, the product collapses toward a simpler search utility and fails to meet the intended baseline.

## Domain-Specific Requirements

Tokenizor’s domain constraints are defined less by external regulation and more by trust-critical engineering: exact bytes, verified retrieval, durable state, explicit recovery, strict trust boundaries, and workflow-level adoption.

### Compliance & Product-Integrity Standards

Tokenizor does not have a formal regulated-industry compliance baseline such as HIPAA or PCI-DSS. However, it does require explicit engineering-integrity standards as baseline product requirements:

- deterministic behavior for core indexing, retrieval, and operational workflows
- idempotent mutation semantics where applicable
- durable and inspectable operational state
- explicit recovery paths rather than hidden retry assumptions
- safe failure over silent corruption
- byte-exact storage and verified retrieval
- auditability of runs, checkpoints, repairs, and integrity failures

These are not optional quality goals. They are domain-defining product constraints.

### Security & Privacy Boundaries

The system operates over repositories that may contain sensitive or proprietary source code. The PRD should therefore require:

- raw source bytes and large derived artifacts remain local by default
- no silent exfiltration of repository content or indexed state
- provider integrations are consumers of Tokenizor capabilities, not authorities over Tokenizor truth
- provider clients do not become the default persistence layer for project, workspace, retrieval, or operational state
- retrieval and operational actions have explicit trust boundaries and diagnosable behavior
- any future remote sync, telemetry, or external export of code-derived data is explicit and opt-in, not implicit

These boundaries are necessary both for privacy and for preserving authoritative system behavior.

### Technical Constraints

The PRD should explicitly encode the following technical constraints:

- exact raw bytes matter; line-ending normalization or decode/re-encode storage behavior is unacceptable for raw content storage
- verified retrieval must be enforced before trusted source is served
- shutdown is not a safe persistence boundary
- long-running operations must be resumable
- recovery must be explicit and inspectable
- corruption must be quarantined rather than silently served or masked
- SpacetimeDB is the authoritative control plane for structured operational state, not the universal raw blob store
- local CAS is required for raw file bytes and other byte-sensitive artifacts

These constraints are fundamental to the product’s trust model.

### Integration Requirements

Integration requirements for this domain are not limited to interoperability. They also include workflow behavior:

- MCP is the baseline interoperability layer
- at least one primary workflow must include a concrete retrieval-adoption mechanism
- provider-complete native integrations are not baseline scope
- Tokenizor remains authoritative for operational state, project/workspace identity, and retrieval truth
- integration surfaces should increase retrieval-first behavior without weakening trust boundaries

This ensures the product is judged both on engine quality and on whether that engine is actually used in real workflows.

### Domain Anti-Patterns to Avoid

The PRD should explicitly guard against the following anti-patterns:

- serving suspect or unverified retrieval silently
- treating shutdown as a safe persistence boundary
- over-trusting provider clients for project/workspace identity or operational truth
- letting adoption surfaces outrun engine trust and retrieval correctness
- forcing raw blobs into SpacetimeDB by default
- hiding corruption behind retries instead of surfacing quarantine or repair states
- making recovery implicit and opaque instead of explicit and inspectable

Avoiding these anti-patterns is central to preserving product trust.

### Key Risks and Mitigations

The most important domain risks for Tokenizor are:

1. **Retrieval integrity failure**
   - mitigation: verification before serving, quarantine on failure, explicit integrity diagnostics

2. **Stale or broken operational state**
   - mitigation: authoritative control-plane state, health inspection, repair workflows, auditable run/checkpoint history

3. **Interrupted indexing without dependable recovery**
   - mitigation: resumable runs, explicit checkpoints, deterministic repair or re-index flows

4. **Weak retrieval adoption despite a capable engine**
   - mitigation: at least one primary workflow adoption mechanism, retrieval-first workflow design, instrumentation around usage behavior

5. **Project/workspace identity drift across clients**
   - mitigation: Tokenizor-owned project/workspace identity model with explicit bindings rather than client-owned truth

6. **Silent leakage or trust-boundary confusion involving proprietary repository content**
   - mitigation: local-by-default storage, explicit trust boundaries, opt-in export/telemetry rules, provider clients treated as consumers rather than authoritative stores

These risks should shape the functional and non-functional requirements that follow.

## Innovation & Novel Patterns

### Detected Innovation Areas

Tokenizor’s innovation is not a brand-new protocol or isolated feature; it is the disciplined combination of trusted retrieval, durable operational state, explicit recovery, and workflow-level retrieval adoption into one AI coding infrastructure product.

The most important innovation areas are:

- **Trusted retrieval as a first-class substrate**
  - Tokenizor is not framed as “better code search.” It treats verified retrieval as core workflow infrastructure for AI coding.

- **Control-plane plus local-CAS split as a product advantage**
  - SpacetimeDB-backed operational state and local byte-exact CAS are treated as complementary product architecture, not incidental implementation detail.

- **Recovery and repair as product behavior**
  - Checkpoints, resumability, quarantine, and repair are exposed as core product behaviors that preserve trust, not hidden backend mechanics.

- **Retrieval adoption/routing as part of the product**
  - Tokenizor challenges the assumption that MCP presence is enough for usage. It treats retrieval-first workflow behavior as something the product must actively enable.

### Market Context & Competitive Landscape

Parts of this product pattern already exist in isolation:

- indexing and search tools exist
- MCP-based retrieval tools exist
- local code intelligence tools exist
- operational state and recovery systems exist
- provider integration surfaces exist

What is uncommon is treating the following as one deliberate product baseline:

- verified byte-exact retrieval
- durable operational control-plane state
- explicit recovery and repair behavior
- project/workspace continuity
- workflow-level retrieval adoption as a first-class requirement

The positioning implication is that Tokenizor should be positioned less as a novel category invention and more as a stronger, more disciplined synthesis for AI coding infrastructure.

### Validation Approach

The innovative aspects should be validated through behavior and trust, not novelty claims.

Validation should focus on:

- whether verified retrieval and continuity materially improve workflow trust
- whether recovery and repair behavior make users keep the system current instead of abandoning it
- whether at least one retrieval-adoption mechanism measurably increases retrieval-first usage
- whether the combined product behaves more like dependable session infrastructure than an occasional tool

### Risk Mitigation

The main innovation risk is not that the core engine lacks value. The main risk is that the full product pattern may underdeliver if retrieval adoption does not improve enough in practice.

Fallback position:

- even if stronger adoption/routing mechanisms underperform, the baseline engine still has standalone value as a high-trust retrieval product if retrieval correctness, continuity, and operational durability are strong

Strategic downside of the fallback:

- the product is materially weaker if it never solves retrieval-first behavior well enough to become routine workflow infrastructure

This means innovation should be staged carefully:
- preserve the baseline engine value
- validate adoption/routing mechanisms with real workflow evidence
- avoid overstating novelty where the real advantage is disciplined integration

## Developer Tool Specific Requirements

### Project-Type Overview

Tokenizor is a developer-tooling product with CLI and MCP delivery surfaces, but it should not be specified as a tools-only utility. The product is the retrieval, indexing, orchestration, and recovery engine behind those surfaces. Project-type requirements should therefore optimize for developer workflow integration, operational clarity, and trustworthy behavior rather than generic end-user UX patterns.

The baseline product floor should be broad enough to meet practical `jcodemunch-mcp` parity expectations. That means broad multi-language tree-sitter-backed coverage is part of baseline scope, while first-class quality focus should concentrate on Rust, Python, TypeScript/JavaScript, and Go. Additional parity languages remain within the baseline floor rather than being deferred as distant stretch scope.

### Technical Architecture Considerations

The baseline architecture for this project type should assume:

- Rust-native implementation for the core engine and MCP-facing surfaces
- SpacetimeDB on the current stable public v2 line as the authoritative control plane
- local byte-exact CAS for raw file bytes and other byte-sensitive artifacts
- MCP as the baseline interoperability layer for AI coding clients
- tools, resources, and prompts all relevant to the product surface, not tools-only thinking
- CLI/operator commands as part of the product’s operational usability, not merely developer conveniences

This project type also requires clear separation between:
- engine behavior and delivery surface behavior
- operational truth and provider-consumer integrations
- trusted retrieval capability and retrieval-adoption mechanisms

### Language Matrix

Baseline language scope should be broad enough to satisfy practical parity expectations from existing `jcodemunch-mcp` users.

**Baseline parity coverage target:**
- Python
- JavaScript / TypeScript
- Go
- Rust
- Java
- PHP
- Dart
- C#
- C
- C++
- Swift
- Ruby
- Perl
- Elixir

Baseline parity means supported retrieval/indexing coverage across this set, not equal maturity across every language in the first release.

**First-class quality focus within baseline:**
- Rust
- Python
- JavaScript / TypeScript
- Go

The PRD should treat these first-class languages as the earliest quality bar for indexing, symbol extraction, and retrieval confidence, while keeping the broader parity language set inside the baseline product floor.

### Installation Methods

The baseline release should support the following installation and distribution paths:

- direct end-user binary / CLI installation
- Tokenizor-managed SpacetimeDB bootstrap using the current stable public release line
- Cargo-based developer install and local development workflow
- MCP registration/integration for primary AI coding CLI workflows

The baseline operator lifecycle should include explicit commands for:
- `init`
- `doctor`
- `migrate`
- `run`

Daemon or service-style packaging may remain later unless it becomes necessary to support the baseline adoption mechanism cleanly.

### Public Surface Requirements

The public surface must not be framed as tools-only.

**Baseline tools:**
- tools are required and ship in the baseline floor

**Baseline resources:**
- resources are baseline-supported and should include at least a minimal useful set such as:
  - repository outline
  - repository health
  - run status

**Prompts direction:**
- prompts are part of baseline product direction and should be designed in from the start
- an initial thin curated set may ship in the baseline if it materially improves usage
- tools and resources remain the higher baseline shipping priority

This project type should therefore be specified as supporting modern MCP surfaces comprehensively enough to align with current client capabilities, while keeping tools and resources ahead of prompt breadth in baseline priority.

### Documentation, Examples, and Migration Guidance

The baseline product for this project type should include documentation and examples that support both adoption and operator confidence.

Required documentation areas:

- quickstart for primary AI coding workflows
- install/bootstrap docs for Tokenizor plus SpacetimeDB
- operator docs for `init`, `doctor`, `migrate`, and `run`
- indexing, recovery, repair, and troubleshooting documentation
- trust-boundary and verified-retrieval explanation
- project/workspace identity explanation
- migration/parity guidance for users coming from `jcodemunch-mcp`

Required example coverage:

- at least one real end-to-end example workflow on a representative repository
- examples that show retrieval-first usage in a primary workflow
- examples that show recovery or troubleshooting behavior without silent failure

### Implementation Considerations

For this project type, implementation quality depends not only on feature presence but on workflow usability and operational clarity.

The PRD should therefore require:

- CLI/MCP-first workflow emphasis, with IDE/editor integration treated as secondary
- predictable installation and bootstrap behavior for advanced end users
- strong operator ergonomics for users acting as their own installers, maintainers, and troubleshooters
- documentation that explains not only how to use the system, but why trust boundaries, verification, and recovery behavior matter
- migration messaging that makes parity expectations explicit without presenting Tokenizor as a mere clone of `jcodemunch-mcp`

This section should anchor later functional requirements around what it means to ship a credible developer infrastructure product rather than a thin tool wrapper.

## Project Scoping & Phased Development

### Baseline Strategy & Philosophy

**MVP Approach:** Tokenizor’s baseline release is a problem-solving platform MVP: the minimum product must already solve dependable retrieval for serious AI coding workflows, which requires shipping a foundational infrastructure layer rather than a thin utility.

This means the release strategy is hybrid:
- **problem-solving MVP in user terms** because it must materially improve real AI coding workflows
- **platform MVP in system terms** because solving that problem requires trusted retrieval, durable state, recovery behavior, and workflow integration

**Resource Requirements:** The baseline release should be planned for a small, high-leverage engineering team, most credibly 2-3 strong engineers with one acting as product/technical lead. The scope is too trust-critical to read as a trivial solo-weekend build, but it does not require a broad cross-functional organization to define or validate the baseline.

### Baseline Feature Set (Phase 1)

**Core User Journeys Supported:**
- primary user success path
- primary user recovery/edge-case path
- operator/self-maintenance path
- troubleshooting/trust-protection path
- integration/adoption path

**Must-Have Capabilities:**
- full `jcodemunch-mcp` functional parity rebuilt properly in Rust
- verified byte-exact retrieval across the baseline language coverage set
- durable project/workspace identity
- full SpacetimeDB-backed operational state for repositories, runs, checkpoints, leases, health, repair, idempotency, and file/symbol metadata
- local byte-exact CAS for raw file bytes and other byte-sensitive artifacts
- resumable indexing, repair, and deterministic re-index behavior
- baseline tools required for indexing, search, outline, retrieval, run inspection, repair, and invalidation
- minimal useful baseline resources such as repository outline, repository health, and run status
- usable MCP access in primary AI coding CLI workflows
- at least one concrete retrieval-adoption mechanism in a primary workflow
- operator lifecycle support including `init`, `doctor`, `migrate`, and `run`
- documentation and examples sufficient for installation, trust understanding, recovery, troubleshooting, and parity onboarding

**Baseline Release Gate:**
Tokenizor’s baseline release is complete only when it proves trusted retrieval, durable operational state, and retrieval-first workflow adoption on real repositories.

In practical terms, this means a serious user can rely on verified retrieval, durable project and operational state, and at least one workflow that causes Tokenizor to be used before brute-force repository exploration.

### Post-Baseline Features

**Phase 2 (Post-Baseline):**
- broader provider coverage beyond the initial workflow focus
- stronger adoption/routing mechanisms across more client paths
- richer MCP resources beyond the minimal useful baseline set
- initial prompt set expansion where it materially improves usage
- broader language maturity improvements beyond the first-class quality focus
- more polished installation and distribution experiences
- team-oriented workflow and operational enhancements where they improve repeatability

**Phase 3 (Expansion):**
- long-lived local runtime or daemon shape where justified by engine pressure and usage evidence
- provider-native adapters where they clearly increase usage frequency
- richer prompts/resources ecosystems
- broader team and organizational deployment maturity
- expanded workflow coverage across additional AI coding environments
- deeper platform behaviors beyond the baseline retrieval substrate

### Risk Mitigation Strategy

**Technical Risks:** The highest technical risk is making trusted retrieval real end-to-end. Scope should protect byte-exact storage, verified retrieval, metadata correctness, integrity failure handling, and quality in the first-class language set before expanding breadth. A close second risk is whether retrieval-adoption mechanisms materially change workflow behavior.

**Market Risks:** The biggest market risk is not whether users like the concept, but whether Tokenizor changes real session behavior enough to become routine infrastructure. The baseline addresses this by requiring repeated usage on real repositories and at least one workflow that measurably increases retrieval-first behavior.

**Resource Risks:** If resources tighten, the first cuts should come from post-baseline growth rather than the baseline floor. The first features to defer are broader provider coverage, prompts breadth, and richer resource breadth beyond the minimal useful baseline set. Daemon/service packaging can remain deferred unless it becomes necessary for the baseline adoption path. Core trust, recovery, continuity, SpacetimeDB integration, CAS, and baseline adoption behavior should not be cut.

## Functional Requirements

### Repository & Workspace Lifecycle

- FR1: Users can initialize Tokenizor for a local repository or folder they want to use in AI coding workflows.
- FR2: Users can register and manage projects and workspaces as durable Tokenizor identities across sessions.
- FR3: Users can associate multiple workspaces or worktrees with the same underlying project where applicable.
- FR4: Users and AI coding workflows can have Tokenizor resolve the active project/workspace from current context or explicit override when a session begins.
- FR5: Users can inspect which projects and workspaces are currently known to Tokenizor.
- FR6: Users can update or migrate Tokenizor project/workspace state when lifecycle changes require it.
- FR7: Users can validate local Tokenizor setup and dependency health before relying on the system in normal work.

### Indexing & Run Management

- FR8: Users can start indexing for a repository or folder and receive a durable run identity for that work.
- FR9: Users can index supported repositories across the baseline language coverage set.
- FR10: Users can re-index previously indexed repositories or workspaces when source state changes.
- FR11: Users can invalidate indexed state for a repository or workspace when they need a clean rebuild.
- FR12: Users can inspect the current status and progress of an indexing run.
- FR13: Users and AI coding clients can observe live or near-live run progress and health state for active indexing work.
- FR14: Users can cancel an active indexing run when they need to stop or restart work.
- FR15: Users can checkpoint long-running indexing work so interrupted progress can be resumed or recovered later.
- FR16: Users can retry supported mutating operations with deterministic idempotent behavior, including rejection of conflicting replays where the same idempotency identity is reused with different effective inputs.

### Code Discovery & Retrieval

- FR17: Users can search indexed repositories by text content.
- FR18: Users can search indexed repositories by symbol.
- FR19: Users can retrieve a structural outline for a file.
- FR20: Users can retrieve a structural outline for a repository.
- FR21: Users can retrieve source for a symbol or equivalent code slice from indexed content.
- FR22: Users can retrieve multiple symbols or code slices in one workflow when needed.
- FR23: Users can discover code using supported languages without having to manually re-explore the repository from scratch each session.
- FR24: AI coding clients can consume Tokenizor retrieval capabilities through baseline MCP integration.

### Trust, Verification & Safe Failure

- FR25: Users can rely on Tokenizor to verify source retrieval before trusted code is served.
- FR26: The system can refuse to serve suspect or unverified retrieval as trustworthy output.
- FR27: Users can see when retrieval has failed verification and understand that the result is blocked, quarantined, or marked suspect.
- FR28: Users can rely on Tokenizor to preserve exact raw source fidelity for retrieval-sensitive content.
- FR29: Users can distinguish between trusted retrieval results and results that require repair or re-index before use.

### Recovery, Repair & Operational Continuity

- FR30: Users can resume interrupted indexing work without losing all prior progress when recovery is possible.
- FR31: Users can trigger deterministic repair or re-index flows when indexed state becomes stale, suspect, or incomplete.
- FR32: Users can inspect repair-related state, including whether a repository, run, or retrieval problem requires action.
- FR33: Users can continue using durable project/workspace context across sessions without repeatedly rebuilding the same repository understanding.
- FR34: The system can preserve operational history for runs, checkpoints, repairs, and integrity-related failures so users can understand what happened.

### Operational Visibility & Maintenance

- FR35: Users can inspect repository health within Tokenizor.
- FR36: Users can inspect run health and status for active or recent work.
- FR37: Users can inspect whether operational state indicates stale, interrupted, or suspect conditions.
- FR38: Users can perform operator lifecycle actions needed to initialize, validate, migrate, run, and maintain the product in local use.
- FR39: Advanced users acting as their own operators can maintain Tokenizor without relying on hidden or implicit system behavior.

### Workflow Integration & Retrieval Adoption

- FR40: Users can connect Tokenizor to primary AI coding CLI workflows they already use.
- FR41: AI coding workflows can access Tokenizor early enough in a session to influence repository exploration behavior.
- FR42: Users can rely on at least one primary workflow in which Tokenizor is used before broad brute-force repository exploration.
- FR43: Users can observe whether Tokenizor retrieval capabilities are being used in active workflows.
- FR44: Integration surfaces can improve retrieval-first behavior without becoming the source of truth for project, workspace, retrieval, or operational state.

### Resources, Prompts & Guidance

- FR45: AI coding clients can access minimal baseline Tokenizor resources such as repository outline, repository health, and run status.
- FR46: Users can access guidance that explains how to use Tokenizor in primary AI coding workflows.
- FR47: Users can access operational guidance for indexing, recovery, repair, troubleshooting, and trust-boundary behavior.
- FR48: Users migrating from `jcodemunch-mcp` can access parity and migration guidance for adopting Tokenizor.
- FR49: AI coding workflows can use a curated prompt surface where it materially improves adoption or retrieval usage, without making prompts the primary product surface.

## Non-Functional Requirements

Tokenizor’s non-functional requirements should optimize for trusted speed, explicit failure, durable recoverability, local-first privacy, and workflow-grade integration quality. These should be treated as baseline targets for the release, not as legal guarantees.

### Performance

Performance targets apply to a warm local index on a representative medium-to-large repository on normal developer hardware.

- **`search_text`**
  - p50: <= 150 ms
  - p95: <= 500 ms

- **`search_symbols`**
  - p50: <= 100 ms
  - p95: <= 300 ms

- **`get_file_outline`**
  - p50: <= 120 ms
  - p95: <= 350 ms

- **Verified retrieval / `get_symbol`**
  - p50: <= 150 ms
  - p95: <= 400 ms

- **Run-status / progress visibility**
  - request latency p50: <= 100 ms
  - request latency p95: <= 250 ms
  - freshness of active progress state: <= 1 second behind actual state under normal operation

These targets matter because retrieval that is trusted but too slow will not become retrieval-first workflow infrastructure.

### Reliability

- Unverified or suspect retrieval must never be silently served as trusted output.
  - target: 100% explicit safe-fail behavior

- Interrupted indexing must recover successfully in the large majority of supported interruption cases.
  - target: >= 95% successful resume/recovery when valid checkpoints exist and underlying source data remains available
  - fallback: 100% explicit deterministic re-index path when recovery is not possible

- Startup must include stale-state recovery behavior.
  - stale lease, interrupted-run, and temporary-state sweep occurs on startup before new mutating work proceeds

- Operational durability must be real, not best-effort.
  - run, checkpoint, repair, and health state must be durably recorded before the system reports those transitions as successful

- Corruption must quarantine, not propagate.
  - parse or retrieval integrity failures should isolate affected artifacts, files, or runs rather than poisoning broader system state

### Security & Privacy

- Raw source bytes and byte-sensitive derived artifacts remain local by default.
- No implicit remote export, sync, or telemetry of code-derived content is permitted.
- Any remote sync, export, or telemetry must be explicit and opt-in.
- Provider clients are consumers of Tokenizor capabilities, not authorities over Tokenizor truth.
- Provider integrations must not silently persist or redefine project/workspace, retrieval, or operational state.
- Operational mutations and integrity-significant events must be diagnosable through audit-friendly history.
- Local control-plane and related runtime surfaces should default to local-only exposure unless explicitly configured otherwise.
- Logs, diagnostics, and telemetry must avoid dumping raw source content by default unless the user explicitly requests it for troubleshooting.

### Scalability

Baseline scalability is defined for realistic local developer usage rather than internet-scale traffic.

The baseline release must credibly support:

- medium-to-large repositories used in serious AI coding workflows
- at least tens of thousands of source files in aggregate indexed state on one developer machine
- repeated use across multiple projects and workspaces/worktrees on the same machine
- concurrent retrieval while indexing is active
- one active indexing workflow per project with overlapping read/retrieval activity across projects

The baseline release must handle representative medium-to-large real-world repositories and repeated local multi-project usage without collapsing retrieval responsiveness or operational clarity.

### Integration Quality

- Tokenizor must be usable from primary AI coding CLI workflows without fragile per-session manual reconfiguration.
- Bootstrap and dependency problems must be diagnosable through `doctor`.
- Integration failures must fail clearly and safely rather than degrading into misleading partial trust.
- Tools, resources, and prompts must degrade safely when a client only partially supports MCP surfaces.
- At least one primary workflow must make retrieval-first behavior materially more likely in practice.
- Integration surfaces must not weaken trust boundaries or authoritative Tokenizor state ownership.

### Accessibility / Operability

- No special regulated accessibility baseline is required for the baseline release.
- CLI output, diagnostics, and documentation must be clear, readable, and scriptable.
- Operator-facing messages must be actionable and understandable to advanced end users, not only system developers.
- Error messages must distinguish between:
  - trust or integrity failure
  - recovery-required state
  - dependency or bootstrap failure
  - integration or configuration failure
- Indexing and repair work should use bounded concurrency and should not make normal local development workflows unusable under expected baseline usage.
