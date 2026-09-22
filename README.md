# RepoTrim

[![CI](https://github.com/matinbodaghi/repotrim/actions/workflows/ci.yml/badge.svg)](https://github.com/matinbodaghi/repotrim/actions/workflows/ci.yml)
[![Security Audit](https://github.com/matinbodaghi/repotrim/actions/workflows/security.yml/badge.svg)](https://github.com/matinbodaghi/repotrim/actions/workflows/security.yml)
[![Crates.io](https://img.shields.io/crates/v/repotrim.svg)](https://crates.io/crates/repotrim)
[![Docs.rs](https://docs.rs/repotrim/badge.svg)](https://docs.rs/repotrim)
[![License](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](LICENSE-MIT)
[![Rust MSRV](https://img.shields.io/badge/rustc-1.90%2B-orange.svg)](rust-toolchain.toml)

> **Mathematically optimal codebase context trimmer for AI coding agents under token budgets.**

RepoTrim constructs a multi-layer Code Property Graph (CPG) across multi-language repositories, performs sparse Personalized PageRank diffusion, and solves submodular knapsack optimization to extract dense, self-contained context skeletons for LLMs and autonomous agents.

---

## The Problem: The High Cost of Whole-File Context

When AI coding agents (Claude Code, Cursor, Windsurf, Antigravity, SWE-bench runners) inspect codebases using raw file-system tools (`list_dir`, `grep_search`, `read_file`), they encounter two fundamental failure modes:

1. **Token Expenditure Inflation:** Inspecting 3 to 6 whole source files consumes **15,000 to 45,000 prompt tokens per task episode**.
2. **Context Degradation ("Lost-in-the-Middle"):** Dumps of irrelevant methods, boilerplate, and peripheral files pollute the model's self-attention context ([Liu et al., 2024](https://doi.org/10.1162/tacl_a_00638)), increasing hallucination and task failures.

### The RepoTrim Solution

```text
Target Codebase (Rust, Python, TS/JS, Go)
   │
   ▼
[Tree-Sitter AST & Symbol Parsing]
   │
   ▼
[Multiplex CSR Code Property Graph]
   │
   ▼
[Approximate Personalized PageRank (ACL Diffusion)]
   │
   ▼
[Budgeted Submodular Knapsack Optimizer (CELF / MCKP)]
   │
   ▼
Mathematically Optimal Context Skeleton (Within Exact Token Budget B)
```

RepoTrim cuts token expenditure by **60% to 80%** while preserving **75% to 100%** of change-critical symbols, boosting agent information density by **10x to 33x**.

---

## 60-Second Quickstart

### 1. Installation

#### Via Cargo
```bash
cargo install repotrim
```

#### Pre-Compiled Binaries
Download pre-built release binaries for Linux (`x86_64`, `aarch64`), macOS (`x86_64`, Apple Silicon `aarch64`), and Windows (`x86_64`) from [GitHub Releases](https://github.com/matinbodaghi/repotrim/releases). See the [Installation Guide](docs/INSTALL.md) for full instructions.

### 2. Basic CLI Usage

```bash
# 1. Trim context around a symbol under a strict 500-token budget
repotrim select --seed ContextSelector --budget 500

# 2. Trim context using natural language search (automatic BM25 / trigram seed discovery)
repotrim select --query "jwt token verification" --budget 1200

# 3. Trim context from unstaged git diff mutations
repotrim select --from-diff --budget 2000

# 4. Generate an agent-ready feature implementation blueprint
repotrim blueprint "implement rate limiter middleware" --budget 2500

# 5. Deeply inspect a symbol, its caller hierarchy, and signature
repotrim inspect --symbol ContextSelector

# 6. Generate evergreen architecture documentation with Mermaid diagrams
repotrim architecture --output docs/ARCHITECTURE.md
```

---

## Model Context Protocol (MCP) Integration

RepoTrim ships with a high-performance, sandboxed [Model Context Protocol](https://modelcontextprotocol.io/) server exposing 8 consolidated tools with lightweight handshake payloads (< 700 BPE tokens).

### 1. Claude Desktop

Add to your `claude_desktop_config.json`:

```json
{
  "mcpServers": {
    "repotrim": {
      "command": "repotrim-mcp",
      "args": ["--allow-root", "/absolute/path/to/your/project"]
    }
  }
}
```

### 2. Cursor IDE

Add to your project's `.cursor/mcp.json`:

```json
{
  "mcpServers": {
    "repotrim": {
      "command": "repotrim-mcp",
      "args": ["--allow-root", "."]
    }
  }
}
```

### 3. Windsurf

Add to your `mcp_config.json`:

```json
{
  "mcpServers": {
    "repotrim": {
      "command": "repotrim-mcp",
      "args": ["--allow-root", "${workspaceFolder}"]
    }
  }
}
```

### 4. Claude Code CLI

```bash
claude mcp add repotrim -- repotrim-mcp --allow-root .
```

---

## Consolidated MCP Tool Suite

| Tool | Responsibility | Typical Budget |
| :--- | :--- | :---: |
| `trim_context` | Budget-bounded submodular context extraction around seeds or query. | 500 – 2,500 |
| `find_symbols` | Unified symbol search, ranking, entrypoint discovery, and signature inspection. | 250 – 1,000 |
| `analyze_graph` | Repository topology stats, Louvain community clusters, and git co-edits. | 400 – 1,500 |
| `analyze_impact` | Transitive blast-radius analysis and downstream callers for target symbols. | 500 – 1,500 |
| `trace_paths` | Boltzmann energy-guided causal paths between dependency nodes. | 300 – 1,000 |
| `navigate_codebase` | Adaptive sequential exploration trajectory across repository boundaries. | 500 – 2,000 |
| `generate_blueprint` | High-density feature specification with inferred anchors and change skeletons. | 2,000 – 4,000 |
| `generate_architecture_docs` | Automated 4-tier architectural specification with subsystem Mermaid diagrams. | 1,500 – 3,000 |

*For detailed input/output schemas and examples, see [`docs/MCP_TOOLS.md`](docs/MCP_TOOLS.md).*

---

## Empirical Verification & Benchmark Summary

### 1. Agent Exploration Harness Study ([`docs/benchmarks/agent_exploration_study.md`](docs/benchmarks/agent_exploration_study.md))

Paired benchmark evaluation comparing unassisted agents (raw file tools) vs. RepoTrim-assisted agents on software engineering tasks:

| Scenario | Raw FS Tokens | RepoTrim Tokens | Token Spend Reduction | Information Density (SPT) |
| :--- | :---: | :---: | :---: | :---: |
| **API Refactoring** | 39,606 | 126 | **99.7%** | **27.1x gain** |
| **Bug Localization** | 6,368 | 166 | **97.4%** | **12.0x gain** |
| **Diff Resolution** | 7,396 | 32 | **99.6%** | **33.0x gain** |
| **Diffusion Optimization** | 12,016 | 170 | **98.6%** | **16.1x gain** |

### 2. Multi-Language Synthetic Corpus Evaluation ([`docs/benchmarks/synthetic_corpus_evaluation.md`](docs/benchmarks/synthetic_corpus_evaluation.md))

Controlled empirical evaluation measuring Recall@Budget and token savings across synthetic multi-file polyglot fixtures:

| Repository | Ecosystem | Budget ($B$) | Token Savings vs Whole Files | Recall@Budget |
| :--- | :---: | :---: | :---: | :---: |
| `ml-forecast-pipeline` | Python / ML | 200 | **62.8%** | **100.0%** |
| `web-dashboard-ui` | TypeScript / React | 200 | **63.6%** | **75.0%** |
| `distributed-rate-limiter` | Go / Microservices | 250 | **59.1%** | **100.0%** |

### 3. Comparative Analysis vs. Alternative Approaches ([`docs/benchmarks/comparative_analysis.md`](docs/benchmarks/comparative_analysis.md))

Ablation contrasting RepoTrim against Aider-style Global PageRank repo-maps, Lexical Search, and Whole-File Dumps:

| Context Strategy | Algorithmic Mechanism | Token Reduction % | Direct Dep Recall | Optimality Bound |
| :--- | :--- | :---: | :---: | :---: |
| **Whole-File Dump** | Raw file concatenation | 0.0% (Exhausts budget) | 84.6% | None |
| **Lexical Search (Grep)** | BM25 / substring line match | 92.6% | 38.5% | None |
| **Aider Repo Map** | Static Global PageRank ($d=0.85$) | 92.3% | 0.0% (Root-biased) | None |
| **RepoTrim (Ours)** | **Multiplex CPG + PPR + CELF** | **92.3%** | **61.5%** | **$(1 - 1/e) \approx 63.2\%$** |

---

## Documentation Roadmap

- [Installation Guide](docs/INSTALL.md): Pre-built binaries, Cargo setup, and building from source.
- [Troubleshooting & FAQ](docs/TROUBLESHOOTING.md): MSRV, C compiler setup, MCP pipe debugging, and security remedies.
- [MCP Tool Specifications](docs/MCP_TOOLS.md): Complete parameter references and JSON-RPC contracts.
- [Comparative Benchmark Analysis](docs/benchmarks/comparative_analysis.md): Empirical study vs. Aider repo-maps and file dumps.
- [Architectural Specification](docs/ARCHITECTURE.md): Multi-layer CPG, CSR matrix, caching, and subsystem design.
- [Mathematical Foundations](docs/THEORY.md): Formal proofs of submodularity, knapsack bounds, and convergence.
- [Security Policy](SECURITY.md): Threat model, `RootGuard` confinement, and vulnerability reporting.

---

## References & Academic Citations

1. **Personalized PageRank & Local Graph Diffusion:**
   - Reid Andersen, Fan Chung, Kevin Lang. *"Local Graph Partitioning using PageRank Vectors"*. In *Proceedings of the 47th Annual IEEE Symposium on Foundations of Computer Science (FOCS '06)*, pp. 475–486, 2006. [DOI: 10.1109/FOCS.2006.44](https://doi.org/10.1109/FOCS.2006.44).
2. **Submodular Function Maximization & Knapsack Approximations:**
   - George L. Nemhauser, Laurence A. Wolsey, Marshall L. Fisher. *"An analysis of approximations for maximizing submodular set functions — I"*. In *Mathematical Programming*, 14(1): 265–294, 1978. [DOI: 10.1007/BF01588971](https://doi.org/10.1007/BF01588971).
   - Samir Khuller, Anna Moss, Joseph (Seffi) Naor. *"The budgeted maximum coverage problem"*. In *Information Processing Letters*, 70(1): 39–45, 1999. [DOI: 10.1016/S0020-0190(99)00031-9](https://doi.org/10.1016/S0020-0190(99)00031-9).
   - Jure Leskovec, Andreas Krause, Carlos Guestrin, Christos Faloutsos, Jeanne VanBriesen, Natalie Glance. *"Cost-effective Outbreak Detection in Networks"*. In *Proceedings of the 13th ACM SIGKDD International Conference on Knowledge Discovery and Data Mining (KDD '07)*, pp. 420–429, 2007. [DOI: 10.1145/1281192.1281239](https://doi.org/10.1145/1281192.1281239).
   - Maxim Sviridenko. *"A note on maximizing a submodular set function subject to a knapsack constraint"*. In *Operations Research Letters*, 32(1): 41–43, 2004. [DOI: 10.1016/S0167-6377(03)00062-2](https://doi.org/10.1016/S0167-6377(03)00062-2).
3. **Multiple-Choice Knapsack Problem (MCKP):**
   - Hans Kellerer, Ulrich Pferschy, David Pisinger. *"Knapsack Problems"*. Springer Berlin, Heidelberg, 2004. [DOI: 10.1007/978-3-540-24777-7](https://doi.org/10.1007/978-3-540-24777-7). Chapter 11.
   - Martin E. Dyer. *"An $O(n)$ algorithm for the multiple-choice knapsack linear program"*. In *Mathematical Programming*, 29(1): 58–63, 1984. [DOI: 10.1007/BF02591602](https://doi.org/10.1007/BF02591602).
4. **Adaptive Submodularity & Sequential Exploration:**
   - Daniel Golovin, Andreas Krause. *"Adaptive Submodularity: Theory and Applications in Active Learning and Stochastic Optimization"*. In *Journal of Artificial Intelligence Research*, 42: 427–486, 2011. [DOI: 10.1613/jair.3380](https://doi.org/10.1613/jair.3380).
5. **Causal Path Inference & Maximum Entropy:**
   - Brian D. Ziebart, Andrew L. Maas, J. Andrew Bagnell, Anind K. Dey. *"Maximum Entropy Inverse Reinforcement Learning"*. In *Proceedings of the 23rd AAAI Conference on Artificial Intelligence (AAAI '08)*, pp. 1433–1438, 2008.
   - Jin Y. Yen. *"Finding the K Shortest Loopless Paths in a Network"*. In *Management Science*, 17(11): 712–716, 1971. [DOI: 10.1287/mnsc.17.11.712](https://doi.org/10.1287/mnsc.17.11.712).
6. **Community Detection & Modularity:**
   - Vincent A. Traag, Ludo Waltman, Nees Jan van Eck. *"From Louvain to Leiden: guaranteeing well-connected communities"*. In *Scientific Reports*, 9(1): 5233, 2019. [DOI: 10.1038/s41598-019-41695-z](https://doi.org/10.1038/s41598-019-41695-z).
   - Santo Fortunato, Marc Barthélemy. *"Resolution limit in modularity detection"*. In *PNAS*, 104(1): 35–41, 2007. [DOI: 10.1073/pnas.0605965104](https://doi.org/10.1073/pnas.0605965104).
7. **Mining Software Repositories & Co-Editing:**
   - Thomas Zimmermann, Peter Weißgerber, Stephan Diehl, Andreas Zeller. *"Mining Version Histories to Guide Software Changes"*. In *IEEE Transactions on Software Engineering*, 31(6): 429–445, 2005. [DOI: 10.1109/TSE.2005.72](https://doi.org/10.1109/TSE.2005.72).
   - Harald Gall, Karin Hajek, Mehdi Jazayeri. *"Detection of logical coupling based on change sets"*. In *Proceedings of ICSM '98*, pp. 159–168, 1998. [DOI: 10.1109/ICSM.1998.738499](https://doi.org/10.1109/ICSM.1998.738499).
8. **Hybrid Lexical-Dense Retrieval & Tokenization:**
   - Stephen E. Robertson, Hugo Zaragoza. *"The Probabilistic Relevance Framework: BM25 and Beyond"*. In *Foundations and Trends in Information Retrieval*, 3(4): 333–389, 2009. [DOI: 10.1561/1500000019](https://doi.org/10.1561/1500000019).
   - Gordon V. Cormack, Charles L. A. Clarke, Stefan Buettcher. *"Reciprocal Rank Fusion Outperforms Condorcet and Individual Rank Learning Methods"*. In *Proceedings of SIGIR '09*, pp. 758–759, 2009. [DOI: 10.1145/1571941.1572114](https://doi.org/10.1145/1571941.1572114).
   - Rico Sennrich, Barry Haddow, Alexandra Birch. *"Neural Machine Translation of Rare Words with Subword Units"*. In *Proceedings of ACL 2016*, pp. 1715–1725. [DOI: 10.18653/v1/P16-1162](https://doi.org/10.18653/v1/P16-1162).
9. **LLM Context Utilization & Code Reasoning:**
   - Nelson F. Liu et al. *"Lost in the Middle: How Language Models Use Long Contexts"*. In *TACL*, 12: 157–173, 2024. [DOI: 10.1162/tacl_a_00638](https://doi.org/10.1162/tacl_a_00638).
   - Carlos E. Jimenez et al. *"SWE-bench: Can Language Models Resolve Real-World GitHub Issues?"* In *ICLR 2024*. [arXiv:2310.06770](https://arxiv.org/abs/2310.06770).
   - Christopher D. Manning, Prabhakar Raghavan, Hinrich Schütze. *"Introduction to Information Retrieval"*. Cambridge University Press, 2008. [DOI: 10.1017/CBO9780511809071](https://doi.org/10.1017/CBO9780511809071).

---

## License

Dual-licensed under either of:
- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT License ([LICENSE-MIT](LICENSE-MIT))

at your option.
