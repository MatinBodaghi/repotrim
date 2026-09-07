# Empirical Dogfooding Benchmark: Phase 3

- **Phase Evaluated:** Phase 3 (Mathematical Engine: ACL Forward-Push PPR + CELF Submodular Knapsack)
- **Target Codebase:** `crates/engine/src/` (RepoTrim Engine)
- **Commit:** `241a74c`
- **Date:** 2026-09-07

---

## 1. Engine & Graph Ingestion Metrics

| Metric | Measured Value |
| :--- | :--- |
| **Source Files Parsed** | 11 Rust files (`src/*.rs`) |
| **Total Source Bytes** | 79,169 bytes |
| **AST Parse Duration** | 168.4 ms (one-time Tree-sitter C runtime pass) |
| **Symbols Extracted** | 97 discrete syntax declarations |
| **Raw Reference Edges** | 619 call / identifier points |
| **Resolved Graph Edges** | 146 directed graph edges |
| **CSR Matrix Build Time** | **992.1 µs (< 1.0 ms)** |
| **Total Repository Symbol Cost** | 2,285 tokens |

---

## 2. Scenario Evaluation & Benchmark Results

### Scenario A1: Method Seed (`ContextSelector::select_context`) — Budget: 300 tokens
* **Execution Latency:** **254.3 µs (0.25 milliseconds)**
* **Tokens Used:** 286 / 300 tokens (95.3% budget efficiency)
* **Token Reduction:** **87.5% reduction** (from 2,285 tokens down to 286 tokens)
* **Symbols Selected:** 16 symbols perfectly distributed across 6 orthogonal files:
  - `src/celf.rs`: `new`, `cmp`, `partial_cmp`
  - `src/ppr.rs`: `new`, `compute`
  - `src/graph.rs`: `symbol`, `symbols`, `is_empty`, `num_symbols`, `transition_csr`
  - `src/csr.rs`: `neighbors`, `row_slice`, `out_degree`
  - `src/selector.rs`: `new`, `select_context`
  - `src/symbol.rs`: `SymbolId`

### Scenario A2, B, C: Struct Seeds (`ContextSelector`, `PprSolver`, `CsrMatrix`)
* **Execution Latency:** 26.2 µs – 40.2 µs
* **Result:** Extracted the struct itself, but had 0 out-edges to methods or field types.

---

## 3. Findings & Required Roadmap Refinements

1. **Strength:**
   - The Forward-Push PPR + CELF Knapsack optimization works flawlessly on call graphs: sub-millisecond execution, zero clustering, and exact budget adherence.
2. **Deficiency Identified:**
   - In Rust, struct definitions don't "call" functions. Their methods live in separate `impl` blocks and their fields hold types.
   - Without $E_{\text{AST}}$ (struct $\leftrightarrow$ method) and $E_{\text{Type}}$ (struct $\to$ field types) edges, structs act as isolated nodes.
3. **Roadmap Action:**
   - Refine **Phase 4** to include **Multiplex Edge Enrichment ($E_{\text{AST}}, E_{\text{Type}}$)** before formatting context skeletons.
