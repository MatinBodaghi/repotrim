# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

---

## [Unreleased]

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
