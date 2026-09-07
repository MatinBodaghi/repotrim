# Empirical Dogfooding Benchmark: Phase 7

- **Phase Evaluated:** Phase 7 (Model Context Protocol / MCP Server Integration over `stdio`)
- **Target Codebase:** Entire RepoTrim Workspace (`crates/engine/` + `crates/cli/` + `crates/mcp-server/`)
- **Date:** 2026-09-07

---

## 1. Engine & Graph Ingestion Metrics (Phase 5 vs. Phase 6 vs. Phase 7 Comparison)

| Metric | Phase 5 Value (Release) | Phase 6 Value (Release) | Phase 7 Value (Release) | Overall Evolution |
| :--- | :--- | :--- | :--- | :--- |
| **Workspace Crates** | 2 (`engine`, `cli`) | 2 (`engine`, `cli`) | **3 (`engine`, `cli`, `mcp-server`)** | Full MCP server product |
| **Source Files Parsed** | 20 Rust files | 22 Rust files | **31 Rust files** | +40.9% codebase scope |
| **Total Source Bytes** | 141,331 bytes | 161,220 bytes | **220,937 bytes** | +37.0% codebase size |
| **Symbols Extracted ($V$)** | 140 declarations | 162 declarations | **206 declarations** | +27.2% symbol inventory |
| **Resolved Graph Edges ($E$)** | 415 edges | 496 edges | **628 edges** | **+26.6% connectivity** |
| **Cold Ingestion Time** | 80.6 ms | 96.2 ms | **118.4 ms** | Full 31-file AST parsing |
| **Warm Ingestion Time** | N/A | 6.08 ms (100% cache) | **6.42 ms (100% cache)** | **18.4x faster** than cold |
| **MCP In-Memory Repeat Call**| N/A | N/A | **<0.5 ms** | Sub-millisecond daemon |
| **MCP Handshake Latency** | N/A | N/A | **<0.1 ms** | Instant JSON-RPC response |
| **Graph Build Time** | 508.8 µs | 614.8 µs | **780.2 µs (0.78 ms)** | <1 ms across 628 edges |
| **Selection Latency** | 139.7 µs | 222.3 µs | **241.5 µs (0.24 ms)** | High-speed CELF solver |
| **Total Codebase Tokens** | 3,791 tokens | 4,518 tokens | **5,607 tokens** | Complete agent platform |

---

## 2. MCP Server Protocol Validation

### Dual Access Architecture
1. **Standalone Binary**: `repotrim-mcp` (`crates/mcp-server`)
2. **Unified CLI Command**: `repotrim mcp [--path <DIR>]` (`crates/cli`)

Both access patterns stream JSON-RPC 2.0 messages over `stdin` and `stdout`, routing all diagnostics, logs, and progress indicators strictly to `stderr`.

### Validated Protocol Endpoints & Tool Handlers

| Method / Tool | Parameters | Latency | Status | Output Summary |
| :--- | :--- | :--- | :--- | :--- |
| **`initialize`** | `protocolVersion: "2024-11-05"` | **< 0.1 ms** | **PASS** | Returns capabilities (`tools: {}`), serverInfo `repotrim-mcp 0.1.0` |
| **`notifications/initialized`** | None (notification) | **< 0.1 ms** | **PASS** | Client handshake completion logged to `stderr`, zero stdout pollution |
| **`ping`** | None | **< 0.1 ms** | **PASS** | Returns empty JSON object `{}` |
| **`tools/list`** | None | **< 0.1 ms** | **PASS** | Returns 4 tool schemas (`trim_context`, `query_graph_stats`, `inspect_symbol`, `clean_cache`) |
| **`tools/call: query_graph_stats`** | `path: "."` | **6.4 ms (first) / 0.4 ms (warm)** | **PASS** | Formats syntax inventory, edge connectivity, and Top 8 Global PageRank hubs |
| **`tools/call: trim_context`** | `seeds: ["ContextSelector"]`, `budget: 500` | **7.2 ms (first) / 0.5 ms (warm)** | **PASS** | Extracts optimal Markdown context within 500 tokens across 10 files |
| **`tools/call: trim_context` (JSON)** | `seeds: ["ContextSelector"]`, `format: "json"` | **7.1 ms (first) / 0.5 ms (warm)** | **PASS** | Returns structured JSON with symbol IDs, token costs, spans, and markdown |
| **`tools/call: inspect_symbol`** | `symbol: "ContextSelector"` | **6.5 ms (first) / 0.4 ms (warm)** | **PASS** | Returns symbol signature, docstrings, 5 outgoing edges, and 4 callers |
| **`tools/call: clean_cache`** | `path: "."` | **1.2 ms** | **PASS** | Removes `.repotrim/` directory and clears daemon in-memory cache |
| **Error Handling** | Invalid JSON / unknown tool | **< 0.1 ms** | **PASS** | Returns standard JSON-RPC `-32700`, `-32601`, or `isError: true` |

---

## 3. Global Centrality Top Architectural Hubs (Phase 7 Codebase)

Query: `query_graph_stats` across 31 files, 206 symbols, 628 edges:
1. `SymbolId` (Struct) — 6.23% PR | 1 edge
2. `fmt` (Method) — 5.37% PR | 1 edge
3. `EngineError` (Enum) — 3.25% PR | 0 edges
4. `EdgeKind` (Enum) — 2.83% PR | 0 edges
5. `RepositoryCache` (Struct) — 2.81% PR | 9 edges
6. `CsrMatrix` (Struct) — 2.73% PR | 8 edges
7. `JsonRpcError` (Struct) — 2.57% PR | 0 edges (Phase 7 new hub!)
8. `SymbolKind` (Enum) — 2.45% PR | 0 edges

---

## 4. Comparative Evolution Findings (Phase 3 $\to$ Phase 7)

```
Resolved Graph Connectivity (E):
Phase 3: [=====                               ] 146 edges
Phase 4: [===========                         ] 341 edges
Phase 5: [==============                      ] 415 edges
Phase 6: [================                    ] 496 edges
Phase 7: [=====================               ] 628 edges (+330.1% over Phase 3)

Warm Query Latency:
Cold Start:           [==============================] 118.4 ms
On-Disk Warm Start:   [==                            ] 6.42 ms
In-Memory MCP Daemon: [                              ] 0.45 ms (263x speedup!)

Product Evolution:
Phase 3: Mathematical Engine Core
Phase 4: Multi-Resolution Context Formatter
Phase 5: User Terminal CLI Tool (`repotrim.exe`)
Phase 6: Incremental BLAKE3 Merkle Disk Caching
Phase 7: Standard Model Context Protocol (MCP) Server for AI Agents
```

---

## 5. Roadmap Assessment & Recommendations for Phase 8

1. **Phase 7 Assessment**:
   - The MCP server operates reliably over `stdio` adhering strictly to JSON-RPC 2.0 and MCP specification `2024-11-05`.
   - In-memory repository caching inside the MCP daemon delivers **sub-millisecond (<1 ms)** responses for AI coding agents during interactive sessions.
   - All 50 workspace tests pass without failure, clippy warnings, or format regressions.

2. **Recommendations for Phase 8 (Empirical Benchmarking & crates.io Release)**:
   - Run empirical head-to-head benchmarks comparing RepoTrim against baseline strategies (Aider repomap, naive vector RAG, whole-file dumping) on standard benchmarks (e.g. SWE-bench or popular open-source repositories).
   - Prepare package manifests and documentation for public `crates.io` release:
     - `repotrim-engine` (library crate)
     - `repotrim-mcp` (MCP server binary and library crate)
     - `repotrim` (CLI tool)
