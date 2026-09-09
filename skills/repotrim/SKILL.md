---
name: repotrim
description: Extract mathematically optimal, token-budgeted code context for AI coding agents using Code Property Graph Personalized PageRank and CELF submodular knapsack optimization.
---

# RepoTrim Agent Skill

RepoTrim packs the highest-value code context into your token budget in sub-millisecond time. Instead of reading whole files or dumping directories into prompts, use RepoTrim to obtain precise, topologically relevant code skeletons.

---

## The 3-Tier Context Funnel

When working on any non-trivial coding task, follow the **3-Tier Context Funnel** to maintain maximum reasoning quality and prevent context window exhaustion:

```
┌─────────────────────────────────────────────────────────────┐
│  Tier 1: Feature Blueprint / Task Objective                │
│  High-density specification (~200 tokens)                   │
└──────────────────────────────┬──────────────────────────────┘
                               │
                               ▼
┌─────────────────────────────────────────────────────────────┐
│  Tier 2: RepoTrim Context Skeleton (via trim_context)       │
│  Topologically relevant callers, callees, types & LOD slices│
│  Budget: 1,000 – 4,000 tokens                               │
└──────────────────────────────┬──────────────────────────────┘
                               │
                               ▼
┌─────────────────────────────────────────────────────────────┐
│  Tier 3: Target Working File                                │
│  Full source file for the exact module being modified       │
└─────────────────────────────────────────────────────────────┘
```

1. **Tier 1 (Task Definition)**: Define the objective or use `generate_blueprint` / `repotrim blueprint` to establish seed anchors and acceptance criteria.
2. **Tier 2 (Dependency Skeleton)**: Query RepoTrim via `trim_context` with your task query or key symbols. This delivers all interconnected types, function signatures, and method skeletons.
3. **Tier 3 (Target File)**: Read only the immediate file you intend to modify in full. Never read surrounding dependency files in full—RepoTrim's Tier 2 skeleton already provides their exact signatures and types.

---

## When to Use RepoTrim vs. Reading Whole Files

| Scenario | What to Do | Tool Call / Command |
|---|---|---|
| Repository onboarding / Architectural orientation | **Use RepoTrim** architecture tool | `generate_architecture_docs()` or `repotrim architecture` |
| Exploring an unfamiliar codebase | **Use RepoTrim** with natural language query | `trim_context(query: "auth token expiration", budget: 3000)` |
| Checking signatures of dependencies | **Use RepoTrim** with symbol seed | `trim_context(seeds: ["ContextSelector", "PprSolver"], budget: 1500)` |
| Inspecting uncommitted git modifications | **Use RepoTrim** with git diff | `trim_context(fromDiff: true, budget: 2000)` |
| Understanding architectural hubs | **Use RepoTrim** graph stats | `query_graph_stats()` |
| Deeply inspecting a single struct/function | **Use RepoTrim** inspect tool | `inspect_symbol(symbol: "MultiplexGraph")` |
| Active pair-programming / live sync | **Run RepoTrim watch daemon** | `repotrim watch` |
| Writing the actual code change | **Read whole file** for the target file only | `view_file(path: "target_file.rs")` |

---

## Token Budget Sizing Heuristics

When invoking `trim_context`, set the `budget` parameter dynamically based on task scope:

- **Auto-Budgeting (`budget: "auto"`)**:
  - Automatically identifies the optimal token budget by applying the Kneedle algorithm (Satopää et al., 2011) to the CELF knapsack cumulative marginal utility curve.
  - Halts context extraction precisely at the knee point where additional code symbols provide diminishing returns.
  - Can be paired with `model: "claude" | "gpt-4o" | "deepseek" | "ollama"` to tune sensitivity and token bounds to the specific LLM architecture.
- **1,000 tokens (Quick Verification)**:
  - Verifying function signatures, parameter types, or return types.
  - Checking interface / trait contracts.
- **3,000 tokens (Standard Feature / Bug Fix - Recommended)**:
  - Typical bug fixes, adding new methods, or writing unit tests.
  - Captures 10–30 relevant symbols across 3–8 files with Level-of-Detail (LOD) control-flow outlines.
- **6,000 tokens (Architectural Refactoring)**:
  - Cross-crate refactoring, renaming core data structures, or breaking API changes.
  - Captures broad dependency neighborhoods and multiple caller/callee layers.

---

## Interpreting Multi-Resolution Level of Detail (LOD)

RepoTrim formats code into 4 discrete Levels of Detail to pack maximum information into your budget:

| LOD Level | Name | Description & Example |
|---|---|---|
| **LOD 0** | `SignatureOnly` | Minimal signature declaration: `pub fn calculate(val: f64) -> f64;` |
| **LOD 1** | `SignatureAndDoc` | Signature preceded by docstrings and type annotations. |
| **LOD 2** | `SlicedBody` | Control-flow outline showing branches (`if`, `for`, `match`, `try/except`, `return`) with implementation bodies collapsed into comments or `pass`. |
| **LOD 3** | `FullBody` | Complete, verbatim function or class implementation. |

> [!IMPORTANT]
> **Do NOT "fix" or hallucinate sliced code.**
> Sliced lines marked with `// ... [sliced] ...`, `# ... [sliced] ...`, or `pass` are deliberate topological abstractions generated by RepoTrim to save your context tokens. They represent real, valid code in the repository. Do not attempt to reimplement them or assume they are syntax errors.

---

## Available MCP Tools

When connected to `repotrim-mcp`, the following tools are available:

### 1. `trim_context`
Extracts mathematically optimal prompt context across Rust, Python, TypeScript/JavaScript, and Go codebases.
- **Parameters**:
  - `query` *(optional string)*: Natural language intent (e.g. `"JWT verification expiration"`). Leverages dense-sparse semantic hybrid retrieval to resolve zero-overlap conceptual queries (e.g. "credentials password storage") to relevant implementations.
  - `seeds` *(optional array of strings)*: Explicit symbol names (e.g. `["ContextSelector"]`).
  - `fromDiff` *(optional boolean)*: If `true`, infers seeds from uncommitted git changes.
  - `budget` *(optional integer or `"auto"`, default: 1000)*: Maximum token budget, or `"auto"` to activate Kneedle auto-budgeting.
  - `model` *(optional string)*: Target model profile for auto-budgeting (`"claude"`, `"gpt-4o"`, `"deepseek"`, `"ollama"`).
  - `path` *(optional string, default: ".")*: Target repository directory.
  - `format` *(optional string: `"markdown"` or `"json"`)*: Output format.

### 2. `query_graph_stats`
Returns codebase inventory, syntax entity counts, and global PageRank architectural hubs.

### 3. `inspect_symbol`
Inspects a specific symbol's declaration, token cost, outgoing dependencies with transition weights, and incoming callers.

### 4. `clean_cache`
Clears the `.repotrim/` incremental AST cache to force a fresh re-scan.

### 5. `generate_blueprint`
Generates a structured `FEATURE_BLUEPRINT.md` pre-populated with inferred seeds and target files for a given task description.

### 6. `generate_architecture_docs`
Generates durable, evergreen repository architecture documentation with modular subsystem clustering, 4-tier layer classification, central PageRank hubs, and Mermaid diagrams.

---

## Live Synchronization Daemon (`repotrim watch`)

During active coding sessions with frequent file changes, run the watch daemon in a background terminal:

```bash
repotrim watch
```

- **Sub-Millisecond Incremental Patching**: Watches filesystem modifications using `notify` with 75ms debouncing and re-parses modified files in <2 ms without rescanning the whole repository.
- **Zero-Latency In-Memory Graph**: Keeps `.repotrim/cache.bin` and the in-memory graph hot, so MCP tool queries (`trim_context`, `inspect_symbol`) return immediately with zero disk I/O latency.

