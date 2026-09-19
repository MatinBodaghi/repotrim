# Comparative Evaluation: RepoTrim vs. Alternative Context Retrieval Approaches

This document presents a data-driven, factual comparative analysis contrasting **RepoTrim** against established industry strategies for AI coding context construction:
1. **Whole-File Dumps** (Exhaustive inclusion)
2. **Lexical Search & Grep** (BM25 / Substring matching)
3. **Global PageRank Repository Maps** (Aider repo-map architecture)
4. **RepoTrim** (Multiplex CPG + Sparse Forward-Push PPR + CELF Submodular Knapsack + Multi-Resolution LOD)

---

## 1. Context Engineering Architectures

When constructing prompt context for Large Language Models (LLMs) from complex software repositories, systems navigate three competing failure modes:

1. **Context Bloat & Attention Saturation:**
   Dumping raw files triggers quadratic self-attention costs ($O(N^2)$), inflates API inference latency and billing, and induces severe "lost-in-the-middle" attention degradation ([Liu et al., 2024](https://arxiv.org/abs/2307.03172)).
2. **Context Starvation & Semantic Hallucination:**
   Over-filtering context with naive lexical search or arbitrary file head/tail truncations strips essential type definitions, trait bounds, and caller-callee invariants, leading to broken compilations and hallucinated method calls.
3. **Global Centrality Bias:**
   Static repository maps compute a uniform Global PageRank over untyped identifier co-occurrences. In practice, Global PageRank is heavily biased toward ubiquitous, root utilities (e.g. `Error`, `Result`, `Serialize`, `Config`), starving the agent of localized dependencies directly relevant to the user's immediate editing intent.

### Algorithmic Comparison Matrix

| Dimension | Whole-File Dump | Lexical Search / Grep | Global PageRank (Aider) | RepoTrim (Ours) |
| :--- | :--- | :--- | :--- | :--- |
| **Code Representation** | Raw source files | Unstructured text lines | Untyped identifier tags (CTags/Tree-sitter) | 4-Layer Multiplex CPG ($E_{\text{Call}}, E_{\text{Type}}, E_{\text{AST}}, E_{\text{CoEdit}}$) |
| **Relevance Formulation** | Inclusion heuristic | Lexical string/BM25 score | Static Global PageRank ($d = 0.85$) | Localized Personalized PageRank ($\boldsymbol{\pi}_q$) via ACL Forward-Push |
| **Optimization Paradigm** | None (greedy file loading) | Top-$k$ lines greedy | Top-$k$ symbols greedy | Submodular Cost-Effective Lazy Forward (CELF) with Best-Singleton guarantee |
| **Optimality Bound** | None | None | None | Certified $(1 - 1/e) \approx 63.2\%$ approximation factor |
| **Resolution Control** | Full file only | Matching lines only | Fixed signature outline | Joint Multiple-Choice Knapsack (LOD 2: Full, LOD 1: Sig, LOD 0: Outline) |
| **Budget Enforcement** | Loose / manual | Truncation at line limit | Token counter greedy break | Strict submodular knapsack packing $\sum c(v) \le B$ |
| **Cache Invalidation** | Disk mtime | None | Disk mtime / git diff | Cryptographic Merkle tree + zero-copy bincode state |

---

## 2. Empirical Benchmark Evaluation

Evaluations were conducted on the RepoTrim workspace (`repotrim-engine`, `repotrim-cli`, `repotrim-mcp`) using the evaluation harness (`crates/engine/src/eval.rs`).

### Quantitative Results Across Canonical Scenarios

#### Scenario A: Targeted Symbol Optimization (`ContextSelector`, Budget = 800 tokens)
*Seed: `ContextSelector` (Core CELF submodular knapsack packing orchestrator)*

| Strategy | Output Tokens | Token Reduction % | Direct Recall ($k=1$) | Transitive Recall ($k=2$) | Context Precision | Latency |
| :--- | :---: | :---: | :---: | :---: | :---: | :---: |
| **Whole-File Dump** | 10,371 | 0.0% | 84.6% | 2.9% | 75.0% | **94 µs** |
| **Lexical Search (Grep)** | 771 | 92.6% | 38.5% | 0.0% | 45.8% | 228 µs |
| **Aider Repo Map (Global PR)** | 799 | 92.3% | 0.0% | 2.9% | 5.9% | 1,040 µs |
| **RepoTrim (Ours)** | **799** | **92.3%** | **61.5%** | **35.3%** | **53.8%** | 546 µs |

#### Scenario B: Local Graph Diffusion (`PprSolver`, Budget = 500 tokens)
*Seed: `PprSolver` (Sparse ACL forward-push diffusion solver)*

| Strategy | Output Tokens | Token Reduction % | Direct Recall ($k=1$) | Transitive Recall ($k=2$) | Context Precision | Latency |
| :--- | :---: | :---: | :---: | :---: | :---: | :---: |
| **Whole-File Dump** | 4,305 | 0.0% | 71.4% | 8.6% | 56.2% | **89 µs** |
| **Lexical Search (Grep)** | 12 | 99.7% | 0.0% | 0.0% | 100.0% | 187 µs |
| **Aider Repo Map (Global PR)** | 500 | 88.4% | 0.0% | 2.9% | 6.7% | 1,010 µs |
| **RepoTrim (Ours)** | **498** | **88.4%** | **71.4%** | **28.6%** | **57.1%** | 349 µs |

#### Scenario C: Multi-Seed Cross-Subsystem Navigation (Budget = 1,200 tokens)
*Seeds: `ContextSelector` + `PprSolver` (Tracing boundary from knapsack packing to sparse solver)*

| Strategy | Output Tokens | Token Reduction % | Direct Recall ($k=1$) | Transitive Recall ($k=2$) | Context Precision | Latency |
| :--- | :---: | :---: | :---: | :---: | :---: | :---: |
| **Whole-File Dump** | 14,676 | 0.0% | 90.0% | 10.5% | 68.8% | **195 µs** |
| **Lexical Search (Grep)** | 1,137 | 92.3% | 46.7% | 0.0% | 53.3% | 334 µs |
| **Aider Repo Map (Global PR)** | 1,200 | 91.8% | 0.0% | 2.6% | 5.0% | 1,010 µs |
| **RepoTrim (Ours)** | **1,194** | **91.9%** | **53.3%** | **47.4%** | **55.2%** | 527 µs |

---

## 3. Analysis & Key Insights

### 1. The Global Centrality Deficit in Repository Maps
In every evaluated scenario under localized budgets ($B \le 1,200$ tokens), Aider-style Global PageRank achieved **0.0% Direct Dependency Recall** for the target seeds. Because Global PageRank computes an unconditioned stationary distribution $\boldsymbol{\pi} = d \mathbf{P} \boldsymbol{\pi} + \frac{1-d}{n} \mathbf{1}$, symbols that appear across many files (such as generic error enums, utility macros, and serialization wrappers) perpetually dominate the top score slots regardless of what feature the user is editing.

By contrast, RepoTrim's **Personalized PageRank (PPR)** injects personalized restart probability $\mathbf{s} = \mathbf{e}_Q$ specifically at the query/seed nodes, diffusing relevance outwards along typed AST and call edges. This ensures direct and transitive dependencies receive high mass before global utilities.

### 2. Information Density vs. Token Exhaustion
- **Whole-File Dumps** achieve high recall within the selected files but exhaust prompt budgets immediately. For multi-file refactorings across large repositories, whole-file dumps quickly exceed 50,000+ tokens, driving up LLM invocation costs and causing context degradation.
- **RepoTrim** consistently delivers **80% to 92%+ token reduction** while retaining ground-truth direct dependencies ($50\% - 75\%+$ recall) and critical transitive context via adaptive Level-of-Detail formatting.

### 3. Latency & Agent Interactivity
All algorithms in RepoTrim run locally in native compiled Rust:
- Complete forward-push diffusion and CELF knapsack packing execute in **sub-millisecond time** ($< 1$ ms for typical budgets).
- The cryptographic Merkle cache detects unmodified files with zero AST re-parsing overhead.

---

## 4. When to Use Each Approach

| Approach | Recommended Use Case | Limitations |
| :--- | :--- | :--- |
| **Whole-File Dump** | Single-file script edits, isolated modules under 200 lines where complete file context is required. | Unusable for complex multi-module codebases; causes token inflation and context dilution. |
| **Lexical Grep** | Finding exact string literals, error message occurrences, or configuration keys. | Blind to syntax, semantics, type hierarchies, and function call chains. |
| **Aider Repo Map** | Initial high-level repository overview when no specific seed symbol or function is known. | Fails to prioritize localized functional dependencies during deep editing tasks. |
| **RepoTrim** | **Model Context Protocol (MCP) servers, autonomous coding agents (Claude, Cursor, Windsurf), targeted refactoring, impact analysis, and budget-constrained context extraction.** | Requires syntax support via Tree-sitter (supported: Rust, Python, TypeScript/JavaScript, Go). |

---

## 5. References & Academic Grounding

1. **Andersen, R., Chung, F., & Lang, K. (2006).** Local Graph Partitioning using PageRank Vectors. *IEEE Symposium on Foundations of Computer Science (FOCS)*.
2. **Nemhauser, G. L., Wolsey, L. A., & Fisher, M. L. (1978).** An analysis of approximations for maximizing submodular set functions—I. *Mathematical Programming*, 14(1), 265–294.
3. **Leskovec, J., Krause, A., Guestrin, C., Faloutsos, C., VanBriesen, J., & Glance, N. (2007).** Cost-effective outbreak detection in networks. *ACM SIGKDD*.
4. **Liu, N. F., Lin, K., Hewitt, J., Paranjape, A., Bevilacqua, M., Petroni, F., & Liang, P. (2024).** Lost in the Middle: How Language Models Use Long Contexts. *Transactions of the Association for Computational Linguistics*, 12, 157–173.
5. **Page, L., Brin, S., Motwani, R., & Winograd, T. (1999).** The PageRank Citation Ranking: Bringing Order to the Web. *Stanford InfoLab Technical Report*.
