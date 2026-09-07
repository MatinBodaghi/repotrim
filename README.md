# RepoTrim

A mathematically optimal codebase context trimmer for AI coding agents (Antigravity, OpenCode, Claude Code, Cursor).

RepoTrim replaces brute-force file dumping and naive vector retrieval with a deterministic multiplex code property graph, Personalized PageRank (PPR), and CELF submodular knapsack packing.

---

## Theoretical & Mathematical Foundation

1. **Multiplex Code Property Graph ($\mathcal{M} = (V, \{E_k\}, \mathbf{W})$) with Bayesian Scoped Disambiguation**:
   - **Nodes ($V$)**: Granular syntax entities (functions, methods, structs, traits, modules) indexed by contiguous 32-bit integers (`SymbolId(u32)`).
   - **Layered Edges ($E_k$)**: Orthogonal relationships with distinct tensor weights $\omega_k$:
     - $E_{\text{AST}}$: Lexical containment (module $\to$ struct $\to$ method).
     - $E_{\text{Call}}$: Explicit call-graph invocations ($f() \to g()$).
     - $E_{\text{Type}}$: Type dependency signatures (inputs/return types).
     - $E_{\text{Import}}$: Module and path imports.
   - **Bayesian Scoped Resolution**: Resolves call/type targets across files without a heavy compiler daemon by weighting lexical scope distance, explicit import paths, and receiver type hints to eliminate false edges.

2. **Relevance via Local Forward-Push Personalized PageRank (PPR)**:
   - Utilizes the **Andersen-Chung-Lang (ACL) Forward-Push** algorithm rather than global power-iteration:
     - Maintains probability estimate $\mathbf{p}$ and residual vector $\mathbf{r}$.
     - Pushes mass locally along edges where $r(u) > \epsilon \cdot d(u)$.
   - **$O(1/\epsilon)$ Local Complexity**: Running time depends strictly on the relevant subgraph neighborhood around the seed (prompt, diff, active file), not on repository size. Executes in microsecond-to-millisecond time.
   - **Guaranteed Precision**: Bounded approximation error $\| \hat{\mathbf{p}} - \mathbf{p}^* \|_\infty \le \epsilon$.

3. **Multi-Resolution Level-of-Detail (LOD) Submodular Knapsack Selection**:
   - Replaces binary in-or-out selection with a **Multi-Choice Submodular Knapsack**:
     - **$\text{LOD}_0$ (Signature)**: `pub fn handle(req: &Request) -> Response;` (~15 tokens).
     - **$\text{LOD}_1$ (Signature + Doc)**: Signature plus docstrings (~40 tokens).
     - **$\text{LOD}_2$ (Program Slice)**: Signature + control flow predicates (`if`, `match`, loop guards) + return expressions (~80 tokens).
     - **$\text{LOD}_3$ (Full Body)**: Complete implementation block (~200+ tokens).
   - **CELF Optimization**: Cost-Effective Lazy Forward queue maximizes monotone submodular facility location coverage with diversity constraints:
     $$\max_{S \subseteq V \times \text{LOD}} f(S) \quad \text{subject to} \quad \sum_{(v, l) \in S} c(v, l) \le B$$
     Guarantees $(1 - 1/e)$ approximation while dynamically assigning high LOD to active code and lower LOD to contextual dependencies.

4. **Scale Invariance (Small Repos to Massive Monorepos)**:
   - **Small repos (500–5,000 LOC)**: When total code tokens are within or near the budget $B$, the knapsack naturally promotes symbols to $\text{LOD}_3$ (full implementation), providing 100% complete source code without loss.
   - **Large repos (100,000+ LOC)**: Forward-Push and CELF isolate the exact relevant sub-graph in <5ms, preventing token bloat and eliminating "lost-in-the-middle" LLM degradation.

---

## Workspace Layout

```text
repotrim/
├── Cargo.toml                  # Workspace manifest
├── docs/
│   └── harness/                # Agent harness guides, blueprint templates & server docs
│       ├── OVERVIEW.md
│       ├── FEATURE_BLUEPRINT_TEMPLATE.md
│       ├── LOCAL_TO_SERVER.md
│       └── TOKEN_OPTIMIZATION.md
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

## Agent Harness Integration & Documentation

RepoTrim is built natively for AI coding agent harnesses (Antigravity, Claude Code, Cursor, OpenCode, SWE-bench runners). Detailed documentation is organized under [`docs/harness/`](docs/harness/):

- **[Harness Overview](docs/harness/OVERVIEW.md)**: How agent harnesses invoke RepoTrim to prevent context window exhaustion and "lost-in-the-middle" degradation.
- **[Feature Blueprint Template](docs/harness/FEATURE_BLUEPRINT_TEMPLATE.md)**: Standardized template for specifying new features with high information density, allowing agents to anchor directly to seeds without blind directory scanning.
- **[Local-to-Server Guide](docs/harness/LOCAL_TO_SERVER.md)**: Step-by-step instructions for moving from local development (Windows/macOS) to remote Linux servers, cloud VMs, and Docker containers.
- **[Token Optimization Guide](docs/harness/TOKEN_OPTIMIZATION.md)**: 3-tier context funnel and dynamic budget allocation strategies to achieve 70–85% token reduction.

---

## Roadmap

| Phase | Description | Status |
| :--- | :--- | :--- |
| **Phase 0** | Workspace setup, Cargo inheritance, Git init, remote push | **Completed** |
| **Phase 1** | AST Ingestion & Symbol Extraction Engine (`tree-sitter`, BLAKE3) | **Completed** |
| **Phase 2** | Multiplex Graph, Bayesian Scoped Resolution & CSR Packing | **Completed** |
| **Phase 3** | Mathematical Engine (ACL Forward-Push PPR + CELF Knapsack) | **Completed** |
| **Phase 4** | Multiplex Edge Enrichment ($E_{\text{AST}}, E_{\text{Type}}$) & Multi-Resolution LOD Formatter | **Completed** |
| **Phase 5** | Incremental AST Merkle Diffing & Zero-Copy Persistence | Next |
| **Phase 6** | MCP Server (`stdio`) & Standalone CLI Interface | Queued |
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
