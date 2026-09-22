# Semantic Versioning & Interface Stability Policy

This document defines the official Semantic Versioning (SemVer 2.0.0) policy, public interface guarantees, deprecation lifecycle, and stability contracts for **RepoTrim**.

---

## 1. Versioning Specification (SemVer 2.0.0)

RepoTrim adheres strictly to [Semantic Versioning 2.0.0](https://semver.org/):

$$\text{Version} = \text{MAJOR}.\text{MINOR}.\text{PATCH}$$

- **MAJOR ($X.0.0$):** Incompatible API changes, breaking modifications to CLI syntax or flags, breaking changes to MCP tool input/output contracts, or removal of deprecated interfaces.
- **MINOR ($1.Y.0$):** Additions of new functionality, new CLI subcommands, new MCP tools, backwards-compatible schema extensions, or performance enhancements that preserve public contracts.
- **PATCH ($1.0.Z$):** Backwards-compatible bug fixes, security hardening, internal algorithm optimizations, and documentation improvements.

---

## 2. Public Contract Boundary

The following components constitute the **SemVer Guaranteed Public Contract** of RepoTrim starting with `v1.0.0`:

```text
┌─────────────────────────────────────────────────────────────────────────────┐
│                    SemVer Guaranteed Public Contract (v1.0.0)               │
├─────────────────────────────────────────────────────────────────────────────┤
│ • 8 MCP Tool Names & JSON Input Schemas                                     │
│ • CLI Command-Line Syntax, Subcommands, and Option Flags                    │
│ • Public Rust Crate APIs exposed by `repotrim-engine`                       │
│ • Structured JSON Output Schema (`-f json` / `--format json`)               │
└──────────────────────────────────────┬──────────────────────────────────────┘
                                       │
                                       │ Explicitly Excluded from SemVer Guarantees
                                       ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│                    Internal Implementation Details (Free to Evolve)         │
├─────────────────────────────────────────────────────────────────────────────┤
│ • Symbol Selection Heuristics & Ranking Tie-Breaking (Quality Enhancements) │
│ • Internal Crate Module Hierarchy & Private Structs                         │
│ • Log Output Formats and Internal Diagnostics (stderr / tracing)            │
│ • Empirical Benchmark Figures & Microsecond Timings                         │
└─────────────────────────────────────────────────────────────────────────────┘
```

### 2.1 Model Context Protocol (MCP) Tools
The 8 canonical MCP tools exposed by `repotrim-mcp` and their input schemas are guaranteed backwards-compatible:
1. `trim_context`
2. `find_symbols`
3. `analyze_graph`
4. `analyze_impact`
5. `trace_paths`
6. `navigate_codebase`
7. `generate_blueprint`
8. `generate_architecture_docs`

**Guarantees:**
- Tool names will not be renamed or removed within a major release series.
- Required input parameters will not be added without a major version bump.
- Optional parameters may be added in minor releases.
- Automated snapshot testing ([`crates/mcp-server/tests/tool_schema_snapshot_test.rs`](file:///crates/mcp-server/tests/tool_schema_snapshot_test.rs)) enforces that the serialized schema payload does not drift or exceed token budgets.

### 2.2 Command-Line Interface (CLI)
The CLI arguments, subcommands, and flags provided by the `repotrim` binary:
- Primary subcommands (`select`, `query`, `impact`, `trace`, `navigate`, `mcp`, `benchmark`, `graph`).
- Standard flags (`--budget`, `--format`, `--path`, `--json`, `--verbose`).
- Exit codes ($0$ for success, non-zero for failures according to standard sysexits conventions).

### 2.3 Structured Output (`--format json` / `-f json`)
The machine-readable JSON output emitted by CLI commands when `-f json` is passed:
- Top-level schema fields (`selected_nodes`, `budget_consumed`, `metrics`, `omitted_diagnostics`).
- Existing JSON keys will not be renamed or removed.
- Additive keys may be introduced in minor releases.

### 2.4 Public Rust Engine APIs (`repotrim-engine`)
Public traits, structs, and functions exported by `repotrim-engine`:
- `CodePropertyGraph` query APIs.
- Token accounting interfaces.
- Personalized PageRank (`ppr`) and CELF knapsack solver contracts.
- Language AST parsing traits (`LanguageParser`).

---

## 3. Explicitly Excluded from SemVer Guarantees

The following areas are considered implementation details and may evolve across minor or patch releases:

1. **Selection Heuristic Tie-Breaking & Quality Improvements:**
   Enhancements to PageRank damping convergence, submodular marginal gain tie-breaking, or container scoping heuristics that improve context relevance without altering public method signatures.
2. **Internal Module Layout:**
   Private functions, internal helper traits, and internal crate layouts not exported from root lib modules.
3. **Log & Trace Formatting:**
   `stderr` diagnostics emitted via `tracing` or `env_logger`. Automated tools must consume stdout via `--format json` or the MCP JSON-RPC protocol, never scraping human-readable terminal output.
4. **Empirical Benchmark Numbers:**
   Microsecond latencies, token counts, and memory numbers documented under `docs/benchmarks/`.

---

## 4. Deprecation & Removal Lifecycle

To provide predictability and zero downtime for AI agent workflows:

1. **Deprecation Notice:**
   Any public interface slated for deprecation will be explicitly marked as deprecated in at least **one minor release ($1.X.0$)** prior to removal.
2. **Compiler & Runtime Warnings:**
   - Rust APIs will carry `#[deprecated(since = "1.X.0", note = "...")]`.
   - CLI flags will emit a diagnostic warning to `stderr` explaining the replacement flag.
   - MCP tools or legacy aliases will include deprecation notices in tool descriptions.
3. **Removal Policy:**
   Deprecated interfaces are **never removed in patch releases**. They may only be removed in the subsequent **major release ($2.0.0$)**.
4. **Documentation:**
   All deprecations, scheduled removals, and migration pathways are cataloged in `CHANGELOG.md`.

---

## 5. Public Surface Freeze Window

Before tagging `v1.0.0`:
- A mandatory **3–4 week public surface calendar freeze** is observed.
- During this window, all CLI flags, MCP tool schemas, and JSON output structures remain strictly locked.
- Only documentation hardening, test additions, bug fixes, and non-breaking internal optimizations are permitted.
- The snapshot suite (`tool_schema_snapshot_test.rs`) runs on every commit to enforce this invariant.
