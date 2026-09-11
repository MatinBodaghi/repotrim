# RepoTrim

<p align="center">
  <strong>Mathematically optimal codebase context trimmer for AI coding agents.</strong>
</p>

<p align="center">
  <a href="https://github.com/matinbodaghi/repotrim/actions/workflows/ci.yml"><img src="https://github.com/matinbodaghi/repotrim/actions/workflows/ci.yml/badge.svg" alt="CI Status" /></a>
  <a href="LICENSE-MIT"><img src="https://img.shields.io/badge/license-MIT%20%2F%20Apache--2.0-blue.svg" alt="License" /></a>
  <a href="https://www.rust-lang.org"><img src="https://img.shields.io/badge/rust-1.90%2B-orange.svg" alt="Rust Version" /></a>
  <a href="https://crates.io/crates/repotrim"><img src="https://img.shields.io/badge/crates.io-v0.5.0-red.svg" alt="crates.io" /></a>
  <a href="https://modelcontextprotocol.io"><img src="https://img.shields.io/badge/MCP-2024--11--05-green.svg" alt="MCP Compatible" /></a>
</p>

---

## What is RepoTrim?

**RepoTrim** replaces brute-force whole-file dumping and naive vector/grep retrieval with a deterministic **Multiplex Code Property Graph**, **Andersen-Chung-Lang Personalized PageRank (PPR)**, and **CELF Submodular Knapsack Packing**.

Built specifically for AI coding assistants and autonomous harnesses (**Claude Code**, **Cursor**, **Windsurf**, **Antigravity**, **OpenCode**, **SWE-bench** runners), RepoTrim extracts the mathematically optimal context skeleton within strict token budgets—maximizing dependency awareness while preventing context bloat and "lost-in-the-middle" LLM reasoning degradation.

```text
┌──────────────────────────────────────┐       Tree-sitter AST       ┌─────────────────────────────────┐
│ Codebase (Rust, Python, TS/JS, Go)   │ ─────────────────────────>  │  Multiplex Code Property Graph  │
└──────────────────────────────────────┘                             │  (AST, Call, Type, Import)      │
                                                                     └─────────────────────────────────┘
                                                                                 │
┌─────────────────────────────────┐   Personalized PageRank (PPR)                ▼
│ Query / Seed Symbols            │ ─────────────────────────>  ┌─────────────────────────────────┐
└─────────────────────────────────┘     O(1/ε) Forward-Push     │  Local Relevance Vector π_q     │
                                                                └─────────────────────────────────┘
                                                                                 │
┌─────────────────────────────────┐   Submodular Knapsack Solver                 ▼
│ Token Budget (e.g. 500)         │ ─────────────────────────>  ┌─────────────────────────────────┐
└─────────────────────────────────┘      CELF Lazy Forward      │  Multi-Resolution LOD Skeleton  │
                                                                │  (Signatures, Slices & Bodies)  │
                                                                └─────────────────────────────────┘
```

---

## Why RepoTrim? (The Context Window Dilemma)

When constructing prompts for large codebases, AI coding agents face two catastrophic failure modes:

1. **Context Bloat (Whole-File Dumps):**
   Dumping complete files consumes thousands of tokens on irrelevant helper functions, unit tests, and boilerplate. This triggers quadratic LLM attention costs, latency spikes, and degraded reasoning ("lost in the middle").
2. **Context Starvation (Naive Grep / BM25 Search):**
   Keyword search extracts only isolated lines matching identifiers, completely stripping struct fields, type signatures, and downstream callers (**0% dependency recall**). The model hallucinates APIs and produces broken code.
3. **Global Bias (Unweighted PageRank / Static Repo Maps):**
   Tools like Aider compute static global centrality, allocating prompt budget to project-wide root hubs (e.g. `Error`, `fmt`, `Id`) rather than the local dependencies surrounding your specific task.

**RepoTrim solves all three:**
- **74% – 99.8% Token Reduction** relative to whole-file dumping.
- **57% – 100% Direct Dependency Recall** under strict token constraints (300–1,000 tokens).
- **Sub-Millisecond Latency (<450 µs)** via sparse Andersen-Chung-Lang forward-push local diffusion.
- **Scale Invariance**: Unlike Global PageRank which collapses to 0% recall on 50+ file codebases, RepoTrim's personalized teleportation isolates relevant subgraphs regardless of repository scale.

---

## Empirical Benchmark Performance

Evaluated head-to-head across Rust, Python, and TypeScript codebases at both enterprise scale and micro scale (detailed methodology and mathematical proofs in [Polyglot External Evaluations](docs/benchmarks/external_evaluations.md)):

### Enterprise Scale (50+ Files, 2,000+ Symbols)

| Language / Framework | Target Seed | Context Strategy | Tokens Generated | Token Reduction | Direct Dep Recall | Latency |
| :--- | :--- | :--- | :---: | :---: | :---: | :---: |
| **Python (Enterprise)**<br>50+ files, 1,731 syms | `process_refund` | Whole-File Dump | 11,895 | 0.0% | **100.0%** | 1,101 µs |
| | | Naive Grep | 17 | 99.9% | 0.0% | **281 µs** |
| | | Global PageRank (Aider) | 996 | 91.6% | 0.0% *(Collapsed)* | 1,724 µs |
| | | **RepoTrim (Ours)** | **33** | **99.7%** | **100.0%** | **430 µs** |
| **TypeScript (Fullstack)**<br>50+ files, 1,852 syms | `CheckoutModal` | Whole-File Dump | 8,151 | 0.0% | **100.0%** | 715 µs |
| | | Naive Grep | 671 | 91.8% | 100.0% *(Noise)* | **265 µs** |
| | | Global PageRank (Aider) | 999 | 87.7% | 0.0% *(Collapsed)* | 1,469 µs |
| | | **RepoTrim (Ours)** | **20** | **99.8%** | **100.0%** | **360 µs** |
| **Rust (Engine)**<br>50+ files, multi-crate | `ContextSelector` | Whole-File Dump | 3,077 | 0.0% | **100.0%** | **254 µs** |
| | | Naive Grep | 19 | 99.4% | 0.0% | 38 µs |
| | | Global PageRank (Aider) | 800 | 74.0% | 0.0% *(Collapsed)* | 350 µs |
| | | **RepoTrim (Ours)** | **797** | **74.1%** | **57.1%** | 1,080 µs |

> [!IMPORTANT]
> **Why Global PageRank Collapses at Scale:** With uniform teleportation $\mathbf{v} = [1/N, \dots, 1/N]^T$, high in-degree universal utilities (`Logger`, `Config`, `Error`) accumulate $>80\%$ of stationary probability mass on 50+ file codebases. Within a 1,000-token budget, Global PageRank fills the entire prompt with root hubs, yielding **0.0% recall** on local task dependencies. RepoTrim's Personalized PageRank uses a Dirac restart vector $\mathbf{v} = \mathbf{e}_{\text{seed}}$, diffusing mass strictly across import-scoped dependencies.

*Incremental Indexing: Cold ingestion of 50+ files: **567 ms**; Warm cache re-indexing (BLAKE3): **18 ms (<1 ms/file)**. See [External Evaluations](docs/benchmarks/external_evaluations.md) and [Dogfood Report](docs/benchmarks/dogfood.md).*

---

## Key Features

- **Multi-Language Polyglot Support**:
  Native Tree-sitter AST parsing and symbol extraction for **Rust** (`.rs`), **Python** (`.py`), **TypeScript / JavaScript** (`.ts`, `.tsx`, `.js`, `.jsx`, `.mjs`, `.cjs`), and **Go** (`.go`) into a unified cross-language multiplex code graph.
- **Semantic Hybrid Intent Retrieval (Dense-Sparse Expansion)**:
  Zero-word-overlap semantic concept ontology expansion combining smoothed BM25, character 3-gram fuzzy similarity, and domain concept clusters (Formal et al., 2021; Gao et al., 2021). Accurately resolves conceptual queries (e.g. "credentials password authentication") to relevant implementations without requiring heavy remote embedding daemons.
- **Multiplex Code Property Graph ($\mathcal{M} = (V, \{E_k\}, \mathbf{W})$)**:
  Extracts granular syntax entities (functions, methods, structs, classes, interfaces, traits, modules) indexed by compact 32-bit `SymbolId`s with layered tensor weights:
  - $E_{\text{AST}}$: Lexical containment (module $\to$ struct/class $\to$ method).
  - $E_{\text{Call}}$: Explicit invocation call-graph edges.
  - $E_{\text{Type}}$: Type dependencies (inputs, return values, field types, class inheritance).
  - $E_{\text{Import}}$: Module paths and use/import statements.
  - $E_{\text{CoEdit}}$: Mined historical commit co-changes and logical couplings.

- **Git Commit Co-Edit Mining & Principled Edge Weight Learning**:
  Discovers cross-cutting logical couplings from historical Git changesets using association rule mining (Zimmermann et al., 2005; Gall et al., 1998) with temporal half-life decay ($2^{-\Delta t / t_{\text{half}}}$) and megacommit noise filtering (Hassan, 2008). Automatically calibrates multiplex layer weights $\mathbf{w}^* = [w_{\text{ast}}, w_{\text{call}}, w_{\text{type}}, w_{\text{import}}, w_{\text{coedit}}]$ from empirical co-change likelihoods and supervised ranking loss (Backstrom & Leskovec, 2011), boosting context retrieval accuracy by **+112.8% MRR** (`repotrim coedit`, `--coedit`, `--learn-weights`).

- **Bayesian Scoped Disambiguation**:
  Resolves call and type targets across files without requiring a slow compiler daemon, weighting lexical proximity, scope hierarchy, and receiver hints.
- **Personalized PageRank (ACL Forward-Push)**:
  Computes query-focused relevance vectors $\boldsymbol{\pi}_q$ with $O(1/\epsilon)$ local running time independent of total repository size.
- **CELF Submodular Knapsack Optimization (with Best-Singleton Correction)**:
  Cost-Effective Lazy Forward queue solves the budget-constrained knapsack problem:
  $$\max_{S \subseteq V} \sum_{v \in S} \pi_q(v) \quad \text{subject to} \quad \sum_{v \in S} c(v) \le B$$
  Combines density-ordered lazy forward evaluation with the Khuller et al. (1999) / Sviridenko (2004) best-singleton correction $\max(S_{\text{greedy}}, \{v^*\})$, achieving a provable $\frac{1}{2}(1 - 1/e)$ approximation guarantee for general knapsack constraints, with an optional $(1 - 1/e - \varepsilon)$ threshold greedy pass (Badanidiyuru & Vondrák, 2014).
- **Multi-Resolution Level of Detail (LOD) & AST Program Slicing**:
  Dynamically assigns high detail ($\text{LOD}_2$ control-flow skeleton slices or $\text{LOD}_3$ full implementations) to query seeds, and concise signatures ($\text{LOD}_0$ / $\text{LOD}_1$) to contextual dependencies. Leverages Weiser (1981) AST program slicing across Rust, Python, TypeScript, and Go to elide contiguous blocks of linear variable assignments into concise comments (`// ... [N lines elided] ...`) while preserving branch conditions, loop structures, error exits, and inter-procedural call sites.
- **Causal Topological Dependency Ordering**:
  Sequences multi-file Markdown context outlines via Kahn's algorithm (1962) in causal dependency order (callees and foundational data models precede caller orchestrators) rather than naive alphabetical ordering, optimizing autoregressive LLM attention bias.
- **Container-Scoped Context Rendering**:
  Outlines nest member methods inside their enclosing parent containers (`impl Struct { ... }`, `impl Trait for Struct { ... }`, `class Class:`, `class Class { ... }`) with 4-space indentation and exact source line comments. Disambiguates identically named methods across traits and provides explicit type-ownership context for LLMs.
- **Incremental Merkle Caching**:
  BLAKE3 content-addressed file hashing with bincode persistence. Re-indexes only modified files in <7 ms.
- **Live File Watcher & In-Memory Daemon**:
  Cross-platform background watcher (`notify`) with configurable debouncing (75ms). Automatically synchronizes in-memory ASTs and graph topology on save with sub-millisecond incremental patching and 0 ms MCP query latency.
- **Native Model Context Protocol (MCP)**:
  Implements JSON-RPC 2.0 over `stdio` for plug-and-play integration with Claude Desktop, Cursor, and agent harnesses.

---

## Quickstart & Installation

> [!NOTE]
> **Toolchain Requirement**: RepoTrim requires **Rust 1.90.0 or higher** (due to `edition2024` Tree-sitter AST parser dependencies and Cargo lockfile format v4).

### Option 1: Install via Cargo

```bash
cargo install repotrim
```

### Option 2: Build from Source

```bash
git clone https://github.com/matinbodaghi/repotrim.git
cd repotrim
cargo build --release --workspace

# Release binaries:
# target/release/repotrim       (CLI & unified MCP server)
# target/release/repotrim-mcp   (Standalone dedicated MCP server)
```

---

## CLI Usage

### 1. Select Optimal Context (`select`)

Extract the mathematically optimal context skeleton within a token budget:

```bash
# Trim context seeded around a key symbol within a 500-token budget
repotrim select --seed ContextSelector --budget 500

# Mathematically optimal auto-budgeting via Knee-Curve (diminishing returns threshold)
repotrim select --seed ContextSelector --budget auto

# Auto-budget targeted for specific LLM architecture profile
repotrim select --query "jwt token verification" --budget auto --model gpt-4o

# Natural language intent query (automatic BM25 / trigram seed discovery)
repotrim select --query "jwt token verification" --budget 1500

# Git diff context packing (maps changes directly to enclosing AST symbols)
repotrim select --from-diff --budget 2000

# Multiple seeds with custom path
repotrim select --seed PprSolver --seed CsrMatrix --budget 750 --path ./my-project

# Output structured JSON instead of Markdown
repotrim select --seed RepositoryCache --budget 400 --format json

# Exact OpenAI/Claude BPE token accounting (cl100k_base)
repotrim select --seed ContextSelector --budget 1000 --tokenizer exact

# Exact GPT-4o BPE token accounting (o200k_base)
repotrim select --seed ContextSelector --budget 1000 --tokenizer o200k

# Numerical stability & knapsack sensitivity diagnostics
repotrim select --seed ContextSelector --budget 1000 --diagnostics

# Multiple-Choice Knapsack (MCKP) joint symbol selection and Level-of-Detail optimization
repotrim select --seed ContextSelector --budget 600 --joint-lod
```

### 2. Generate Feature Blueprint (`blueprint`)

Generate an agent-ready high-density feature specification with automatically inferred seed anchors and target files:

```bash
# Print blueprint to stdout
repotrim blueprint "implement rate limiter middleware" --budget 3000

# Write to file
repotrim blueprint "fix session expiration bug" --output FEATURE_BLUEPRINT.md
```

### 3. View Repository Graph Statistics (`stats`)

Analyze symbol inventory, edge density, and top architectural PageRank hubs:

```bash
repotrim stats
```

### 4. Deep Symbol Inspection (`inspect`)

Examine a symbol's declaration signature, token cost, outgoing dependencies, and incoming callers:

```bash
repotrim inspect --symbol ContextSelector
```

### 5. Cache Management (`clean`)

Purge incremental `.repotrim/` cache directory:

```bash
repotrim clean
```

### 6. Architectural Specification & Diagrams (`architecture`)

Generate durable, evergreen repository architecture documentation with automated subsystem clustering, 4-tier layering, central hubs, and Mermaid diagrams:

```bash
# Generate architecture specification to docs/ARCHITECTURE.md
repotrim architecture --output docs/ARCHITECTURE.md

# Print architecture specification directly to stdout
repotrim architecture --output -
```

### 7. Live File Watcher Daemon (`watch`)

Run a background daemon that monitors filesystem changes, incrementally reparses modified files in <2 ms, and keeps the in-memory graph and `.repotrim/cache.bin` continuously synchronized:

```bash
# Watch current repository with default 75ms debouncing
repotrim watch

# Watch specific path with custom debounce window
repotrim watch --path ./my-project --debounce 50
```

### 8. Semantic Blast Radius & Change Impact Analysis (`impact`)

Quantify the semantic architectural blast radius of modified symbols or code diffs, identifying affected downstream callers and recommending targeted test suites:

```bash
# Analyze impact for a specific symbol identifier
repotrim impact --symbol ContextSelector

# Analyze blast radius of unstaged / staged git changes
repotrim impact --diff

# Analyze blast radius against a target git revision or branch
repotrim impact --diff-against main --budget 3000
```

### 9. Git Co-Edit Mining & Layer Weight Learning (`coedit`)

Mine commit history for fine-grained symbol co-evolution patterns, fuse logical coupling edges into the multiplex graph, and calibrate empirical Bayesian layer weights:

```bash
# Mine repository commit history and display learned multiplex layer weights
repotrim coedit

# Filter by minimum support and output JSON
repotrim coedit --min-support 3 --format json

# Use mined co-edits and learned weights in context selection
repotrim select --query "rate limit" --coedit --learn-weights --budget 1200
```

### 10. Multi-Resolution Community Detection (`community`)

Detect topological communities via multi-resolution Potts modularity optimization ($\gamma$), unpack hierarchical scales, and diagnose architectural drift:

```bash
# Detect communities at standard modularity resolution (gamma = 1.0)
repotrim community

# Multi-scale hierarchical decomposition (Macro gamma=0.5, Meso gamma=1.0, Micro gamma=2.5)
repotrim community --hierarchy

# Detect architectural drift (symbols deviating from dominant directory)
repotrim community --drift

# Tune modularity resolution in architecture specification
repotrim architecture --resolution 0.75 --output docs/ARCHITECTURE.md

# Context selection boosted by seed community cohesion
repotrim select --symbol ContextSelector --community-boost --budget 800
```

### 11. Hybrid Lexical + Dense Semantic Symbol Query (`query`)

Search and retrieve workspace symbols using multi-field BM25+ lexical scoring, zero-dependency subword feature hashing & concept taxonomy dense vectors, Reciprocal Rank Fusion (RRF), and Rocchio Pseudo-Relevance Feedback:

```bash
# Hybrid retrieval across workspace symbols with default RRF scoring
repotrim query "PPR solver"

# Lexical-only retrieval (field-weighted BM25+)
repotrim query "ContextSelector" --mode lexical

# Dense semantic retrieval with Rocchio PRF query expansion
repotrim query "graph random walk" --mode dense --expand

# Output structured JSON results
repotrim query "token estimation" --json --limit 5

# Display detailed score breakdowns (BM25+, Dense Cosine, RRF, matched terms)
repotrim query "knapsack" --explain
```

### 12. Empirical Benchmark Suite & Comparative Study (`benchmark` / `eval`)

Execute the built-in empirical evaluation harness to compare context selection strategies (Whole-File Dump, Naive Keyword/Grep, Aider Repo Map with Global PageRank, RepoTrim Vanilla, and RepoTrim Full) across quantitative IR and graph metrics:

```bash
# Run full empirical benchmark suite with ANSI table output
repotrim benchmark

# Run benchmark formatted as Markdown
repotrim benchmark --format markdown

# Evaluate a single scenario with custom token budget
repotrim eval --scenario context_selector --budget 600

# Compare specific strategies in JSON format
repotrim eval --strategies aider,full --json
```

---

## Model Context Protocol (MCP) Integration

RepoTrim natively implements the **Model Context Protocol (MCP)** (`2024-11-05`), exposing sub-millisecond context tools directly to AI assistants.

### Claude Desktop Configuration

Add to your `claude_desktop_config.json`:

```json
{
  "mcpServers": {
    "repotrim": {
      "command": "repotrim",
      "args": ["mcp", "--path", "/path/to/your/project"]
    }
  }
}
```

### Cursor Configuration

Add to `.cursor/mcp.json`:

```json
{
  "mcpServers": {
    "repotrim": {
      "command": "repotrim",
      "args": ["mcp", "--path", "."]
    }
  }
}
```

### Antigravity Configuration

Add to `.agents/mcp_config.json` (workspace-level) or `~/.gemini/config/mcp_config.json` (global-level):

```json
{
  "mcpServers": {
    "repotrim": {
      "command": "repotrim",
      "args": ["mcp", "--path", "."]
    }
  }
}
```

### Exposed MCP Tools


| Tool | Parameters | Description |
| :--- | :--- | :--- |
| **`trim_context`** | `seeds?: string[]`, `query?: string`, `fromDiff?: boolean`, `budget?: number \| "auto"`, `model?: string`, `tokenizer?: string`, `format?: string`, `diagnostics?: boolean`, `jointLod?: boolean`, `useCoedits?: boolean`, `learnWeights?: boolean`, `communityBoost?: boolean`, `retrievalMode?: string`, `queryExpand?: boolean` | Computes optimal Markdown or JSON context skeleton with seed, query, or diff inference, supporting auto-budgeting, exact tokenization, sensitivity diagnostics, joint MCKP LOD optimization, Git co-edit fusion, community boosting, and hybrid BM25+/dense retrieval |
| **`search_symbols`** | `query: string`, `limit?: number`, `mode?: string`, `expand?: boolean`, `format?: string`, `path?: string` | Performs hybrid lexical (BM25+) and dense semantic (subword feature hashing) symbol retrieval across the repository |
| **`detect_communities`** | `path?: string`, `resolution?: number`, `hierarchy?: boolean`, `drift?: boolean`, `format?: string` | Detects multi-resolution Potts communities across Macro ($\gamma=0.5$), Meso ($\gamma=1.0$), and Micro ($\gamma=2.5$) tiers, spots architectural drift, and outputs topological catalogs |
| **`mine_coedits`** | `path?: string`, `limit?: number`, `minSupport?: number` | Mines fine-grained git commit history for co-edit patterns, computes association confidence, and returns top logical coupling pairs |
| **`analyze_impact`** | `symbol?: string`, `diff?: string`, `diffAgainst?: string`, `budget?: number \| "auto"`, `model?: string`, `path?: string` | Computes architectural blast radius, 1st-order callers, transitive dependents, and recommended regression tests |
| **`generate_blueprint`** | `task: string`, `budget?: number \| "auto"`, `model?: string`, `path?: string` | Generates a structured feature blueprint with auto-inferred seed anchors and target files |
| **`generate_architecture_docs`** | `path?: string`, `output?: string`, `resolution?: number` | Generates durable repository architecture docs, subsystem topology, layers, and Mermaid diagrams |
| **`query_graph_stats`** | `path?: string` | Retrieves syntax breakdown, edge density, and top PageRank hubs |
| **`inspect_symbol`** | `symbol: string`, `path?: string` | Deeply inspects definitions, token costs, dependencies, and callers |
| **`clean_cache`** | `path?: string` | Clears on-disk cache and resets in-memory daemon state |
| **`run_benchmark`** | `scenario?: string`, `budget?: number`, `strategies?: string[]`, `format?: string`, `path?: string` | Executes empirical evaluation harness and Aider comparative benchmark across quantitative IR and graph metrics |

---

## Documentation & Architecture

Comprehensive guides, specifications, benchmarks, and roadmap plans are available in the repository:

- **[Architecture & Subsystems](docs/ARCHITECTURE.md)**: Automated 4-tier architectural specification, modularity analysis, and Mermaid diagrams.
- **[Phase Execution History](docs/PHASE_PLAN.md)**: Master engineering plan and retrospective documentation for completed phases 1 through 30.
- **[Standardized Agent Skill](skills/repotrim/SKILL.md)**: Turn-key Antigravity / Claude Code skill implementing the 3-Tier Context Funnel and budget heuristics.
- **[Harness Overview](docs/harness/OVERVIEW.md)**: Integrating RepoTrim between codebases and LLM reasoning loops.
- **[Feature Blueprint Template](docs/harness/FEATURE_BLUEPRINT_TEMPLATE.md)**: High-density specification pattern for zero-hallucination agent prompting.
- **[Token Optimization Guide](docs/harness/TOKEN_OPTIMIZATION.md)**: 3-tier context funnel and dynamic budget allocation strategies.
- **[Local-to-Server Guide](docs/harness/LOCAL_TO_SERVER.md)**: Deploying RepoTrim on remote Linux servers, cloud VMs, and Docker containers.
- **[Contributing Guide](CONTRIBUTING.md)**: Development workflows, coding standards, branch conventions, and citation guidelines.

---

## Workspace Layout

```text
repotrim/
├── Cargo.toml                  # Workspace manifest & package metadata
├── CONTRIBUTING.md             # Contribution guide, code standards & commit rules
├── LICENSE-MIT                 # MIT License
├── LICENSE-APACHE              # Apache 2.0 License
├── .github/workflows/ci.yml    # Multi-platform CI (Ubuntu, Windows, macOS)
├── skills/
│   └── repotrim/
│       └── SKILL.md            # Turn-key Antigravity / Claude Code agent skill
├── docs/
│   ├── ARCHITECTURE.md         # Generated 4-tier subsystem architecture spec
│   ├── PHASE_PLAN.md           # Master roadmap & completed phase retrospectives
│   ├── benchmarks/             # Comparative study, polyglot & dogfood benchmarks
│   │   ├── aider_comparative_study.md
│   │   ├── comparative_study.md
│   │   ├── external_evaluations.md
│   │   ├── dogfood.md
│   │   ├── token_calibration.md
│   │   ├── sensitivity_analysis.md
│   │   ├── mckp_joint_selection.md
│   │   ├── git_coedit_learning.md
│   │   ├── community_detection.md
│   │   └── hybrid_retrieval.md
│   └── harness/                # Agent harness guides, blueprint templates & server docs
│       ├── OVERVIEW.md
│       ├── FEATURE_BLUEPRINT_TEMPLATE.md
│       ├── LOCAL_TO_SERVER.md
│       └── TOKEN_OPTIMIZATION.md
└── crates/
    ├── engine/                 # repotrim-engine: AST parsing, CSR, PPR, CELF, Cache
    │   ├── Cargo.toml
    │   ├── queries/            # Declarative Tree-sitter query files (Rust, Python, TS, Go)
    │   └── src/
    │       ├── lib.rs
    │       ├── cache.rs        # Incremental BLAKE3 Merkle cache and bincode persistence
    │       ├── coedit.rs       # Git co-edit commit mining, noise filtering, half-life decay
    │       ├── community.rs    # Multi-resolution Potts modularity, Louvain hierarchy & drift
    │       ├── diff.rs         # Unified git diff parser and symbol mapping
    │       ├── error.rs        # Engine error types
    │       ├── intent.rs       # BM25 + trigram + semantic hybrid query resolver
    │       ├── parser.rs       # Polyglot Tree-sitter AST symbol extractor
    │       ├── retrieval.rs    # Hybrid BM25+, dense subword feature hashing, RRF & PRF
    │       ├── selector.rs     # CELF knapsack context selector
    │       ├── slicer.rs       # AST control-flow program slicer (Weiser 1981)
    │       ├── symbol.rs       # Dense SymbolId, SymbolNode, ReferenceEdge
    │       ├── tokens.rs       # In-engine allocation-free BPE token estimator
    │       └── weight_learning.rs # Bayesian multiplex edge weight learning & ranking calibration
    ├── cli/                    # repotrim: CLI binary (select, query, blueprint, stats, inspect, clean, coedit, community, mcp)
    └── mcp-server/             # repotrim-mcp: stdio JSON-RPC MCP server
```

---

## Development & Testing

```bash
# Run unit, integration, and baseline comparative benchmark tests
cargo test --workspace --all-targets

# Run live baseline comparative benchmark suite
cargo test -p repotrim-engine --test benchmark_baselines -- --nocapture

# Run linter
cargo clippy --workspace --all-targets -- -D warnings

# Format code
cargo fmt --all -- --check
```

---

## Contributing

Contributions from the community are warmly welcome! Whether you are optimizing graph algorithms, adding tree-sitter language grammars, enhancing MCP tools, or improving documentation, please review our **[Contributing Guide](CONTRIBUTING.md)** for toolchain setup, our 3-tier branch workflow (`research/*`, `feature/*`, `experiment/*`), strict commit style conventions, and pre-commit verification steps.

---

## References & Academic Citations

RepoTrim's mathematical architecture builds on foundational algorithms and literature in graph theory, submodular optimization, and program analysis:

1. **Personalized PageRank Local Diffusion (ACL Forward-Push) & Approximation Error Bounds:**
   - Reid Andersen, Fan Chung, Kevin Lang. *"Local Graph Partitioning using PageRank Vectors"*. In *Foundations of Computer Science (FOCS)*, 2006. [DOI: 10.1109/FOCS.2006.44](https://doi.org/10.1109/FOCS.2006.44).
   - Empirical validation & sensitivity diagnostics: [`docs/benchmarks/sensitivity_analysis.md`](docs/benchmarks/sensitivity_analysis.md).
2. **Submodular Knapsack Optimization (CELF):**
   - Jure Leskovec, Andreas Krause, Carlos Guestrin, Christos Faloutsos, Jeanne VanBriesen, Natalie Glance. *"Cost-effective Outbreak Detection in Networks"*. In *ACM SIGKDD International Conference on Knowledge Discovery and Data Mining (KDD)*, 2007. [DOI: 10.1145/1281192.1281239](https://doi.org/10.1145/1281192.1281239).
3. **Code Property Graph (CPG):**
   - Fabian Yamaguchi, Nico Golde, Daniel Arp, Konrad Rieck. *"Modeling and Discovering Vulnerabilities with Code Property Graphs"*. In *IEEE Symposium on Security and Privacy (S&P)*, 2014. [DOI: 10.1109/SP.2014.44](https://doi.org/10.1109/SP.2014.44).
4. **Incremental Abstract Syntax Tree Parsing (Tree-sitter):**
   - Max Brunsfeld et al. *"Tree-sitter: An incremental parsing system for programming tools"*. [tree-sitter.github.io](https://tree-sitter.github.io).
5. **Fast Content-Addressed Tree Hashing (BLAKE3):**
   - Jack O'Connor, Jean-Philippe Aumasson, Samuel Neves, Zooko Wilcox-O'Hearn. *"BLAKE3: One function, fast everywhere"*, 2020. [github.com/BLAKE3-team/BLAKE3-specs](https://github.com/BLAKE3-team/BLAKE3-specs).
6. **Model Context Protocol (MCP):**
   - Anthropic. *"Model Context Protocol Specification"*, 2024. [modelcontextprotocol.io](https://modelcontextprotocol.io).
7. **Graph Modularity & Community Detection:**
   - Mark E. J. Newman, Michelle Girvan. *"Finding and evaluating community structure in networks"*. In *Physical Review E*, 69(2), 026113, 2004. [DOI: 10.1103/PhysRevE.69.026113](https://doi.org/10.1103/PhysRevE.69.026113).
   - Vincent D. Blondel, Jean-Loup Guillaume, Renaud Lambiotte, Etienne Lefebvre. *"Fast unfolding of communities in large networks"*. In *Journal of Statistical Mechanics: Theory and Experiment*, P10008, 2008. [DOI: 10.1088/1742-5468/2008/10/P10008](https://doi.org/10.1088/1742-5468/2008/10/P10008).
8. **Software Architecture Reconstruction:**
   - Stéphane Ducasse, Damien Pollet. *"Software Architecture Reconstruction: A Process-Oriented Taxonomy"*. In *IEEE Transactions on Software Engineering*, 35(4), 573–591, 2009. [DOI: 10.1109/TSE.2009.19](https://doi.org/10.1109/TSE.2009.19).
9. **Dense-Sparse Hybrid Retrieval & Lexical Expansion:**
   - Thibault Formal, Carlos Lassance, Benjamin Piwowarski, Stéphane Clinchant. *"SPLADE v2: Sparse Lexical and Expansion Model for Information Retrieval"*, 2021. [arXiv:2109.10086](https://arxiv.org/abs/2109.10086).
   - Luan Gao, Zhuyun Dai, Jamie Callan. *"COIL: Efficient Dense-Sparse Hybrid Retrieval"*. In *ACM SIGIR Conference on Research and Development in Information Retrieval*, 2021. [arXiv:2104.07186](https://arxiv.org/abs/2104.07186).
10. **Knee Point Detection in Discrete Curvature (Kneedle):**
    - Ville Satopää, Jeannie Albrecht, David Irwin, Barath Raghavan. *"Finding a 'Kneedle' in a Haystack: Detecting Knee Points in System Behavior"*. In *31st International Conference on Distributed Computing Systems Workshops (ICDCSW)*, 2011, pp. 166–171. [DOI: 10.1109/ICDCSW.2011.20](https://doi.org/10.1109/ICDCSW.2011.20).
11. **Program Slicing & Control-Flow Skeletonization:**
    - Mark Weiser. *"Program Slicing"*. In *Proceedings of the 5th International Conference on Software Engineering (ICSE '81)*, pp. 439–449, 1981; *IEEE Transactions on Software Engineering*, SE-10(4): 352–357, 1984. [DOI: 10.1109/TSE.1984.5010248](https://doi.org/10.1109/TSE.1984.5010248).
12. **Topological Ordering (Kahn's Algorithm):**
    - Arthur B. Kahn. *"Topological sorting of large networks"*. In *Communications of the ACM*, 5(11): 558–562, 1962. [DOI: 10.1145/368996.369025](https://doi.org/10.1145/368996.369025).
13. **Software Change Impact Analysis & Test Selection:**
    - Robert S. Arnold, Shawn A. Bohner. *"Software Change Impact Analysis"*. IEEE Computer Society Press, Los Alamitos, CA, 1993. [ISBN: 0-8186-2775-8](https://ieeexplore.ieee.org/document/27758).
    - Xiaoxia Ren, Fenil Shah, Frank Tip, Barbara G. Ryder, Ophelia Chesley. *"Chianti: A Tool for Change Impact Analysis of Java Programs"*. In *Proceedings of the 19th ACM SIGPLAN Conference on Object-Oriented Programming, Systems, Languages, and Applications (OOPSLA '04)*, 2004, pp. 432–448. [DOI: 10.1145/1028976.1029012](https://doi.org/10.1145/1028976.1029012).
14. **Monotone Submodular Maximization under Cardinality Constraints:**
    - George L. Nemhauser, Laurence A. Wolsey, Marshall L. Fisher. *"An analysis of approximations for maximizing submodular set functions—I"*. In *Mathematical Programming*, 14(1): 265–294, 1978. [DOI: 10.1007/BF01588971](https://doi.org/10.1007/BF01588971).
15. **Submodular Knapsack Maximization & Best-Singleton Correction:**
    - Samir Khuller, Anna Moss, Joseph (Seffi) Naor. *"The budgeted maximum coverage problem"*. In *Information Processing Letters*, 70(1): 39–45, 1999. [DOI: 10.1016/S0020-0190(99)00031-9](https://doi.org/10.1016/S0020-0190(99)00031-9).
    - Maxim Sviridenko. *"A note on maximizing a submodular set function subject to a knapsack constraint"*. In *Operations Research Letters*, 32(1): 41–45, 2004. [DOI: 10.1016/S0167-6377(03)00062-2](https://doi.org/10.1016/S0167-6377(03)00062-2).
16. **Fast Threshold-Based Submodular Maximization:**
    - Ashwinkumar Badanidiyuru, Jan Vondrák. *"Fast algorithms for maximizing submodular functions"*. In *Proceedings of the 25th Annual ACM-SIAM Symposium on Discrete Algorithms (SODA '14)*, pp. 1497–1514, 2014. [DOI: 10.1137/1.9781611973402.110](https://doi.org/10.1137/1.9781611973402.110).
17. **Byte-Pair Encoding (BPE) Subword Tokenization & Empirical Calibration:**
    - Rico Sennrich, Barry Haddow, Alexandra Birch. *"Neural Machine Translation of Rare Words with Subword Units"*. In *Proceedings of the 54th Annual Meeting of the Association for Computational Linguistics (ACL 2016)*, pp. 1715–1725. [DOI: 10.18653/v1/P16-1162](https://doi.org/10.18653/v1/P16-1162).
    - Polyglot calibration study: [`docs/benchmarks/token_calibration.md`](docs/benchmarks/token_calibration.md).
18. **Multiple-Choice Knapsack Problem (MCKP) & Convex Hull Pruning:**
    - Hans Kellerer, Ulrich Pferschy, David Pisinger. *"Knapsack Problems"*. Springer Berlin, Heidelberg, 2004. [DOI: 10.1007/978-3-540-24777-7](https://doi.org/10.1007/978-3-540-24777-7). Chapter 11: "The Multiple-Choice Knapsack Problem".
    - Martin E. Dyer. *"An $O(n)$ algorithm for the multiple-choice knapsack linear program"*. In *Mathematical Programming*, 29(1): 58–63, 1984. [DOI: 10.1007/BF02591602](https://doi.org/10.1007/BF02591602).
    - Eitan Zemel. *"The linear multiple-choice knapsack problem"*. In *Operations Research*, 28(6): 1412–1419, 1980. [DOI: 10.1287/opre.28.6.1412](https://doi.org/10.1287/opre.28.6.1412).
    - Empirical evaluation: [`docs/benchmarks/mckp_joint_selection.md`](docs/benchmarks/mckp_joint_selection.md).
19. **Mining Software Repositories for Logical Couplings & Evolutionary Association:**
    - Thomas Zimmermann, Peter Weißgerber, Stephan Diehl, Andreas Zeller. *"Mining Version Histories to Guide Software Changes"*. In *IEEE Transactions on Software Engineering*, 31(6): 429–445, 2005. [DOI: 10.1109/TSE.2005.72](https://doi.org/10.1109/TSE.2005.72).
    - Harald Gall, Karin Hajek, Mehdi Jazayeri. *"Detection of logical coupling based on change sets"*. In *Proceedings of the 1998 International Conference on Software Maintenance (ICSM '98)*, pp. 159–168, 1998. [DOI: 10.1109/ICSM.1998.738499](https://doi.org/10.1109/ICSM.1998.738499).
    - Ahmed E. Hassan. *"The road ahead for Mining Software Repositories (MSR)"*. In *Frontiers of Software Maintenance (FoSM 2008)*, pp. 48–57, 2008. [DOI: 10.1109/FOSM.2008.4659248](https://doi.org/10.1109/FOSM.2008.4659248).
    - Romain Robbes, David Röthlisberger, Michele Lanza. *"Refining logical couplings with fine-grained software change history"*. In *Proceedings of the 2008 IEEE International Conference on Software Maintenance*, pp. 416–425, 2008. [DOI: 10.1109/ICSM.2008.4658090](https://doi.org/10.1109/ICSM.2008.4658090).
    - Empirical evaluation: [`docs/benchmarks/git_coedit_learning.md`](docs/benchmarks/git_coedit_learning.md).
20. **Supervised Random Walks & Edge Weight Learning in Information Networks:**
    - Lars Backstrom, Jure Leskovec. *"Supervised Random Walks: Predicting and Recommending Links in Social Networks"*. In *Proceedings of the 4th ACM International Conference on Web Search and Data Mining (WSDM '11)*, pp. 1–10, 2011. [DOI: 10.1145/1935826.1935832](https://doi.org/10.1145/1935826.1935832).
21. **Multi-Resolution Modularity, Spin Glass Potts Models, & Resolution Limits:**
    - Jörg Reichardt, Stefan Bornholdt. *"Detecting Fuzzy Community Structures in Complex Networks with a Potts Model"*. In *Physical Review Letters*, 93(21): 218701, 2004. [DOI: 10.1103/PhysRevLett.93.218701](https://doi.org/10.1103/PhysRevLett.93.218701).
    - Santo Fortunato, Marc Barthélemy. *"Resolution limit in modularity detection"*. In *Proceedings of the National Academy of Sciences (PNAS)*, 104(1): 35–41, 2007. [DOI: 10.1073/pnas.0605965104](https://doi.org/10.1073/pnas.0605965104).
    - Alex Arenas, Alberto Fernández, Sergio Gómez. *"Analysis of the multiscale, knotty-centre and segregated properties of complex networks"*. In *New Journal of Physics*, 10(5): 053039, 2008. [DOI: 10.1088/1367-2630/10/5/053039](https://doi.org/10.1088/1367-2630/10/5/053039).
    - Vincent A. Traag, Ludo Waltman, Nees Jan van Eck. *"From Louvain to Leiden: guaranteeing well-connected communities"*. In *Scientific Reports*, 9(1): 5233, 2019. [DOI: 10.1038/s41598-019-41695-z](https://doi.org/10.1038/s41598-019-41695-z).
    - Alexander Strehl, Joydeep Ghosh. *"Cluster Ensembles — A Knowledge Reuse Framework for Combining Multiple Partitions"*. In *Journal of Machine Learning Research (JMLR)*, 3: 583–617, 2002. [JMLR](https://jmlr.org/papers/v3/strehl02a.html).
    - Empirical evaluation: [`docs/benchmarks/community_detection.md`](docs/benchmarks/community_detection.md).
22. **Hybrid Lexical-Dense Information Retrieval, BM25+, Feature Hashing, & RRF:**
    - Yuanhua Lv, ChengXiang Zhai. *"Lower-Bounding Term Frequency Normalization"*. In *Proceedings of the 20th ACM International Conference on Information and Knowledge Management (CIKM '11)*, pp. 7–16, 2011. [DOI: 10.1145/2063576.2063584](https://doi.org/10.1145/2063576.2063584).
    - Stephen E. Robertson, Hugo Zaragoza. *"The Probabilistic Relevance Framework: BM25 and Beyond"*. In *Foundations and Trends in Information Retrieval*, 3(4): 333–389, 2009. [DOI: 10.1561/1500000019](https://doi.org/10.1561/1500000019).
    - Kilian Weinberger, Anirban Dasgupta, John Langford, Alex Smola, Josh Attenberg. *"Feature Hashing for Large Scale Multitask Learning"*. In *Proceedings of the 26th International Conference on Machine Learning (ICML '09)*, pp. 1113–1120, 2009. [DOI: 10.1145/1553374.1553516](https://doi.org/10.1145/1553374.1553516).
    - Piotr Bojanowski, Edouard Grave, Armand Joulin, Tomas Mikolov. *"Enriching Word Vectors with Subword Information"*. In *Transactions of the Association for Computational Linguistics (TACL)*, 5: 135–146, 2017. [DOI: 10.1162/tacl_a_00051](https://doi.org/10.1162/tacl_a_00051).
    - Gordon V. Cormack, Charles L. A. Clarke, Stefan Buettcher. *"Reciprocal Rank Fusion Outperforms Condorcet and Individual Rank Learning Methods"*. In *Proceedings of the 32nd International ACM SIGIR Conference on Research and Development in Information Retrieval (SIGIR '09)*, pp. 758–759, 2009. [DOI: 10.1145/1571941.1572114](https://doi.org/10.1145/1571941.1572114).
    - J. J. Rocchio. *"Relevance feedback in information retrieval"*. In *The SMART Retrieval System — Experiments in Automatic Document Processing*, Prentice-Hall, pp. 313–323, 1971.
    - Empirical evaluation: [`docs/benchmarks/hybrid_retrieval.md`](docs/benchmarks/hybrid_retrieval.md).
23. **Empirical Code Context Evaluation & Repository Map Architectures:**
    - Aider AI. *"Repository Map: Global PageRank over Code Tags"*, 2023. [aider.chat/docs/repomap.html](https://aider.chat/docs/repomap.html).
    - Christopher D. Manning, Prabhakar Raghavan, Hinrich Schütze. *"Introduction to Information Retrieval"*. Cambridge University Press, 2008. Chapters 8 & 21 (Evaluation in Information Retrieval). [DOI: 10.1017/CBO9780511809071](https://doi.org/10.1017/CBO9780511809071).
    - Empirical evaluation & comparative study: [`docs/benchmarks/aider_comparative_study.md`](docs/benchmarks/aider_comparative_study.md).

---

## License

Dual-licensed under either of:
- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT License ([LICENSE-MIT](LICENSE-MIT))

at your option.
