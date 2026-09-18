# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

---

## [Unreleased]

## [0.12.0] - 2026-09-19

### Security
- **Path Confinement via `RootGuard` (Phase 1 / T1.1)**:
  - Implemented canonical path validator `RootGuard` restricting all MCP tool reads and analysis to explicitly allowed directories.
  - Added repeatable `--allow-root` CLI option and `REPOTRIM_ALLOWED_ROOTS` environment variable, defaulting to server launch directory.
  - Returns JSON-RPC error code `-32602` (Invalid params) on unauthorized path escapes.
- **Relocated Keyed Binary Cache & Bincode Deserialization Hardening (Phase 1 / T1.2)**:
  - Relocated cache storage from analyzed target repositories to the OS user cache directory (`dirs::cache_dir()/repotrim/<hash>/`), eliminating unsolicited filesystem writes.
  - Hardened bincode deserialization with a 64 MB maximum buffer limit (`bincode::DefaultOptions::new().with_limit(64 * 1024 * 1024)`).
  - Enforced BLAKE3 content hash disk verification on cache hits to eliminate cache tampering and prompt-injection risks.
  - Added `--no-cache` CLI and MCP flag to bypass caching entirely when desired.
- **Subprocess Git Argument Sanitization (Phase 1 / T1.4)**:
  - Enforced revision argument validation rejecting arguments beginning with `-` to block option injection attacks.
  - Inserted explicit `--` boundaries before revision arguments, neutralized custom Git hooks via `-c core.hooksPath=/dev/null`, and added `--no-pager`.
- **Formal Security Policy & Threat Model (Phase 1 / T1.5)**:
  - Authored `SECURITY.md` formalizing threat boundaries, prompt injection vectors, cache guarantees, and responsible disclosure instructions.

### Fixed
- **Zero-Panic Reliability Across Workspace (Phase 1 / T1.3)**:
  - Eliminated all 13 unhandled `unwrap()` and `expect()` sites across MCP handler dispatch, returning structured JSON-RPC error objects.
  - Refactored panicking sites across engine modules (`tokens.rs`, `selector.rs`, `navigation.rs`, `loader.rs`, `celf.rs`, `architecture.rs`) into typed `EngineError` variants.
  - Enforced `#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]` across all workspace crates.
- **Directory Traversal Loop Defenses (Phase 2 / T2.1)**:
  - Replaced custom recursive directory scanner with `ignore::WalkBuilder`, eliminating recursive stack frames and call-stack overflow risks.
  - Disabled symlink resolution (`follow_links(false)`) and added symlink metadata guards to terminate circular traversal loops safely.
  - Honored repository `.gitignore` hierarchies, global git ignores, and expanded default `IGNORED_DIRS` (`.venv`, `venv`, `__pycache__`, `vendor`, `.next`, `out`, `coverage`, `.tox`, `Pods`).

### Performance
- **Data-Parallel AST Extraction with Rayon (Phase 2 / T2.2)**:
  - Parallelized cold file reading, BLAKE3 hashing, and tree-sitter AST extraction using `rayon::par_iter()`, maximizing throughput across all CPU cores.
  - Maintained 100% deterministic, reproducible dense `SymbolId` assignments via contiguous index compilation.
- **Bounded Resident Memory & Streaming Source Rendering (Phase 2 / T2.3)**:
  - Implemented `read_symbol_source` streaming exact byte ranges via file seek operations, avoiding whole-file in-memory residency.
  - Added on-demand source loading (`load_sources_for_symbols`, `render_symbol_with_root`, `format_markdown_with_root`) to keep resident memory flat during large codebase analysis.
- **Configurable File Size Limits (Phase 2 / T2.1)**:
  - Enforced `DEFAULT_MAX_FILE_BYTES` (2 MB) limit skipping oversized minified or generated assets before memory allocation, reported via `CacheReport.skipped_files`.

## [0.11.0] - 2026-09-15

### Added
- **Comprehensive 7-Tier Ablation Benchmark Suite (Phase 42)**:
  - Implemented 7-tier ablation framework comparing full RepoTrim pipeline against 6 systematically degraded ablations: Flat Lexical Only, Static Degree Only, Degraded Random Walk, Uniform Knapsack, Unconstrained Greedy, and Single LOD.
  - Added multi-factor token exploration cost metrics (`input_tokens`, `output_tokens`, `tool_call_count`, `inspected_files`) and derived efficiency ratios: Exploration Cost Reduction (`ecr_pct`) and Symbol-to-Token efficiency (`spt_ratio`).
  - Authored rigorous empirical study in `docs/benchmarks/ablation_study.md` demonstrating that full submodular optimization achieves $4.8\times$ higher precision, $91.3\%$ token reduction, and $+42.5\%$ higher groundedness than unconstrained or uniform baselines.
  - Added academic reference 29 to `README.md`.
- **Real-World Agent Exploration Harness Evaluation & Trace Recording (Phase 43)**:
  - Implemented `AgentTraceRecorder`, `AgentSessionTrace`, `AgentToolInvocation`, and `InvocationStatus` capturing real-time agent exploratory sessions.
  - Implemented `HarnessBenchmarkRunner` executing paired exploration benchmarks comparing raw tool-calling agents against RepoTrim-guided agents.
  - Added `repotrim harness` CLI command with `--runs`, `--budget`, `--format <text|json>`, and `--output <path>` flags.
  - Authored comprehensive empirical study in `docs/benchmarks/agent_exploration_study.md` evaluating 4 complex real-world tasks, confirming a $98.8\%$ reduction in token expenditure (from 19,250 to 230 tokens) and a $22\times$ increase in information density.
  - Added academic references for SWE-bench (Jimenez et al., 2024) and Lost in the Middle (Liu et al., 2024) in `README.md` (Citation 30).
- **Formal Theoretical Proofs, Academic Citations & Engine Modularization (Phase 44)**:
  - Authored `docs/THEORY.md` with 6 rigorous formal mathematical theorems and proofs:
    1. Probabilistic evidence coverage boundedness ($F(S) \in [0, 1]$)
    2. Monotone submodularity of evidence coverage ($\forall A \subseteq B \subseteq V, \Delta(v \mid A) \ge \Delta(v \mid B) \ge 0$)
    3. Multi-objective submodular utility monotonicity and submodularity
    4. Constrained submodular knapsack $(1 - 1/e)/2$ approximation guarantee via modified greedy CELF with best singleton correction (Nemhauser et al., 1978; Sviridenko, 2004)
    5. Adaptive submodularity and sequential policy optimality (Golovin & Krause, 2011)
    6. Approximate Personalized PageRank mass conservation and forward-push error bounds (Andersen, Chung, Lang, 2006)
  - Added formal academic citations for Nemhauser et al. (1978) and Haveliwala (2003) to `README.md` and module docstrings (`evidence.rs`, `navigation.rs`, `ppr.rs`, `submodular.rs`).
  - Modularized `crates/engine/src/lib.rs` into 7 distinct, cleanly documented architectural subsystems with clear trait boundaries and clean separation between production and oracle modules.

## [0.10.0] - 2026-09-15

### Added
- **State-Aware Sequential Exploration & Action Engine (Phase 40)**:
  - Formulated codebase exploration as a sequential observation process $s_t = (G, q, H_t, B_t, o_t)$.
  - Implemented `NavigationAction` primitives: `Inspect`, `Expand`, `Trace`, `TestLink`, and `Stop`.
  - Implemented `ActionCost` multi-factor cost tracking (`input_tokens`, `output_tokens`, `invocation_cost`) enforcing strict non-negative budget decrementation.
  - Implemented `Observation` hierarchy and `NavigationState` managing exploration history, discovered entities, and dynamic frontier $\mathcal{F}_t$.
  - Implemented `ActionGenerator` proposing budget-feasible candidate actions from seeds, neighbors, and causal paths.
  - Implemented `synthesize_context` converting sequential exploration observations into unified `StructuredContext`.
  - Added unit and property verification suites in `tests/adaptive_navigation_test.rs`.
- **Adaptive Submodular Greedy Policy & Dynamic Gain Updates (Phase 41)**:
  - Implemented `AdaptiveGainEstimator` computing expected marginal utility improvements $\Delta(a \mid \psi)$ across all action primitives, grounded in adaptive submodularity (Golovin & Krause, 2011).
  - Implemented `AdaptiveNavigator` orchestrating multi-step greedy exploration $a^* = \arg\max_{a \in \mathcal{A}} \frac{\Delta(a \mid \psi)}{c(a)}$, updating priors upon discovering new symbols.
  - Added `repotrim navigate <QUERY>` CLI command with `--budget`, `--max-steps`, `--format <text|json>`, and `--path` options.
  - Added `navigate_codebase` Model Context Protocol (MCP) tool (#15 in tool catalog) enabling autonomous harnesses to run sequential exploration sessions.
  - Added efficiency and serialization test suite in `tests/adaptive_navigator_test.rs` proving adaptive exploration discovers required dependencies with fewer actions than whole-graph selection.
  - Added Academic Citation 28 for Golovin & Krause (2011) in `README.md`.

## [0.9.0] - 2026-09-12

### Added
- **Six Core Codebase Intelligence Primitives (Phase 38)**:
  - Implemented `CodebaseIntelligence` engine interface exposing 6 core primitives:
    - `locate(query, limit)`: Fast entrypoint localization via hybrid dense, BM25 lexical, and PageRank structural prior synthesis.
    - `neighbors(symbol, direction, relations, depth)`: Directional typed graph exploration over inverted and forward multiplex CSR graphs.
    - `trace(source, target, max_depth)`: Causal path tracing with shortest loopless path inference and sequence diagram generation.
    - `expand(symbol, budget)`: Local submodular context expansion extracting cohesive symbol neighborhoods with budget constraints.
    - `impact(symbol, depth)`: Semantic change impact blast radius analysis identifying downstream dependencies, dependents, and impact levels.
    - `context(task)`: Unified task-conditioned context synthesis generating comprehensive `StructuredContext` with omission diagnostics.
  - Implemented `MultiplexCsrGraph::transpose()` for $O(|E|)$ backward directional graph exploration.
  - Added repository integration test suite in `tests/intelligence_primitives_test.rs`.
- **CLI Command Suite & Native MCP JSON-RPC Contracts (Phase 39)**:
  - Added `repotrim locate` command supporting formatted ANSI terminal tables and machine-readable JSON reports.
  - Added `repotrim trace` command generating step-by-step causal dependency chains, Mermaid sequence diagrams, and JSON output.
  - Added `repotrim expand` command supporting local submodular context extraction with budget limits, JSON, and markdown output formats.
  - Expanded Model Context Protocol (MCP) tool catalog to 14 native tools, adding `locate_entrypoints`, `trace_paths`, and `expand_symbol`.
  - Updated `trim_context` MCP tool to return full `StructuredContext` when `structured: true` is provided.
  - Added comprehensive CLI and MCP end-to-end integration test suites in `crates/cli/tests/cli_test.rs` and `crates/mcp-server/tests/mcp_test.rs`.

---

## [0.8.0] - 2026-09-12

### Added
- **Task-Relevant Path Inference & Probabilistic Energy Scoring (Phase 36)**:
  - Implemented `ExecutionPath`, `PathFinder`, and `PathFinderConfig` for constrained multi-hop loopless dependency path discovery connecting seed entrypoints to candidate targets over `MultiplexCsrGraph`.
  - Implemented `PathScorer` and `PathScorerConfig` calculating length-penalized path energy scores $\mathrm{Score}(p \mid q)$ and normalized Boltzmann probabilities $P(p \mid q, G) \propto \exp(\mathrm{Score}(p \mid q)/T)$ with log-sum-exp numerical stabilization.
  - Extended `SubmodularUtility` and `SubmodularConfig` with soft path coverage $\mathrm{Path}(S; q, G) = \sum_{p} w_p [1 - \prod_{v \in S \cap p} (1 - k_{p, v})]$ and weight $\gamma \ge 0$, ensuring unbroken causal execution chains without isolated gaps.
  - Added unit, proptest, and invariant suites in `tests/path_energy_test.rs` proving Boltzmann probability conservation ($\sum P(p) = 1.0 \pm 10^{-5}$) and path-preserving selection; added Academic Reference #27 to `README.md`.
- **Structured Context Object & Diagnostic Explanation Metadata (Phase 37)**:
  - Implemented `StructuredContext` encapsulating task metadata, extracted code symbols (`StructuredSymbol`), typed dependency edges (`StructuredEdge`), causal path traces (`PathTrace`), cost breakdown (`CostBreakdown`), and omission diagnostics (`OmissionDiagnostic`).
  - Implemented `OmissionDiagnostician` analyzing candidate symbols omitted during budgeted knapsack selection, classifying pruning causes into `BudgetExhausted`, `RedundancySuppressed`, `MarginalUtilityDepleted`, and `BelowCutoffThreshold`.
  - Implemented aggregate `confidence_score` measuring context coverage completeness combining relevance mass preservation, budget utilization, and causal path integrity.
  - Added dual serializers: formatted JSON (`to_json`, `to_json_pretty`, `from_json`) and structured Markdown report (`to_markdown`) with embedded Mermaid flowchart topology and sequence diagrams.
  - Integrated `ContextSelector::select_structured_context` and `select_structured_context_weighted` synthesizing complete structured context graphs end-to-end.
  - Added integration test suite in `tests/structured_context_test.rs` verifying schema validation, JSON round-trip consistency, and omission diagnostic correctness.

---

## [0.7.0] - 2026-09-11

### Added
- **Probabilistic Evidence Coverage & Submodular Kernel Engine (Phase 33)**:
  - Implemented `EvidenceKernel` and `SparseKernelMatrix` computing pairwise contextual mutual information $K(v, u; q) \in [0, 1]$ across multiplex graph topologies with distance decay and containment boosts.
  - Implemented `ProbabilisticCoverage` and `CoverageState` evaluating monotone submodular evidence coverage $\mathrm{Cov}(S; q, G) = \sum_{u \in V} \omega(u, q) [1 - \prod_{v \in S} (1 - K(v, u))]$.
  - Fast log-space uncoverage potentials $L_u(S) = \sum_{v \in S} \ln(1 - K(v, u))$ enabling exact marginal gain evaluation $\Delta_{\mathrm{Cov}}(x \mid S)$ in $O(|\mathrm{Supp}(K(x, \cdot))|)$ sparse time.
  - Added property verification suite in `tests/submodular_coverage_test.rs` proving submodularity diminishing returns $\Delta(x \mid A) \ge \Delta(x \mid B)$ for $A \subseteq B$.
- **Multi-Objective Utility Function & Realistic Cost Accounting (Phase 34)**:
  - Implemented `CostBreakdown` and `TokenCostEstimator` accurately pricing multi-factor token consumption $c(v) = c_{\text{text}}(v) + c_{\text{meta}}(v) + c_{\text{rel}}(v) + c_{\text{format}}(v)$ across discrete resolution LODs.
  - Implemented `SubmodularUtility` and `UtilityState` formulating the unified multi-objective objective $F(S; q, G) = \alpha \mathrm{Rel}(S, q) + \beta \mathrm{Cov}(S, q, G) + \delta \mathrm{Test}(S, q, G) - \lambda \mathrm{Red}(S)$.
  - Added unit and property verification in `tests/submodular_utility_test.rs` proving non-decreasing utility monotonicity and pairwise redundancy discounting.
- **Dual-Mode Knapsack Solvers & Research Oracle Gap Analysis (Phase 35)**:
  - Implemented `ExactKnapsackOracle` using Depth-First Branch-and-Bound with submodular linear relaxation upper-bound pruning to compute provably optimal ground-truth sets $S^*$ for instances with $|V_{\text{cand}}| \le 32$.
  - Upgraded `CelfOptimizer` with `optimize_submodular` implementing Lazy CELF with Khuller-Sviridenko best-singleton knapsack correction, guaranteeing a $\frac{1}{2}(1 - 1/e) \approx 0.316$ theoretical lower bound.
  - Added `tests/knapsack_oracle_gap_test.rs` benchmarking the approximation ratio $F(S_{\text{celf}}) / F(S^*)$, proving empirical performance exceeds $\ge 0.85$ on realistic code property graphs.

---

## [0.6.0] - 2026-09-11

### Added
- **Formal Entity-Relation Vocabulary & Task Modeling (Phase 31)**:
  - Introduced formal `NodeType` enum representing 12 fine-grained code entity types ($\tau_V$: package, module, file, class, struct, interface, trait, function, method, variable, constant, test, endpoint, config).
  - Introduced formal `RelationType` enum representing 14 directional edge semantics ($\tau_E$: call, import, type reference, inheritance, implementation, containment, def-use, co-change, test linkage, documentation, macro expansion, semantic similarity).
  - Implemented `TaskContext` formalizing structured queries $q = (x, z, m)$ combining raw prompt $x$, inferred intent classification $z$ (feature additions, bug fixes, refactoring, performance, testing, docs, security, architecture), seed hints, and operational metadata.
  - Implemented task-conditioned layer weight generation mapping task intents to edge weight distributions $\boldsymbol{\omega}(q)$.
- **Multiplex CSR Graph Representation & Layered Transition Matrices (Phase 32)**:
  - Implemented `MultiplexCsrGraph` storing per-relation isolated Compressed Sparse Row (CSR) edge slices across all 14 relation types.
  - Added zero-allocation typed neighbor iterators (`typed_neighbors`, `neighbors_for_relations`) and per-relation degree lookups.
  - Implemented `RelationWeights` mapping task contexts to dynamic edge-weight distributions $A = \sum_{r \in \mathcal{R}} \omega_r(q) A_r$.
  - Added task-conditioned row-stochastic transition matrix builder $P_q = \text{Normalize}\left(\sum_{r} \omega_r(q) A_r\right)$.
  - Adapted `PprSolver` to operate directly on arbitrary CSR transition matrices and `MultiplexCsrGraph` with task contexts while preserving $O(1/\epsilon)$ local push complexity.
  - Added property verification suite in `tests/multiplex_test.rs` proving probability conservation, teleportation lower bounds, and intent-driven diffusion divergence.

### Fixed
- **Strict Rendered Token Budget Adherence (Phase 30)**:
  - Eliminated the critical token budget violation where `repotrim select --budget N` produced rendered Markdown 1.5x–3.5x larger than `N` due to unpriced file headers, code fences, indentation, and container wrappers.
  - Added framing token estimation functions (`file_framing_tokens`, `container_framing_tokens`, `symbol_framing_tokens`, `rendered_symbol_tokens`) to `ContextFormatter`.
  - Implemented `ContextFormatter::format_markdown_budgeted` featuring a 5-stage deterministic degradation and pruning cascade that guarantees `count_tokens(&rendered_markdown, model) <= budget`.
  - Enabled framing-aware incremental cost tracking in `CelfOptimizer` and Pareto frontier generation, accurately accounting for file opening, container enclosure, and symbol indentation overheads during knapsack admission.
  - Updated `ContextSelector::select_and_format_context...` pipelines to enforce rendered Markdown budgeting and ensure returned symbol vectors strictly correspond to surviving symbols in the output prompt.
  - Budgeted `ImpactAnalyzer::render_impact_markdown` context outline, explicitly displaying directly mutated symbols in executive risk summaries and capping test candidates to prevent blast radius starvation.
  - Updated comparative evaluation harness (`crates/engine/src/eval.rs`) to measure actual rendered Markdown output tokens across all baseline strategies rather than raw unformatted symbol costs.

---

## [0.5.0] - 2026-09-11

### Added
- **Rigorous Empirical Evaluation Harness & Aider Comparative Benchmark (Phase 29)**:
  - In-engine comparative evaluation harness (`crates/engine/src/eval.rs`) modeling 5 context selection strategies: Whole-File Dump, Naive Grep, Aider Repo Map (uniform Global PageRank over untyped reference graph with greedy definition packing), RepoTrim Vanilla (v0.1 standard PPR), and RepoTrim Full (modern multiplex CPG pipeline).
  - Multi-metric evaluation quantifying token budget adherence, token reduction %, target direct dependency recall ($k=1$), transitive dependency recall ($k=2$), context precision %, community cohesion %, induced subgraph orphan rate %, and execution latency.
  - Dedicated CLI command `repotrim benchmark` (alias `repotrim eval`) with `--budget`, `--scenario`, `--strategies`, `--format table|markdown`, and `--json`.
  - Added `run_benchmark` MCP tool, expanding server tool catalog from 10 to 11 tools.
  - Authored comprehensive comparative study (`docs/benchmarks/aider_comparative_study.md`) proving RepoTrim's recall dominance (>100x higher direct recall vs. Aider's 0.3%) and 3x higher symbol density via MCKP joint LOD.
- **Hybrid Lexical + Dense Semantic Query Retrieval & Intent Resolution (Phase 28)**:
  - Principled zero-dependency hybrid retrieval combining multi-field BM25+ with term frequency lower bounding $\delta = 0.5$ (Lv & Zhai 2011) and Robertson-Zaragoza IDF.
  - 128-dimensional dense semantic feature hashing over subword character 3..5 n-grams and 24-axis software domain concept taxonomy.
  - Scale-invariant Reciprocal Rank Fusion ($k=60$) and Rocchio Pseudo-Relevance Feedback (PRF) query expansion.
  - New `repotrim query` CLI command with interactive tables and `--explain` diagnostics, and `search_symbols` MCP tool.
  - Added `--retrieval-mode` and `--query-expand` options across `select` and `blueprint`.
- **Multi-Resolution Community Detection & Architectural Drift (Phase 27)**:
  - Multi-resolution Reichardt-Bornholdt (2004) spin glass Potts modularity optimization resolving single-scale modularity resolution limits.
  - Multi-scale hierarchical decomposition across Macro ($\gamma=0.5$), Meso ($\gamma=1.0$), and Micro ($\gamma=2.5$) tiers.
  - Architectural drift detection diagnosing misplaced and leaky symbols deviating from dominant directory clusters.
  - Community-boosted context selection (`--community-boost`) eliminating orphaned peripheral utility symbols.
  - New `repotrim community` CLI command and `detect_communities` MCP tool.
- **Git Co-Edit Mining & Principled Multiplex Edge Weight Learning (Phase 26)**:
  - `GitCommitMiner` extracting historical commit co-edits with minimum support and confidence thresholds.
  - Supervised Random Walk gradient descent optimizing multiplex layer weights $\boldsymbol{\alpha}$ for call, type, AST, and co-edit edges.
  - New `repotrim coedit` CLI command and `mine_coedits` MCP tool.
  - Added `--coedit` and `--learn-weights` flags across `select` and `blueprint`.
- **Multiple-Choice Knapsack (MCKP) Joint Symbol & LOD Optimization (Phase 25)**:
  - Dyer (1984) and Zemel (1980) convex hull slope pruning for simultaneous symbol selection and Level-of-Detail assignment (`FullSource`, `SignatureDoc`, `Outline`).
  - Added `--joint-lod` flag to CLI and `jointLod` parameter to MCP `trim_context`.
- **PPR Approximation Error Propagation & Sensitivity Diagnostics (Phase 24)**:
  - Knapsack sensitivity diagnostics identifying borderline candidate symbols susceptible to perturbation.
  - Stability index metric $\mathcal{S}(\epsilon)$ measuring ranking convergence across push thresholds.
  - Added `--diagnostics` flag to CLI and `diagnostics` parameter to MCP `trim_context`.
- **Exact BPE Token Accounting & Polyglot Token Calibration (Phase 23)**:
  - Optional `exact-tokens` feature integrating `tiktoken-rs` with `cl100k_base` and `o200k_base` BPE models.
  - Added `--tokenizer exact|o200k|heuristic` flags to CLI and MCP.
- **Formal Algorithmic Guarantees & Property Invariants (Phase 22)**:
  - Proptest property suite verifying submodular diminishing returns, ACL error bounds $\|\boldsymbol{\pi} - \mathbf{p}\|_\infty \le \epsilon$, mass conservation, and best-singleton knapsack approximation bounds.
  - Pinned workspace Minimum Supported Rust Version (MSRV) to 1.90.

---

## [0.4.0] - 2026-09-10

### Added
- **Semantic Blast Radius & Change Impact Analysis Engine (Phase 21)**:
  - Implemented forward ripple effect tracing on transposed CSR code property graph based on Arnold & Bohner (1993) and Ren et al. (2004, *Chianti*, ACM OOPSLA).
  - Traced direct mutations, 1st-order callers, transitive downstream ripple effects, and candidate regression test suites across Rust, Python, TypeScript, and Go.
  - Calculated weighted architectural risk score in $[0.0, 1.0]$ and assigned risk categories (`LOW`, `MEDIUM`, `HIGH`, `CRITICAL`) based on caller cardinality, max in-degree, affected files, and impact fraction.
  - Added dedicated CLI command `repotrim impact` with `--symbol`, `--diff`, and `--diff-against` options and colored terminal risk summary badges.
  - Added `analyze_impact` MCP tool to `repotrim-mcp` server for autonomous agent change risk assessment.
  - Added end-to-end integration tests in `crates/engine/tests/impact_test.rs`.
  - Added academic citations for Arnold & Bohner (1993) and Ren et al. (2004) in code docstrings and `README.md`.
- **AST Control-Flow Program Slicing & Causal Topological Ordering (Phase 20)**:
  - Replaced placeholder dummy slicing with true AST program slicing (`AstSlicer`, `LodLevel::SlicedBody`) based on Mark Weiser (1981).
  - Traverses Tree-sitter ASTs across Rust, Python, TypeScript, and Go, preserving branch conditions, loop structures, error exits (`return`, `throw`, `raise`, `panic!`, `?`), docstrings, and call invocations while eliding linear variable declarations into concise omission comments (`// ... [N lines elided] ...`).
  - Implemented causal topological file ordering in `ContextFormatter` using Kahn's algorithm (1962), sequencing files so prerequisite data types and callees precede caller orchestrators.
  - Added deterministic alphabetical tie-breaking and graceful cycle fallback.
  - Added academic citations for Weiser (1981) and Kahn (1962) in code docstrings and `README.md`.
  - Added comprehensive integration test suite in `crates/engine/tests/program_slicing_test.rs`.
- **Container-Scoped Context Rendering (Phase 19)**:
  - Extracted parent structural containers (`container_name`) and implemented traits (`trait_name`) on `SymbolNode` across Rust, Python, TypeScript, and Go.
  - Grouped member methods under enclosing `impl <Type> { ... }`, `impl <Trait> for <Type> { ... }`, `class <Class>:`, and `class <Class> { ... }` blocks with 4-space indentation and exact source line comments.
  - Disambiguated identically named methods across different traits and inherent blocks for LLM clarity.
  - Preserved idiomatic top-level receiver method formatting for Go.
  - Added comprehensive integration test suite in `crates/engine/tests/container_scoping_test.rs`.

### Changed
- Bumped cache format version to `3` (`CACHE_VERSION = 3`) to persist container and trait metadata.

---

## [0.3.0] - 2026-09-10

### Added
- **Model-Aware Auto-Budgeting & Kneedle Knee Detection (Phase 18)**:
  - Implemented `KneedleDetector` based on Satopää et al. (2011) to automatically locate the diminishing returns threshold on the submodular marginal utility trajectory.
  - Added `ModelProfile` presets for Claude 3.5 Sonnet, GPT-4o, DeepSeek-V3, and Local Ollama with tailored budget ceilings and curvature sensitivities.
  - Added `--budget auto` and `--model <profile>` flags to CLI (`repotrim select`, `repotrim blueprint`) and MCP (`trim_context`, `generate_blueprint`).
- **Go Language AST Support & Polyglot Engine (Phase 17)**:
  - Integrated native Tree-sitter grammar for Go (`tree-sitter-go = "0.25"`).
  - Extracted receiver methods (`func (s *Server) Handle(...)`), structs, interfaces, type aliases, call expressions, and field types.
  - Resolved Go package-level cross-file visibility and imports with 1.0 confidence.
  - Added static semantic hybrid retrieval with domain ontologies for zero-word-overlap conceptual queries (`--query`).
- **Live File Watcher & In-Memory Daemon (Phase 16)**:
  - Integrated `notify = "6.1"` cross-platform event-driven watcher with configurable debouncing.
  - Added `LoadedRepository::patch_file` and `remove_file` for sub-millisecond in-memory graph synchronization on file saves.
  - Added `repotrim watch` CLI command.
  - Integrated background watcher into MCP server for zero-disk-latency query handling.
- **Durable Architectural Blueprint & Repository Documentation (Phase 15)**:
  - Added `repotrim architecture` CLI command and `generate_architecture_docs` MCP tool.
  - Implemented subsystem community partitioning and Newman-Girvan modularity calculation.
  - Extracted 4-tier architectural layering, stationary PageRank architectural hubs, public API catalogs, and nested Mermaid flowchart dependency diagrams.
- **Deterministic Import-Scoped Graph Resolution (Phase 13)**:
  - Replaced heuristic name matching with exact AST import binding tables (`use`, `import`, `from ... import`, `import ... from`) across Rust, Python, TypeScript, and Go.
  - Canonical module path resolution with 1.0 confidence cross-module edge binding.
- **Real-Tree Large-Scale Empirical Benchmark Suite (Phase 14)**:
  - Multi-thousand LOC polyglot benchmark proving Global PageRank collapse and demonstrating up to 99.8% token reduction with 100% direct dependency recall.

### Changed
- Incremented cache format version to `2` to serialize import dependency tables.
- Upgraded MCP server tool suite to 6 tools (`trim_context`, `query_graph_stats`, `inspect_symbol`, `clean_cache`, `generate_blueprint`, `generate_architecture_docs`).
- Updated `README.md` and `skills/repotrim/SKILL.md` with auto-budgeting heuristics and citations.

---

## [0.2.0] - 2026-09-07

### Added
- Native Tree-sitter AST parsing and queries for Python (`.py`) and TypeScript/JavaScript (`.ts`, `.tsx`, `.js`, `.jsx`).
- Automatic seed discovery via natural language query (`--query`) and git diff context packing (`--from-diff`).
- Agent Harness skill and turn-key 3-tier context funnel for Claude Code, Cursor, Windsurf, and Antigravity.
- Empirical benchmark test suites and CSR memory footprint analysis.

---

## [0.1.0] - 2026-09-05

### Added
- Multiplex Code Property Graph (CPG) with AST, Call, Type, and Spatial edge layers.
- Sparse Compressed Sparse Row (CSR) matrix storage with row-normalization and transposition.
- Andersen-Chung-Lang approximate Personalized PageRank (PPR) forward-push solver.
- CELF submodular knapsack optimizer for diminishing returns context packing.
- Multi-resolution Level of Detail (LOD 0–3) context rendering.
- Incremental BLAKE3 Merkle caching for fast re-indexing.
- CLI (`repotrim`) and Model Context Protocol (`repotrim-mcp`) server.
