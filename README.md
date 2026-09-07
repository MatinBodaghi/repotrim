# RepoTrim

A mathematically optimal codebase context trimmer for AI coding agents (Antigravity, OpenCode, Claude Code, Cursor).

RepoTrim replaces brute-force file dumping and naive vector retrieval with a deterministic multiplex code property graph, Personalized PageRank (PPR), and CELF submodular knapsack packing.

---

## Theoretical & Mathematical Foundation

1. **Multiplex Code Property Graph ($\mathcal{M} = (V, \{E_k\}, \mathbf{W})$)**:
   - **Nodes ($V$)**: Granular syntax entities (functions, methods, structs, traits, modules) indexed by contiguous 32-bit integers (`SymbolId(u32)`).
   - **Layered Edges ($E_k$)**: Orthogonal relationships with distinct tensor weights $\omega_k$:
     - $E_{\text{AST}}$: Lexical containment (module $\to$ struct $\to$ method).
     - $E_{\text{Call}}$: Explicit call-graph invocations ($f() \to g()$).
     - $E_{\text{Type}}$: Type dependency signatures (inputs/return types).
     - $E_{\text{Import}}$: Module and path imports.

2. **Relevance via Multiplex Personalized PageRank (PPR)**:
   - Unified transition probability matrix: $\mathbf{P} = \sum \omega_k \mathbf{D}_k^{-1} \mathbf{A}_k$.
   - Power iteration with restart damping ($\alpha \approx 0.15$): $\mathbf{r}^{(t+1)} = (1 - \alpha)\mathbf{P}^T \mathbf{r}^{(t)} + \alpha \mathbf{p}_0$.
   - Personalization seed $\mathbf{p}_0$ concentrates probability mass on symbols directly touched by the user's prompt, diff, or active file.

3. **CELF Submodular Knapsack Selection (Anti-Clustering)**:
   - Maximizes submodular objective $\max_{S \subseteq V} f(S)$ subject to token budget $\sum_{v \in S} c(v) \le B$.
   - Combines facility location coverage with entropy/diversity penalties to guarantee $(1 - 1/e)$ approximation without single classes monopolizing the budget.

4. **Interprocedural Program Slicing (2-CFL Reachability)**:
   - Discards internal function assignments and logs while preserving signatures, control flow predicates, and return expressions.

---

## Workspace Layout

```text
repotrim/
├── Cargo.toml                  # Workspace manifest
├── queries/                    # Tree-sitter declarative S-expression queries
│   └── rust.scm
└── crates/
    ├── engine/                 # repotrim-engine: AST parsing, CSR, PPR, CELF
    │   ├── Cargo.toml
    │   └── src/
    │       ├── lib.rs
    │       ├── error.rs        # Engine error types
    │       ├── symbol.rs       # Dense SymbolId, SymbolNode, ReferenceEdge
    │       ├── tokens.rs       # In-engine allocation-free BPE token estimator
    │       └── parser.rs       # Tree-sitter driver & signature extractor
    ├── cli/                    # repotrim: Standalone terminal interface (Phase 6)
    └── mcp-server/             # repotrim-mcp: stdio JSON-RPC MCP server (Phase 6)
```

---

## Roadmap

| Phase | Description | Status |
| :--- | :--- | :--- |
| **Phase 0** | Workspace setup, Cargo inheritance, Git init, remote push | **Completed** |
| **Phase 1** | AST Ingestion & Symbol Extraction Engine (`tree-sitter`, BLAKE3) | **Completed** |
| **Phase 2** | In-Memory Multiplex Graph & CSR Matrix Packing | Queued |
| **Phase 3** | Mathematical Engine (Power-Iteration PPR + CELF Knapsack) | Queued |
| **Phase 4** | Slicing & Skeleton Context Formatter | Queued |
| **Phase 5** | Incremental AST Merkle Diffing & Persistence | Queued |
| **Phase 6** | MCP Server (`stdio`) & CLI Interface | Queued |
| **Phase 7** | Empirical Benchmarking (Token reduction vs. Aider/RAG) | Queued |
| **Phase 8** | Open-Source Packaging & crates.io Release | Queued |

---

## Development & Testing

```bash
# Run unit and integration tests
cargo test --workspace

# Run linter
cargo clippy --workspace -- -D warnings

# Format code
cargo fmt --check
```
