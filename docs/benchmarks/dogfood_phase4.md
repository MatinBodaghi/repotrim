# Empirical Dogfooding Benchmark: Phase 4

- **Phase Evaluated:** Phase 4 (Multiplex Edge Enrichment & Multi-Resolution LOD Context Formatter)
- **Target Codebase:** `crates/engine/src/` (RepoTrim Engine)
- **Date:** 2026-09-07

---

## 1. Engine & Graph Ingestion Metrics (Phase 3 vs. Phase 4 Comparison)

| Metric | Phase 3 Value | Phase 4 Value | Evolution & Delta |
| :--- | :--- | :--- | :--- |
| **Source Files Parsed** | 11 Rust files | 12 Rust files | +1 file (`src/formatter.rs`) |
| **Total Source Bytes** | 79,169 bytes | 107,691 bytes | +36.0% codebase growth |
| **AST Parse Duration** | 168.4 ms | 192.7 ms | Linear AST scaling |
| **Symbols Extracted** | 97 declarations | 116 declarations | +19.6% symbol coverage |
| **Raw Reference Edges** | 619 call edges | 1,113 multiplex edges | **+79.8% raw edge extraction** |
| **Resolved Graph Edges ($E$)** | 146 edges | 341 edges | **+133.6% graph connectivity** |
| **CSR Matrix Build Time** | 992.1 µs | 1,221.5 µs (1.22 ms) | Sub-2ms graph compilation |
| **Total Repository Symbol Cost** | 2,285 tokens | 2,946 tokens | +661 tokens total inventory |

---

## 2. Scenario Evaluation & Benchmark Results

### Scenario A1: Method Seed (`select_context`) — Budget: 300 tokens
* **Execution Latency:** **571.9 µs (0.57 milliseconds)**
* **Tokens Used:** 295 / 300 tokens (98.3% budget efficiency)
* **Token Reduction:** **90.0% reduction** (from 2,946 down to 295 tokens)
* **Symbols Selected:** 21 symbols spanning 6 modules (`celf.rs`, `csr.rs`, `graph.rs`, `ppr.rs`, `resolver.rs`, `selector.rs`).

### Scenario A2: Struct Seed (`ContextSelector`) — Budget: 200 tokens
* **Phase 3 Result:** 1 symbol (isolated struct with 0 out-edges).
* **Phase 4 Result:** **15 symbols selected** spanning `selector.rs`, `ppr.rs`, `celf.rs`, `graph.rs`, `resolver.rs`, `symbol.rs`, `tokens.rs`.
* **Execution Latency:** 288.7 µs
* **Tokens Used:** 192 / 200 tokens (96.0% budget efficiency)
* **Token Reduction:** **93.5% reduction**
* **Significance:** $E_{\text{AST}}$ reverse containment and $E_{\text{Type}}$ field references enabled the struct to diffuse directly into its constituent fields (`PprSolver`, `CelfOptimizer`) and member methods (`new`, `select_context`, `select_and_format_context`).

### Scenario B: Struct Seed (`PprSolver`) — Budget: 400 tokens
* **Phase 3 Result:** 1 symbol (isolated struct).
* **Phase 4 Result:** **27 symbols selected**
* **Execution Latency:** 455.8 µs
* **Tokens Used:** 397 / 400 tokens (99.3% budget efficiency)
* **Token Reduction:** **86.5% reduction**
* **Key Context:** Captures `PprConfig`, `compute`, `new`, `CsrMatrix`, `EdgeKind`, `resolve`, `SymbolId`.

### Scenario C: Struct Seed (`CsrMatrix`) — Budget: 600 tokens
* **Phase 3 Result:** 1 symbol (isolated struct).
* **Phase 4 Result:** **15 symbols selected**
* **Execution Latency:** 165.6 µs
* **Tokens Used:** 393 / 600 tokens (exhausted relevant subgraph)
* **Token Reduction:** **86.7% reduction**
* **Key Context:** Captures `from_edges`, `num_edges`, `out_degree`, `neighbors`, `weights`, `row_slice`, `row_normalized`, `transpose`, `SymbolId`.

### Scenario D: End-to-End Pipeline & Multi-Resolution LOD Formatting
* **Seed:** `select_context` method | **Budget:** 400 tokens
* **Pipeline Latency:** **655.7 µs (< 0.7 milliseconds total)**
  - Includes PPR local forward-push diffusion.
  - Includes CELF submodular knapsack selection.
  - Includes dynamic Level-of-Detail (LOD) token budget assignment.
  - Includes deterministic Markdown generation grouped by canonical file paths.
* **Selected Symbols:** 27 symbols formatted into 2,090 bytes of Markdown.
* **Output Quality:**
  - Files grouped under standard `### File: path` headers.
  - Syntax highlighted in fenced ```` ```rust ```` blocks.
  - Precise line intervals documented (`// Lines 21-27`).
  - Full implementation bodies rendered for root seeds; docstrings and compact signatures rendered for surrounding context.

---

## 3. Comparative Findings: Phase 3 vs. Phase 4

```
Resolved Graph Edges (E):
Phase 3: [============                    ] 146 edges
Phase 4: [================================] 341 edges (+133.6%)

Struct Seeding Yield (ContextSelector):
Phase 3: [*                               ] 1 symbol (isolated)
Phase 4: [***************                 ] 15 symbols (connected cluster)

End-to-End Formatting Latency:
Phase 4: 655.7 µs (Sub-millisecond prompt preparation)
```

1. **Resolution of Struct Isolation:**
   - The introduction of bidirectional $E_{\text{AST}}$ containment edges (`impl Struct -> struct Struct` and reverse) and $E_{\text{Type}}$ field/parameter references completely eliminated the "isolated struct" failure mode identified in Phase 3.
2. **Sub-Millisecond End-to-End Latency:**
   - Despite adding full Markdown rendering and dynamic LOD assignment, the complete pipeline executes in **0.65 ms**, well below the target budget of 50 ms for interactive agent CLI tools.
3. **Budget Adherence:**
   - In all tested scenarios, token usage stayed strictly below the configured token budget while maintaining 86.5% – 93.5% token reduction relative to the raw codebase.

---

## 4. Recommendations for Phase 5

The core mathematical and formatting engines are now complete, benchmarked, and robust.
The next phase (**Phase 5**) should focus on:
1. **CLI Binary (`crates/cli`)**:
   - Build the user-facing CLI binary `repotrim` supporting subcommands:
     - `repotrim select --seed <IDENT> --budget <TOKENS> --path <REPO>`
     - `repotrim inspect` (showing graph statistics, top PageRank hubs)
     - `repotrim config` (setting layer weights and solver tolerances)
2. **Standard Output & Pipe Formats**:
   - Provide `--format markdown` (human/LLM readable) and `--format json` (for agent tool call integration).
