# Model Context Protocol (MCP) Tool Reference

`repotrim-mcp` implements a high-performance Model Context Protocol (MCP) server over standard input/output (`stdio`) adhering to the JSON-RPC 2.0 specification (`protocolVersion: 2024-11-05`). It enables AI coding agents—such as Claude Code, Cursor, Antigravity, and custom agentic frameworks—to intelligently query, navigate, inspect, and slice large codebases without flooding agent context windows.

---

## 1. Architecture & Design Principles

### 1.1 Token-Efficient Handshake
Traditional MCP servers declare dozens of verbose tools with deeply nested schemas and expansive docstrings, consuming 3,000–5,000 tokens during the initial `tools/list` handshake. In conversational agents, this overhead persists across every subsequent turn.

`repotrim-mcp` is engineered for extreme token frugality:
- **Consolidated Surface:** 15 fragmented tools are unified into **8 streamlined primitives**.
- **Compact Schemas:** Concise descriptions and minimal schema definitions lower the `tools/list` handshake payload to **< 670 BPE tokens** (`cl100k_base`), an **80.3% reduction** over uncompressed baselines.
- **Zero-Breakage Backward Compatibility:** All 15 legacy tool names remain supported via dispatch aliases in `execute_tool`. Existing agent prompts, configs, and automation scripts continue functioning without disruption.

### 1.2 Security & Sandboxing
Every tool call enforces directory boundary checks via `RootGuard`:
- Relative paths are resolved against the declared repository root.
- Path traversal attacks (`../`) targeting parent directories are rejected with security errors.
- Symlinks pointing outside the repository tree are blocked.
- Git arguments are sanitized to block arbitrary flag injections.

---

## 2. Consolidated Tool Catalog

Below are the 8 active tools declared in `tools/list`:

### 2.1 `trim_context`
Extracts optimal, graph-conditioned context slices adhering to a strict token budget. Combines Personalized PageRank (PPR) with a fractional knapsack solver to rank and pack code symbols and file definitions.

- **Parameters:**
  | Parameter | Type | Required | Default | Description |
  | :--- | :--- | :--- | :--- | :--- |
  | `seeds` | `array<string>` | No | `[]` | Seed symbol names or file paths |
  | `task` | `string` | No | `""` | Natural language task description for auto-seeding |
  | `budget` | `integer \| "auto"` | No | `2000` | Target token budget or `"auto"` for knee-curve tuning |
  | `model` | `string` | No | `"cl100k"` | Target tokenizer architecture (`claude`, `gpt-4o`, `cl100k`) |
  | `format` | `string` | No | `"markdown"` | Output format: `"markdown"` or `"json"` |
  | `path` | `string` | No | `"."` | Target repository directory |

- **Example Call:**
  ```json
  {
    "name": "trim_context",
    "arguments": {
      "task": "Add AST symbol extraction for Rust",
      "budget": 1500
    }
  }
  ```

---

### 2.2 `find_symbols`
A unified search, discovery, and inspection primitive. Operates across three modalities:
1. **Search Modality (`query`):** Performs lexical and dense substring matching across all symbols.
2. **Inspect Modality (`symbol`):** Retrieves exact declaration signatures, docstrings, line numbers, and incoming/outgoing call references for a symbol.
3. **Locate Modality (`locate: true`):** Uses BM25-like task relevance scoring to identify candidate entrypoint symbols for an engineering task.

- **Parameters:**
  | Parameter | Type | Required | Default | Description |
  | :--- | :--- | :--- | :--- | :--- |
  | `query` | `string` | No | `""` | Search query or task description |
  | `symbol` | `string` | No | `""` | Specific symbol name to inspect |
  | `locate` | `boolean` | No | `false` | Whether to rank entrypoints by task relevance |
  | `limit` | `integer` | No | `10` | Maximum number of results to return |
  | `format` | `string` | No | `"markdown"` | Output format: `"markdown"` or `"json"` |
  | `path` | `string` | No | `"."` | Target repository directory |

- **Example Call (Inspect):**
  ```json
  {
    "name": "find_symbols",
    "arguments": {
      "symbol": "ContextSelector"
    }
  }
  ```

---

### 2.3 `analyze_graph`
Unifies graph topology inspection, Louvain community detection, and Git co-edit temporal coupling into a single analytical tool.

- **Parameters:**
  | Parameter | Type | Required | Default | Description |
  | :--- | :--- | :--- | :--- | :--- |
  | `aspect` | `string` | No | `"stats"` | Mode: `"stats"`, `"communities"`, or `"coedits"` |
  | `resolution` | `number` | No | `1.0` | Modularity resolution parameter $\gamma$ for community clustering |
  | `maxCommits` | `integer` | No | `100` | Maximum Git commit history depth for co-edit mining |
  | `minSupport` | `integer` | No | `3` | Minimum co-occurrence count threshold |
  | `path` | `string` | No | `"."` | Target repository directory |

- **Example Call (Community Detection):**
  ```json
  {
    "name": "analyze_graph",
    "arguments": {
      "aspect": "communities",
      "resolution": 1.2
    }
  }
  ```

---

### 2.4 `analyze_impact`
Calculates the downstream blast radius and transitive dependencies affected by modifying a given symbol or file.

- **Parameters:**
  | Parameter | Type | Required | Default | Description |
  | :--- | :--- | :--- | :--- | :--- |
  | `symbol` | `string` | No | `""` | Target symbol name to evaluate |
  | `file` | `string` | No | `""` | Target file path to evaluate |
  | `depth` | `integer` | No | `3` | Maximum transitive traversal depth |
  | `path` | `string` | No | `"."` | Target repository directory |

- **Example Call:**
  ```json
  {
    "name": "analyze_impact",
    "arguments": {
      "symbol": "SymbolGraph"
    }
  }
  ```

---

### 2.5 `trace_paths`
Discovers and ranks multi-hop causal execution paths between source and target symbols using Boltzmann path energy scoring, rendering Mermaid sequence diagrams.

- **Parameters:**
  | Parameter | Type | Required | Default | Description |
  | :--- | :--- | :--- | :--- | :--- |
  | `source` | `string` | Yes | — | Source entrypoint symbol name |
  | `target` | `string` | Yes | — | Target destination symbol name |
  | `task` | `string` | No | `""` | Task description to condition path scoring |
  | `includeMermaid` | `boolean` | No | `true` | Include Mermaid sequence diagram in output |
  | `path` | `string` | No | `"."` | Target repository directory |

- **Example Call:**
  ```json
  {
    "name": "trace_paths",
    "arguments": {
      "source": "McpServer::run_loop",
      "target": "execute_tool"
    }
  }
  ```

---

### 2.6 `navigate_codebase`
Autonomously explores the codebase using an adaptive submodular greedy policy (Golovin & Krause, 2011) to discover relevant symbols, causal execution paths, and typed context within a strict token ceiling.

- **Parameters:**
  | Parameter | Type | Required | Default | Description |
  | :--- | :--- | :--- | :--- | :--- |
  | `query` | `string` | Yes | — | Natural language task prompt or issue description |
  | `budget` | `integer` | No | `2000` | Token budget ceiling for exploration output |
  | `maxSteps` | `integer` | No | `15` | Maximum exploration steps |
  | `path` | `string` | No | `"."` | Target repository directory |

- **Example Call:**
  ```json
  {
    "name": "navigate_codebase",
    "arguments": {
      "query": "How are MCP tools dispatched and confined?",
      "budget": 1200
    }
  }
  ```

---

### 2.7 `generate_blueprint`
Produces a high-level architectural blueprint mapping subsystem boundaries, dependencies, and entrypoints.

- **Parameters:**
  | Parameter | Type | Required | Default | Description |
  | :--- | :--- | :--- | :--- | :--- |
  | `task` | `string` | Yes | — | Engineering task description |
  | `budget` | `integer \| "auto"` | No | `3000` | Recommended token budget for context slicing |
  | `path` | `string` | No | `"."` | Target repository directory |

- **Example Call:**
  ```json
  {
    "name": "generate_blueprint",
    "arguments": {
      "task": "Migrate database pool to asynchronous connection manager"
    }
  }
  ```

---

### 2.8 `generate_architecture_docs`
Generates comprehensive repository architecture documentation (`ARCHITECTURE.md`) including subsystem topology, architectural layers, central hubs, Mermaid diagrams, and public API inventories.

- **Parameters:**
  | Parameter | Type | Required | Default | Description |
  | :--- | :--- | :--- | :--- | :--- |
  | `path` | `string` | No | `"."` | Target repository directory |
  | `output` | `string` | No | `""` | Destination file path (e.g. `docs/ARCHITECTURE.md`) |
  | `resolution` | `number` | No | `1.0` | Modularity resolution parameter $\gamma$ |

- **Example Call:**
  ```json
  {
    "name": "generate_architecture_docs",
    "arguments": {
      "output": "docs/ARCHITECTURE.md"
    }
  }
  ```

---

## 3. Backward-Compatible Legacy Aliases

To guarantee that existing client prompts and integrations do not break, `repotrim-mcp` automatically dispatches the following legacy tool names:

| Legacy Tool Name | Consolidated Target | Parameter Mapping / Behavior |
| :--- | :--- | :--- |
| `inspect_symbol` | `find_symbols` | Routed to inspect handler with symbol signature, docstring, and refs |
| `search_symbols` | `find_symbols` | Lexical & dense search query |
| `locate_entrypoints` | `find_symbols` | Task relevance entrypoint ranking |
| `query_graph_stats` | `analyze_graph` | Graph node/edge counts and PageRank hubs |
| `detect_communities` | `analyze_graph` | Modularity clustering and community partition |
| `mine_coedits` | `analyze_graph` | Git commit co-occurrence mining |
| `expand_symbol` | `trim_context` | Context expansion around a specific seed symbol |
| `run_benchmark` | Internal | MCP benchmark evaluation suite |
| `clean_cache` | Internal | Cache purge and re-indexing |

---

## 4. Best Practices for Coding Agents

1. **Initial Exploration:** Start by querying `find_symbols` with a task prompt or `navigate_codebase` to discover relevant entrypoint symbols within a conservative budget (800–1,200 tokens).
2. **Impact & Safety Checks:** Before modifying foundational symbols, execute `analyze_impact` to assess ripple effects across dependent modules.
3. **Context Packing:** Once seeds are identified, call `trim_context` with an explicit token budget tailored to your model's reasoning window (e.g. 2,000–4,000 tokens).
4. **Architectural Overview:** For unfamiliar codebases, call `generate_blueprint` or `analyze_graph` with `aspect: "communities"` to understand macro modularity before reading implementation files.
