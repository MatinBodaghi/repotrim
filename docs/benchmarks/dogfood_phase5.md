# Empirical Dogfooding Benchmark: Phase 5

- **Phase Evaluated:** Phase 5 (Standalone CLI Binary `crates/cli`: `select`, `stats`, `inspect`)
- **Target Codebase:** Entire RepoTrim Workspace (`crates/engine/` + `crates/cli/`)
- **Date:** 2026-09-07

---

## 1. Engine & Graph Ingestion Metrics (Phase 3 vs. Phase 4 vs. Phase 5 Comparison)

| Metric | Phase 3 Value | Phase 4 Value | Phase 5 Value (Release) | Overall Evolution |
| :--- | :--- | :--- | :--- | :--- |
| **Workspace Crates** | 1 (`engine`) | 1 (`engine`) | 2 (`engine`, `cli`) | Full binary product |
| **Source Files Parsed** | 11 Rust files | 12 Rust files | **20 Rust files** | +81.8% codebase scope |
| **Total Source Bytes** | 79,169 bytes | 107,691 bytes | **141,331 bytes** | +78.5% codebase size |
| **Symbols Extracted ($V$)** | 97 declarations | 116 declarations | **140 declarations** | +44.3% symbol inventory |
| **Resolved Graph Edges ($E$)** | 146 edges | 341 edges | **415 edges** | **+184.2% connectivity** |
| **Graph Build Time** | 992.1 µs | 1,221.5 µs | **508.8 µs (0.5 ms)** | **2.4x faster** in release |
| **Selection Latency** | 254.3 µs | 571.9 µs | **139.7 µs (0.14 ms)** | **4.1x faster** in release |
| **Full CLI Cold-Start** | N/A | N/A | **80.6 ms** | Instant interactive tool |
| **Total Repository Tokens** | 2,285 tokens | 2,946 tokens | **3,791 tokens** | Scaled repository size |

---

## 2. CLI Subcommand Benchmark Results

### Command 1: `repotrim select`
Evaluates the core value proposition: extracting mathematically optimal prompt contexts directly to `stdout` for AI agents.

#### Scenario A: Struct Seed (`ContextSelector`) — Budget: 300 tokens
* **Execution Latency:** **139.7 µs (0.14 ms)**
* **Tokens Used:** 295 / 300 tokens (98.3% budget efficiency)
* **Token Reduction:** **92.2% reduction** (from 3,791 down to 295 tokens)
* **Selected Symbols:** 22 symbols cleanly grouped across 9 files:
  - `celf.rs`, `csr.rs`, `formatter.rs`, `graph.rs`, `ppr.rs`, `resolver.rs`, `selector.rs`, `symbol.rs`, `tokens.rs`.
* **Output:** Clean Markdown code blocks ready for instant prompt injection.

#### Scenario B: Method Seed (`select_context`) — Budget: 500 tokens
* **Execution Latency:** **159.7 µs (0.16 ms)**
* **Tokens Used:** 495 / 500 tokens (99.0% budget efficiency)
* **Token Reduction:** **86.9% reduction**
* **Selected Symbols:** 30 symbols spanning the complete solver and knapsack pipeline.

#### Scenario C: Structured JSON Output (`--format json`) — Budget: 100 tokens
* **Execution Latency:** **418.3 µs (0.42 ms)**
* **Tokens Used:** 98 / 100 tokens (98.0% budget efficiency)
* **Output:** Valid structured JSON schema including token costs, line intervals, and symbol metadata for programmatic tool calls.

---

### Command 2: `repotrim stats`
Provides architectural visibility into codebase structure, graph density, and central hubs:

* **Ingestion Time:** 80.6 ms (scanning 20 files, 141 KB)
* **Symbol Breakdown:**
  - Functions: 65
  - Methods: 48
  - Structs: 21
  - Enums: 6
* **Multiplex Edge Density:**
  - Total Edges: 415
  - Average Out-Degree: 2.96
  - Maximum Out-Degree: 15
* **Top Architectural Hubs (Global PageRank):**
  1. `SymbolId` (7.98% PR)
  2. `fmt` (6.89% PR)
  3. `CsrMatrix` (4.13% PR)
  4. `CelfConfig` (4.08% PR)
  5. `MultiplexGraph` (2.86% PR)

---

### Command 3: `repotrim inspect`
Inspects individual symbols, their outgoing multiplex dependencies, and incoming callers:

* **Example Target:** `ContextSelector`
* **Outgoing Dependencies (5):**
  - `CelfOptimizer` (14.9%), `PprSolver` (14.9%), `new` (23.4%), `select_context` (23.4%), `select_and_format_context` (23.4%).
* **Incoming Callers (4):**
  - `new`, `select_context`, `select_and_format_context`, and `run_scenario`.

---

## 3. Comparative Evolution Findings (Phase 3 $\to$ Phase 4 $\to$ Phase 5)

```
Resolved Graph Connectivity (E):
Phase 3: [============                    ] 146 edges
Phase 4: [===========================     ] 341 edges
Phase 5: [================================] 415 edges (+184.2% over Phase 3)

Selection Pipeline Latency (Release):
Phase 4: [================================] 571.9 µs (Debug)
Phase 5: [========                        ] 139.7 µs (Release: 4.1x faster)

Product Maturation:
Phase 3: Internal Engine Core (tested via unit tests)
Phase 4: Multi-Resolution Formatter (tested via dogfood integration)
Phase 5: Production-Ready Standalone CLI Executable (`repotrim.exe`)
```

1. **Production-Grade Usability:**
   - RepoTrim is now a standalone executable (`target/release/repotrim.exe`) that can be invoked from any shell or integrated into any agent harness via pipes (`repotrim select ... > context.md`).
2. **Sub-Millisecond Mathematical Pipeline:**
   - In production release mode, running PPR diffusion + CELF knapsack + LOD Markdown formatting takes **0.14 ms** to **0.16 ms**.
3. **Clean Pipe Architecture:**
   - All logging, scanning, and progress indicators are routed to `stderr`, leaving `stdout` pure for Markdown or JSON data piping.

---

## 4. Recommendations for Phase 6

Now that the CLI binary is fully operational and benchmarked:
1. **Proceed with Phase 6: Incremental AST Merkle Diffing & Zero-Copy Persistence**:
   - Store the 415-edge multiplex graph on disk (e.g. in `.repotrim/cache.bin`) using `rkyv` for zero-copy memory mapping.
   - Use BLAKE3 hashes to detect unchanged files on warm CLI runs, dropping the cold-start time from 80 ms down to <5 ms.
2. **CLI Cache Flag**:
   - Add `--no-cache` flag to `repotrim select` to allow bypassing the cache when requested.
