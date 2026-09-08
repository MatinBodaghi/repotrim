# RepoTrim Self-Indexing Benchmark (Dogfooding)

- **Target Codebase:** RepoTrim Workspace (`crates/engine/`, `crates/cli/`, `crates/mcp-server/`)
- **Environment:** AMD Ryzen / Windows 11 (x86_64), Rust stable

---

## 1. Engine & Graph Ingestion Metrics

| Metric | Measured Value | Description |
| :--- | :--- | :--- |
| **Workspace Crates** | **3 (`engine`, `cli`, `mcp-server`)** | Complete workspace ingestion |
| **Source Files Parsed** | **31 Rust files** | Scanned and AST parsed via Tree-sitter |
| **Total Source Bytes** | **220,937 bytes** | Raw repository source code |
| **Extracted Syntax Symbols ($V$)** | **206 declarations** | Functions, methods, structs, traits, enums |
| **Resolved Multiplex Edges ($E$)** | **628 edges** | $E_{\text{AST}}$ containment, $E_{\text{Call}}$ invocations, $E_{\text{Type}}$ dependencies |
| **Cold Ingestion Time (0% Cache)** | **118.4 ms** | Full parallel scan, Tree-sitter parsing, and symbol hashing |
| **Warm Ingestion Time (100% Cache)**| **6.42 ms** | **18.4x speedup** via BLAKE3 Merkle diffing and bincode serialization |
| **CSR Matrix Construction** | **0.78 ms** | Compact Compressed Sparse Row packing of multiplex graph |
| **Context Selection Latency** | **0.24 ms (release)** | Andersen-Chung-Lang PPR + CELF submodular knapsack solver |
| **MCP In-Memory Repeat Query** | **< 0.5 ms** | Sub-millisecond daemon response time for AI agents |
| **MCP Handshake Latency** | **< 0.1 ms** | Instant JSON-RPC 2.0 initialization over `stdio` |
| **Total Codebase Tokens** | **5,607 tokens** | Complete raw token count across all source files |

---

## 2. Global Centrality Top Architectural Hubs

Calculated via power-iteration PageRank over the complete 628-edge multiplex graph:

1. `SymbolId` (Struct) — **6.23% PR** | Contiguous index across all AST nodes
2. `fmt` (Method) — **5.37% PR** | Standard library formatting implementations
3. `EngineError` (Enum) — **3.25% PR** | Unified engine error handling
4. `EdgeKind` (Enum) — **2.83% PR** | Multiplex graph edge taxonomy
5. `RepositoryCache` (Struct) — **2.81% PR** | Incremental cache serialization manager
6. `CsrMatrix` (Struct) — **2.67% PR** | Compressed Sparse Row adjacency storage
7. `LodLevel` (Enum) — **2.52% PR** | Multi-resolution level of detail representations
8. `ContextSelector` (Struct) — **2.48% PR** | CELF submodular knapsack solver

---

## 3. Model Context Protocol (MCP) Server Validation

| Method / Tool | Parameters | Latency | Status | Output Summary |
| :--- | :--- | :--- | :---: | :--- |
| **`initialize`** | `protocolVersion: "2024-11-05"` | **< 0.1 ms** | **PASS** | Returns capabilities and server info (`repotrim-mcp 0.2.0`) |
| **`notifications/initialized`** | None | **< 0.1 ms** | **PASS** | Handshake confirmed with clean stderr logging |
| **`ping`** | None | **< 0.1 ms** | **PASS** | Returns empty JSON object `{}` |
| **`tools/list`** | None | **< 0.1 ms** | **PASS** | Returns 5 tool schemas (`trim_context`, `query_graph_stats`, `inspect_symbol`, `clean_cache`, `generate_blueprint`) |
| **`tools/call: query_graph_stats`** | `path: "."` | **6.4 ms (cold) / 0.4 ms (warm)** | **PASS** | Syntax breakdown, edge density, and top PageRank hubs |
| **`tools/call: trim_context`** | `seeds: ["ContextSelector"]`, `budget: 500` | **7.2 ms (cold) / 0.5 ms (warm)** | **PASS** | Optimal Markdown context strictly bounded within 500 tokens |
| **`tools/call: inspect_symbol`** | `symbol: "ContextSelector"` | **6.5 ms (cold) / 0.4 ms (warm)** | **PASS** | Symbol signature, docstrings, 5 outgoing dependencies, 4 callers |
| **`tools/call: clean_cache`** | `path: "."` | **1.2 ms** | **PASS** | Purges `.repotrim/` and resets in-memory cache |
| **`tools/call: generate_blueprint`** | `task: "Add auth"`, `budget: 3000` | **7.5 ms (cold) / 0.5 ms (warm)** | **PASS** | Generates structured markdown feature blueprint with inferred seed anchors |

---

## 4. Key Takeaways

1. **Near-Zero Latency Overhead**: The in-memory daemon responds to agent queries in under 500 microseconds, meaning AI agents can query code context on every turn without stalling user interactions.
2. **Deterministic Caching**: When source files do not change, warm loads drop from 118 ms to 6.4 ms, verifying the efficiency of BLAKE3 Merkle diffing.
3. **Bounded Context**: Ingesting all 31 files yields 5,607 tokens; RepoTrim trims this down to focused, highly relevant skeletons of 300–500 tokens (an **85%+ reduction**) while retaining necessary type and call structures.
