# Empirical Dogfooding Benchmark: Phase 6

- **Phase Evaluated:** Phase 6 (Incremental AST Merkle Diffing & Fast Binary Persistence)
- **Target Codebase:** Entire RepoTrim Workspace (`crates/engine/` + `crates/cli/`)
- **Date:** 2026-09-07

---

## 1. Engine & Graph Ingestion Metrics (Phase 4 vs. Phase 5 vs. Phase 6 Comparison)

| Metric | Phase 4 Value | Phase 5 Value (Release) | Phase 6 Value (Release) | Overall Evolution |
| :--- | :--- | :--- | :--- | :--- |
| **Workspace Crates** | 1 (`engine`) | 2 (`engine`, `cli`) | 2 (`engine`, `cli`) | Full binary product |
| **Source Files Parsed** | 12 Rust files | 20 Rust files | **22 Rust files** | +83.3% from Phase 4 |
| **Total Source Bytes** | 107,691 bytes | 141,331 bytes | **161,220 bytes** | +49.7% from Phase 4 |
| **Symbols Extracted ($V$)** | 116 declarations | 140 declarations | **162 declarations** | +39.7% symbol inventory |
| **Resolved Graph Edges ($E$)** | 341 edges | 415 edges | **496 edges** | **+45.5% connectivity** |
| **Cold Ingestion Time** | N/A | 80.6 ms | **96.2 ms** | Tree-sitter full AST parsing |
| **Warm Ingestion Time** | N/A | N/A | **6.08 ms (100% warm hit)** | **14.7x faster** ingestion |
| **Partial Recompute (1 edit)**| N/A | N/A | **21.8 ms (95.5% warm hit)**| Selective 1-file parse |
| **Graph Build Time** | 1,221.5 µs | 508.8 µs | **614.8 µs (0.6 ms)** | <1 ms across 496 edges |
| **Selection Latency** | 571.9 µs | 139.7 µs | **222.3 µs (0.22 ms)** | Sub-millisecond CELF solver |
| **Total Repository Tokens** | 2,946 tokens | 3,791 tokens | **4,518 tokens** | Comprehensive codebase |

---

## 2. Incremental Caching & Binary Persistence Validation

### Architectural Highlights of Phase 6 Implementation
1. **Per-File AST Fingerprinting (`FileCacheEntry`):**
   - Each source file caches its relative path, timestamp (`mtime_nanos`), BLAKE3 256-bit cryptographic digest, parsed symbols, and reference edges.
   - Reference edges maintain target identifiers (`target_ident: String`) rather than fragile hardcoded numeric IDs, allowing per-file isolation.
2. **Two-Tier Validation Hierarchy:**
   - **Tier 1 (Fast Path):** Exact `mtime_nanos` equality check (<0.1 ms across all workspace files, zero file reads).
   - **Tier 2 (Merkle Digest):** If timestamp changed (e.g. `git checkout` or `touch`), hashes file content with BLAKE3. If hash matches prior entry, timestamp is updated in-place without re-parsing.
   - **Tier 3 (AST Recompute):** Only files with modified digests trigger Tree-sitter parsing.
3. **Atomic Persistence:**
   - Saved to `.repotrim/cache.bin` via `bincode 1.3` with atomic file renaming (`cache.bin.tmp` $\to$ `cache.bin`), preventing torn writes during process termination.
4. **Deterministic Contiguous Re-Indexing:**
   - `RepositoryCache::compile_symbols_and_edges` assigns contiguous `SymbolId(0..N)` deterministically in lexicographical path order, immediately consumed by `MultiplexGraph::build`.

---

## 3. CLI Subcommand Benchmark Results

### Benchmark 1: Cold Start vs. Warm Start Ingestion
```
Cold Ingestion (0/22 cached):  [==============================] 96.27 ms
Warm Ingestion (22/22 cached): [==] 6.08 ms  (14.7x speedup!)
```
* **Cold Ingestion:** 96.27 ms to parse 22 files, 161 KB source text with Tree-sitter.
* **Warm Ingestion:** 6.08 ms to validate timestamps, deserialize cache via `bincode`, and re-index 162 symbols.
* **Cache File Size:** `.repotrim/cache.bin` is ~45 KB on disk.

### Benchmark 2: Selective Single-File Invalidation
When touching and editing 1 source file (`crates/cli/tests/cli_test.rs`):
* **Cache Diagnostic:** `Incremental Cache: 21/22 files cached (95.5% warm hit)`.
* **Execution Time:** **21.8 ms** (vs 96.2 ms cold start — a **4.4x speedup** on incremental edit).
* Unchanged 21 files are reused without touching the Tree-sitter parser.

### Benchmark 3: `repotrim select` Context Extraction
Command: `repotrim select --seed ContextSelector --budget 1000`
* **Warm Ingestion:** 7.4 ms (22/22 warm hits)
* **PPR & CELF Knapsack:** 222.3 µs (0.22 ms)
* **Tokens Extracted:** 998 / 1000 tokens (99.8% budget utilization)
* **Symbols Selected:** 48 symbols across 10 files (`cache.rs`, `celf.rs`, `csr.rs`, `error.rs`, `formatter.rs`, `graph.rs`, `ppr.rs`, `resolver.rs`, `selector.rs`, `symbol.rs`).
* **Token Reduction:** **77.9% reduction** from 4,518 down to 998 tokens.

### Benchmark 4: Global Centrality Top Architectural Hubs
Command: `repotrim stats`
1. `SymbolId` (Struct) — 7.71% PR | 1 edge
2. `fmt` (Method) — 6.65% PR | 1 edge
3. `EngineError` (Enum) — 3.85% PR | 0 edges
4. `EdgeKind` (Enum) — 3.50% PR | 0 edges
5. `RepositoryCache` (Struct) — **3.48% PR | 9 edges** (Phase 6 new hub!)
6. `CsrMatrix` (Struct) — 3.40% PR | 8 edges
7. `SymbolKind` (Enum) — 3.03% PR | 0 edges
8. `repo_root` (Function) — 2.23% PR | 0 edges
9. `MultiplexGraph` (Struct) — 2.23% PR | 15 edges
10. `new` (Method) — 2.03% PR | 1 edge

### Benchmark 5: Cache Management Subcommands
* `repotrim clean`: Removes `.repotrim/` cache directory cleanly.
* `--no-cache`: Bypasses `.repotrim/cache.bin` on `select`, `stats`, and `inspect` commands for testing and debugging.

---

## 4. Comparative Evolution Findings (Phase 3 $\to$ Phase 6)

```
Resolved Graph Connectivity (E):
Phase 3: [=======                             ] 146 edges
Phase 4: [=================                   ] 341 edges
Phase 5: [=====================               ] 415 edges
Phase 6: [=========================           ] 496 edges (+239.7% over Phase 3)

Warm Ingestion Latency:
Cold Start: [==============================] 96.2 ms
Warm Start: [==                            ] 6.08 ms (14.7x faster)

Mathematical Pipeline Latency:
Graph Build:     0.61 ms
CELF Knapsack:   0.22 ms
Total Core Math: 0.83 ms
```

---

## 5. Roadmap Assessment & Recommendations for Phase 7

1. **Phase 6 Success Assessment:**
   - The caching subsystem achieves 100% cache hit rates on warm invocations, slashing CLI response times down to **6 ms**.
   - Selective invalidation correctly isolates single-file edits in ~21 ms.
   - Zero test regressions across 45 unit and integration tests.

2. **Recommendations for Phase 7 (Model Context Protocol / MCP Server Integration):**
   - With warm CLI queries operating in single-digit milliseconds, RepoTrim is primed for daemon/agent RPC integration.
   - In **Phase 7**, implement the standard Model Context Protocol (MCP) server over `stdio` using `rmcp` or JSON-RPC 2.0.
   - Expose tools: `trim_context(seeds, budget, file_path)`, `query_graph_stats()`, and `inspect_symbol(name)`.
   - AI agent tools (e.g. Claude Desktop, Cursor, Gemini Antigravity) will be able to query RepoTrim directly in real-time as an active coding assistant plugin.
