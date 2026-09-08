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
- **80% – 87% Token Reduction** relative to whole-file dumping.
- **60% – 78% Direct Dependency Recall** under strict token constraints (300–500 tokens).
- **Sub-Millisecond Latency (<250 µs release)** via sparse Andersen-Chung-Lang forward-push local diffusion.

---

## Empirical Benchmark Performance

Evaluated head-to-head on identical queries across Rust, Python, and TypeScript codebases (detailed methodology in [Comparative Study](docs/benchmarks/comparative_study.md) and [Polyglot External Evaluations](docs/benchmarks/external_evaluations.md)):

| Language / Framework | Target Seed | Context Strategy | Tokens Generated | Token Reduction | Direct Dep Recall | Latency |
| :--- | :--- | :--- | :---: | :---: | :---: | :---: |
| **Python (FastAPI)** | `login` | Whole-File Dump | 294 | 0.0% | **100.0%** | 32 µs |
| | | Naive Grep | 5 | 98.3% | 0.0% | **8 µs** |
| | | Global PageRank | 93 | 68.4% | **100.0%** | 134 µs |
| | | **RepoTrim (Ours)** | **86** | **70.7%** | **100.0%** | 294 µs |
| **TypeScript (React)** | `UserProfileCard` | Whole-File Dump | 243 | 0.0% | **100.0%** | 23 µs |
| | | Naive Grep | 7 | 97.1% | 0.0% | **1 µs** |
| | | Global PageRank | 93 | 61.7% | **100.0%** | 14 µs |
| | | **RepoTrim (Ours)** | **93** | **61.7%** | **100.0%** | 148 µs |
| **Rust (Engine)** | `ContextSelector` | Whole-File Dump | 3,077 | 0.0% | **100.0%** | 266 µs |
| | | Naive Grep | 26 | 99.2% | 0.0% | **40 µs** |
| | | Global PageRank | 497 | 83.8% | 0.0% | 353 µs |
| | | **RepoTrim (Ours)** | **491** | **84.0%** | **42.9%** | 828 µs |

*Self-indexing dogfooding metrics (31 files, 206 symbols, 628 edges): Cold ingestion: 118 ms; Warm ingestion (BLAKE3 cache): **6.4 ms (18.4x faster)**; MCP repeat query: **<0.5 ms**. See [Dogfood Report](docs/benchmarks/dogfood.md) and [External Evaluations](docs/benchmarks/external_evaluations.md).*

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

---

## License

Dual-licensed under either of:
- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT License ([LICENSE-MIT](LICENSE-MIT))

at your option.
