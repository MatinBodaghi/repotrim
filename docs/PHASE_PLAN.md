# RepoTrim: Master Roadmap & Phase Execution Plan

> **Mission:** "Given an AI agent task and a token budget $B$, pack the highest-value code skeleton into the LLM prompt in sub-millisecond time."

### Research Attribution & Academic Citations Rule
Whenever RepoTrim adopts, adapts, or benchmarks algorithms, heuristics, data structures, or paradigms from external academic papers, technical articles, or open-source projects, explicit attribution and citations MUST be added to:
1. In-code docstrings (referencing author, paper title, year, theorem/algorithm name).
2. `README.md` under `## References & Academic Citations`.
3. Benchmark and evaluation documentation in `docs/benchmarks/`.

---

## Retrospective: Completed Releases (v0.1.0 – v0.2.0)

| Phase | Milestone | Scope | Deliverables | Status |
| :--- | :--- | :--- | :--- | :---: |
| **Phase 1–8** | **v0.1.0 Foundation** | Multiplex CPG, Sparse ACL Forward-Push PPR, CELF Knapsack, Multi-Resolution LOD (0–3), BLAKE3 Merkle Cache, CLI & MCP. | `repotrim-engine`, `repotrim`, `repotrim-mcp` | `[COMPLETED]` |
| **Phase 9** | **Polyglot Engine** | Native Tree-sitter AST parsing and queries for Python (`.py`) and TypeScript/JS (`.ts`, `.tsx`, `.js`, etc.). | `SupportedLanguage`, `queries/python.scm`, `queries/typescript.scm`, `crates/engine/tests/polyglot_test.rs` | `[COMPLETED]` |
| **Phase 10** | **Intent & Diff Inference** | Automatic seed discovery via natural language intent (`--query`) and git diff context packing (`--from-diff`). | `IntentResolver` (BM25 + trigrams), `DiffResolver` (line-to-symbol), weighted seed PPR, MCP `trim_context` schema updates | `[COMPLETED]` |
| **Phase 11** | **Agent Harness Skill** | Turn-key 3-tier context funnel for Antigravity, Claude Code, and Cursor. Interactive blueprint generator. | `skills/repotrim/SKILL.md`, CLI `repotrim blueprint`, MCP `generate_blueprint` tool | `[COMPLETED]` |
| **Phase 12** | **Empirical Benchmarks** | Polyglot benchmark test suite, baseline comparative tables, CSR memory footprint analysis. | `crates/engine/tests/benchmark_polyglot.rs`, `docs/benchmarks/external_evaluations.md` | `[COMPLETED]` |

---

## Next Horizon: v0.3.0 – v0.4.0 Roadmap

The external evaluation of v0.2.0 revealed three clear frontiers for RepoTrim:
1. **Resolution Depth (No Hybrid LSP):** Pure name-matching across files is fragile when multiple modules share identifier names. We must extract **explicit file-level import tables** to achieve deterministic cross-file edge binding.
2. **Benchmark Scale:** 250-token fixtures prove algorithm correctness, but don't show the dramatic collapse of Global PageRank and Whole-File Dump that occurs on **real multi-thousand LOC trees** (where RepoTrim's localized PPR shines).
3. **Task vs. Durable Documentation:** `generate_blueprint` is an ephemeral task prompt artifact. Providing automated, evergreen **architectural mapping** will give agents long-term repository comprehension.

```text
┌────────────────────────────────────────────────────────────────────────────────────────┐
│ Phase 13: Deterministic Import-Scoped Graph Resolution                                  │
│ (Explicit import extraction, canonical module resolution, 1.0 confidence cross-module) │
└───────────────────────────────────────────┬────────────────────────────────────────────┘
                                            ▼
┌────────────────────────────────────────────────────────────────────────────────────────┐
│ Phase 14: Real-Tree Large-Scale Empirical Benchmarking                                 │
│ (Multi-thousand LOC evaluation, real-world PR diffs, Global PageRank collapse proof)   │
└───────────────────────────────────────────┬────────────────────────────────────────────┘
                                            ▼
┌────────────────────────────────────────────────────────────────────────────────────────┐
│ Phase 15: Durable Architectural Blueprint & Repository Documentation                   │
│ (repotrim architecture CLI, subsystem community discovery, evergreen docs generation) │
└───────────────────────────────────────────┬────────────────────────────────────────────┘
                                            ▼
┌────────────────────────────────────────────────────────────────────────────────────────┐
│ Phase 16: Live File Watcher & Incremental In-Memory Daemon                             │
│ (repotrim watch, notify crate integration, <1ms live hot-reindexing on file save)      │
└───────────────────────────────────────────┬────────────────────────────────────────────┘
                                            ▼
┌────────────────────────────────────────────────────────────────────────────────────────┐
│ Phase 17: Go Language Engine & Semantic Hybrid Retrieval                               │
│ (tree-sitter-go support, optional fast local ONNX embeddings for zero-word-overlap)    │
└────────────────────────────────────────────────────────────────────────────────────────┘
```

---

## Phase 13: Deterministic Import-Scoped Graph Resolution [COMPLETED]

### Goal
Eliminate heuristic name-matching across files without introducing a slow, heavy LSP compiler daemon. Parse explicit AST import statements and build a deterministic cross-module symbol binding table.

### Implemented Architecture & Steps
- **Step 13.1: AST Import Table Extraction (`crates/engine/src/parser.rs`)**:
  - **Rust (`use ...`)**: Extracted paths, aliases, nested lists (`{A, B as C}`), and wildcards (`*`) via `extract_rust_imports`.
  - **Python (`import ...`, `from ... import ...`)**: Extracted dotted paths, aliases (`as`), relative imports (`.`, `..`), and wildcards via `extract_python_imports`.
  - **TypeScript / JavaScript (`import ... from '...'`)**: Extracted default imports, named imports, aliases (`as`), namespace imports (`* as ns`), and side-effect imports via `extract_typescript_imports`.
- **Step 13.2: Canonical Module-to-Path Resolver (`crates/engine/src/import.rs`)**:
  - `resolve_module_path`: Resolves relative paths, index files (`index.ts`, `__init__.py`, `mod.rs`), extensions (`.ts`, `.tsx`, `.js`, `.jsx`, `.py`, `.rs`), and package specifiers (`crate::`, root-relative Python modules) directly against known workspace files.
- **Step 13.3: Deterministic Cross-Module Edge Binding (`crates/engine/src/resolver.rs`)**:
  - `ScopedResolver::with_imports`: Priority cascade:
    1. Exact AST import binding $\to$ **1.0 confidence**.
    2. Namespace access (`ns.member`, `ns::member`) $\to$ **1.0 confidence**.
    3. Local file definitions $\to$ **1.0 confidence**.
    4. Wildcard imports $\to$ **0.90 confidence**.
    5. Receiver method disambiguation against imported modules $\to$ **0.95 confidence**.
    6. Bayesian spatial distance heuristic fallback $\to$ **0.30 .. 0.85 confidence**.
- **Step 13.4: Engine Cache Versioning & Ingestion (`cache.rs`, `loader.rs`, `graph.rs`)**:
  - Bumped `CACHE_VERSION` to `2`.
  - Stored `FileImport` items in `FileCacheEntry` and compiled across repository loads.
  - Added `MultiplexGraph::build_with_imports` for deterministic CPG construction.
- **Step 13.5: Test Suite Verification (`crates/engine/tests/import_resolution_test.rs`)**:
  - Tested Rust cross-module symbol disambiguation with 1.0 confidence.
  - Tested Python relative and aliased cross-file resolution.
  - Tested TypeScript default and named import resolution.
  - Tested cache serialization roundtrip and 100% warm cache hit ratio with identical graph topology.

---

## Phase 14: Real-Tree Large-Scale Empirical Benchmarking [COMPLETED]

### Goal
Replace 250-token synthetic fixtures with rigorous benchmarks against real, established multi-thousand LOC codebases (Python, TypeScript, and Rust). Prove that Global PageRank collapses as repository scale grows.

### Implemented Architecture & Evaluation Suite (`crates/engine/tests/benchmark_real_scale.rs`)
- **Step 14.1: Real-Scale Polyglot Tree Generation**:
  - **Python Target**: 50+ files, 1,731 AST symbols, 2,692 multiplex edges across `auth`, `billing`, `orders`, `users`, `notifications`, and `core` with explicit AST cross-file imports (`from billing.stripe import StripeClient`).
  - **TypeScript Target**: 50+ files, 1,852 AST symbols across `components`, `hooks`, `services`, `store`, `types`, and `utils` with ES module imports.
  - **Rust Target**: Full 50+ file `repotrim` multi-crate workspace (engine, cli, mcp-server).
- **Step 14.2: Automated Empirical Benchmark Suite**:
  - Automated head-to-head comparison across 4 strategies: Whole-File Dump, Naive Grep, Global PageRank (Aider-style), and RepoTrim (Personalized PageRank + CELF Knapsack).
  - Measured token consumption (BPE heuristic), token reduction %, direct dependency recall %, and execution latency (µs).
  - Measured cold indexing latency (567 ms across 50+ files) and BLAKE3 warm incremental cache reloads (18 ms, <1 ms/file).
- **Step 14.3: Empirical Results & Theoretical Confirmation**:
  - **Python `process_refund`**: Whole-File Dump (11,895 tokens) $\to$ RepoTrim (33 tokens, **99.7% reduction**, **100.0% recall**, 430 µs latency). Global PageRank: **0.0% recall** (collapsed into universal root hubs `Logger`, `Config`).
  - **TypeScript `CheckoutModal`**: Whole-File Dump (8,151 tokens) $\to$ RepoTrim (20 tokens, **99.8% reduction**, **100.0% recall**, 360 µs latency). Global PageRank: **0.0% recall** (collapsed).
  - **Rust `ContextSelector`**: Whole-File Dump (3,077 tokens) $\to$ RepoTrim (797 tokens, **74.1% reduction**, **57.1% recall**, 1,080 µs latency). Global PageRank: **0.0% recall** (collapsed).
- **Step 14.4: Documentation Updates**:
  - Documented complete real-scale empirical tables, micro-scale comparisons, and mathematical scale-collapse proofs in `docs/benchmarks/external_evaluations.md` and `README.md`.

---

## Phase 15: Durable Architectural Blueprint & Repository Documentation [COMPLETED]

### Goal
Address the distinction between ephemeral task blueprints (`generate_blueprint`) and durable repository documentation. Provide an automated command that generates evergreen architectural documentation for the human team and agent harnesses.

### Implemented Architecture & Features
- **Step 15.1: Architectural Community & Subsystem Detection (`crates/engine/src/architecture.rs`)**:
  - Partitioned symbols into architectural communities based on package boundaries and directory hierarchy.
  - Computed inter-subsystem directed edge weights $\mathbf{M}[C_i, C_j]$ and Newman-Girvan modularity:
    $$Q = \sum_c \left[ \frac{e(c, c)}{m} - \frac{k_c^{\text{out}} k_c^{\text{in}}}{m^2} \right]$$
  - Implemented 4-tier architectural layering (`Presentation` Layer 1, `Domain` Layer 2, `Infrastructure` Layer 3, `Core` Layer 4) using topological net flow and semantic path priors.
  - Identified central architectural hubs combining in-degree centrality and uniform stationary PageRank distribution.
  - Extracted public API and interface catalog across all subsystems.
  - Generated valid Mermaid flowchart dependency diagrams nested in architectural layer subgraphs.
- **Step 15.2: CLI `repotrim architecture` Command (`crates/cli/src/commands/architecture.rs`)**:
  - Added CLI command: `repotrim architecture --output docs/ARCHITECTURE.md` (or `--output -` for stdout).
  - Automatically writes evergreen specification with tables, Mermaid diagrams, central hubs, and API inventories.
- **Step 15.3: MCP Protocol Integration (`generate_architecture_docs`) (`crates/mcp-server/src/handler.rs`)**:
  - Added 6th MCP tool: `generate_architecture_docs(path?: string, output?: string)`.
  - Enables instant architectural onboarding for AI coding assistants before touching source code.
- **Step 15.4: Test Suite & Documentation Verification**:
  - Added `crates/engine/tests/architecture_test.rs` testing modularity, community detection, and Mermaid rendering.
  - Added CLI test `test_cli_architecture` in `crates/cli/tests/cli_test.rs`.
  - Added MCP test in `crates/mcp-server/tests/mcp_test.rs` (validating 6 declared MCP tools).
  - Dogfooded by generating `docs/ARCHITECTURE.md` for `repotrim`.
  - Added formal citations in `README.md` and docstrings (Newman & Girvan 2004, Blondel et al. 2008, Ducasse & Pollet 2009).

---

## Phase 16: Live File Watcher & Incremental In-Memory Daemon [COMPLETED]

### Goal
Provide instantaneous (<0.1 ms) MCP query response times during active agent pair-programming by keeping an in-memory graph updated in real time as files are saved.

### Completed Implementation:
- **Step 16.1: Cross-Platform File System Watcher (`crates/engine/src/watcher.rs`)**:
  - Integrated `notify = "6.1"` cross-platform filesystem watcher backend (`RepositoryWatcher`).
  - Implemented configurable debouncing window (default 75ms) to coalesce rapid IDE editor write bursts.
  - Implemented recursive directory filtering (`is_ignored_path`) ignoring `.git`, `target`, `node_modules`, `.cargo`, `dist`, `build`, `.idea`, `.vscode`, `.gemini`, and `.repotrim`.
- **Step 16.2: Incremental In-Memory Single-File Patching (`crates/engine/src/loader.rs`)**:
  - Added `LoadedRepository::patch_file` and `LoadedRepository::remove_file` for sub-millisecond incremental re-parsing.
  - BLAKE3 content-hash check avoids unnecessary Tree-sitter re-parsing on metadata/timestamp touches.
  - Re-indexes symbols and edge tables, updates `LoadedRepository::cache`, and persists atomically to `.repotrim/cache.bin`.
- **Step 16.3: Daemon CLI Mode (`crates/cli/src/commands/watch.rs`)**:
  - Added `repotrim watch --path . --debounce 75` command.
  - Displays live console feedback (`⚡ Patched 'src/lib.rs'`, `🗑 Removed 'src/old.py'`, `✓ Graph synchronized`).
- **Step 16.4: Zero-Disk-Latency In-Memory MCP Synchronization (`crates/mcp-server/src/handler.rs`)**:
  - `McpHandler` automatically attaches a live background `RepositoryWatcher` upon first workspace query.
  - Subsequent tool calls query a warm, pre-synchronized in-memory graph with 0 ms disk read overhead.
- **Step 16.5: Comprehensive Integration Test Suite**:
  - Added `crates/engine/tests/watcher_test.rs` validating live file edits, polyglot additions, ignored path suppression, and removals.
  - Added CLI test `test_cli_watch_help` in `crates/cli/tests/cli_test.rs`.

---

## Phase 17: Go Language Engine & Semantic Hybrid Retrieval [COMPLETED]

### Goal
Expand language coverage to Go (cloud-native microservices, Kubernetes, systems code) and introduce zero-word-overlap semantic hybrid retrieval for conceptual `--query` matching without heavy runtime dependencies.

### Completed Implementation:
- **Step 17.1: Go Tree-sitter Grammar & AST Extractor (`parser.rs`, `queries/go.scm`)**:
  - Integrated `tree-sitter-go = "0.25"` grammar.
  - Authored declarative AST patterns in `crates/engine/queries/go.scm` extracting functions, receiver methods (`func (s *Server) Handle(...)`), structs, interfaces, type aliases, call expressions, and field type references.
  - Classifies receiver methods as `SymbolKind::Method` and dynamically links `EdgeKind::AstParent` edges to their parent receiver structs.
  - Extracts Go import declarations (`extract_go_imports`) handling single imports, parenthesized import blocks, named aliases, and wildcard/dot imports.
- **Step 17.2: Go Symbol, Edge, and Package Resolution (`import.rs`, `resolver.rs`)**:
  - `resolve_go_module`: Resolves Go module paths and package directory relative paths against known workspace files.
  - Enhanced `ScopedResolver` to support Go package namespaces across sibling files in the same directory (1.0 confidence).
  - Implemented intra-package sibling visibility (1.0 confidence) for unqualified cross-file calls within the same package directory.
- **Step 17.3: Go Formatter & Integration Test Suite (`formatter.rs`, `tests/go_test.rs`)**:
  - Added Go language tag inference (`language_tag_for_path`) and syntax rendering (omits semicolons for Go signatures).
  - Created `crates/engine/tests/go_test.rs` covering Go AST parsing, receiver methods, docstring extraction, multi-file Go package imports, and quad-polyglot integration (Rust + Python + TS + Go in a single graph).
- **Step 17.4: Semantic Hybrid Retrieval & Zero-Overlap Intent Ranking (`intent.rs`)**:
  - Implemented static domain concept ontology clusters (Auth/Security, DB/Storage, HTTP/Network, Pipeline/AST, Algorithms/Graph, Lifecycle/Concurrency, Config, Diagnostics).
  - Dense-sparse lexical-semantic expansion (Formal et al., 2021 SPLADE v2; Gao et al., 2021 COIL) solving vocabulary mismatch without heavy remote embedding models.
  - Zero-overlap conceptual queries (e.g. "credentials password authentication") retrieve relevant implementations (e.g. `verify_signature`, `authenticate_user`) with non-zero ranked confidence.
- **Step 17.5: Multi-Part Modular Commits & Verification**:
  - Verified with 88 passing tests across workspace, zero clippy warnings (`-D warnings`), and clean formatting.

---

## Phase 18: Model-Aware Auto-Budgeting & Knee-Curve Token Tuning [COMPLETED]

### Goal
Eliminate manual guess-work around token budgets by automatically detecting the optimal diminishing-returns threshold ("knee point") along the CELF knapsack cumulative marginal utility trajectory, tailored to specific LLM context profiles (Claude 3.5 Sonnet, GPT-4o, DeepSeek-V3, Local Ollama).

### Completed Implementation:
- **Step 18.1: Kneedle Knee Point Detection & CELF Tracing (`knee.rs`, `celf.rs`)**:
  - Implemented `KneedleDetector` based on Satopää et al. (2011) detecting maximum perpendicular distance $d_i = y'_i - x'_i$ on normalized discrete utility curves.
  - Added `CelfTraceStep` and `CelfOptimizer::optimize_with_trace` capturing cumulative tokens and utilities at every knapsack addition step.
  - Added unit tests `test_kneedle_*` and `test_celf_optimize_with_trace`.
- **Step 18.2: Model Profile Presets & Auto-Budgeting Pipeline (`model.rs`, `selector.rs`)**:
  - Defined `ModelProfile` with presets for `claude_3_5_sonnet`, `gpt_4o`, `deepseek_v3`, and `local_ollama` specifying token limits and curvature sensitivity $S$.
  - Added `AutoBudgetReport` tracking optimal budget, knee tokens, knee utility ratio, and symbol candidate counts.
  - Implemented `ContextSelector::select_context_auto`, `select_context_auto_weighted`, `select_and_format_context_auto`, and `select_and_format_context_auto_weighted`.
- **Step 18.3: CLI & MCP Integration (`select.rs`, `blueprint.rs`, `handler.rs`)**:
  - Updated CLI `repotrim select` and `repotrim blueprint` to accept `--budget auto` and `--model <name>`.
  - Exposed `model` parameter and `"auto"` budget support across MCP `trim_context` and `generate_blueprint` tools.
  - Added structured auto-budget diagnostic reports to JSON output and Markdown HTML comments.
  - Added comprehensive integration tests in `cli_test.rs` and `mcp_test.rs`.
- **Step 18.4: Documentation, Citations & Benchmark Maintenance**:
  - Cited Satopää et al. (2011) in `README.md` under `## References & Academic Citations`.
  - Updated `skills/repotrim/SKILL.md` with auto-budget heuristics.
  - Verified 93 passing workspace tests, clean formatting (`cargo fmt`), and zero clippy warnings.

## Phase 19: Container-Scoped Context Rendering [COMPLETED]

### Goal
Group member methods under their enclosing parent containers (`impl Struct { ... }`, `impl Trait for Struct { ... }`, `class Class:`, `class Class { ... }`) in the markdown context outline, disambiguating identically named methods across traits and providing clear ownership context to LLMs.

### Completed Implementation:
- **Step 19.1: Container and Trait Metadata Extraction (`symbol.rs`, `parser.rs`, `cache.rs`)**:
  - Added `container_name: Option<String>` and `trait_name: Option<String>` to `SymbolNode`.
  - Updated AST extraction across Rust, Python, TypeScript, and Go to populate enclosing container and trait names.
  - Bumped binary cache version to `CACHE_VERSION = 3`.
- **Step 19.2: Container-Scoped Outline Formatter (`formatter.rs`)**:
  - Grouped member methods by `(container_name, trait_name)` within each file.
  - Synthesized idiomatic container headers and footers for Rust (`impl T { ... }`, `impl Tr for T { ... }`), Python (`class C:`), and TypeScript (`class C { ... }`).
  - Rendered nested methods, line numbers, and docstrings with consistent 4-space indentation.
  - Preserved top-level receiver method formatting for Go (`func (s *Server) Handle(...)`).
  - Handled Python class definition deduplication when both class and member methods are selected.
- **Step 19.3: Comprehensive Integration Test Suite (`tests/container_scoping_test.rs`)**:
  - Tested Rust inherent and trait impl scoping with multi-method grouping.
  - Validated disambiguation of identically named methods across multiple traits.
  - Tested Python class method nesting and docstring alignment.
  - Tested TypeScript class scoping.
  - Verified Go receiver methods remain idiomatic top-level functions without synthetic blocks.
- **Step 19.4: Documentation and Verification**:
  - Documented container-scoped rendering in `README.md` and `docs/PHASE_PLAN.md`.
  - Maintained zero clippy warnings (`-D warnings`) and 100% test pass rate across all 98 workspace tests.

---

## Phase 20: AST Control-Flow Program Slicing & Causal Topological Ordering [COMPLETED]

### Goal
Replace placeholder dummy slicing with true Tree-sitter AST program slicing based on Mark Weiser (1981), preserving branches, loops, inter-procedural calls, and return expressions while eliding contiguous blocks of linear variable declarations into concise omission markers. Sequence multi-file context outlines in causal topological dependency order (Kahn's algorithm, 1962) so foundational data structures and callees precede caller orchestrators.

### Completed Implementation:
- **Step 20.1: AST Control-Flow Program Slicer (`crates/engine/src/slicer.rs`)**:
  - Implemented `AstSlicer` implementing control-flow skeletonization (Weiser, 1981).
  - Traversed Tree-sitter ASTs for Rust, Python, TypeScript, and Go.
  - Preserved branch conditions (`if`, `else`, `match`, `switch`), loops (`for`, `while`, `loop`), error propagation and exits (`return`, `throw`, `raise`, `panic!`, `?`), and invocations (`call_expression`, `method_invocation`, `macro_invocation`).
  - Preserved Python docstrings and Rust block tail return expressions.
  - Elided contiguous blocks of linear variable declarations and assignments into concise comments (`// ... [N lines elided] ...` or `# ... [N lines elided] ...`).
  - Handled short functions (<= 6 lines) and non-function symbols without modification.
- **Step 20.2: Formatter Integration & Topological File Ordering (`crates/engine/src/formatter.rs`)**:
  - Connected `AstSlicer::slice_symbol` into `LodLevel::SlicedBody`.
  - Implemented `sort_files_topologically` using Kahn's algorithm (1962).
  - Built directed dependency graph based on cross-file symbol usage in signatures and container blocks, filtering common language keywords and primitives.
  - Used `BTreeSet` priority queue to deterministically break ties alphabetically, with cycle fallback preserving canonical order.
- **Step 20.3: Comprehensive Integration Test Suite (`crates/engine/tests/program_slicing_test.rs`)**:
  - Added end-to-end integration tests for AST slicing across Rust, Python, TypeScript, and Go.
  - Validated causal topological file ordering where `models.rs` precedes `routes.rs` and `main.rs`.
- **Step 20.4: Academic Citations & Documentation**:
  - Added Weiser (1981) and Kahn (1962) citations in docstrings, `README.md`, and `docs/PHASE_PLAN.md`.
  - Maintained zero clippy warnings (`-D warnings`) and 100% test pass rate across all 103 workspace tests.

---

## Phase 21: Semantic Blast Radius & Change Impact Analysis Engine [COMPLETED]

### Goal
Implement Change Impact Analysis (CIA) based on foundational software engineering literature (Arnold & Bohner 1993; Ren et al. 2004 *Chianti*, ACM OOPSLA). Leverage transposed CSR sparse matrix traversals on the multiplex code property graph to quantify the semantic blast radius of code modifications, trace 1st-order and transitive downstream callers, identify affected regression test targets across Rust, Python, TypeScript, and Go, compute architectural risk metrics (`LOW`, `MEDIUM`, `HIGH`, `CRITICAL`), and expose dedicated CLI (`repotrim impact`) and MCP (`analyze_impact`) interfaces.

### Completed Implementation:
- **Step 21.1: Core Impact Engine (`crates/engine/src/impact.rs` & `crates/engine/src/graph.rs`)**:
  - Implemented `CsrMatrix::transpose()` and `MultiplexGraph::transpose_csr()` to invert directed dependency edges into incoming caller lookups in $O(1)$ time.
  - Implemented `ImpactAnalyzer` running breadth-first search on transposed CSR to locate 1st-order direct callers and transitive ripple effects up to configurable depths.
  - Implemented polyglot test target identification (`is_test_symbol`) recognizing Rust, Python, TypeScript, and Go test conventions.
  - Calculated normalized architectural risk scores in $[0.0, 1.0]$ and discrete levels (`LOW`, `MEDIUM`, `HIGH`, `CRITICAL`).
  - Implemented `to_markdown_context` generating executive summaries, recommended test candidate checklists, and multi-resolution LOD outlines.
- **Step 21.2: Dedicated CLI Command (`crates/cli/src/commands/impact.rs`)**:
  - Added `repotrim impact` subcommand with `--symbol <IDENT>`, `--diff`, `--diff-against <REV>`, `--budget`, and `--json` flags.
  - Added rich terminal visualization with colored risk badges, impact summary tables, recommended test suite candidate lists, and syntax-highlighted blast radius outlines.
- **Step 21.3: Model Context Protocol Tool (`crates/mcp-server/src/handler.rs`)**:
  - Added `analyze_impact` tool exposing change impact analysis to autonomous agent harnesses.
  - Supported query by symbol, unified diff string, or git revision baseline.
- **Step 21.4: Integration Tests & Academic Citations**:
  - Added end-to-end integration test suite in `crates/engine/tests/impact_test.rs`.
  - Added citations for Arnold & Bohner (1993) and Ren et al. (2004) in code docstrings and `README.md`.

---

## Phase 22: Algorithmic Guarantees, MSRV Pinning & Property Invariants [COMPLETED]

### Goal
Eliminate the theoretical gap in CELF knapsack selection by implementing the best-singleton correction (Khuller et al., 1999; Sviridenko, 2004) and threshold greedy (Badanidiyuru & Vondrák, 2014), guaranteeing a provable $\frac{1}{2}(1 - 1/e)$ approximation for submodular knapsack maximization. Pin and enforce MSRV to 1.90 across all crates, CI matrix, and documentation. Establish property-based invariant fuzzing (`proptest`) for submodularity, knapsack budget bounds, PPR probability mass conservation, and ACL error bounds.

### Completed Implementation:
- **Step 22.1: MSRV Pinning & CI Workflow (`Cargo.toml`, `ci.yml`, `README.md`)**:
  - Pinned `rust-version = "1.90"` in root `[workspace.package]` and inherited across `repotrim-engine`, `repotrim`, and `repotrim-mcp`.
  - Added explicit MSRV verification job in `.github/workflows/ci.yml` verifying compilation against Rust 1.90.0.
  - Updated README badges and documentation noting `rust-1.90+` requirements.
- **Step 22.2: Submodular Knapsack Best-Singleton Correction & Threshold Greedy (`crates/engine/src/celf.rs`)**:
  - Addressed the unbounded worst-case density-greedy gap under knapsack constraints (variable token costs).
  - Implemented the Khuller-Moss-Naor (1999) & Sviridenko (2004) best-singleton correction:
    $$S^* = \arg\max \left\{ f(S_{\text{greedy}}), \max_{v \in V: c(v) \le B} f(\{v\}) \right\}$$
    achieving a provable $\frac{1}{2}(1 - 1/e) \approx 0.316$ approximation guarantee in $O(|V| \log |V|)$ time with zero additional asymptotic overhead.
  - Implemented Badanidiyuru & Vondrák (2014) threshold-based greedy pass in `optimize_threshold_greedy` achieving $(1 - 1/e - \varepsilon)$ in $O\left(\frac{|V|}{\varepsilon} \log \frac{|V|}{\varepsilon}\right)$ time.
  - Added adversarial unit test `test_knapsack_adversarial_singleton_beats_greedy` verifying that single high-value items correctly supersede fragmented low-density items.
- **Step 22.3: Property-Based Invariant Fuzzing (`crates/engine/tests/property_invariants.rs`)**:
  - Integrated `proptest = "1.6"` testing mathematical invariants over arbitrary random graph topologies and budgets.
  - Property 1: Knapsack Budget Invariant ($\sum_{v \in S} c(v) \le B$).
  - Property 2: Monotone Submodular Diminishing Returns ($\Delta(x \mid A) \ge \Delta(x \mid B)$ for $A \subseteq B$).
  - Property 3: Best Singleton Knapsack Bound Guarantee ($f(S) \ge \max_{v: c(v) \le B} f(\{v\})$).
  - Property 4: PPR Probability Mass Conservation ($\sum p(v) \le 1.0 + 10^{-4}$ and $\forall v: p(v) \ge 0$).
  - Property 5: ACL Forward-Push Error Bound ($|\hat{p}(v) - p^*(v)| \le \frac{\varepsilon}{\alpha} \deg(v) + \text{tol}$).
- **Step 22.4: Literature Citations & Attributions**:
  - Added in-code docstring citations and README entries for Nemhauser et al. (1978), Khuller et al. (1999), Sviridenko (2004), and Badanidiyuru & Vondrák (2014).

---

## Phase 23: Exact Token Accounting & Empirical Cost Calibration [COMPLETED]

### Goal
Eliminate token budget inaccuracies arising from hardcoded character-to-token heuristics. Provide pluggable exact BPE subword tokenization alongside an empirically calibrated polyglot heuristic.

### Implemented Architecture & Steps
- **Step 23.1: Pluggable Tokenizer Engine (`crates/engine/src/tokens.rs`)**:
  - Added `TokenizerModel` enum supporting `FastHeuristic` (4.0 chars/token), `CalibratedHeuristic` (3.47 chars/token, $m = 0.2882$), `Cl100kBase` (GPT-4 / Claude-approximate BPE), and `O200kBase` (GPT-4o BPE).
  - Integrated `tiktoken-rs = "0.6"` under the `exact-tokens` cargo feature with lazy static singleton tokenizers.
  - Implemented exact per-symbol token caching and context formatting across `ContextSelector`.
- **Step 23.2: CLI & MCP Integration**:
  - Exposed `--tokenizer <fast|calibrated|exact|cl100k|o200k>` across `repotrim select`, `repotrim blueprint`, and `repotrim impact`.
  - Added `tokenizer` property to MCP `trim_context` schema with full backward compatibility.
- **Step 23.3: Polyglot Empirical Calibration Study**:
  - Evaluated 4,142 code symbols across Rust, Python, TypeScript, and Go in `crates/engine/tests/token_calibration_test.rs`.
  - Documented findings in `docs/benchmarks/token_calibration.md` ($r = 0.9908$, $R^2 = 0.9818$, optimal ratio $0.2882$ tokens/char).
  - Added academic citation for Sennrich et al. (ACL 2016).

---

## Phase 24: PPR Approximation Error Propagation & Sensitivity Diagnostics [COMPLETED]

### Goal
Provide rigorous theoretical error bounds for ACL forward-push diffusion and analyze how truncation residual errors propagate into the submodular knapsack decision boundary.

### Implemented Architecture & Steps
- **Step 24.1: Theoretical Error Bounds & Residual Vector Tracking (`crates/engine/src/ppr.rs`)**:
  - Extended `PprSolver` to compute and return `PprResult` containing stationary values $\mathbf{p}$, residual mass vector $\mathbf{r}$, max residual, and per-vertex theoretical error bounds $\delta(v) \le \frac{\max(\varepsilon, r_{\max})}{\alpha} \cdot \max(1.0, d_{\text{in}}(v))$ (Andersen, Chung & Lang, 2006).
- **Step 24.2: Knapsack Sensitivity & Stability Index (`crates/engine/src/celf.rs`)**:
  - Evaluated marginal density uncertainty intervals $[\Delta_{\min}(v)/c(v), \Delta_{\max}(v)/c(v)]$ using the monotone lower-bound property of forward-push residuals.
  - Formulated the micro-economic knapsack substitution test: detected borderline rival pairs where an unselected item could supersede a selected item under worst-case bounded error perturbation.
  - Computed the Knapsack Stability Index $S \in [0.0, 1.0]$ representing the proportion of unconditionally stable selections.
- **Step 24.3: CLI, MCP, and Empirical Evaluation Harness**:
  - Added `--diagnostics` / `--sensitivity` CLI flag to `repotrim select` and `diagnostics: boolean` to MCP `trim_context`.
  - Authored empirical validation study in `docs/benchmarks/sensitivity_analysis.md` across varying $\varepsilon$ sweeps ($10^{-2}$ to $10^{-6}$) and degree regimes.
  - Added integration test suite in `crates/engine/tests/sensitivity_test.rs`.

---

## Phase 25: Multiple-Choice Knapsack (MCKP) Joint Selection & LOD [COMPLETED]

### Goal
Eliminate the decoupled knapsack flaw where symbols are selected based on full body cost before LOD assignment, stranding token capacity and excluding high-value symbols with large bodies. Formulate joint selection and LOD assignment as a Multiple-Choice Knapsack Problem (MCKP) with submodular objective and upper convex hull pruning.

### Implemented Architecture & Steps
- **Step 25.1: Pareto Frontier & Upper Convex Hull Pruning (`crates/engine/src/celf.rs`)**:
  - Defined `LodOption` and `LodWeights` for discrete resolutions ($\text{SignatureOnly}$, $\text{SignatureAndDoc}$, $\text{SlicedBody}$, $\text{FullBody}$).
  - Implemented `build_pareto_frontier` pruning dominated points and interior non-convex points where $\text{slope}(A, B) \le \text{slope}(B, C)$ (Dyer, 1984; Zemel, 1980).
  - Implemented `CelfOptimizer::optimize_mckp` with lazy incremental CELF priority queue and Khuller et al. (1999) best-singleton knapsack correction.
- **Step 25.2: Selector Joint LOD Pipeline (`crates/engine/src/selector.rs`)**:
  - Added `select_and_format_context_joint_lod`, `select_and_format_context_weighted_joint_lod`, `select_and_format_context_auto_joint_lod`, and `select_and_format_context_auto_weighted_joint_lod`.
  - Linked Kneedle curvature detector to MCKP diagnostic trace curves for auto-budgeting.
- **Step 25.3: CLI, MCP, and Empirical Evaluation Harness**:
  - Added `--joint-lod` (alias `--mckp`) flag to `repotrim select` and `jointLod: boolean` to MCP `trim_context`.
  - Added integration test suites in `crates/engine/tests/mckp_test.rs`, `crates/cli/tests/cli_test.rs`, and `crates/mcp-server/tests/mcp_test.rs`.
  - Authored empirical evaluation benchmark in `docs/benchmarks/mckp_joint_selection.md` demonstrating +36.0% budget utilization improvement, -93.1% stranded tokens, and +150% symbol coverage in tight budgets.
  - Added academic citations for Kellerer et al. (2004), Dyer (1984), and Zemel (1980) in `README.md`.

---

## Phase 26: Git Co-Edit Mining & Principled Edge Weight Learning [COMPLETED]

### Goal
Complement static syntactic references with dynamic evolutionary software patterns. Mine historical commit changesets to detect logical couplings (files/symbols frequently modified together despite lacking direct syntactic references) and empirically learn multiplex `LayerWeights` via empirical Bayesian co-change likelihood and supervised ranking loss.

### Implemented Architecture & Steps
- **Step 26.1: Multiplex Graph Evolution (`crates/engine/src/symbol.rs`, `crates/engine/src/graph.rs`)**:
  - Added `EdgeKind::CoEdit` to multiplex CPG edge vocabulary.
  - Extended `LayerWeights` with `co_edit: f32` (default 0.50).
  - Implemented `MultiplexGraph::build_with_all` supporting both deterministic AST imports and evolutionary co-edit couplings.
- **Step 26.2: Git Commit Mining & Association Metrics (`crates/engine/src/coedit.rs`)**:
  - Extracted commit changesets using `git log -p --unified=0` with AST line span matching to identify enclosing symbols.
  - Implemented megacommit noise filtering ($>25$ files or $>40$ symbols ignored).
  - Applied exponential temporal half-life decay: $w_{\text{decay}} = 2^{-\Delta t / t_{\text{half}}}$ with default $t_{\text{half}} = 90$ days.
  - Computed directional association metrics: support $S(u, v)$, directional confidence $C(u \to v) = \frac{S(u, v)}{S(u)}$, and symmetric Jaccard index $J(u, v) = \frac{S(u, v)}{S(u) + S(v) - S(u, v)}$.
  - Keyed disk cache (`.repotrim/coedit.bin`) to Git HEAD commit hash for $<1\text{ms}$ warm hits with graceful non-git fallback.
- **Step 26.3: Empirical Bayesian Weight Learning (`crates/engine/src/weight_learning.rs`)**:
  - Implemented `EdgeWeightLearner` evaluating empirical co-change likelihood $P(\text{co-change} \mid \text{edge type } k)$.
  - Applied Bayesian prior shrinkage: $w_k = \frac{N_k}{N_k + \tau} \hat{w}_k + \frac{\tau}{N_k + \tau} w_{k, 0}$ ($\tau = 25$).
  - Implemented supervised ranking evaluation measuring Mean Reciprocal Rank (MRR) across historical commit validation queries.
- **Step 26.4: CLI, MCP, and Empirical Evaluation Harness**:
  - Implemented `repotrim coedit` command with rich colored terminal report and `--json` export.
  - Added `--coedit` and `--learn-weights` flags across `repotrim select` and `repotrim blueprint`.
  - Added `mine_coedits` tool and `useCoedits`, `learnWeights` properties to MCP `trim_context`.
  - Authored comprehensive empirical benchmark in `docs/benchmarks/git_coedit_learning.md` demonstrating **+112.8% MRR improvement** ($0.3172 \to 0.6750$).
  - Added academic citations for Zimmermann et al. (2005), Gall et al. (1998), Hassan (2008), Robbes et al. (2008), and Backstrom & Leskovec (2011).

---

## Phase 27: Multi-Resolution Community Detection [COMPLETED]

### Goal
Resolve the modularity resolution limit (Fortunato & Barthélemy, 2007) inherent in standard single-scale community clustering. Implement Reichardt-Bornholdt (2004) spin glass Potts modularity optimization across multi-scale hierarchical tiers (Macro $\gamma = 0.5$, Meso $\gamma = 1.0$, Micro $\gamma = 2.5$), analyze topological-to-filesystem architectural drift, and bias context selection toward cohesive topological communities.

### Implemented Architecture & Steps
- **Step 27.1: Multi-Resolution Potts Modularity Engine (`crates/engine/src/community.rs`)**:
  - Implemented `CommunityDetector` with multi-resolution Potts quality function:
    $$Q(\gamma) = \sum_{c} \left[ \frac{e(c, c)}{2m} - \gamma \left( \frac{k_c}{2m} \right)^2 \right]$$
  - Multi-pass Louvain algorithm with local moving $\Delta Q$ and super-node graph contraction preserving exact node degree invariants $\sum_{u \in C} k_u$.
  - Structured multi-scale hierarchy analyzer generating macro ($\gamma=0.5$), meso ($\gamma=1.0$), and micro ($\gamma=2.5$) tiers.
  - Symmetrized undirected CPG projection $A_{uv} = \max(W_{uv}, W_{vu})$.
- **Step 27.2: Architectural Drift Detection (`crates/engine/src/community.rs`)**:
  - Computed community dominant directories $D^*(C)$ and directory purity $\text{Purity}(C)$.
  - Diagnosed misplaced and leaky abstractions when symbol directory deviates from dominant directory with confidence $\ge 50\%$.
- **Step 27.3: Integration with Architecture Recovery & Context Selection (`architecture.rs`, `selector.rs`)**:
  - Added resolution parameter `--resolution` to `ArchitectureReport` and `repotrim architecture` CLI / MCP.
  - Implemented `ContextSelector::apply_community_boost` and `select_and_format_context_with_community` boosting seed-cohort symbols and eliminating orphaned utilities.
- **Step 27.4: CLI, MCP, and Empirical Evaluation**:
  - Implemented `repotrim community` CLI command supporting `--resolution`, `--hierarchy`, `--drift`, and `--json`.
  - Added `--community-boost` to `repotrim select`.
  - Added `detect_communities` tool to MCP server, plus `resolution` to `generate_architecture_docs` and `communityBoost` to `trim_context`.
  - Authored empirical evaluation benchmark in `docs/benchmarks/community_detection.md` showing +26.3% context cohesion and resolution limit mitigation.
  - Added academic citations for Reichardt & Bornholdt (2004), Fortunato & Barthélemy (2007), Arenas et al. (2008), Traag et al. (2019), and Strehl & Ghosh (2002) in `README.md`.

---

## Phase 28: Hybrid Lexical + Dense Semantic Query Retrieval & Intent Resolution `[COMPLETED]`

### Goal
Implement principled, zero-dependency information retrieval bridging human natural language intents and code symbol identifiers. Combine multi-field BM25+ with lower-bound term frequency normalization (Lv & Zhai 2011), 128-dimensional dense semantic feature hashing and software domain concept taxonomy embeddings (Weinberger et al. 2009; Bojanowski et al. 2017), scale-invariant Reciprocal Rank Fusion (Cormack et al. 2009), and Rocchio Pseudo-Relevance Feedback (Rocchio 1971).

### Implemented Architecture & Steps
- **Step 28.1: Polyglot Tokenization & Subword Hashing (`crates/engine/src/retrieval.rs`)**:
  - Implemented `PolyglotTokenizer` with polyglot identifier segmentation (CamelCase, PascalCase, snake_case, kebab-case, acronym handling like `HTTPClient`, alphanumeric transitions like `OAuth2`), stopword elimination, and 3..5 character n-gram extraction.
  - 24-axis canonical software domain concept taxonomy (`SOFTWARE_TAXONOMY`) mapping graph algorithms, optimization, tokenization, and architecture concepts into semantic clusters.
  - Implemented `DenseEmbedder` producing 128-dimensional $L_2$-normalized dense vectors (104 subword hash dims + 24 concept dims) with cosine similarity.
- **Step 28.2: Multi-Field BM25+ Lexical Retrieval (`crates/engine/src/retrieval.rs`)**:
  - Multi-field Robertson-Zaragoza IDF and length normalization across symbol names ($b=0.5, w=4.0$), signatures ($b=0.75, w=2.0$), file paths ($b=0.4, w=1.5$), and docstrings ($b=0.8, w=1.0$).
  - BM25+ term frequency lower bound $\delta = 0.5$ (Lv & Zhai 2011) guaranteeing non-zero term presence scores.
- **Step 28.3: Scale-Invariant Reciprocal Rank Fusion & Rocchio PRF (`crates/engine/src/retrieval.rs`)**:
  - Combined lexical and dense rankings using Reciprocal Rank Fusion ($k=60$) with exact declaration boosting ($2.5\times$) and non-test symbol prioritization.
  - Implemented `RocchioExpander` for blind relevance feedback, extracting top-IDF expansion terms from initial candidates.
  - Re-routed `IntentResolver::resolve_query` and `resolve_query_with_config` to `HybridRetriever`.
- **Step 28.4: CLI, MCP, and Empirical Evaluation**:
  - Implemented `repotrim query "<text>"` CLI command with `--limit`, `--mode hybrid|lexical|dense`, `--expand`, `--explain`, and `--json`.
  - Added `--retrieval-mode` and `--query-expand` to `repotrim select` and `repotrim blueprint`.
  - Added `search_symbols` tool to MCP server (expanding MCP catalog from 9 to 10 tools), and supported `retrievalMode` and `queryExpand` in `trim_context`.
  - Authored empirical evaluation benchmark in `docs/benchmarks/hybrid_retrieval.md` demonstrating +31.8% Recall@5 and sub-2ms retrieval latency.
  - Added academic citations for Lv & Zhai (2011), Robertson & Zaragoza (2009), Weinberger et al. (2009), Bojanowski et al. (2017), Cormack et al. (2009), and Rocchio (1971) in `README.md`.

---

## Phase 29: Rigorous Empirical Evaluation Harness & Aider Comparative Study `[COMPLETED]`

### Goal
Implement a mathematically principled, in-engine evaluation harness that quantitatively proves RepoTrim's context selection superiority over industry baselines, specifically Aider's Global PageRank repository map, Whole-File Dumps, and Naive Keyword/Grep Search. Measure token budget adherence, token reduction %, target direct (1st-order) and transitive (2nd-order) dependency recall, context precision, subgraph community cohesion, induced orphan symbol rate, and microsecond latency.

### Implemented Architecture & Steps
- **Step 29.1: In-Engine Evaluation Harness & Baseline Modeling (`crates/engine/src/eval.rs`)**:
  - Implemented `ContextStrategy` enum: `WholeFile`, `NaiveGrep`, `AiderRepoMap`, `RepoTrimVanilla`, and `RepoTrimFull`.
  - Implemented exact Aider Repo Map emulation: uniform Global PageRank ($d = 0.85$, uniform teleportation $\frac{1}{|V|} \mathbf{1}$) over untyped reference graphs, and greedy definition packing.
  - Implemented `BenchmarkRunner` with canonical scenarios (`ContextSelector`, `PprSolver`, `DiffResolver`, `Multi-Seed`, `Query Intent`).
  - Extracted 8 quantitative IR & graph metrics per run: Budget Adherence, Token Reduction %, Direct Recall ($k=1$), Transitive Recall ($k=2$), Context Precision %, Community Cohesion %, Induced Subgraph Orphan Rate %, and Latency.
  - Implemented `BenchmarkSummary::summarize()` computing strategy-grouped mean aggregates.
- **Step 29.2: CLI Evaluation Command (`crates/cli/src/commands/benchmark.rs`)**:
  - Added `repotrim benchmark` (alias `repotrim eval`) command with `--budget`, `--scenario`, `--strategies`, `--format table|markdown`, and `--json`.
  - Pretty ANSI terminal comparative tables and structured JSON export.
- **Step 29.3: Model Context Protocol Tool (`crates/mcp-server/src/handler.rs`)**:
  - Added `run_benchmark` tool (expanding catalog from 10 to 11 tools).
  - Supports automated empirical evaluations inside AI agent workflows with JSON or Markdown output.
- **Step 29.4: Empirical Comparative Study & Academic Attribution**:
  - Authored comprehensive empirical evaluation in `docs/benchmarks/aider_comparative_study.md`:
    - Proved mathematically why uniform Global PageRank suffers near-zero localized recall (0.3% vs. RepoTrim's 34.6%–52.9%).
    - Demonstrated that RepoTrim Full packs **3.05x more symbols** under the same budget via MCKP joint LOD selection (52 vs. 17 symbols).
    - Verified sub-5ms end-to-end evaluation latency across the entire repository.
  - Added academic citations for Aider AI (2023) and Manning et al. (2008) in `README.md`.

---

## Order of Execution & Milestones

```text
Milestone v0.3.0 (Precision & Real Scale):
   ├── Phase 13: Deterministic Import-Scoped Resolution [COMPLETED]
   └── Phase 14: Real-Tree Large-Scale Empirical Benchmarks [COMPLETED]

Milestone v0.4.0 (Durable Architecture & Real-Time Loop):
   ├── Phase 15: Durable Architecture Docs Generator [COMPLETED]
   └── Phase 16: Live File Watcher & In-Memory Daemon [COMPLETED]

Milestone v0.5.0 (Ecosystem Expansion):
   └── Phase 17: Go Language Support & Hybrid Semantic Retrieval [COMPLETED]

Milestone v0.6.0 (Mathematical Auto-Budgeting):
   └── Phase 18: Model-Aware Auto-Budgeting & Knee-Curve Token Tuning [COMPLETED]

Milestone v0.7.0 (Context Structure & LLM Usability):
   ├── Phase 19: Container-Scoped Context Rendering [COMPLETED]
   └── Phase 20: AST Program Slicing & Causal Topological Ordering [COMPLETED]

Milestone v0.8.0 (Safety & Change Impact Analysis):
   └── Phase 21: Semantic Blast Radius & Change Impact Analysis [COMPLETED]

Milestone v0.9.0 (Scientific Rigor & Theoretical Guarantees):
   ├── Phase 22: Algorithmic Guarantees, MSRV Pinning & Property Invariants [COMPLETED]
   ├── Phase 23: Exact Token Accounting & Cost Calibration [COMPLETED]
   ├── Phase 24: PPR Approximation Error Propagation & Sensitivity Diagnostics [COMPLETED]
   ├── Phase 25: Multiple-Choice Knapsack (MCKP) Joint Selection & LOD [COMPLETED]
   ├── Phase 26: Git Co-Edit Mining & Principled Edge Weight Learning [COMPLETED]
   ├── Phase 27: Multi-Resolution Community Detection [COMPLETED]
   ├── Phase 28: Hybrid Lexical + Dense Semantic Query Retrieval [COMPLETED]
   └── Phase 29: Rigorous Empirical Evaluation Harness & Aider Comparative Study [COMPLETED]
```




