# RepoTrim

<p align="center">
  <strong>Mathematically optimal codebase context trimmer for AI coding agents.</strong>
</p>

<p align="center">
  <a href="https://github.com/matinbodaghi/repotrim/actions/workflows/ci.yml"><img src="https://github.com/matinbodaghi/repotrim/actions/workflows/ci.yml/badge.svg" alt="CI Status" /></a>
  <a href="LICENSE-MIT"><img src="https://img.shields.io/badge/license-MIT%20%2F%20Apache--2.0-blue.svg" alt="License" /></a>
  <a href="https://www.rust-lang.org"><img src="https://img.shields.io/badge/rust-1.80%2B-orange.svg" alt="Rust Version" /></a>
  <a href="https://crates.io/crates/repotrim"><img src="https://img.shields.io/badge/crates.io-v0.2.0-red.svg" alt="crates.io" /></a>
  <a href="https://modelcontextprotocol.io"><img src="https://img.shields.io/badge/MCP-2024--11--05-green.svg" alt="MCP Compatible" /></a>
</p>

---

## What is RepoTrim?

**RepoTrim** replaces brute-force whole-file dumping and naive vector/grep retrieval with a deterministic **Multiplex Code Property Graph**, **Andersen-Chung-Lang Personalized PageRank (PPR)**, and **CELF Submodular Knapsack Packing**.

Built specifically for AI coding assistants and autonomous harnesses (**Claude Code**, **Cursor**, **Windsurf**, **Antigravity**, **OpenCode**, **SWE-bench** runners), RepoTrim extracts the mathematically optimal context skeleton within strict token budgets—maximizing dependency awareness while preventing context bloat and "lost-in-the-middle" LLM reasoning degradation.

```text
┌─────────────────────────────────┐       Tree-sitter AST       ┌─────────────────────────────────┐
│ Codebase (Rust, Python, TS/JS)  │ ─────────────────────────>  │  Multiplex Code Property Graph  │
└─────────────────────────────────┘                             │  (AST, Call, Type, Import)      │
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
  Native Tree-sitter AST parsing and symbol extraction for **Rust** (`.rs`), **Python** (`.py`), and **TypeScript / JavaScript** (`.ts`, `.tsx`, `.js`, `.jsx`, `.mjs`, `.cjs`) into a unified cross-language multiplex code graph.
- **Multiplex Code Property Graph ($\mathcal{M} = (V, \{E_k\}, \mathbf{W})$)**:
  Extracts granular syntax entities (functions, methods, structs, classes, interfaces, traits, modules) indexed by compact 32-bit `SymbolId`s with layered tensor weights:
  - $E_{\text{AST}}$: Lexical containment (module $\to$ struct/class $\to$ method).
  - $E_{\text{Call}}$: Explicit invocation call-graph edges.
  - $E_{\text{Type}}$: Type dependencies (inputs, return values, field types, class inheritance).
  - $E_{\text{Import}}$: Module paths and use/import statements.

- **Bayesian Scoped Disambiguation**:
  Resolves call and type targets across files without requiring a slow compiler daemon, weighting lexical proximity, scope hierarchy, and receiver hints.
- **Personalized PageRank (ACL Forward-Push)**:
  Computes query-focused relevance vectors $\boldsymbol{\pi}_q$ with $O(1/\epsilon)$ local running time independent of total repository size.
- **CELF Submodular Knapsack Optimization**:
  Cost-Effective Lazy Forward queue solves the budget-constrained facility location problem:
  $$\max_{S \subseteq V} \sum_{v \in S} \pi_q(v) \quad \text{subject to} \quad \sum_{v \in S} c(v) \le B$$
- **Multi-Resolution Level of Detail (LOD)**:
  Dynamically assigns high detail ($\text{LOD}_2$ slices or $\text{LOD}_3$ full implementations) to query seeds, and concise signatures ($\text{LOD}_0$ / $\text{LOD}_1$) to contextual dependencies.
- **Incremental Merkle Caching**:
  BLAKE3 content-addressed file hashing with bincode persistence. Re-indexes only modified files in <7 ms.
- **Native Model Context Protocol (MCP)**:
  Implements JSON-RPC 2.0 over `stdio` for plug-and-play integration with Claude Desktop, Cursor, and agent harnesses.

---

## Quickstart & Installation

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

# Natural language intent query (automatic BM25 / trigram seed discovery)
repotrim select --query "jwt token verification" --budget 1500

# Git diff context packing (maps changes directly to enclosing AST symbols)
repotrim select --from-diff --budget 2000

# Multiple seeds with custom path
repotrim select --seed PprSolver --seed CsrMatrix --budget 750 --path ./my-project

# Output structured JSON instead of Markdown
repotrim select --seed RepositoryCache --budget 400 --format json
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
| **`trim_context`** | `seeds?: string[]`, `query?: string`, `fromDiff?: boolean`, `budget: number`, `format?: string` | Computes optimal Markdown or JSON context skeleton with seed, query, or diff inference |
| **`generate_blueprint`** | `task: string`, `budget?: number`, `path?: string` | Generates a structured feature blueprint with auto-inferred seed anchors |
| **`query_graph_stats`** | `path?: string` | Retrieves syntax breakdown, edge density, and top PageRank hubs |
| **`inspect_symbol`** | `symbol: string`, `path?: string` | Deeply inspects definitions, token costs, dependencies, and callers |
| **`clean_cache`** | `path?: string` | Clears on-disk cache and resets in-memory daemon state |

---

## Agent Harness Integration

RepoTrim is designed natively for autonomous agent loops (SWE-bench, Antigravity, Claude Code). In-depth architectural guides and turn-key skills are available:

- **[Standardized Agent Skill](skills/repotrim/SKILL.md)**: Turn-key Antigravity / Claude Code skill implementing the 3-Tier Context Funnel and budget heuristics.
- **[Harness Overview](docs/harness/OVERVIEW.md)**: Integrating RepoTrim between codebases and LLM reasoning loops.
- **[Feature Blueprint Template](docs/harness/FEATURE_BLUEPRINT_TEMPLATE.md)**: High-density specification pattern for zero-hallucination agent prompting.
- **[Token Optimization Guide](docs/harness/TOKEN_OPTIMIZATION.md)**: 3-tier context funnel and dynamic budget allocation strategies.
- **[Local-to-Server Guide](docs/harness/LOCAL_TO_SERVER.md)**: Deploying RepoTrim on remote Linux servers, cloud VMs, and Docker containers.

---

## Workspace Layout

```text
repotrim/
├── Cargo.toml                  # Workspace manifest & package metadata
├── LICENSE-MIT                 # MIT License
├── LICENSE-APACHE              # Apache 2.0 License
├── .github/workflows/ci.yml    # Multi-platform CI (Ubuntu, Windows, macOS)
├── skills/
│   └── repotrim/
│       └── SKILL.md            # Turn-key Antigravity / Claude Code agent skill
├── docs/
│   ├── benchmarks/             # Comparative study, polyglot & dogfood benchmarks
│   │   ├── comparative_study.md
│   │   ├── external_evaluations.md
│   │   └── dogfood.md
│   └── harness/                # Agent harness guides, blueprint templates & server docs
│       ├── OVERVIEW.md
│       ├── FEATURE_BLUEPRINT_TEMPLATE.md
│       ├── LOCAL_TO_SERVER.md
│       └── TOKEN_OPTIMIZATION.md
└── crates/
    ├── engine/                 # repotrim-engine: AST parsing, CSR, PPR, CELF, Cache
    │   ├── Cargo.toml
    │   ├── queries/            # Declarative Tree-sitter query files (Rust, Python, TS)
    │   └── src/
    │       ├── lib.rs
    │       ├── cache.rs        # Incremental BLAKE3 Merkle cache and bincode persistence
    │       ├── diff.rs         # Unified git diff parser and symbol mapping
    │       ├── error.rs        # Engine error types
    │       ├── intent.rs       # BM25 + trigram natural language query resolver
    │       ├── parser.rs       # Polyglot Tree-sitter AST symbol extractor
    │       ├── selector.rs     # CELF knapsack context selector
    │       ├── symbol.rs       # Dense SymbolId, SymbolNode, ReferenceEdge
    │       └── tokens.rs       # In-engine allocation-free BPE token estimator
    ├── cli/                    # repotrim: CLI binary (select, blueprint, stats, inspect, clean, mcp)
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

## References & Academic Citations

RepoTrim's mathematical architecture builds on foundational algorithms and literature in graph theory, submodular optimization, and program analysis:

1. **Personalized PageRank Local Diffusion (ACL Forward-Push):**
   - Reid Andersen, Fan Chung, Kevin Lang. *"Local Graph Partitioning using PageRank Vectors"*. In *Foundations of Computer Science (FOCS)*, 2006. [DOI: 10.1109/FOCS.2006.44](https://doi.org/10.1109/FOCS.2006.44).
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

---

## License

Dual-licensed under either of:
- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT License ([LICENSE-MIT](LICENSE-MIT))

at your option.
