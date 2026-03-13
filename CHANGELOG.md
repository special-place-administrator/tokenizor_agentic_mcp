# Changelog

## [0.20.3](https://github.com/special-place-administrator/tokenizor_agentic_mcp/compare/v0.20.2...v0.20.3) (2026-03-13)


### Bug Fixes

* address review findings — OR search terms, range validation, depth 3, schema docs, insert spacing, token counter ([249f987](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/249f9876196de831e1dc5d6a34b8dd5cf284e1f1))

## [0.20.2](https://github.com/special-place-administrator/tokenizor_agentic_mcp/compare/v0.20.1...v0.20.2) (2026-03-13)


### Bug Fixes

* address review findings — diff_symbols filter, search prefix matching, impact messaging, chunk line numbers ([44298df](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/44298df66c3b742f2736fcd33151884b2118cbe6))
* find_dependents follows pub use re-export chains for Rust modules ([a48aee2](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/a48aee2b7180dc0b669f72b8b94f4e36b0284e0a))
* type-aware reference filtering reduces false positive warnings in replace_symbol_body ([b24dd0c](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/b24dd0cdcb5dbed44edb077959c48972563b8845))

## [0.20.1](https://github.com/special-place-administrator/tokenizor_agentic_mcp/compare/v0.20.0...v0.20.1) (2026-03-13)


### Bug Fixes

* disable git2 SSH/HTTPS features to remove OpenSSL dependency ([0167936](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/0167936069c2481cdbb07fb4e4bb5acdbd483131))

## [0.20.0](https://github.com/special-place-administrator/tokenizor_agentic_mcp/compare/v0.19.0...v0.20.0) (2026-03-13)


### Features

* add git2 library wrapper for in-process git operations ([dc5e146](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/dc5e146cccc1a8f78af7916104cf9ae87c2ab375))
* replace git CLI with git2 library in tools and diff_symbols ([db3824a](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/db3824aff0197cdc7408a82d71add37e6ae2b2e2))
* replace git log CLI with git2 library in temporal analysis ([f6877eb](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/f6877ebc81e3d8f6a6ebc6001cdbecc29979292c))

## [0.19.0](https://github.com/special-place-administrator/tokenizor_agentic_mcp/compare/v0.18.0...v0.19.0) (2026-03-13)


### Features

* update README for 24 tools, add CLAUDE.md, rename prompts to tokenizor-* prefix ([949738f](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/949738f88ce47fa3ede4a5e919127787678017bb))

## [0.18.0](https://github.com/special-place-administrator/tokenizor_agentic_mcp/compare/v0.17.1...v0.18.0) (2026-03-13)


### Features

* add depth parameter to explore for enriched symbol analysis ([a81fdad](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/a81fdad79b2ff06b96e9e841041e617675182652))
* add routing hint, code_only flag, and update stale tool descriptions ([b941eff](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/b941effdef0b608b01bb6394330f65459107f403))

## [0.17.1](https://github.com/special-place-administrator/tokenizor_agentic_mcp/compare/v0.17.0...v0.17.1) (2026-03-13)


### Bug Fixes

* resolve all actionable issues from external review ([3e11288](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/3e112880a6802182bf59c5159373fc3ab636a240))

## [0.17.0](https://github.com/special-place-administrator/tokenizor_agentic_mcp/compare/v0.16.2...v0.17.0) (2026-03-13)


### Features

* **edit:** Tier 2 batch tools — batch_edit, batch_rename, batch_insert ([859271d](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/859271d7a1b7f2954335002e2aa3c8588cae2109))

## [0.16.2](https://github.com/special-place-administrator/tokenizor_agentic_mcp/compare/v0.16.1...v0.16.2) (2026-03-13)


### Bug Fixes

* auto-indent replace_symbol_body + update edit tool descriptions ([a113c7b](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/a113c7b5444656cb3935a22defdede73ba538c2f))

## [0.16.1](https://github.com/special-place-administrator/tokenizor_agentic_mcp/compare/v0.16.0...v0.16.1) (2026-03-13)


### Bug Fixes

* auto-indent replace_symbol_body + update edit tool descriptions ([d1d22ca](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/d1d22ca3b26b59fcd107ad62e294abe10a0de678))
* auto-indent replace_symbol_body + update edit tool descriptions ([39f8fec](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/39f8fec7b95ccdb2d383c9089f6d267f4a86b69c))

## [0.16.0](https://github.com/special-place-administrator/tokenizor_agentic_mcp/compare/v0.15.0...v0.16.0) (2026-03-13)


### Features

* add symbol-addressed edit tools (Tier 1) ([3dec094](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/3dec094cb89417e7b7208caea808b151f109dbf1))
* rewrite tool descriptions with NOT-for redirects, fix verbosity polish ([466f207](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/466f207b1959df08f30d04d7e6eb3338938340d0))
* symbol-addressed edit tools + description redirects + verbosity fixes ([ba9e587](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/ba9e587d2c38e08b9e0881ebfc02bbf1c18db283))

## [0.15.0](https://github.com/special-place-administrator/tokenizor_agentic_mcp/compare/v0.14.2...v0.15.0) (2026-03-13)


### Features

* add token savings — verbosity param, sections filter, compact modes ([0184917](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/018491738218681c5d6c85c6fee267ca321a8aaa))

## [0.14.2](https://github.com/special-place-administrator/tokenizor_agentic_mcp/compare/v0.14.1...v0.14.2) (2026-03-13)


### Bug Fixes

* simplify release pipeline — let PAT-triggered run handle release ([508fbc2](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/508fbc247d7b54f79cce14526b8e755fca7acdba))

## [0.14.1](https://github.com/special-place-administrator/tokenizor_agentic_mcp/compare/v0.14.0...v0.14.1) (2026-03-13)


### Bug Fixes

* retry PR label lookup with 60s timeout for auto-merge ([1ee07e6](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/1ee07e61c5bae54a6bdeac8949bcd4d7b9d07b0c))

## [0.14.0](https://github.com/special-place-administrator/tokenizor_agentic_mcp/compare/v0.13.4...v0.14.0) (2026-03-13)


### Features

* `tokenizor-mcp init` now registers MCP server + bumps to v0.2.1 ([b2126ed](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/b2126eda812237e2bc3dc03b7ef6d2f961c94735))
* **01-01:** rewrite domain types, error.rs, lib.rs — establish v2 module skeleton ([3aa5d92](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/3aa5d92570bd065f2775a08e713e513dffd284d4))
* **01-02:** implement LiveIndex store, discovery, and query modules ([0410419](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/04104198f7e9945b396952c631b5684767fc60f1))
* **01-03:** integration tests + fix retrieval_conformance.rs for v2 ([4a3b93e](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/4a3b93ea1105c90e9c8aee151336ab9632940d07))
* **01-03:** minimal v2 main.rs entry point ([67dc213](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/67dc213e73f86c41a29c57b5b77018fb74713bc1))
* **02-01:** add src/protocol/format.rs with all formatter functions ([368e2c0](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/368e2c08dc3e02fcf7271f304280cd51ac9a6ea7))
* **02-01:** LiveIndex empty/reload/SystemTime, IndexState::Empty, SymbolKind Display ([035277b](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/035277b054cd89c40cf978f7269979796274ba1c))
* **02-02:** all 10 MCP tool handlers + input param structs ([aded9b1](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/aded9b1049e2637983d72c235b7e8ae67a2e6d24))
* **02-02:** TokenizorServer struct + ServerHandler impl + pub mod protocol ([8325190](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/832519015233bd5fa824a18967debc4f8b1602be))
* **02-03:** rewrite main.rs as persistent v2 MCP server ([8f38388](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/8f38388fedceb5aa4c4ca03d6e85b93a0f0a1bb9))
* **03-01:** extended HealthStats with watcher fields + dynamic health_report ([50e25a1](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/50e25a19a6e5b040973fd83abed4cf4b79d55cd9))
* **03-01:** LiveIndex mutation methods + watcher type stubs ([52f7cf2](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/52f7cf23f8bd14550d55390cb59ea5ab2cda1c65))
* **03-02:** implement watcher core — event processing, path normalization, lifecycle ([7028a3b](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/7028a3b467511b99ae345878e656d30c886e360f))
* **03-03:** integration tests + fix blocking recv in run_watcher ([88229cd](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/88229cd5324c0c991653bf43234af156e10dee0b))
* **03-03:** wire file watcher into main.rs, TokenizorServer, and health/index_folder tools ([ddce97d](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/ddce97d5f2550d0840d0a360cbb34ac4e53921b3))
* **04-01:** implement tree-sitter xref extraction for all 6 languages ([12f1904](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/12f1904248ec92603fee24efc9b926eb7b3e1864))
* **04-02:** cross-reference query methods with filtering and alias resolution ([5bea7f9](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/5bea7f9e167024bd243f7f7ab30dd375520b7571))
* **04-02:** verify watcher xref pipeline and add XREF-08 incremental update test ([21ae69a](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/21ae69a11b35e047783a15b9eb91ca608e7686be))
* **04-03:** add find_references, find_dependents, get_context_bundle tool handlers ([fe78101](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/fe781012ef74ddcfe5d4f143776118df67b58a14))
* **05-01:** add sidecar module structure, port/PID file management, and new deps ([47f1606](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/47f160632efd51d85e532e20b4c6d80af89bf65f))
* **05-01:** implement sidecar router, 5 endpoint handlers, and spawn_sidecar ([dd6a61e](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/dd6a61ef700b506e238a0ceeaa160144a976a605))
* **05-02:** add CLI types and hook subcommand with fail-open JSON output ([e4d6715](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/e4d6715e3c195a716735b20a27860091b3ce720f))
* **05-02:** add tokenizor init command — idempotent settings.json merge ([5f6733e](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/5f6733e800157ece7d22f27f2354e0caf9a6b5cf))
* **05-03:** integration tests for sidecar, hooks, and init ([32c6d00](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/32c6d00da8b105b920e837108a3d39b55ea04410))
* **05-03:** wire CLI dispatch and sidecar spawn into main.rs ([d9e4d83](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/d9e4d83e2794d2c8912955ac650d740933f2c879))
* **06-01:** add TokenStats, SidecarState, build_with_budget; update router and server ([72bc6c3](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/72bc6c3dcc994b53ec032f33924b99ac784ca7e9))
* **06-01:** enrich all sidecar handlers with formatted text, budget enforcement, token tracking ([32d67dc](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/32d67dc273eda3651c9783571606c378dbdcd1f9))
* **06-02:** single stdin-routed PostToolUse entry with auto-migration ([c7502b2](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/c7502b2247ffb4ea3aab7f12c8a81b458c1c81af))
* **06-02:** stdin JSON routing, Write subcommand, abs-to-rel path conversion ([bb2d083](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/bb2d0830341f968d33d63e0d0ae6580fbfdc1d36))
* **06-03:** wire token savings from sidecar into MCP health tool ([b8827f8](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/b8827f852646346f6a659bd4c8c242d3c5cb3181))
* **07-01:** add C/C++ xref queries and grammar integration tests ([cbe36ca](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/cbe36caf706bc76a35effaf764abb78eadaaa809))
* **07-02:** create TrigramIndex module and integrate into LiveIndex ([7ee7e94](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/7ee7e944a5b77c4f361d5d52ff4cfff5b1f265e1))
* **07-02:** wire trigram search, scored symbol ranking, and file tree tool ([3abdb3b](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/3abdb3b55c5ab0c35902c6cae208a2d2e71a48bb))
* **07-03:** add persistence module with snapshot types and serialize/deserialize ([2fe7168](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/2fe716875dda0aea9579c0e9c77686a3695afd40))
* **07-03:** wire persistence into main.rs with shutdown hook and startup load path ([4c07981](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/4c079817877fd3f4bd43d83389bb809f1e82964c))
* **07:** add symbol extraction for C#, Ruby, PHP, Swift, Kotlin, Dart, Perl, Elixir ([e33decb](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/e33decb122f3d48838849e7da32e4ea5da336401))
* **07:** add xref queries for C#, Ruby, PHP, Swift, Kotlin, Dart, Perl, Elixir ([2a7f2c4](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/2a7f2c4c67652ae4664919099b93a37f322015f0))
* **07:** upgrade tree-sitter to 0.26 and enable PHP, Swift, Perl parsing ([a83e536](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/a83e5365c1b9bf39ad2b60eafcaf76a774255a81))
* add around-line file content reads ([413abe7](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/413abe7c867edfd26752daa4bf6e7f8b0795f886))
* add around-match file content reads and refresh README ([9406955](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/94069558f45b8b2a0ea57e45410a58a8e96940a8))
* add deterministic file content chunking ([731bebd](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/731bebd00c8042476eb180529e984e78e57b7423))
* add exact-selector context navigation ([36243df](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/36243dffe29b34b91bb5183c4bd0f237d9c145e2))
* add exact-selector symbol context lookup ([fc28210](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/fc2821033c082fc8c002785501652289b112d01b))
* add explore tool for concept-based codebase exploration ([e7a2364](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/e7a23642730253f6e4df005eecf135460db7e6cf))
* add fully automated deployment scripts and quick start ([b2417a2](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/b2417a2194803774d506d826778001b1f56570d4))
* add Gemini CLI support (init, MCP registration, auto-allow) ([b2429b6](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/b2429b62b4e2032521ffbb2f2d1cb8eea0615883))
* add get_co_changes, diff_symbols tools + UX improvements ([c00c2f8](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/c00c2f84395a6a7f680cbc0d32838356ce106dfc))
* add import/export summaries to get_file_context ([62f38a9](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/62f38a949bbf4009e0d94b68a7566a20c0855b37))
* add Mermaid and DOT graph output for find_dependents ([127fed2](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/127fed25b4ff07ce48f82fabc03750c53d98f58b))
* add prebuilt binary distribution via npm and GitHub Releases ([ddbc5db](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/ddbc5db8ace06f955dda1ef9626218314fbabae0))
* add recursive type resolution to get_context_bundle ([d2caaca](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/d2caaca004c764d7593f69a2dce25ddf542e9324))
* add scoped search_symbols filters ([3ec4dc7](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/3ec4dc77d3de2f5ee764bb0438916f265824e78e))
* add trait/interface implementation mapping with find_implementations tool ([4be3610](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/4be36105fa605fca6b3e33e67bdba6fa79258ede))
* auto-allow all Tokenizor tools during init (no more permission prompts) ([948f360](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/948f3605da9279ae67d76f330b737009a526abf2))
* complete Epic 1 — Reliable Local Setup and Workspace Identity ([b539b76](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/b539b7692aa367e50dfbd6760ecc63af1c2148a3))
* complete Phase B - implement trace_symbol tool ([3869941](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/3869941d08a243f65b9ffc18673caac63b22f410))
* complete Phase C - implement inspect_match and locality ranking ([8d0dff9](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/8d0dff9b781fddc03bd7fbaa9c04058042b142b1))
* complete scoped search_text upgrades ([4025717](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/402571778aced351d1cb9106204a917dfd6667dc))
* **control-plane:** land story 4.3 with review fixes ([b84ce37](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/b84ce37c5ea156731a46c45befbc5ec208bcbfa7))
* daemon resilience and zero-touch install ([0d3bd80](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/0d3bd80614c720233ffb188ef1827500e83dbbc6))
* expand file content read ergonomics ([16b3a09](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/16b3a0965d9f05f248cbee4b2fdeff3d9117b57d))
* expand prompt context exact hint routing ([b7b0c42](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/b7b0c427b562c0e07ea0b661679334d279ebe955))
* expand prompt context line hint parsing ([ad0c162](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/ad0c162c74ce4c4fb6b7c2934df027df86455352))
* expand tokenizor shared MCP capabilities ([459bbb5](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/459bbb59d3de7702b66649e67e3ad325ab79b021))
* extend prompt context exact alias routing ([10294c5](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/10294c57e4f97f9c5f8cabc85ac5b2b61fbc620e))
* extend prompt context module alias routing ([1084256](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/10842566f40f7463345aabdff10e93cd5037aeb4))
* extend prompt context slash hint routing ([242ef24](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/242ef240dc6a9aecc15a064087023cdbce45b7e7))
* git temporal intelligence with churn, ownership, and co-change analysis ([d4cd579](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/d4cd579e6f842db7cc2aadd14700d42dd997d411))
* implement Epic 3 — trusted code discovery and verified retrieval ([a06e67a](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/a06e67a094a1060435853eae6b33f07ce09b9375))
* implement Story 2.1 — durable run identity ([0c54f13](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/0c54f13b7d0caf1fba907a18e581bbfd1839a6d0))
* implement Story 2.10 — invalidate indexed state for untrusted use ([d2555cf](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/d2555cf794b2cd037fdce257cd7a50e03f800e52))
* implement Story 2.11 — reject conflicting idempotent replays ([68537e4](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/68537e44264775e57bbfe03bc67b286b4f0f6610))
* implement Story 2.2 — quality-focus language indexing ([b6f6f6f](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/b6f6f6f7a11ce63fd749e7e8d4e2f5d1821bb006))
* implement Story 2.3 — persist file/symbol metadata ([a1c7342](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/a1c7342af18b21ae75de0b9c1bf7266d9ddbbb15))
* implement Story 2.4 — broader language onboarding pattern ([70cc654](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/70cc654c7975ef035875c6708ab09b1bd941ac57))
* implement Story 2.5 — run status and health inspection ([9e07b71](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/9e07b71d2e8f1fff0644eca472fcb0d41d1a3232))
* implement Story 2.7 — cancel an active indexing run safely ([9ba4a5d](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/9ba4a5d2f2856a7236428e26cd2dfa924a257f0f))
* implement Story 2.8 — checkpoint long-running indexing work ([1055568](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/105556817d2cd10078dd0eab0d9c794dac674cdd))
* implement Story 2.9 — re-index managed repository deterministically ([7696d7c](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/7696d7cd0b32bc8eac953a37af55db2b768f9d4c))
* improve prompt context symbol disambiguation ([144377d](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/144377daef8adeb5a7e80c87b441964ba96cd495))
* module-path-aware find_dependents for lib.rs, mod.rs, __init__.py, index.js ([ea2655c](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/ea2655c4c68f17bf42e914da5aa57e86c456f468))
* search_files changed_with parameter — find co-changing files via git temporal coupling ([95b5901](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/95b5901524cce5073d318c151d8c02c88201e621))
* search_text follow_refs — inline callers of enclosing symbol ([ae64a9a](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/ae64a9a12284953e7ece3e8e0da39389e533e398))
* search_text group_by parameter — deduplicate by symbol or filter imports ([8fcf8a7](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/8fcf8a7fc36685ee8688b02be08461f08971be97))
* start Phase B - implement trace_symbol tool and add handoff summary ([dd3af33](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/dd3af336acb024ada2b76d51d5ea35428e751671))
* **story-4.6:** complete IntegrityEvent instrumentation and land story ([7e8057b](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/7e8057bfbe9f541467afba8435b9e74ccddc8573))
* **story-4.7:** unified action classification with review fixes ([78dad2a](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/78dad2a0398f028265233235556dbe00f52c919e))
* suppress noisy search_symbols results by default ([3255928](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/32559289e0242d755af06896f0aa4d902a002a55))
* symbol-aware context in search_text — show enclosing symbol for each match ([73ee432](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/73ee43249263c5338236c24d00b7c645da9e4d4a))
* tokenizor v2 rewrite — in-memory LiveIndex with parasitic hook integration ([3cbc63c](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/3cbc63c350f2cafd8b77601db3235bdbba779271))


### Bug Fixes

* **02-01:** mark doctest as text to fix compilation ([92e5998](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/92e5998d6f15419c5cdde7da370abb3ea147153f))
* **05-02:** re-add pub mod cli to lib.rs after plan 01 metadata commit overwrote it ([79cb27e](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/79cb27e01e3c26e84fa88c7c7a34e588608071fd))
* **06-02:** make hook helper fns pub and fix run_hook signature in integration tests ([36d45de](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/36d45defe68af2996502c94b5fa7008a5a2222ef))
* **07:** use box-drawing chars in tier headers per CONTEXT.md ([bb22570](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/bb225701a514c477c376e7e2bc0763c667381ed6))
* add actions:write permission for workflow re-trigger ([83aeef5](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/83aeef5cc953782045dd6620c75d0c48bde461d4))
* address code review findings for Story 2.2 ([27fd538](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/27fd538bb9604e7f3c8f8b1553743ae0b581b58b))
* address code review findings for Story 2.3 ([a6c70c0](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/a6c70c01b5dfe8416d998ffcfafa3f8199e1e1c6))
* address code review findings for Story 2.4 ([2383dd7](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/2383dd74ad9bb8659dc38a169116c5a8a4df5af4))
* address code review findings for Story 2.6 ([8609d1c](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/8609d1caacce4c4e026c52ec5fc3ec7cebbec9c7))
* address code review findings for Story 2.8 ([d105124](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/d105124c74aa8a05ef2bedfe266cd76785c29cc5))
* auto-merge release-please PRs for continuous deployment ([e58ce57](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/e58ce57ce77d56e66219a6e7663459f72bfb8dc2))
* auto-register repo on index_folder and clear invalidation on reindex ([49ad871](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/49ad871bb43fde64ceddb9e5f77a8e2ab4ddc78b))
* **ci:** drop macOS builds, keep Windows and Linux only ([d265d65](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/d265d65f1cfe89d9e7e29ce12f45fe7257e3cddb))
* **ci:** use macos-latest for x64 cross-compile (macos-13 deprecated) ([bb9d65c](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/bb9d65c8b9ebbdb8f65920830dc97fd70771f29f))
* expose typed parameter schemas for all 18 MCP tools ([c8369d8](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/c8369d8c138bb18e5c7b546cb4d5d296087178a6))
* gate Windows-specific path tests with #[cfg(windows)] ([6d9f0b9](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/6d9f0b9a727a215bf499bd0329c0d107823f6a5c))
* gate Windows-specific path tests with #[cfg(windows)] ([dcc963b](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/dcc963b4d36c2048b7ad82b9dce320bbfb22b50e))
* gate Windows-specific path tests with #[cfg(windows)] ([df57ac0](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/df57ac0a8c98564c80896b91ac04c10bd18a9d7a))
* gate Windows-specific path tests with #[cfg(windows)] ([1710de0](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/1710de0ca89d0bffe124994d91b2b714ea389312))
* gate Windows-specific path tests with #[cfg(windows)] ([32f2964](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/32f2964da4f41897ac4988393968fde7505662f5))
* gate Windows-specific path tests with #[cfg(windows)] ([a1cae52](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/a1cae520cbf8809e2d4c97d6f21307012c554509))
* handle locked binary on Windows during npm update ([b57aa8c](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/b57aa8cf25fbcaf8e3e6e8d6625f7f989418c43c))
* hook detection for tokenizor-mcp binary name and npx cache warning ([d0ad70a](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/d0ad70a52297826c2187499d62e030ca55b4ee6d))
* improve context_bundle output quality and symbol_context guidance ([45eb6e4](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/45eb6e412bd1abf76d3430f761def6f319a3384d))
* improve file watcher burst handling and evict idle trackers ([442d240](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/442d24031d71607e4d84d47e32d70a65f2ec5a4c))
* improve search ranking, symbol diff accuracy, test filtering, and error messages ([13ebb36](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/13ebb3637075c2e1bf6f187b5f07722c2cd9ecec))
* include tool description rewrites in release ([afbad0c](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/afbad0cfeb117ff29a9b77d2673531ceff0941cb))
* install binary to ~/.tokenizor/bin/ to avoid Windows file-lock ([da9c40a](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/da9c40a2ccd4987d061e574dbcc5f18f13a2679c))
* keep each Where-Object filter as a single-line expression. ([e29bace](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/e29bace4e0dfa80f2a7daa6cabf3ef0936436c26))
* lenient parameter deserialization for MCP clients that stringify values ([5d613d4](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/5d613d4b9e06e296b3f3ce9bbd4d133a0a94b726))
* make installer tests host-agnostic ([6eb0bfa](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/6eb0bfada2f70bc86a4974a1f812bc7f315a63d0))
* make npm updates replace locked windows binaries ([da3f24d](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/da3f24d808aee39512545e29db989b7d4bb2f428))
* npm wrapper and release pipeline for v2 ([ab794da](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/ab794dae0105cd8bc49466bcab04d27c6fc38457))
* PowerShell -and operator parsing in install scripts ([e29bace](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/e29bace4e0dfa80f2a7daa6cabf3ef0936436c26))
* prevent analyze_file_impact from destroying index, fix close_ses… ([5af91cf](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/5af91cf906781b8c902b7ac96637a066a572d915))
* prevent analyze_file_impact from destroying index, fix close_session deadlock ([9e29787](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/9e29787709f064538ff379c234172c11e43d69cd))
* prevent analyze_file_impact index corruption and close_session deadlock ([a7e6ff3](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/a7e6ff393713c9f3be680f62338ebecab71a6731))
* re-trigger workflow after auto-merge for full automation ([2e73e1d](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/2e73e1dbcf110625c08cbd258a9b852590956b2f))
* refuse to auto-index home dirs, drive roots, and system paths ([d459bbf](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/d459bbf7bac24f9747bb23f7370f62efd328d664))
* **release:** document conventional commit requirement ([1867bce](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/1867bce6a312079e6edd5b8ccf16fc0b43f4089d))
* replace deprecated macos-13 runner with macos-latest ([4ab6e72](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/4ab6e72f1bfab0f2bdefdf915e6ff3c5d0e472ef))
* resolve 6 confirmed bugs across watcher, daemon, trigram, discovery ([a50b723](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/a50b7232519cd640aa9140f1e0e6c032fac43eeb))
* robust auto-merge for release-please PRs ([940b9b6](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/940b9b6bab4bfa22bf4455cc6474dea300f213a5))
* simplify deployment to standard MCP lifecycle ([aebf7f8](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/aebf7f85703a9010793ac4030d16dde98b7167c2))
* single-run release pipeline — no second trigger needed ([973428e](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/973428e730d1ea4d42e525597f0fa7048ca57500))
* split-brain after index_folder, empty search_symbols guard, inspect_match bounds check ([00cf4be](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/00cf4be964f1314fa7930d68e31dac2327dced27))
* **story-4.6:** code review fixes for operational history ([b8c9ab6](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/b8c9ab6bda749e1e0428c4ba5706db807ea31a9d))
* version-aware npm update + --version flag ([b935bb0](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/b935bb0ea7cc52916abace2435873f19dfe4c01d))

## [0.13.4](https://github.com/special-place-administrator/tokenizor_agentic_mcp/compare/v0.13.3...v0.13.4) (2026-03-13)


### Bug Fixes

* re-trigger workflow after auto-merge for full automation ([2e73e1d](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/2e73e1dbcf110625c08cbd258a9b852590956b2f))

## [0.13.3](https://github.com/special-place-administrator/tokenizor_agentic_mcp/compare/v0.13.2...v0.13.3) (2026-03-13)


### Bug Fixes

* robust auto-merge for release-please PRs ([940b9b6](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/940b9b6bab4bfa22bf4455cc6474dea300f213a5))

## [0.13.2](https://github.com/special-place-administrator/tokenizor_agentic_mcp/compare/v0.13.1...v0.13.2) (2026-03-13)


### Bug Fixes

* auto-merge release-please PRs for continuous deployment ([e58ce57](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/e58ce57ce77d56e66219a6e7663459f72bfb8dc2))

## [0.13.1](https://github.com/special-place-administrator/tokenizor_agentic_mcp/compare/v0.13.0...v0.13.1) (2026-03-13)


### Bug Fixes

* include tool description rewrites in release ([afbad0c](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/afbad0cfeb117ff29a9b77d2673531ceff0941cb))

## [0.13.0](https://github.com/special-place-administrator/tokenizor_agentic_mcp/compare/v0.12.0...v0.13.0) (2026-03-13)


### Features

* add get_co_changes, diff_symbols tools + UX improvements ([c00c2f8](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/c00c2f84395a6a7f680cbc0d32838356ce106dfc))


### Bug Fixes

* lenient parameter deserialization for MCP clients that stringify values ([5d613d4](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/5d613d4b9e06e296b3f3ce9bbd4d133a0a94b726))

## [0.12.0](https://github.com/special-place-administrator/tokenizor_agentic_mcp/compare/v0.11.4...v0.12.0) (2026-03-13)


### Features

* add explore tool for concept-based codebase exploration ([e7a2364](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/e7a23642730253f6e4df005eecf135460db7e6cf))
* add Gemini CLI support (init, MCP registration, auto-allow) ([b2429b6](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/b2429b62b4e2032521ffbb2f2d1cb8eea0615883))
* auto-allow all Tokenizor tools during init (no more permission prompts) ([948f360](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/948f3605da9279ae67d76f330b737009a526abf2))
* search_files changed_with parameter — find co-changing files via git temporal coupling ([95b5901](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/95b5901524cce5073d318c151d8c02c88201e621))
* search_text follow_refs — inline callers of enclosing symbol ([ae64a9a](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/ae64a9a12284953e7ece3e8e0da39389e533e398))
* search_text group_by parameter — deduplicate by symbol or filter imports ([8fcf8a7](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/8fcf8a7fc36685ee8688b02be08461f08971be97))
* symbol-aware context in search_text — show enclosing symbol for each match ([73ee432](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/73ee43249263c5338236c24d00b7c645da9e4d4a))


### Bug Fixes

* gate Windows-specific path tests with #[cfg(windows)] ([6d9f0b9](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/6d9f0b9a727a215bf499bd0329c0d107823f6a5c))
* split-brain after index_folder, empty search_symbols guard, inspect_match bounds check ([00cf4be](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/00cf4be964f1314fa7930d68e31dac2327dced27))

## [0.11.4](https://github.com/special-place-administrator/tokenizor_agentic_mcp/compare/v0.11.3...v0.11.4) (2026-03-13)


### Bug Fixes

* gate Windows-specific path tests with #[cfg(windows)] ([dcc963b](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/dcc963b4d36c2048b7ad82b9dce320bbfb22b50e))
* gate Windows-specific path tests with #[cfg(windows)] ([df57ac0](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/df57ac0a8c98564c80896b91ac04c10bd18a9d7a))

## [0.11.3](https://github.com/special-place-administrator/tokenizor_agentic_mcp/compare/v0.11.2...v0.11.3) (2026-03-13)


### Bug Fixes

* gate Windows-specific path tests with #[cfg(windows)] ([1710de0](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/1710de0ca89d0bffe124994d91b2b714ea389312))
* gate Windows-specific path tests with #[cfg(windows)] ([32f2964](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/32f2964da4f41897ac4988393968fde7505662f5))

## [0.11.2](https://github.com/special-place-administrator/tokenizor_agentic_mcp/compare/v0.11.1...v0.11.2) (2026-03-13)


### Bug Fixes

* prevent analyze_file_impact index corruption and close_session deadlock ([a7e6ff3](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/a7e6ff393713c9f3be680f62338ebecab71a6731))

## [0.11.1](https://github.com/special-place-administrator/tokenizor_agentic_mcp/compare/v0.11.0...v0.11.1) (2026-03-13)


### Bug Fixes

* prevent analyze_file_impact from destroying index, fix close_ses… ([5af91cf](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/5af91cf906781b8c902b7ac96637a066a572d915))
* prevent analyze_file_impact from destroying index, fix close_session deadlock ([9e29787](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/9e29787709f064538ff379c234172c11e43d69cd))

## [0.11.0](https://github.com/special-place-administrator/tokenizor_agentic_mcp/compare/v0.10.0...v0.11.0) (2026-03-13)


### Features

* complete Phase B - implement trace_symbol tool ([3869941](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/3869941d08a243f65b9ffc18673caac63b22f410))
* complete Phase C - implement inspect_match and locality ranking ([8d0dff9](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/8d0dff9b781fddc03bd7fbaa9c04058042b142b1))
* start Phase B - implement trace_symbol tool and add handoff summary ([dd3af33](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/dd3af336acb024ada2b76d51d5ea35428e751671))


### Bug Fixes

* improve context_bundle output quality and symbol_context guidance ([45eb6e4](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/45eb6e412bd1abf76d3430f761def6f319a3384d))
* improve file watcher burst handling and evict idle trackers ([442d240](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/442d24031d71607e4d84d47e32d70a65f2ec5a4c))
* resolve 6 confirmed bugs across watcher, daemon, trigram, discovery ([a50b723](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/a50b7232519cd640aa9140f1e0e6c032fac43eeb))

## [0.10.0](https://github.com/special-place-administrator/tokenizor_agentic_mcp/compare/v0.9.1...v0.10.0) (2026-03-12)


### Features

* git temporal intelligence with churn, ownership, and co-change analysis ([d4cd579](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/d4cd579e6f842db7cc2aadd14700d42dd997d411))

## [0.9.1](https://github.com/special-place-administrator/tokenizor_agentic_mcp/compare/v0.9.0...v0.9.1) (2026-03-12)


### Bug Fixes

* keep each Where-Object filter as a single-line expression. ([e29bace](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/e29bace4e0dfa80f2a7daa6cabf3ef0936436c26))
* PowerShell -and operator parsing in install scripts ([e29bace](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/e29bace4e0dfa80f2a7daa6cabf3ef0936436c26))

## [0.9.0](https://github.com/special-place-administrator/tokenizor_agentic_mcp/compare/v0.8.0...v0.9.0) (2026-03-12)


### Features

* daemon resilience and zero-touch install ([0d3bd80](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/0d3bd80614c720233ffb188ef1827500e83dbbc6))

## [0.8.0](https://github.com/special-place-administrator/tokenizor_agentic_mcp/compare/v0.7.0...v0.8.0) (2026-03-12)


### Features

* add trait/interface implementation mapping with find_implementations tool ([4be3610](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/4be36105fa605fca6b3e33e67bdba6fa79258ede))

## [0.7.0](https://github.com/special-place-administrator/tokenizor_agentic_mcp/compare/v0.6.0...v0.7.0) (2026-03-12)


### Features

* add import/export summaries to get_file_context ([62f38a9](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/62f38a949bbf4009e0d94b68a7566a20c0855b37))

## [0.6.0](https://github.com/special-place-administrator/tokenizor_agentic_mcp/compare/v0.5.0...v0.6.0) (2026-03-12)


### Features

* add recursive type resolution to get_context_bundle ([d2caaca](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/d2caaca004c764d7593f69a2dce25ddf542e9324))

## [0.5.0](https://github.com/special-place-administrator/tokenizor_agentic_mcp/compare/v0.4.2...v0.5.0) (2026-03-12)


### Features

* add Mermaid and DOT graph output for find_dependents ([127fed2](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/127fed25b4ff07ce48f82fabc03750c53d98f58b))

## [0.4.2](https://github.com/special-place-administrator/tokenizor_agentic_mcp/compare/v0.4.1...v0.4.2) (2026-03-12)


### Bug Fixes

* improve search ranking, symbol diff accuracy, test filtering, and error messages ([13ebb36](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/13ebb3637075c2e1bf6f187b5f07722c2cd9ecec))

## [0.4.1](https://github.com/special-place-administrator/tokenizor_agentic_mcp/compare/v0.4.0...v0.4.1) (2026-03-12)


### Bug Fixes

* **release:** document conventional commit requirement ([1867bce](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/1867bce6a312079e6edd5b8ccf16fc0b43f4089d))

## [0.4.0](https://github.com/special-place-administrator/tokenizor_agentic_mcp/compare/tokenizor_agentic_mcp-v0.3.12...tokenizor_agentic_mcp-v0.4.0) (2026-03-12)


### Features

* `tokenizor-mcp init` now registers MCP server + bumps to v0.2.1 ([b2126ed](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/b2126eda812237e2bc3dc03b7ef6d2f961c94735))
* **01-01:** rewrite domain types, error.rs, lib.rs — establish v2 module skeleton ([3aa5d92](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/3aa5d92570bd065f2775a08e713e513dffd284d4))
* **01-02:** implement LiveIndex store, discovery, and query modules ([0410419](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/04104198f7e9945b396952c631b5684767fc60f1))
* **01-03:** integration tests + fix retrieval_conformance.rs for v2 ([4a3b93e](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/4a3b93ea1105c90e9c8aee151336ab9632940d07))
* **01-03:** minimal v2 main.rs entry point ([67dc213](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/67dc213e73f86c41a29c57b5b77018fb74713bc1))
* **02-01:** add src/protocol/format.rs with all formatter functions ([368e2c0](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/368e2c08dc3e02fcf7271f304280cd51ac9a6ea7))
* **02-01:** LiveIndex empty/reload/SystemTime, IndexState::Empty, SymbolKind Display ([035277b](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/035277b054cd89c40cf978f7269979796274ba1c))
* **02-02:** all 10 MCP tool handlers + input param structs ([aded9b1](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/aded9b1049e2637983d72c235b7e8ae67a2e6d24))
* **02-02:** TokenizorServer struct + ServerHandler impl + pub mod protocol ([8325190](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/832519015233bd5fa824a18967debc4f8b1602be))
* **02-03:** rewrite main.rs as persistent v2 MCP server ([8f38388](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/8f38388fedceb5aa4c4ca03d6e85b93a0f0a1bb9))
* **03-01:** extended HealthStats with watcher fields + dynamic health_report ([50e25a1](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/50e25a19a6e5b040973fd83abed4cf4b79d55cd9))
* **03-01:** LiveIndex mutation methods + watcher type stubs ([52f7cf2](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/52f7cf23f8bd14550d55390cb59ea5ab2cda1c65))
* **03-02:** implement watcher core — event processing, path normalization, lifecycle ([7028a3b](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/7028a3b467511b99ae345878e656d30c886e360f))
* **03-03:** integration tests + fix blocking recv in run_watcher ([88229cd](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/88229cd5324c0c991653bf43234af156e10dee0b))
* **03-03:** wire file watcher into main.rs, TokenizorServer, and health/index_folder tools ([ddce97d](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/ddce97d5f2550d0840d0a360cbb34ac4e53921b3))
* **04-01:** implement tree-sitter xref extraction for all 6 languages ([12f1904](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/12f1904248ec92603fee24efc9b926eb7b3e1864))
* **04-02:** cross-reference query methods with filtering and alias resolution ([5bea7f9](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/5bea7f9e167024bd243f7f7ab30dd375520b7571))
* **04-02:** verify watcher xref pipeline and add XREF-08 incremental update test ([21ae69a](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/21ae69a11b35e047783a15b9eb91ca608e7686be))
* **04-03:** add find_references, find_dependents, get_context_bundle tool handlers ([fe78101](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/fe781012ef74ddcfe5d4f143776118df67b58a14))
* **05-01:** add sidecar module structure, port/PID file management, and new deps ([47f1606](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/47f160632efd51d85e532e20b4c6d80af89bf65f))
* **05-01:** implement sidecar router, 5 endpoint handlers, and spawn_sidecar ([dd6a61e](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/dd6a61ef700b506e238a0ceeaa160144a976a605))
* **05-02:** add CLI types and hook subcommand with fail-open JSON output ([e4d6715](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/e4d6715e3c195a716735b20a27860091b3ce720f))
* **05-02:** add tokenizor init command — idempotent settings.json merge ([5f6733e](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/5f6733e800157ece7d22f27f2354e0caf9a6b5cf))
* **05-03:** integration tests for sidecar, hooks, and init ([32c6d00](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/32c6d00da8b105b920e837108a3d39b55ea04410))
* **05-03:** wire CLI dispatch and sidecar spawn into main.rs ([d9e4d83](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/d9e4d83e2794d2c8912955ac650d740933f2c879))
* **06-01:** add TokenStats, SidecarState, build_with_budget; update router and server ([72bc6c3](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/72bc6c3dcc994b53ec032f33924b99ac784ca7e9))
* **06-01:** enrich all sidecar handlers with formatted text, budget enforcement, token tracking ([32d67dc](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/32d67dc273eda3651c9783571606c378dbdcd1f9))
* **06-02:** single stdin-routed PostToolUse entry with auto-migration ([c7502b2](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/c7502b2247ffb4ea3aab7f12c8a81b458c1c81af))
* **06-02:** stdin JSON routing, Write subcommand, abs-to-rel path conversion ([bb2d083](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/bb2d0830341f968d33d63e0d0ae6580fbfdc1d36))
* **06-03:** wire token savings from sidecar into MCP health tool ([b8827f8](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/b8827f852646346f6a659bd4c8c242d3c5cb3181))
* **07-01:** add C/C++ xref queries and grammar integration tests ([cbe36ca](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/cbe36caf706bc76a35effaf764abb78eadaaa809))
* **07-02:** create TrigramIndex module and integrate into LiveIndex ([7ee7e94](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/7ee7e944a5b77c4f361d5d52ff4cfff5b1f265e1))
* **07-02:** wire trigram search, scored symbol ranking, and file tree tool ([3abdb3b](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/3abdb3b55c5ab0c35902c6cae208a2d2e71a48bb))
* **07-03:** add persistence module with snapshot types and serialize/deserialize ([2fe7168](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/2fe716875dda0aea9579c0e9c77686a3695afd40))
* **07-03:** wire persistence into main.rs with shutdown hook and startup load path ([4c07981](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/4c079817877fd3f4bd43d83389bb809f1e82964c))
* **07:** add symbol extraction for C#, Ruby, PHP, Swift, Kotlin, Dart, Perl, Elixir ([e33decb](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/e33decb122f3d48838849e7da32e4ea5da336401))
* **07:** add xref queries for C#, Ruby, PHP, Swift, Kotlin, Dart, Perl, Elixir ([2a7f2c4](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/2a7f2c4c67652ae4664919099b93a37f322015f0))
* **07:** upgrade tree-sitter to 0.26 and enable PHP, Swift, Perl parsing ([a83e536](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/a83e5365c1b9bf39ad2b60eafcaf76a774255a81))
* add around-line file content reads ([413abe7](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/413abe7c867edfd26752daa4bf6e7f8b0795f886))
* add around-match file content reads and refresh README ([9406955](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/94069558f45b8b2a0ea57e45410a58a8e96940a8))
* add deterministic file content chunking ([731bebd](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/731bebd00c8042476eb180529e984e78e57b7423))
* add exact-selector context navigation ([36243df](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/36243dffe29b34b91bb5183c4bd0f237d9c145e2))
* add exact-selector symbol context lookup ([fc28210](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/fc2821033c082fc8c002785501652289b112d01b))
* add fully automated deployment scripts and quick start ([b2417a2](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/b2417a2194803774d506d826778001b1f56570d4))
* add prebuilt binary distribution via npm and GitHub Releases ([ddbc5db](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/ddbc5db8ace06f955dda1ef9626218314fbabae0))
* add scoped search_symbols filters ([3ec4dc7](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/3ec4dc77d3de2f5ee764bb0438916f265824e78e))
* complete Epic 1 — Reliable Local Setup and Workspace Identity ([b539b76](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/b539b7692aa367e50dfbd6760ecc63af1c2148a3))
* complete scoped search_text upgrades ([4025717](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/402571778aced351d1cb9106204a917dfd6667dc))
* **control-plane:** land story 4.3 with review fixes ([b84ce37](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/b84ce37c5ea156731a46c45befbc5ec208bcbfa7))
* expand file content read ergonomics ([16b3a09](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/16b3a0965d9f05f248cbee4b2fdeff3d9117b57d))
* expand prompt context exact hint routing ([b7b0c42](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/b7b0c427b562c0e07ea0b661679334d279ebe955))
* expand prompt context line hint parsing ([ad0c162](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/ad0c162c74ce4c4fb6b7c2934df027df86455352))
* expand tokenizor shared MCP capabilities ([459bbb5](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/459bbb59d3de7702b66649e67e3ad325ab79b021))
* extend prompt context exact alias routing ([10294c5](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/10294c57e4f97f9c5f8cabc85ac5b2b61fbc620e))
* extend prompt context module alias routing ([1084256](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/10842566f40f7463345aabdff10e93cd5037aeb4))
* extend prompt context slash hint routing ([242ef24](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/242ef240dc6a9aecc15a064087023cdbce45b7e7))
* implement Epic 3 — trusted code discovery and verified retrieval ([a06e67a](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/a06e67a094a1060435853eae6b33f07ce09b9375))
* implement Story 2.1 — durable run identity ([0c54f13](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/0c54f13b7d0caf1fba907a18e581bbfd1839a6d0))
* implement Story 2.10 — invalidate indexed state for untrusted use ([d2555cf](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/d2555cf794b2cd037fdce257cd7a50e03f800e52))
* implement Story 2.11 — reject conflicting idempotent replays ([68537e4](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/68537e44264775e57bbfe03bc67b286b4f0f6610))
* implement Story 2.2 — quality-focus language indexing ([b6f6f6f](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/b6f6f6f7a11ce63fd749e7e8d4e2f5d1821bb006))
* implement Story 2.3 — persist file/symbol metadata ([a1c7342](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/a1c7342af18b21ae75de0b9c1bf7266d9ddbbb15))
* implement Story 2.4 — broader language onboarding pattern ([70cc654](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/70cc654c7975ef035875c6708ab09b1bd941ac57))
* implement Story 2.5 — run status and health inspection ([9e07b71](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/9e07b71d2e8f1fff0644eca472fcb0d41d1a3232))
* implement Story 2.7 — cancel an active indexing run safely ([9ba4a5d](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/9ba4a5d2f2856a7236428e26cd2dfa924a257f0f))
* implement Story 2.8 — checkpoint long-running indexing work ([1055568](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/105556817d2cd10078dd0eab0d9c794dac674cdd))
* implement Story 2.9 — re-index managed repository deterministically ([7696d7c](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/7696d7cd0b32bc8eac953a37af55db2b768f9d4c))
* improve prompt context symbol disambiguation ([144377d](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/144377daef8adeb5a7e80c87b441964ba96cd495))
* module-path-aware find_dependents for lib.rs, mod.rs, __init__.py, index.js ([ea2655c](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/ea2655c4c68f17bf42e914da5aa57e86c456f468))
* **story-4.6:** complete IntegrityEvent instrumentation and land story ([7e8057b](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/7e8057bfbe9f541467afba8435b9e74ccddc8573))
* **story-4.7:** unified action classification with review fixes ([78dad2a](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/78dad2a0398f028265233235556dbe00f52c919e))
* suppress noisy search_symbols results by default ([3255928](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/32559289e0242d755af06896f0aa4d902a002a55))
* tokenizor v2 rewrite — in-memory LiveIndex with parasitic hook integration ([3cbc63c](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/3cbc63c350f2cafd8b77601db3235bdbba779271))


### Bug Fixes

* **02-01:** mark doctest as text to fix compilation ([92e5998](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/92e5998d6f15419c5cdde7da370abb3ea147153f))
* **05-02:** re-add pub mod cli to lib.rs after plan 01 metadata commit overwrote it ([79cb27e](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/79cb27e01e3c26e84fa88c7c7a34e588608071fd))
* **06-02:** make hook helper fns pub and fix run_hook signature in integration tests ([36d45de](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/36d45defe68af2996502c94b5fa7008a5a2222ef))
* **07:** use box-drawing chars in tier headers per CONTEXT.md ([bb22570](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/bb225701a514c477c376e7e2bc0763c667381ed6))
* address code review findings for Story 2.2 ([27fd538](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/27fd538bb9604e7f3c8f8b1553743ae0b581b58b))
* address code review findings for Story 2.3 ([a6c70c0](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/a6c70c01b5dfe8416d998ffcfafa3f8199e1e1c6))
* address code review findings for Story 2.4 ([2383dd7](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/2383dd74ad9bb8659dc38a169116c5a8a4df5af4))
* address code review findings for Story 2.6 ([8609d1c](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/8609d1caacce4c4e026c52ec5fc3ec7cebbec9c7))
* address code review findings for Story 2.8 ([d105124](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/d105124c74aa8a05ef2bedfe266cd76785c29cc5))
* auto-register repo on index_folder and clear invalidation on reindex ([49ad871](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/49ad871bb43fde64ceddb9e5f77a8e2ab4ddc78b))
* **ci:** drop macOS builds, keep Windows and Linux only ([d265d65](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/d265d65f1cfe89d9e7e29ce12f45fe7257e3cddb))
* **ci:** use macos-latest for x64 cross-compile (macos-13 deprecated) ([bb9d65c](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/bb9d65c8b9ebbdb8f65920830dc97fd70771f29f))
* expose typed parameter schemas for all 18 MCP tools ([c8369d8](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/c8369d8c138bb18e5c7b546cb4d5d296087178a6))
* handle locked binary on Windows during npm update ([b57aa8c](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/b57aa8cf25fbcaf8e3e6e8d6625f7f989418c43c))
* hook detection for tokenizor-mcp binary name and npx cache warning ([d0ad70a](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/d0ad70a52297826c2187499d62e030ca55b4ee6d))
* install binary to ~/.tokenizor/bin/ to avoid Windows file-lock ([da9c40a](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/da9c40a2ccd4987d061e574dbcc5f18f13a2679c))
* make installer tests host-agnostic ([6eb0bfa](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/6eb0bfada2f70bc86a4974a1f812bc7f315a63d0))
* make npm updates replace locked windows binaries ([da3f24d](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/da3f24d808aee39512545e29db989b7d4bb2f428))
* npm wrapper and release pipeline for v2 ([ab794da](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/ab794dae0105cd8bc49466bcab04d27c6fc38457))
* refuse to auto-index home dirs, drive roots, and system paths ([d459bbf](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/d459bbf7bac24f9747bb23f7370f62efd328d664))
* replace deprecated macos-13 runner with macos-latest ([4ab6e72](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/4ab6e72f1bfab0f2bdefdf915e6ff3c5d0e472ef))
* simplify deployment to standard MCP lifecycle ([aebf7f8](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/aebf7f85703a9010793ac4030d16dde98b7167c2))
* **story-4.6:** code review fixes for operational history ([b8c9ab6](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/b8c9ab6bda749e1e0428c4ba5706db807ea31a9d))
* version-aware npm update + --version flag ([b935bb0](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/b935bb0ea7cc52916abace2435873f19dfe4c01d))
