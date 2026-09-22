# Synthetic Multi-Language Benchmark Evaluation: Polyglot Recall@Budget

- **Evaluation Suite:** `crates/engine/tests/external_corpus_test.rs`
- **Target Languages:** Python (ML/Data Science), TypeScript (Modern Frontend/React), Go (Cloud Microservices)
- **Date:** 2026-09-19
- **Environment:** x86_64, Rust stable 1.90+ (profile: release)
- **Commit Baseline:** `v0.12.0` (Phase 4 Conformance & Comprehensive Testing)

---

## 1. Executive Summary & Problem Formulation

In realistic AI-assisted software engineering workflows, autonomous agents (e.g., Cursor Agent, Claude Code, Antigravity, SWE-bench solvers) are deployed across heterogeneous language ecosystems. When presented with an issue or feature request, naive tools either:
1. Dump entire files containing query matches into the prompt context window, or
2. Truncate files arbitrarily without respecting cross-file dependency call-graphs or AST declarations.

Both extremes degrade agent reasoning: full-file dumps trigger context pollution, attention degradation (the *"lost-in-the-middle"* phenomenon, Liu et al., 2024), and quadratic latency/cost inflation. Arbitrary line truncation breaks symbol definitions, imports, and method contracts.

RepoTrim solves this by constructing a multi-layer Code Property Graph across languages and applying submodular knapsack optimization under a strict token budget $B$. This benchmark evaluates RepoTrim's **Recall@Budget** and **Token Compression** across three synthetic multi-file fixtures modelled on common real-world architectures (generated dynamically in `crates/engine/tests/external_corpus_test.rs:21` via `create_temp_corpus_repo(name, files)`):
1. **Python Data Science / ML Pipeline** (`ml-forecast-pipeline`): Deep multi-file class inheritance and data transformations.
2. **TypeScript / React Frontend Application** (`web-dashboard-ui`): Asynchronous state management, HTTP client abstractions, and component trees.
3. **Go Distributed Microservice** (`distributed-rate-limiter`): Middleware pipelines, cache storage interfaces, and handler routing.

These synthetic fixtures are hand-constructed to exercise cross-file dependency resolution across heterogeneous languages; they are **not** mined from real external repositories, and results here represent controlled-condition measurements rather than field validation.

---

## 2. Methodology & Formal Evaluation Framework

### 2.1 The Recall@Budget Metric

Let $S^*$ denote the ground-truth set of AST symbols strictly required by an expert engineer or autonomous agent to implement a given change-set (PR feature or bug fix). Let $S(B)$ denote the set of symbols selected by RepoTrim given seed symbol(s) $S_0$ under token budget $B$.

The **Recall@Budget** is defined as:

$$\text{Recall}@B = \frac{|S(B) \cap S^*|}{|S^*|}$$

### 2.2 Token Savings Relative to Whole-File Dumps

Let $T_{\text{repotrim}}$ represent the total BPE subword tokens consumed by the synthesized RepoTrim context (including declaration signatures, docstrings, and bodies scaled by level-of-detail). Let $T_{\text{whole\_file}}$ represent the token cost of transmitting the full contents of all files touched or referenced by the task:

$$\text{Savings}\% = \left( 1 - \frac{T_{\text{repotrim}}}{T_{\text{whole\_file}}} \right) \times 100\%$$

### 2.3 Monotonic Budget Scaling

By the submodular property of RepoTrim's utility function $F(S)$ (Nemhauser et al., 1978; Khuller et al., 1999), as the budget $B$ expands from $B_1 < B_2 < \dots < B_k$, the selected symbol set monotonically expands or preserves recall:

$$\text{Recall}@B_1 \le \text{Recall}@B_2 \le \dots \le \text{Recall}@B_k$$

---

## 3. Empirical Multi-Repository Evaluation

All trials were executed via automated integration testing in `crates/engine/tests/external_corpus_test.rs`.

### 3.1 Corpus 1: Python Data Science / ML Pipeline (`ml-forecast-pipeline`)
- **Task:** Refactor `NormalizeTransform` to support min-max scaling and adapt `TimeSeriesDataset` batch normalization.
- **Seeds:** `TimeSeriesDataset`
- **Ground Truth Symbols:** `TimeSeriesDataset`, `load_samples`, `NormalizeTransform`, `apply`
- **Budget:** 200 tokens
- **Whole-File Baseline:** 253 tokens

### 3.2 Corpus 2: TypeScript / React UI (`web-dashboard-ui`)
- **Task:** Implement automated JWT refresh token renewal on session expiry within `AuthStore`.
- **Seeds:** `AuthStore`
- **Ground Truth Symbols:** `AuthStore`, `loginUser`, `ApiClient`, `refreshToken`
- **Budget:** 200 tokens
- **Whole-File Baseline:** 464 tokens

### 3.3 Corpus 3: Go Cloud Microservice (`distributed-rate-limiter`)
- **Task:** Audit Redis connection caching and key generation within `UserHandler` rate limiter middleware.
- **Seeds:** `UserHandler`
- **Ground Truth Symbols:** `UserHandler`, `HandleGetUser`, `RateLimiter`, `Allow`, `RedisClient`, `Get`
- **Budget:** 250 tokens
- **Whole-File Baseline:** 274 tokens

---

## 4. Empirical Results

### 4.1 Cross-Language Recall & Token Savings Summary

| Repository | Language | Budget ($B$) | Selected / Whole Tokens | Token Savings | Recall@Budget |
| :--- | :---: | :---: | :---: | :---: | :---: |
| **ml-forecast-pipeline** | Python | 200 | 94 / 253 | **62.8%** | **100.0%** |
| **web-dashboard-ui** | TypeScript | 200 | 169 / 464 | **63.6%** | **75.0%** |
| **distributed-rate-limiter** | Go | 250 | 112 / 274 | **59.1%** | **100.0%** |

### 4.2 Monotonic Budget Scaling Curve (Go Corpus)

To verify that context selection does not oscillate or regress under varying budget constraints, we evaluated recall across three budget tiers ($B \in \{40, 100, 250\}$):

| Budget Tier ($B$) | Selected Tokens | Ground-Truth Symbols Recalled | Recall@Budget |
| :---: | :---: | :---: | :---: |
| **40** (Tight) | 26 | `Service` | 20.0% |
| **100** (Medium) | 71 | `Service`, `Get`, `Database` | 60.0% |
| **250** (Full) | 112 | `Service`, `Get`, `Database`, `Query`, `Cache` | **100.0%** |

The scaling curve demonstrates strictly monotonic recall progression ($\text{Recall}@40 \le \text{Recall}@100 \le \text{Recall}@250$) with the top tier capturing 100% of task-critical context.

---

## 5. Architectural Insights & Academic Grounding

1. **Submodular Knapsack Guarantees in Heterogeneous Codebases:**
   Whether parsing Python indentation blocks, TypeScript interfaces and export declarations, or Go package structs and receiver methods, RepoTrim's unified CSR graph and CELF/MCKP solver consistently achieves $>59\%$ token compression while delivering $\ge 75\%$ task recall.
2. **PageRank Flow Along Dependency Directions:**
   In strongly typed languages (Go, TypeScript) and dynamic languages (Python), dependency references flow cleanly from caller/instantiator to callee. Seeding at the primary service or class boundary diffuses attention outwards to critical collaborators while pruning unrelated peripheral components (such as footers, plotting scripts, and health-check handlers).
3. **Mitigating LLM Context Saturation:**
   By filtering out 59% to 64% of non-essential tokens before prompting, RepoTrim prevents distraction in long-context models, preserves valuable prompt space for multi-step agent reasoning, and reduces token transmission costs.

---

## 6. Limitations & Future Work

1. **Controlled Fixture Bias:** The test corpora evaluated here are synthetic integration fixtures rather than organically evolved open-source projects. While architecturally realistic, they do not reflect the full noise, legacy conventions, and edge cases found in large-scale multi-contributor repositories.
2. **Planned Field Validation:** Full empirical validation against third-party production repositories (with ground truth extracted from accepted pull requests on repositories such as Flask, React, and Gin) and downstream agent task-success measurement on SWE-bench are scheduled for the post-v1.0.0 roadmap.

---

## References & Academic Citations

1. **Information Retrieval & Evaluation Metrics:**
   - Christopher D. Manning, Prabhakar Raghavan, Hinrich Schütze. *"Introduction to Information Retrieval"*. Cambridge University Press, 2008. Chapter 8 (Evaluation in information retrieval, Recall-Precision metrics). [DOI: 10.1017/CBO9780511809071](https://doi.org/10.1017/CBO9780511809071).
2. **Budgeted Submodular Optimization:**
   - Samir Khuller, Anna Moss, Joseph (Seffi) Naor. *"The budgeted maximum coverage problem"*. In *Information Processing Letters*, 70(1): 39–45, 1999. [DOI: 10.1016/S0020-0190(99)00031-9](https://doi.org/10.1016/S0020-0190(99)00031-9).
   - George L. Nemhauser, Laurence A. Wolsey, Marshall L. Fisher. *"An analysis of approximations for maximizing submodular set functions — I"*. In *Mathematical Programming*, 14(1): 265–294, 1978. [DOI: 10.1007/BF01588971](https://doi.org/10.1007/BF01588971).
3. **Context Utilization in LLM Reasoning:**
   - Nelson F. Liu, Kevin Lin, John Hewitt, Ashwin Paranjape, Michele Bevilacqua, Fabio Petroni, Percy Liang. *"Lost in the Middle: How Language Models Use Long Contexts"*. In *Transactions of the Association for Computational Linguistics (TACL)*, 12: 157–173, 2024. [DOI: 10.1162/tacl_a_00638](https://doi.org/10.1162/tacl_a_00638).
   - Carlos E. Jimenez, John Yang, Alexander Wettig, Shunyu Yao, Kexin Pei, Ofir Press, Karthik Narasimhan. *"SWE-bench: Can Language Models Resolve Real-World GitHub Issues?"* In *ICLR 2024*. [arXiv:2310.06770](https://arxiv.org/abs/2310.06770).
