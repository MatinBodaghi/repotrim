# RepoTrim: Production Hardening, Security & v1.0.0 Master Plan

> **Mission:** "Transition RepoTrim from an algorithmically sound research prototype (`v0.11.0`) into an enterprise-grade, secure, performant, and packaged codebase intelligence standard (`v1.0.0`)—establishing strict filesystem confinement, eliminating prompt-injection channels, collapsing MCP token overhead by >60%, and delivering turnkey distribution with verifiable empirical guarantees."

**Audit Basis:** Repository cloned at commit `86b0b24` (`v0.11.0`), built with `cargo build --workspace --release` (clean), `cargo test --workspace` (320 passed / 0 failed), MCP server exercised over live stdio JSON-RPC, CLI exercised across `select`, `expand`, `navigate`, `locate`, `trace`, `stats`. Every finding is tied to a file, a command output, or an empirical reproduction. Findings marked **[REPRODUCED]** were demonstrated live on the running binary.

---

## 1. Executive Summary & Readiness Assessment

### 1.1 What This Repository Is
RepoTrim is a high-performance Rust workspace composed of three layered crates designed to extract and select a minimal, maximally-relevant subgraph of a target codebase to feed AI coding agents under a strict token budget:

- `crates/engine`: AST parsing (tree-sitter, 4 languages: Rust, Python, TypeScript, Go), typed multiplex code graph, Personalized PageRank (PPR), submodular knapsack selection, multi-factor cost accounting, and adaptive navigation policy. 40 source modules, 34 integration test files.
- `crates/mcp-server`: stdio JSON-RPC Model Context Protocol (MCP) server exposing 15 tools to agentic harnesses.
- `crates/cli`: 17 subcommands exposing the engine's capabilities to humans and scripts.

```text
┌─────────────────────────────────────────────────────────────────────────────┐
│                            RepoTrim v1.0.0 Stack                            │
├─────────────────────────────────────────────────────────────────────────────┤
│ CLI Subcommands (17)                   MCP Server (JSON-RPC stdio)          │
│ select, expand, navigate, trace...     tools/list, tools/call, resources    │
└──────────────────────────────────────┬──────────────────────────────────────┘
                                       ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│ Security & Isolation Boundary: RootGuard & Sanitized Subprocess Execution   │
│ - Canonical root enforcement           - Memory-safe path traversal         │
│ - Externalized, keyed cache storage    - Git argument injection defense     │
└──────────────────────────────────────┬──────────────────────────────────────┘
                                       ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│ Ingestion & Parsing Engine (ignore crate + tree-sitter + Rayon)             │
│ - Gitignore-aware, loop-safe walking   - Bounded resident file storage      │
│ - File-size guards (max 2 MB default)  - Zero-copy AST symbol extraction    │
└──────────────────────────────────────┬──────────────────────────────────────┘
                                       ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│ Mathematical Core: Multiplex Graph & Submodular Optimization                │
│ - Multiplex CSR Adjacency A = Σ ω_r A_r- Sparse ACL Forward-Push PPR        │
│ - Monotone Probabilistic Coverage      - Lazy-Greedy CELF Knapsack          │
└─────────────────────────────────────────────────────────────────────────────┘
```

### 1.2 Strongest Aspects
- **Mathematically Sound Algorithmic Core:** Personalized PageRank implements the Andersen–Chung–Lang (ACL) local forward-push diffusion with provable $O(1/\epsilon)$ runtime and strict error bounds. The knapsack optimizer incorporates the Khuller–Moss–Naor singleton correction, guaranteeing a true approximation ratio of $\frac{1}{2}(1 - 1/e) \approx 0.316$ for submodular maximization under budget constraints.
- **Scientific Honesty & Rigor:** Documentation in `docs/benchmarks/ablation_study.md` openly reports tiers where naive full-file baselines out-perform RepoTrim on direct-dependency recall, avoiding confirmation bias.
- **Exemplary Test Discipline:** 320 unit, integration, and property tests. Includes property-based testing of graph invariants (`tests/property_invariants.rs`) and an exact branch-and-bound oracle (`tests/knapsack_oracle_gap_test.rs`) that continuously bounds the empirical-to-theoretical approximation ratio gap.
- **Clean Git Repository Hygiene:** `.gitignore` is comprehensive, preventing binary caches (`.repotrim/*.bin`) from leaking into Git tracking (verified via `git ls-files`). Zero `TODO`, `FIXME`, or dead-code stubs in production paths.
- **Cross-Platform CI Matrix:** GitHub Actions workflows test Linux, macOS, and Windows with strict formatting checks, `clippy -- -D warnings`, and dedicated Minimum Supported Rust Version (MSRV) enforcement.

### 1.3 Critical Vulnerabilities & Systemic Risks
1. **Unconfined Filesystem Traversal (P0 Security):** The MCP server has no root isolation. Any arbitrary host path provided in tool arguments is loaded, analyzed, and returned. **[REPRODUCED]**
2. **Untrusted Cache Deserialization & Prompt Injection (P0 Security):** The binary cache `<target>/.repotrim/cache.bin` resides inside the scanned repository. Cloning a hostile repository allows unauthenticated arbitrary symbol injection into the LLM's context.
3. **Symlink Cycle Ingestion & Unbounded Memory Growth (P1 Correctness):** Ingestion resolves directory symlinks without cycle detection, causing recursive infinite loops and memory exhaustion. **[REPRODUCED]**
4. **Severe MCP Tool List Token Overhead (P1 Token Efficiency):** `tools/list` consumes ~3,390 tokens per agent handshake. For a standard 2,000-token context budget, self-description costs more than the context retrieved. **[REPRODUCED]**
5. **Zero External Corpus Validation:** All existing benchmarks are measured on RepoTrim's own repository against synthetic tasks, leaving external generalization unproven.

### 1.4 Overall v1.0.0 Readiness Assessment
The mathematical and algorithmic foundation of RepoTrim is production-grade. However, the product boundary—security sandboxing, packaging, distribution, discoverability, and protocol ergonomics—remains at a prototype level (`v0.4` tier). 

> [!IMPORTANT]
> The gap to `v1.0.0` is not algorithmic quality—it is operational surface area that was neglected while research velocity focused on mathematical coverage models. Reaching `v1.0.0` requires freezing the public API, locking down security boundaries, slashing tool overhead, and publishing installable artifacts.

---

## 2. Git Branching & Engineering Discipline

### 2.1 Branch Taxonomy & Roles
All work targeting `v1.0.0` originates from and merges back into `dev`. Branches strictly observe this 4-tier taxonomy:

| Branch Prefix | Purpose | Scope & Activities |
| :--- | :--- | :--- |
| `security/*` | **Security Sandboxing & Isolation** | Filesystem confinement (`RootGuard`), cache relocation, deserialization defense, subprocess argument hardening. |
| `hardening/*` | **Engine Reliability & Correctness** | Cycle-safe traversal (`ignore` crate), resident memory bounding, panic elimination, resource limits. |
| `feature/*` | **MCP Optimization & Packaging** | MCP tool consolidation, token compression, crates.io manifests, `rust-toolchain.toml`, tracing instrumentation. |
| `experiment/*` | **Evaluation & External Benchmarks** | MCP session token profiling, external repository evaluation harnesses, multi-language coverage verification. |

```text
main (v0.11.0 stable) ──────────────────────────────[v1.0.0-rc.1]────[v1.0.0 release]───►
  │                                                        ▲                 ▲
  └───► dev (Integration) ─────────────────────────────────┴─────────────────┴──────────►
          │
          ├── security/*    (RootGuard, cache sandboxing, git arg sanitization)
          ├── hardening/*   (ignore-crate traversal, zero panics, memory bounds)
          ├── feature/*     (MCP consolidation, crates.io packaging, tracing)
          └── experiment/*  (Session token cost, external multi-repo recall)
```

### 2.2 Quality & Commit Governance
1. **Commit Message Specification:** Strict compliance with `AGENTS.md`:
   - **Summary Line:** `type: Imperative description` ($\le 50$ characters, capitalized, no trailing period).
   - **Blank Line:** Exactly one newline between summary and body.
   - **Body:** Hard-wrapped to 72 characters, explaining design rationale and invariants.
   - **Constraints:** Never include `Co-authored-by` or mention co-authors.
2. **Quality Gates:**
   - `cargo test --workspace --all-targets` must pass 100%.
   - `cargo clippy --workspace --all-targets -- -D warnings` must produce zero warnings.
   - `#![deny(clippy::unwrap_used, clippy::expect_used)]` enforced in `engine` and `mcp-server`.
   - `cargo fmt --check` must be clean.
3. **Git Push Policy:** Commits remain local. Never run `git push` without explicit user authorization.

---

## 3. Master Milestone & Implementation Phase Registry

| Milestone | Target Version | Scope & Focus | Associated Branches | Status |
| :--- | :--- | :--- | :--- | :---: |
| **Milestone 7** | `v0.12.0` | Security Sandboxing & Ingestion Hardening | `security/path-confinement`<br>`security/cache-hardening`<br>`hardening/ignore-traversal` | `[PLANNED]` |
| **Milestone 8** | `v0.13.0` | MCP Surface Consolidation & Session Token Efficiency | `feature/mcp-consolidation`<br>`experiment/mcp-session-cost`<br>`feature/tracing-observability` | `[PLANNED]` |
| **Milestone 9** | `v0.14.0` | Packaging, Documentation & Developer Experience | `feature/crates-publish-prep`<br>`feature/dx-onboarding`<br>`experiment/external-eval` | `[PLANNED]` |
| **Milestone 10** | `v1.0.0` | API Stability Freeze, Release Verification & Launch | `hardening/release-readiness`<br>`feature/v1-launch` | `[PLANNED]` |

> [!NOTE]
> Implementation phases are executed in strict dependency order: Security and Ingestion Hardening (Milestone 7) establish non-negotiable boundaries before MCP surface consolidation (Milestone 8) and public distribution (Milestone 9).

---

## 4. Deep Audit Findings & Subsystem Analysis

### 4.1 Repository Map & Surface Evaluation

| Path | Responsibility | Assessment & Architectural Status |
| :--- | :--- | :--- |
| `crates/engine/src/` (40 modules) | Parsing, multiplex graph, PPR, knapsack, cost, navigation | Algorithmatically sound. Needs panic elimination and memory bounding. |
| `crates/engine/queries/*.scm` | Tree-sitter AST queries for Rust, Python, TypeScript, Go | Cleanly externalized and modular. |
| `crates/engine/tests/` (34 files) | Integration, property-based, and benchmark tests | Strong. Lacks security boundary and traversal cycle regression tests. |
| `crates/mcp-server/src/` | JSON-RPC protocol, tool dispatch, stdio transport | High vulnerability surface: no path isolation, 13 unhandled panics. |
| `crates/cli/src/commands/` (17 files) | Individual subcommand implementations | Clean, decoupled command structure. |
| `docs/` (6 top-level + 13 benchmarks) | Theory, architecture specifications, benchmark reports | Cluttered with 128 KB of internal historical phase documents. |
| `skills/repotrim/SKILL.md` | AI coding harness skill definition | Valuable integration asset; completely undocumented in README. |
| `.github/workflows/` | `ci.yml`, `release.yml` | Sound CI basis; lacks supply-chain auditing, feature matrix, and checksums. |
| `.agents/`, `.gitmessage`, `AGENTS.md` | Agent and commit conventions | Well-maintained; requires cross-linking from `CONTRIBUTING.md`. |

### 4.2 Architectural Analysis & End-to-End Execution Flow

```text
CLI / MCP Request Entry
  │
  ▼
LoadedRepository::load_with_options() [loader.rs]
  ├── RepositoryCache::load_from_file() [cache.rs] ──► [SECURITY HAZARD: Untrusted bincode in target]
  ├── scan_directory() [loader.rs] ──────────────────► [CORRECTNESS HAZARD: Direct recursion, symlink loop]
  └── AstExtractor::parse_file_with_imports() [parser.rs]
  │
  ▼
MultiplexGraph / CsrGraph Construction [multiplex.rs, csr.rs]
  │
  ▼
PersonalizedPageRank Computation (ACL Forward-Push Diffusion) [ppr.rs]
  │
  ▼
CelfOptimizer / MCKP Knapsack Solver (Probabilistic Coverage) [celf.rs, submodular.rs]
  │
  ▼
Realistic RenderCostModel Calculation [cost.rs]
  │
  ▼
ContextFormatter (StructuredContext -> Markdown / JSON) [formatter.rs]
```

#### Undocumented Architectural Assumptions
1. **Implicit Repository Trust:** The engine assumes scanned codebases are fully trusted, deserializing binary caches directly from repository directories.
2. **Symlink-Free Filesystem:** Assumes directory structures contain no circular symbolic links.
3. **Small File Assumption:** Reads files completely into RAM via `fs::read` (`loader.rs:104`) without file-size limits.
4. **Monolithic In-Memory Residency:** Maintains all file contents in memory simultaneously (`file_sources: HashMap<PathBuf, String>`).
5. **Shallow Directory Nesting:** Directory traversal relies on unconstrained call-stack recursion.

---

### 4.3 Model Context Protocol (MCP) Analysis

- **Current State:** 15 tools exposed over stdio JSON-RPC. Protocol version hardcoded to `2024-11-05` (`protocol.rs:4`). Declared capabilities: `tools` only.

#### Critical Findings
- **M1 — Absence of `resources` and `prompts` Capabilities:** Context delivery is forced exclusively through `tools/call`. The model must actively execute tool calls rather than referencing declarative repository context resources.
- **M2 — Hardcoded Protocol Version Without Negotiation:** The server echoes `2024-11-05` regardless of client capability negotiation.
- **M3 — Server Panics on Malformed Inputs (13 Unhandled Sites):** Non-test paths in `handler.rs` contain 13 instances of `unwrap()`, `expect()`, or `panic!`. In stdio transport, a panic terminates the process, severing the JSON-RPC pipe.
- **M4 — Unbounded Execution & Lack of Request Cancellation:** `run_loop` in `server.rs` processes requests synchronously without timeouts or cancellation tokens.
- **M5 — Redundant Tool Catalog & Cognitive Load:** 15 distinct tools with overlapping semantics force models to process excessive schema tokens, increasing tool-selection error rates.

---

### 4.4 Token-Efficiency & Whole-Session Cost Analysis

#### Verified Token Savings (Empirical Proof)
The render cost accounting model introduced in `cost.rs` strictly adheres to token budgets:

| Command | Budget | Rendered Tokens | Utilization Ratio | Status |
| :--- | :--- | :--- | :--- | :---: |
| `expand CelfOptimizer` | 150 | 145 | 96.7% | PASS |
| `expand CelfOptimizer` | 300 | 298 | 99.3% | PASS |
| `expand CelfOptimizer` | 600 | 596 | 99.3% | PASS |
| `expand CelfOptimizer` | 1200 | 1,199 | 99.9% | PASS |
| `select --seed CelfOptimizer` | 400 | 381 | 95.2% | PASS |

#### Remaining Token Waste
- **T1 — Excessive Handshake Overhead [REPRODUCED]:**
  ```bash
  $ printf '...initialize...\n...tools/list...\n' | ./target/release/repotrim-mcp
  # Result: tools/list payload = 13,561 characters ≈ 3,390 tokens
  ```
  Every session pays a 3,390-token tax before performing work. For standard 2,000-token budgets, initialization consumes more tokens than the delivered context.
- **T2 — Tool Splintering:** Seven distinct tools (`trim_context`, `locate_entrypoints`, `trace_paths`, `expand_symbol`, `search_symbols`, `inspect_symbol`, `generate_blueprint`) fragment context retrieval across disparate schemas.
- **T3 — Symlink Duplication Token Inflation [REPRODUCED]:** Traversal loops repeatedly ingest files, creating artificial nodes and distorting PageRank mass distributions.
- **T4 — Unbudgeted MCP Responses:** Several tools (`query_graph_stats`, `generate_architecture_docs`) emit unconstrained Markdown responses ignoring budget parameters.

#### Whole-Session Cost Mathematical Formulation
Benchmark evaluation must measure total session cost rather than isolated CLI selection:
$$\mathrm{Cost}_{\mathrm{session}} = T_{\mathrm{tools/list}} + \sum_{i=1}^{N} \left( T_{\mathrm{call\_args}}^{(i)} + T_{\mathrm{result}}^{(i)} \right) + \sum_{j=1}^{M} T_{\mathrm{retry}}^{(j)}$$

---

### 4.5 Code Quality Findings

- **C1 [P1] — Symlink Traversal Without Loop Detection [REPRODUCED]:** `loader.rs:380-403` uses `path.is_dir()`, which silently follows symlinks. A circular symlink fixture generated paths 40 levels deep, ingesting identical files dozens of times.
- **C2 [P1] — Direct Recursive Directory Traversal:** Deeply nested source hierarchies risk call-stack overflow.
- **C3 [P1] — Missing File Size Limits:** `loader.rs:104` blindly reads any matching extension into memory. Large minified bundles (e.g. 50 MB `.js`) degrade performance.
- **C4 [P1] — Incomplete Ignored Directory Set:** Hardcoded `IGNORED_DIRS` (`loader.rs:11-22`) omits `.venv`, `venv`, `__pycache__`, `vendor`, `.next`, `out`, `coverage`, `.tox`, `Pods`. Scanned repositories' `.gitignore` files are ignored.
- **C5 [P1] — Panics in Public Crates:** 13 panics in `handler.rs`, 3 in `tokens.rs`, 2 in `selector.rs`/`celf.rs`.
- **C6 [P2] — Monolithic MCP Handler Module:** `handler.rs` (1,500+ lines) bundles schemas, protocol dispatch, and tool execution into a single file.
- **C7 [P2] — Unbounded In-Memory Source Buffering:** `LoadedRepository.file_sources: HashMap<PathBuf, String>` holds entire repositories in memory simultaneously.
- **C8 [P2] — Sequential File Ingestion:** AST parsing is embarrassingly parallel but executes on a single thread.

---

### 4.6 Security Vulnerabilities & Threat Model

#### S1 [P0] — Arbitrary Filesystem Read via MCP Path Argument [REPRODUCED]
`handler.rs:621-626` resolves paths without verification:
```rust
fn resolve_path(&self, args: &serde_json::Value) -> PathBuf {
    args.get("path").and_then(|p| p.as_str()).map(PathBuf::from)
        .unwrap_or_else(|| self.default_root.clone())
}
```
**Exploit Demonstration:**
```json
tools/call query_graph_stats {"path": "/tmp/fake_private"}
→ "Directory: /tmp/fake_private" ... "private_api_logic (Function) ... src/secret.rs:L1"
```
The server reads paths anywhere on the host filesystem accessible to the process, allowing prompt injections or models to exfiltrate private files.

#### S2 [P0] — Untrusted Cache Deserialization & Prompt Injection
`cache.rs:63-70` deserializes `<target>/.repotrim/cache.bin` located inside the target repository. A malicious repository can craft `cache.bin` containing fabricated symbols and malicious prompt injections that are rendered directly into model context without source verification.

#### S3 [P1] — Unsolicited Filesystem Writes [REPRODUCED]
Executing tools against external directories silently creates `.repotrim/` directories on target read operations.

#### S4 [P1] — Host Absolute Path Leakage
`loader.rs:397` falls back to absolute filesystem paths when `strip_prefix` fails, exposing host directory layouts in model outputs.

#### S5 [P1] — Missing Supply-Chain Scanning
No automated dependency audits (`cargo-audit`, `cargo-deny`, or Dependabot) exist.

#### S6 [P2] — Undocumented Threat Model
Absence of `SECURITY.md` leaves deployment boundaries and threat models undefined.

#### S7 [P1] — Subprocess Argument Injection in Git Commands
`handler.rs:1490` passes model-controlled strings into `DiffResolver::get_git_diff_against`, invoking `Command::new("git").args(["diff", revision])`. A revision starting with `-` (e.g. `--output=/tmp/pwn`) is parsed by git as a flag.

#### Formal Security Boundaries for v1.0.0

| Boundary | Enforced Security Control | Verification Metric |
| :--- | :--- | :--- |
| **Model $\to$ MCP Server** | `RootGuard` path allowlist, strict JSON validation, zero panics | Path escape tests return JSON-RPC `-32602` |
| **MCP Server $\to$ Codebase** | Read-only enforcement; no `.repotrim` creation in target | Zero target-tree filesystem writes |
| **Codebase $\to$ Model Context** | Keyed external cache; mandatory blake3 verification against disk bytes | Poisoned `cache.bin` ignored |
| **Server Subprocesses $\to$ Host** | Git revision validation (`^[A-Za-z0-9._/~^@{}-]+$`), `--` argument separation, hooks disabled | Flag injection rejected |

---

### 4.7 Git, CI/CD & Supply Chain Hygiene

#### Missing GitHub Health Files
| File Path | Priority | Purpose & Governance |
| :--- | :---: | :--- |
| `SECURITY.md` | **P0** | Formal vulnerability disclosure path and security boundary declaration. |
| `.github/dependabot.yml` | **P1** | Automated dependency updates for Cargo crates and GitHub Actions. |
| `.github/ISSUE_TEMPLATE/bug_report.yml` | **P1** | Structured issue reporting capturing OS, Rust version, and target language. |
| `.github/ISSUE_TEMPLATE/feature_request.yml` | **P2** | Standardized feature proposals. |
| `.github/PULL_REQUEST_TEMPLATE.md` | **P2** | Checklist enforcing formatting, tests, and commit discipline per `AGENTS.md`. |
| `CODE_OF_CONDUCT.md` | **P2** | Standard Contributor Covenant 2.1 community health document. |
| `.editorconfig` | **P2** | Workspace-wide indent, newline, and whitespace rules. |

#### CI/CD Pipeline Gaps
1. **Untested Feature Combinations:** The `exact-tokens` feature is never compiled with `--no-default-features` in CI.
2. **Unenforced Dependency Locking:** CI runs without `--locked`, permitting non-reproducible dependency resolution.
3. **MSRV Verification Incomplete:** MSRV CI job runs `cargo check` rather than `cargo test`.
4. **Missing Security Scans:** No automated `cargo-audit` or `cargo-deny` steps.
5. **Incomplete Cross-Compilation Matrix:** `release.yml` omits `aarch64-unknown-linux-gnu`.
6. **Missing Release Verification:** Release assets lack `.sha256` checksums.
7. **No Automated Crates.io Publishing:** Crates are not published to crates.io despite declared documentation URLs.

---

### 4.8 Packaging, Cargo Manifests & Distribution Deficits

- **P1 — Missing Workspace README Inheritance:** Child crates do not declare `readme.workspace = true`, leaving packages without READMEs on crates.io.
- **P2 — Missing Package Include/Exclude Directives:** Manifests lack `exclude` filters, which would publish internal `docs/` (219 KB) and test fixtures to crates.io.
- **P3 — Manifest Metadata Incomplete:** Missing badge configurations and explicit package keywords.
- **P4 — Zero Installation Channels:** Users must manually clone and compile with Rust $\ge 1.90$.
- **P5 — Missing `rust-toolchain.toml`:** Contributors on older Rust versions encounter confusing `edition2024` compilation failures.
- **P6 — Document Clutter:** `docs/` mixes user guides with 128 KB of internal historical phase records.

---

### 4.9 Testing Deficits & Empirical Measurement Matrix

| Testing Gap | Priority | Failure Mode & Remediation |
| :--- | :---: | :--- |
| **MCP Protocol Conformance** | **P0** | No validation for malformed JSON-RPC, missing IDs, batch requests, or oversized parameters. |
| **Security Regression Harness** | **P0** | No automated tests validating that path traversal and cache injection attacks are blocked. |
| **Hostile Filesystem Tests** | **P1** | No unit tests covering symlink cycles or deeply recursive trees. |
| **Feature Flag Coverage** | **P1** | Heuristic tokenization (`--no-default-features`) is never tested. |
| **Session-Level Token Profiling** | **P1** | No test tracking MCP handshake token cost regressions. |
| **External Repository Corpus** | **P1** | All evaluations use RepoTrim's own source code. |
| **AST Parser Fuzzing** | **P2** | No `cargo-fuzz` targets testing tree-sitter against adversarial code. |
| **Large-Scale Monorepo Scaling** | **P2** | Memory and latency on repos with $\ge 10,000$ files are unmeasured. |

#### Empirical Measurement Methodology for v1.0.0
- **Whole-Session Cost:** Automated headless MCP stdio harness summing handshake, call, and response payloads.
- **External Recall@Budget:** Benchmark on 5–10 third-party repositories against accepted GitHub PR diffs.
- **Task Resolution Rate:** Standardized evaluation on a fixed subset of SWE-bench-lite tasks comparing agent success.
- **Memory & Latency Scaling:** Benchmarking wall-clock time and maximum resident set size (RSS) across repos with $10^2$, $10^3$, and $10^4$ files.

---

### 4.10 Documentation & Onboarding Findings

- **D1 — Overwhelming README:** Current README (46 KB) functions as a technical specification rather than an onboarding guide.
- **D2 — Broken Installation Instructions:** Instructions refer to binaries that are not published.
- **D3 — Missing MCP Client Configuration:** Lacks ready-to-copy JSON configuration blocks for Claude Desktop, Cursor, and Windsurf.
- **D4 — Missing Troubleshooting Guide:** No FAQ addressing MSRV or `edition2024` errors.
- **D5 — Undocumented Harness Skill:** `skills/repotrim/SKILL.md` is omitted from user documentation.
- **D6 — Missing MCP Tool Reference:** 15 tools lack user-facing schema and usage documentation.
- **D7 — Planning Documents Exposed at Root:** Internal planning files clutter the `docs/` hierarchy.
- **D8 — Incomplete API Documentation:** No `#![warn(missing_docs)]` or verified docs.rs builds.

---

### 4.11 SEO, Category Positioning & Discoverability

| Discovery Friction Point | Identified Gap | Remediation Plan |
| :--- | :--- | :--- |
| **GitHub Topics** | Unset | Add: `mcp`, `model-context-protocol`, `ai-coding-agent`, `code-analysis`, `token-optimization`, `llm-context`, `tree-sitter`, `rust`, `developer-tools`, `claude`. |
| **Repository Description** | Generic summary | Update to: *"High-performance MCP server providing mathematically optimal codebase context to AI coding agents under token budgets."* |
| **Crate Keywords** | Missing `mcp` keyword | Update `Cargo.toml` keywords: `["mcp", "llm", "context", "codebase", "ast"]`. |
| **Search-Optimized README** | Technical title | Structure README around search intent ("Reduce coding agent context cost"). |
| **Social Preview Asset** | Absent | Generate high-contrast SVG/PNG social preview card. |
| **Competitive Comparison** | Missing context | Add factual comparison table contrasting RepoTrim with Aider repo-map and naive dumps. |

---

### 4.12 Developer Experience (DX) Friction Matrix

| Onboarding Step | Identified Friction | Remediation Plan |
| :--- | :--- | :--- |
| **Clone & Setup** | Missing toolchain configuration | Commit `rust-toolchain.toml` pinning Rust 1.90. |
| **Build** | Confusing error on older Rust versions | Enforce toolchain checks with clear error messaging. |
| **Configuration** | All options require CLI flags | Introduce optional `.repotrim.toml` configuration support. |
| **Diagnostics** | No logging framework; debug via `eprintln!` | Integrate `tracing` with `RUST_LOG` filtering to stderr. |
| **Contributing** | No automated local validation hooks | Add pre-commit script mirroring CI quality checks. |
| **Release** | Undocumented release steps | Document tagging, changelog generation, and publishing in `CONTRIBUTING.md`. |

---

### 4.13 Performance, Scalability & Large-Repo Boundaries

| Scalability Dimension | Current System Status | Hardened v1.0.0 Architecture |
| :--- | :--- | :--- |
| **Ingestion Parallelism** | Sequential file parsing loop | Data-parallel parsing via `rayon::par_iter`. |
| **Resident Memory** | Entire codebase stored in memory | Store file byte offsets; stream content on render. |
| **Symlink Cycles** | Unbounded recursive loop | Cycle-safe traversal using visited inode sets via `ignore`. |
| **Large File Handling** | Full file reads without size limits | Skip files exceeding `max_file_bytes` (default 2 MB) with diagnostics. |
| **Cancellation & Timeouts** | Synchronous, uncancelable MCP loops | Bounded request timeouts with cooperative cancellation. |
| **Monorepo Scaling** | Untested on repositories $> 100$ files | Validated and profiled on repositories with $\ge 10,000$ files. |

---

### 4.14 Missing Repository Components Registry

```text
├── .editorconfig                                (P2 - Formatting consistency)
├── .github/
│   ├── dependabot.yml                           (P1 - Automated dependency security)
│   ├── ISSUE_TEMPLATE/
│   │   ├── bug_report.yml                       (P1 - Structured bug reports)
│   │   └── feature_request.yml                  (P2 - Feature suggestions)
│   ├── PULL_REQUEST_TEMPLATE.md                 (P2 - PR quality checklist)
│   └── workflows/
│       └── security.yml                         (P0 - cargo-deny & cargo-audit CI)
├── CODE_OF_CONDUCT.md                           (P2 - Contributor Covenant 2.1)
├── SECURITY.md                                  (P0 - Security policy & disclosure)
├── rust-toolchain.toml                          (P1 - Toolchain pinning to 1.90)
├── crates/engine/src/security.rs                (P0 - RootGuard path confinement)
├── docs/
│   ├── INSTALL.md                               (P1 - Installation guide)
│   ├── MCP_TOOLS.md                             (P1 - Frozen MCP tool reference)
│   ├── TROUBLESHOOTING.md                       (P1 - Common error remedies)
│   └── benchmarks/
│       └── mcp_session_cost.md                  (P1 - MCP session token measurements)
└── docs/internal/                               (P2 - Relocated historical plans)
```

---

### 4.15 v1.0.0 Blocker Registry

| ID | Blocker Description | Category | Resolution Target |
| :--- | :--- | :--- | :--- |
| **B1** | Enforce MCP path confinement via `RootGuard` | Security | Phase 1 (T1.1) |
| **B2** | Eliminate untrusted cache deserialization and relocate cache | Security | Phase 1 (T1.2) |
| **B3** | Prevent unsolicited filesystem writes to target directories | Security | Phase 1 (T1.2) |
| **B4** | Eliminate unhandled panics across engine and MCP server | Reliability | Phase 1 (T1.3) |
| **B5** | Fix symlink cycles, recursion depth, and file size limits | Correctness | Phase 2 (T2.1) |
| **B5b**| Sanitize git subprocess revision arguments | Security | Phase 1 (T1.4) |
| **B6** | Deliver turnkey installation (crates.io, binary releases, checksums) | Distribution | Phase 5 (T5.3) |
| **B7** | Configure workspace manifest metadata and packaging filters | Packaging | Phase 5 (T5.1) |
| **B8** | Rewrite README with working install and MCP client config | Documentation | Phase 6 (T6.1) |
| **B9** | Publish `SECURITY.md` defining threat model and reporting path | Security | Phase 1 (T1.5) |
| **B10**| Implement security and MCP conformance regression test suites | Testing | Phase 4 (T4.1, T4.2) |
| **B11**| Add supply-chain scanning, feature matrix, and `--locked` to CI | Supply Chain | Phase 4 & 5 (T4.3, T5.4) |
| **B12**| Benchmark and document whole-session MCP token costs | Token Efficiency | Phase 3 (T3.1) |
| **B13**| Consolidate MCP tools and compress schemas ($\le 1,200$ tokens) | Token Efficiency | Phase 3 (T3.2) |
| **B14**| Freeze and document public MCP interface with SemVer stability | API Stability | Phase 3 (T3.3) |

> [!NOTE]
> Non-blockers for `v1.0.0` (scheduled for post-v1 releases): Full SWE-bench-lite evaluation runs, memory-mapped source paging, and declarative MCP `resources` capabilities.

---

### 4.16 Strategic Insights & Architectural Findings

- **A1 — Evaluation Metric Misalignment:** Benchmarks currently profile CLI selection output, while end users consume RepoTrim via MCP servers. Profiling whole-session token costs is essential to substantiate product claims.
- **A2 — Cache Invalidation Correctness Hazard:** Caching relies on file modification times (`mtime`). File operations that preserve timestamps (e.g. `git checkout`, archive extraction) can serve stale context to the model.
- **A3 — Scientific Honesty as a Differentiator:** Transparently highlighting scenarios where naive baselines outperform RepoTrim establishes credibility with engineering users.
- **A4 — Release Velocity vs. Interface Stability:** Rapid iteration across `v0.6.0` through `v0.11.0` destabilized the public interface. `v1.0.0` requires freezing public contracts.
- **A5 — Clean Codebase Foundation:** Zero network dependencies, no hardcoded secrets, zero TODO/FIXME markers, and clean separation between crates make the codebase well-positioned for stabilization.

---

## 5. Target v1.0.0 System Architecture

```text
┌─────────────────────────────────────────────────────────────────────────────┐
│                       External AI Coding Harnesses                          │
│                (Claude Desktop, Cursor, Windsurf, Claude Code)              │
└──────────────────────────────────────┬──────────────────────────────────────┘
                                       │ JSON-RPC over stdio
                                       ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│                           repotrim-mcp (Hardened)                           │
│ - Strict JSON-RPC error handling (zero unhandled panics)                    │
│ - Compressed tool catalog (≤ 8 consolidated tools, ≤ 1,200 schema tokens)   │
│ - Protocol version negotiation                                              │
│ - Structured logging to stderr via tracing crate                            │
└──────────────────────────────────────┬──────────────────────────────────────┘
                                       │
                                       ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│                      Security Boundary Layer (RootGuard)                    │
│ - Resolves canonical paths against allowed roots (launch directory default) │
│ - Blocks directory traversal and symlink escapes                            │
│ - Sanitizes subprocess arguments (disables hooks, separates flags with --)  │
└──────────────────────────────────────┬──────────────────────────────────────┘
                                       │
                                       ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│                            repotrim-engine Core                             │
│ ┌──────────────────────────────────┐      ┌───────────────────────────────┐ │
│ │ Ingestion (ignore crate + Rayon) │      │ Relocated Keyed Cache Storage │ │
│ │ - Respects target .gitignore     │      │ - ~/.cache/repotrim/<hash>/   │ │
│ │ - Cycle-safe traversal           │      │ - Blake3-keyed verification   │ │
│ │ - max_file_bytes limits (2 MB)   │      │ - 64 MB bincode limit         │ │
│ └─────────────────┬────────────────┘      └───────────────────────────────┘ │
│                   ▼                                                         │
│ ┌─────────────────────────────────────────────────────────────────────────┐ │
│ │ Algorithmic Processing Core                                             │ │
│ │ - Multiplex CSR Graph              - Submodular Knapsack Optimization   │ │
│ │ - ACL Forward-Push Diffusion (PPR) - Multi-Factor Render Cost Accounting│ │
│ └─────────────────────────────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## 6. Detailed Implementation Roadmap & Commit Breakdown

### Phase 1: Security Hardening (Blocks v1.0.0)

#### T1.1: Implement `RootGuard` Path Confinement
- **Branch:** `security/path-confinement`
- **Objective:** Restrict all filesystem reads and graph analyses to verified, canonical paths within explicit allowed directories.
- **Technical Specification:**
  - Create `crates/engine/src/security.rs` defining `RootGuard`.
  - Validate that canonicalized target paths start with an allowed root.
  - Default MCP allowed root to launch directory; add `--allow-root` (repeatable) and `REPOTRIM_ALLOWED_ROOTS` env var.
  - Return JSON-RPC error `-32602` (Invalid params) on path escape.
- **Blocks v1:** Yes (B1). **Complexity:** Medium.
- **Done When:** Automated tests verify that requests accessing paths outside allowed roots are rejected with descriptive errors.

##### Commit Breakdown
- **Commit 1.1.1 (`feat: Introduce RootGuard filesystem boundary`)**:
  ```text
  feat: Introduce RootGuard filesystem boundary

  Add RootGuard path validator enforcing canonical root containment.
  Reject path traversal attempts and symlink escapes before any
  filesystem interaction occurs.
  ```
- **Commit 1.1.2 (`feat: Wire RootGuard into MCP path resolution`)**:
  ```text
  feat: Wire RootGuard into MCP path resolution

  Update resolve_path in mcp-server to validate targets against allowed
  roots. Expose --allow-root CLI parameter and environment variable.
  ```
- **Commit 1.1.3 (`test: Add security path confinement tests`)**:
  ```text
  test: Add security path confinement tests

  Add integration tests asserting that path escape attempts return
  JSON-RPC error -32602 without touching unauthorized directories.
  ```

---

#### T1.2: Relocate and Harden Binary Cache Storage
- **Branch:** `security/cache-hardening`
- **Objective:** Prevent repository-based cache tampering, eliminate unsolicited writes, and protect against malicious deserialization.
- **Technical Specification:**
  - Relocate cache storage to `dirs::cache_dir()/repotrim/<blake3(canonical_root)>/`.
  - Enforce a 64 MB deserialization size limit via `bincode::DefaultOptions::with_limit`.
  - Validate file blake3 hashes against disk bytes on cache retrieval, deprecating modification-time-only trust.
  - Add `--no-cache` flag to CLI and MCP.
- **Blocks v1:** Yes (B2, B3). **Complexity:** Medium.
- **Done When:** Analyzing a repository containing a forged `.repotrim/cache.bin` derives symbols exclusively from disk source code.

##### Commit Breakdown
- **Commit 1.2.1 (`feat: Relocate binary cache to user cache directory`)**:
  ```text
  feat: Relocate binary cache to user cache directory

  Move cache storage from target repository to OS cache directory
  keyed by repository root hash. Eliminate unsolicited writes.
  ```
- **Commit 1.2.2 (`feat: Harden bincode deserialization with limits`)**:
  ```text
  feat: Harden bincode deserialization with limits

  Configure bincode with 64 MB maximum buffer limit and enforce blake3
  content verification on cache hits to eliminate poisoning risks.
  ```
- **Commit 1.2.3 (`test: Verify cache isolation against hostile fixtures`)**:
  ```text
  test: Verify cache isolation against hostile fixtures

  Add tests confirming that hostile cache files inside target trees are
  ignored and that clean source parsing proceeds securely.
  ```

---

#### T1.3: Eliminate Unhandled Panics Across Server and Engine
- **Branch:** `hardening/zero-panics`
- **Objective:** Convert all panicking call sites to structured `Result` returns and enforce clean error propagation.
- **Technical Specification:**
  - Audit and refactor 13 panic/unwrap sites in `crates/mcp-server/src/handler.rs`.
  - Refactor panic sites in `crates/engine/src/{tokens,selector,celf,navigation,loader,architecture}.rs`.
  - Add `#![deny(clippy::unwrap_used, clippy::expect_used)]` to crate roots.
- **Blocks v1:** Yes (B4). **Complexity:** Medium.
- **Done When:** All workspace crates compile cleanly with deny lints enabled, and malformed inputs return JSON-RPC errors.

##### Commit Breakdown
- **Commit 1.3.1 (`refactor: Remove panics from MCP handler dispatch`)**:
  ```text
  refactor: Remove panics from MCP handler dispatch

  Replace unwrap and expect calls in handler.rs with structured error
  propagation returning JSON-RPC error objects on failures.
  ```
- **Commit 1.3.2 (`refactor: Replace unwrap sites across engine crates`)**:
  ```text
  refactor: Replace unwrap sites across engine crates

  Convert potential panics in token estimation, selection, and loader
  modules into typed EngineError variants.
  ```
- **Commit 1.3.3 (`ci: Enforce unwrap deny lints across workspace`)**:
  ```text
  ci: Enforce unwrap deny lints across workspace

  Enable clippy::unwrap_used and clippy::expect_used deny lints in
  lib.rs roots, permitting unwraps exclusively in test fixtures.
  ```

---

#### T1.4: Sanitize Git Subprocess Arguments
- **Branch:** `security/git-sanitization`
- **Objective:** Prevent argument injection when delegating differential analysis to the `git` binary.
- **Technical Specification:**
  - Validate revisions against `^[A-Za-z0-9._/~^@{}-]+$` and reject arguments beginning with `-`.
  - Insert `--` separators before revision arguments.
  - Append `-c core.hooksPath=/dev/null` and `--no-pager` to git invocations.
- **Blocks v1:** Yes (B5b). **Complexity:** Low.
- **Done When:** Invocations of `analyze_impact` with inputs like `--output=/tmp/leak` return validation errors.

##### Commit Breakdown
- **Commit 1.4.1 (`fix: Sanitize revision arguments for git diff subprocesses`)**:
  ```text
  fix: Sanitize revision arguments for git diff subprocesses

  Add validation rejecting revisions starting with flags and insert
  argument separators before passing user inputs to git.
  ```
- **Commit 1.4.2 (`test: Add git argument injection regression tests`)**:
  ```text
  test: Add git argument injection regression tests

  Verify that hostile flag-shaped revision strings are rejected
  without executing git diff subprocesses.
  ```

---

#### T1.5: Publish Formal Security Policy and Threat Model
- **Branch:** `security/documentation`
- **Objective:** Document security assumptions, vulnerability reporting channels, and isolation guarantees.
- **Technical Specification:** Author `SECURITY.md` covering threat boundaries, RootGuard confinement, and reporting contact.
- **Blocks v1:** Yes (B9). **Complexity:** Low.
- **Done When:** `SECURITY.md` is committed at the repository root.

##### Commit Breakdown
- **Commit 1.5.1 (`docs: Author SECURITY.md and vulnerability disclosure policy`)**:
  ```text
  docs: Author SECURITY.md and vulnerability disclosure policy

  Define formal threat model, RootGuard boundaries, cache security
  guarantees, and responsible vulnerability reporting instructions.
  ```

---

### Phase 2: Ingestion Correctness & Performance

#### T2.1: Modernize Traversal Using the `ignore` Crate
- **Branch:** `hardening/ignore-traversal`
- **Objective:** Resolve symlink recursion loops, honor repository `.gitignore` rules, and guard against out-of-memory errors.
- **Technical Specification:**
  - Replace `scan_directory` in `crates/engine/src/loader.rs` with `ignore::WalkBuilder`.
  - Configure `follow_links(false)`, `git_ignore(true)`, and `hidden(true)`.
  - Enforce `max_file_bytes` (default 2 MB) with skip reporting.
- **Blocks v1:** Yes (B5). **Complexity:** Medium.
- **Done When:** Symlink loop fixtures ingest target files exactly once, and files exceeding size limits are skipped with warnings.

##### Commit Breakdown
- **Commit 2.1.1 (`feat: Adopt ignore WalkBuilder for directory scanning`)**:
  ```text
  feat: Adopt ignore WalkBuilder for directory scanning

  Replace custom recursive directory scanner with ignore WalkBuilder.
  Respect target gitignore rules and eliminate recursive stack frames.
  ```
- **Commit 2.1.2 (`feat: Add file size limits and symlink loop guards`)**:
  ```text
  feat: Add file size limits and symlink loop guards

  Disable symlink resolution to prevent traversal cycles and add
  configurable max_file_bytes limit skipping oversized assets.
  ```
- **Commit 2.1.3 (`test: Validate traversal cycle defenses and gitignore`)**:
  ```text
  test: Validate traversal cycle defenses and gitignore

  Add integration tests verifying that circular symlinks terminate
  safely and that gitignored vendor paths are excluded.
  ```

---

#### T2.2: Parallelize File Parsing via Rayon
- **Branch:** `feature/parallel-ingestion`
- **Objective:** Accelerate cold repository ingestion by parallelizing tree-sitter AST parsing across CPU threads.
- **Technical Specification:**
  - Convert file parsing loop in `crates/engine/src/loader.rs` to `rayon::par_iter`.
  - Aggregate symbol tables and cross-file edge maps concurrently.
- **Blocks v1:** No. **Complexity:** Low.
- **Done When:** Cold ingestion benchmarks demonstrate measurable speedup on multi-core hardware.

##### Commit Breakdown
- **Commit 2.2.1 (`perf: Parallelize file parsing with rayon`)**:
  ```text
  perf: Parallelize file parsing with rayon

  Convert sequential file extraction to parallel iterator using rayon,
  scaling cold ingestion throughput across available CPU cores.
  ```

---

#### T2.3: Bound Resident File Memory
- **Branch:** `feature/bounded-memory`
- **Objective:** Eliminate whole-file resident memory consumption for large codebases.
- **Technical Specification:**
  - Refactor `LoadedRepository.file_sources` to store file metadata and byte offsets.
  - Stream source code excerpts on-demand during snippet rendering.
- **Blocks v1:** No. **Complexity:** High.
- **Done When:** Resident memory usage remains flat during analysis of large fixture repositories.

##### Commit Breakdown
- **Commit 2.3.1 (`refactor: Stream source text by byte offset on render`)**:
  ```text
  refactor: Stream source text by byte offset on render

  Replace in-memory source string map with byte offset references,
  reading source content on-demand during snippet formatting.
  ```

---

### Phase 3: MCP Surface Consolidation & Token Efficiency

#### T3.1: Profile and Enforce MCP Session Token Costs
- **Branch:** `experiment/mcp-session-cost`
- **Objective:** Measure whole-session token costs and establish automated regression assertions in CI.
- **Technical Specification:**
  - Create integration test `crates/mcp-server/tests/session_cost_test.rs` driving the binary over stdio.
  - Assert that `tools/list` payload remains below 1,200 tokens.
  - Document benchmark methodology in `docs/benchmarks/mcp_session_cost.md`.
- **Blocks v1:** Yes (B12). **Complexity:** Low.
- **Done When:** CI validates that MCP handshake costs adhere to token ceilings.

##### Commit Breakdown
- **Commit 3.1.1 (`test: Add MCP whole-session token cost harness`)**:
  ```text
  test: Add MCP whole-session token cost harness

  Implement integration test driving repotrim-mcp over stdio to measure
  total session tokens across initialization and tool calls.
  ```
- **Commit 3.1.2 (`docs: Document MCP session token benchmark methodology`)**:
  ```text
  docs: Document MCP session token benchmark methodology

  Add docs/benchmarks/mcp_session_cost.md detailing handshake token
  measurements and establishing evaluation baselines.
  ```

---

#### T3.2: Consolidate MCP Tool Surface
- **Branch:** `feature/mcp-consolidation`
- **Objective:** Reduce cognitive load and handshake overhead by consolidating 15 tools into $\le 8$ streamlined primitives.
- **Technical Specification:**
  - Merge retrieval tools (`locate_entrypoints`, `search_symbols`, `inspect_symbol`) into `find_symbols`.
  - Merge graph analysis tools into `analyze_graph`.
  - Compress JSON schema property descriptions, moving extended guides to documentation.
- **Blocks v1:** Yes (B13). **Complexity:** Medium.
- **Done When:** Total `tools/list` payload size drops to $\le 1,200$ tokens without losing capability.

##### Commit Breakdown
- **Commit 3.2.1 (`refactor: Consolidate redundant MCP context tools`)**:
  ```text
  refactor: Consolidate redundant MCP context tools

  Merge overlapping symbol search and inspection tools into a unified
  find_symbols primitive supporting ranked discovery and inspection.
  ```
- **Commit 3.2.2 (`refactor: Compress MCP tool descriptions and schemas`)**:
  ```text
  refactor: Compress MCP tool descriptions and schemas

  Shorten tool docstrings and parameter schemas to reduce total
  tools/list payload size below 1,200 tokens.
  ```
- **Commit 3.2.3 (`test: Validate consolidated MCP tool operations`)**:
  ```text
  test: Validate consolidated MCP tool operations

  Add integration tests verifying that consolidated primitives preserve
  all retrieval, tracing, and inspection functionality.
  ```

---

#### T3.3: Freeze Public MCP Tool Contract
- **Branch:** `feature/mcp-freeze`
- **Objective:** Formalize public tool schemas and establish SemVer stability policies.
- **Technical Specification:**
  - Author `docs/MCP_TOOLS.md` with complete input/output specifications.
  - Add schema snapshot tests that fail if tool signatures change unexpectedly.
- **Blocks v1:** Yes (B14). **Complexity:** Low.
- **Done When:** Schema snapshot tests pass and documentation reflects all public tools.

##### Commit Breakdown
- **Commit 3.3.1 (`docs: Author comprehensive MCP_TOOLS reference guide`)**:
  ```text
  docs: Author comprehensive MCP_TOOLS reference guide

  Document all public MCP tools, parameters, output structures, and
  recommended usage patterns for AI coding agents.
  ```
- **Commit 3.3.2 (`test: Add snapshot assertions for MCP tool schemas`)**:
  ```text
  test: Add snapshot assertions for MCP tool schemas

  Introduce schema snapshot tests asserting that public tool contracts
  remain stable against unintended regressions.
  ```

---

#### T3.4: Introduce MCP Budget Controls and Protocol Negotiation
- **Branch:** `feature/mcp-budgeting`
- **Objective:** Ensure all context-returning tools observe token constraints and support standard protocol negotiation.
- **Technical Specification:**
  - Add `max_tokens` support to `query_graph_stats` and `generate_architecture_docs`.
  - Implement dynamic protocol version negotiation in initialization handshakes.
- **Blocks v1:** No. **Complexity:** Low.
- **Done When:** All MCP tools adhere to requested token limits and echo negotiated protocol revisions.

##### Commit Breakdown
- **Commit 3.4.1 (`feat: Add token budgeting to unbounded MCP tools`)**:
  ```text
  feat: Add token budgeting to unbounded MCP tools

  Introduce max_tokens bounds to graph stats and architecture tools,
  preventing unbounded Markdown responses in model context.
  ```
- **Commit 3.4.2 (`feat: Support client protocol version negotiation`)**:
  ```text
  feat: Support client protocol version negotiation

  Echo client-requested protocol version during initialization when
  compatible, replacing hardcoded version string.
  ```

---

### Phase 4: Conformance & Comprehensive Testing

#### T4.1: Build Security Regression Suite
- **Branch:** `security/regression-suite`
- **Objective:** Prevent regressions on path escapes, cache poisoning, and symlink loops.
- **Technical Specification:** Create `crates/engine/tests/security_test.rs` covering all identified vulnerability vectors.
- **Blocks v1:** Yes (B10). **Complexity:** Medium.
- **Done When:** Security test suite passes 100% in CI.

##### Commit Breakdown
- **Commit 4.1.1 (`test: Add security regression test suite`)**:
  ```text
  test: Add security regression test suite

  Add automated tests covering path traversal, unauthorized writes,
  malicious cache deserialization, and symlink cycle termination.
  ```

---

#### T4.2: Implement JSON-RPC Protocol Conformance Suite
- **Branch:** `feature/mcp-conformance`
- **Objective:** Verify protocol compliance under malformed and boundary JSON-RPC inputs.
- **Technical Specification:** Create `crates/mcp-server/tests/conformance_test.rs` covering invalid JSON, unknown methods, missing IDs, and wrong parameter types.
- **Blocks v1:** Yes. **Complexity:** Low.
- **Done When:** Server survives all invalid inputs, returning standard JSON-RPC error codes.

##### Commit Breakdown
- **Commit 4.2.1 (`test: Add MCP protocol conformance test suite`)**:
  ```text
  test: Add MCP protocol conformance test suite

  Verify error handling for malformed JSON, unknown methods, missing
  IDs, and type mismatches across stdio transport.
  ```

---

#### T4.3: Expand CI Feature Matrix & Enforce Strict Locking
- **Branch:** `hardening/ci-matrix`
- **Objective:** Guarantee build reproducibility and test non-default feature configurations.
- **Technical Specification:** Update `.github/workflows/ci.yml` with `--no-default-features`, `--all-features`, `--locked`, and upgrade MSRV to `cargo test`.
- **Blocks v1:** Yes (B11). **Complexity:** Low.
- **Done When:** CI passes across the full feature matrix.

##### Commit Breakdown
- **Commit 4.3.1 (`ci: Add feature matrix and locked dependency validation`)**:
  ```text
  ci: Add feature matrix and locked dependency validation

  Test default, no-default, and all-features configurations with
  --locked in CI, and upgrade MSRV verification to cargo test.
  ```

---

#### T4.4: Deploy Multi-Repository External Evaluation Corpus
- **Branch:** `experiment/external-corpus`
- **Objective:** Measure retrieval quality and Recall@Budget across third-party open-source codebases.
- **Technical Specification:** Build a test harness evaluating recall against accepted PR diffs from 5–10 external repositories.
- **Blocks v1:** No. **Complexity:** High.
- **Done When:** External evaluation report is published in `docs/benchmarks/`.

##### Commit Breakdown
- **Commit 4.4.1 (`test: Add multi-repository evaluation harness`)**:
  ```text
  test: Add multi-repository evaluation harness

  Build evaluation runner measuring Recall@Budget across external
  open-source repositories using real PR change sets.
  ```

---

### Phase 5: Packaging, Toolchains & Release Infrastructure

#### T5.1: Configure Package Manifests and Workspace Metadata
- **Branch:** `feature/packaging-metadata`
- **Objective:** Prepare crates for clean crates.io publishing.
- **Technical Specification:**
  - Add `readme.workspace = true` to all child crate manifests.
  - Define `exclude` rules filtering documentation and test fixtures.
  - Update root keywords to include `mcp`.
- **Blocks v1:** Yes (B7). **Complexity:** Low.
- **Done When:** `cargo package` builds clean packages containing only required runtime assets.

##### Commit Breakdown
- **Commit 5.1.1 (`chore: Configure workspace manifest packaging metadata`)**:
  ```text
  chore: Configure workspace manifest packaging metadata

  Inherit workspace README across child crates, configure package
  exclude filters, and update keywords to include mcp.
  ```

---

#### T5.2: Pin Minimum Toolchain in `rust-toolchain.toml`
- **Branch:** `hardening/toolchain-pin`
- **Objective:** Provide a seamless build experience for new contributors.
- **Technical Specification:** Add `rust-toolchain.toml` specifying `channel = "1.90"`.
- **Blocks v1:** Yes. **Complexity:** Trivial.
- **Done When:** Fresh clones on supported environments automatically select the verified Rust channel.

##### Commit Breakdown
- **Commit 5.2.1 (`chore: Pin workspace MSRV in rust-toolchain.toml`)**:
  ```text
  chore: Pin workspace MSRV in rust-toolchain.toml

  Add rust-toolchain.toml specifying channel 1.90 to prevent build
  failures on outdated toolchains.
  ```

---

#### T5.3: Automate Multi-Platform Release Distribution
- **Branch:** `feature/release-automation`
- **Objective:** Distribute pre-compiled release binaries and publish packages to crates.io.
- **Technical Specification:**
  - Update `.github/workflows/release.yml` to compile `x86_64` and `aarch64` binaries for Linux, macOS, and Windows.
  - Generate SHA-256 checksum files for all release assets.
  - Automate topological publishing to crates.io (`engine` $\to$ `mcp-server` $\to$ `cli`).
- **Blocks v1:** Yes (B6). **Complexity:** Medium.
- **Done When:** Tagging a release candidate builds verified assets and checksums in GitHub Releases.

##### Commit Breakdown
- **Commit 5.3.1 (`ci: Add aarch64 targets and checksums to release workflow`)**:
  ```text
  ci: Add aarch64 targets and checksums to release workflow

  Expand release matrix with aarch64 Linux targets and automate
  generation of SHA-256 checksums for release assets.
  ```
- **Commit 5.3.2 (`ci: Add automated topological crates.io publishing`)**:
  ```text
  ci: Add automated topological crates.io publishing

  Configure automated publishing pipeline releasing repotrim-engine,
  repotrim-mcp, and repotrim sequentially upon version tags.
  ```

---

#### T5.4: Implement Supply-Chain Auditing Workflows
- **Branch:** `security/supply-chain`
- **Objective:** Protect dependencies against known vulnerabilities and license violations.
- **Technical Specification:** Add `.github/dependabot.yml` and `.github/workflows/security.yml` executing `cargo-deny` and `cargo-audit`.
- **Blocks v1:** Yes (B11). **Complexity:** Low.
- **Done When:** Security scanning workflows run cleanly in CI.

##### Commit Breakdown
- **Commit 5.4.1 (`ci: Add cargo-deny and cargo-audit security workflows`)**:
  ```text
  ci: Add cargo-deny and cargo-audit security workflows

  Add security workflow validating dependencies against advisory
  databases, prohibited licenses, and banned crates.
  ```
- **Commit 5.4.2 (`ci: Configure Dependabot for cargo and github-actions`)**:
  ```text
  ci: Configure Dependabot for cargo and github-actions

  Add dependabot.yml automating weekly dependency update checks for
  Cargo dependencies and GitHub Actions.
  ```

---

### Phase 6: Documentation & Onboarding Overhaul

#### T6.1: Streamline README for Rapid Onboarding
- **Branch:** `docs/readme-overhaul`
- **Objective:** Deliver a focused, accessible front door for the project ($\le 8$ KB).
- **Technical Specification:**
  - Structure around problem statement, installation, 60-second quickstart, and MCP client configurations.
  - Provide copy-pasteable JSON configuration blocks for Claude Desktop, Cursor, and Windsurf.
  - Include verified ablation benchmark summary table.
- **Blocks v1:** Yes (B8). **Complexity:** Medium.
- **Done When:** README provides a working quickstart that can be completed in under two minutes.

##### Commit Breakdown
- **Commit 6.1.1 (`docs: Rewrite README with focused quickstart and MCP configs`)**:
  ```text
  docs: Rewrite README with focused quickstart and MCP configs

  Condense README to clear onboarding guide featuring installation
  steps, copy-pasteable MCP configs, and benchmark summaries.
  ```

---

#### T6.2: Author Dedicated Installation and Troubleshooting Guides
- **Branch:** `docs/onboarding-guides`
- **Objective:** Address common setup friction and environmental failures.
- **Technical Specification:** Author `docs/INSTALL.md` and `docs/TROUBLESHOOTING.md` (covering toolchain and MSRV resolutions).
- **Blocks v1:** Yes. **Complexity:** Medium.
- **Done When:** Guides are published and cross-linked from README.

##### Commit Breakdown
- **Commit 6.2.1 (`docs: Author INSTALL and TROUBLESHOOTING guides`)**:
  ```text
  docs: Author INSTALL and TROUBLESHOOTING guides

  Add comprehensive installation documentation for all platforms and
  author troubleshooting guide addressing MSRV and runtime issues.
  ```

---

#### T6.3: Relocate Internal Planning Artifacts
- **Branch:** `docs/internal-reorg`
- **Objective:** Clean up the public documentation tree.
- **Technical Specification:** Move `PHASE_PLAN.md`, `REPOTRIM_MATHEMATICAL_EVOLUTION.md`, and historical notes into `docs/internal/`.
- **Blocks v1:** No. **Complexity:** Trivial.
- **Done When:** Root `docs/` contains only user-facing documentation and benchmark studies.

##### Commit Breakdown
- **Commit 6.3.1 (`docs: Move internal phase history into docs/internal`)**:
  ```text
  docs: Move internal phase history into docs/internal

  Reorganize documentation directory by relocating internal phase
  history documents out of the public documentation root.
  ```

---

#### T6.4: Enforce Public API Documentation
- **Branch:** `docs/api-coverage`
- **Objective:** Provide complete rustdoc coverage across public library interfaces.
- **Technical Specification:** Add `#![warn(missing_docs)]` to `repotrim-engine` and verify clean `cargo doc` builds.
- **Blocks v1:** No. **Complexity:** Low.
- **Done When:** `cargo doc --workspace --no-deps` completes with zero warnings.

##### Commit Breakdown
- **Commit 6.4.1 (`docs: Document public engine items and enable missing_docs`)**:
  ```text
  docs: Document public engine items and enable missing_docs

  Add docstrings for all exported engine types and enable missing_docs
  lint to maintain documentation standards.
  ```

---

### Phase 7: GitHub Quality, SEO & Community Readiness

#### T7.1: Configure Repository Metadata and Search Optimization
- **Branch:** `feature/seo-metadata`
- **Objective:** Maximize search discoverability across GitHub and search engines.
- **Technical Specification:** Configure GitHub repository topics, set high-intent keywords in `Cargo.toml`, and author meta descriptions.
- **Blocks v1:** No. **Complexity:** Trivial.
- **Done When:** Repository metadata and search assets are configured.

##### Commit Breakdown
- **Commit 7.1.1 (`chore: Optimize repository metadata and package keywords`)**:
  ```text
  chore: Optimize repository metadata and package keywords

  Update Cargo keywords and document GitHub repository topics to
  maximize discoverability for agent context optimization terms.
  ```

---

#### T7.2: Add Community Health Templates
- **Branch:** `community/health-files`
- **Objective:** Provide standardized templates for bug reports, feature requests, and community guidelines.
- **Technical Specification:** Add `CODE_OF_CONDUCT.md`, `.editorconfig`, issue forms, and PR templates.
- **Blocks v1:** No. **Complexity:** Low.
- **Done When:** Community health files are active in repository settings.

##### Commit Breakdown
- **Commit 7.2.1 (`docs: Add community health files and issue templates`)**:
  ```text
  docs: Add community health files and issue templates

  Add Code of Conduct, EditorConfig, structured issue forms, and PR
  template enforcing repository development conventions.
  ```

---

#### T7.3: Publish Factual Comparative Benchmark Analysis
- **Branch:** `docs/comparative-study`
- **Objective:** Provide a data-driven comparison against alternative context retrieval strategies.
- **Technical Specification:** Document objective trade-offs contrasting RepoTrim with Aider repo-maps and full-file dumps using empirical ablation data.
- **Blocks v1:** No. **Complexity:** Low.
- **Done When:** Comparative analysis section is published in documentation.

##### Commit Breakdown
- **Commit 7.3.1 (`docs: Add comparative evaluation vs alternative approaches`)**:
  ```text
  docs: Add comparative evaluation vs alternative approaches

  Add factual comparative analysis contrasting RepoTrim against Aider
  repo-map and naive baselines using empirical benchmark data.
  ```

---

### Phase 8: Structured Observability & Diagnostics

#### T8.1: Integrate Structured Tracing
- **Branch:** `feature/tracing-instrumentation`
- **Objective:** Provide runtime diagnostics for ingestion, PageRank convergence, and selection without corrupting stdio transport.
- **Technical Specification:**
  - Add `tracing` and `tracing-subscriber` dependencies.
  - Instrument ingestion, PPR diffusion, and knapsack optimization spans.
  - Route all logs strictly to `stderr` in `repotrim-mcp` to preserve stdio JSON-RPC streams.
- **Blocks v1:** No. **Complexity:** Medium.
- **Done When:** Setting `RUST_LOG=repotrim=debug` outputs structured spans to stderr during CLI and MCP runs.

##### Commit Breakdown
- **Commit 8.1.1 (`feat: Instrument engine and MCP server with tracing`)**:
  ```text
  feat: Instrument engine and MCP server with tracing

  Add structured tracing spans across graph operations and configure
  stderr-only log subscribers to protect MCP stdio streams.
  ```

---

## 7. File-by-File Change Plan

| Action | Target File Path | Primary Responsibility & Justification |
| :--- | :--- | :--- |
| **Create** | `crates/engine/src/security.rs` | Implement `RootGuard` canonical path confinement (S1, B1). |
| **Create** | `SECURITY.md` | Formal threat model and vulnerability disclosure policy (S6, B9). |
| **Create** | `rust-toolchain.toml` | Pin workspace MSRV to 1.90 to guarantee build stability (P5). |
| **Create** | `.github/dependabot.yml` | Automated dependency updates for Cargo and GitHub Actions (S5). |
| **Create** | `.github/ISSUE_TEMPLATE/bug_report.yml` | Structured bug reports capturing platform and language data. |
| **Create** | `.github/ISSUE_TEMPLATE/feature_request.yml` | Standardized feature proposals. |
| **Create** | `.github/PULL_REQUEST_TEMPLATE.md` | PR quality checklist enforcing tests and commit formatting. |
| **Create** | `.github/workflows/security.yml` | Automated `cargo-deny` and `cargo-audit` supply-chain CI (B11). |
| **Create** | `CODE_OF_CONDUCT.md` | Contributor Covenant 2.1 community health standard. |
| **Create** | `.editorconfig` | Standardized cross-editor indentation and formatting rules. |
| **Create** | `docs/MCP_TOOLS.md` | Complete reference documentation for frozen MCP tools (B14). |
| **Create** | `docs/INSTALL.md` | Multi-platform installation and setup instructions (B6). |
| **Create** | `docs/TROUBLESHOOTING.md` | Remediation guide for MSRV errors and MCP integration. |
| **Create** | `docs/benchmarks/mcp_session_cost.md` | Whole-session MCP token cost benchmarks and baselines (B12). |
| **Create** | `crates/mcp-server/tests/session_cost_test.rs` | Integration test asserting MCP handshake token ceilings (B12). |
| **Create** | `crates/mcp-server/tests/conformance_test.rs` | JSON-RPC protocol error handling and conformance suite (B10). |
| **Create** | `crates/engine/tests/security_test.rs` | Security regression tests for path escape and cache poisoning. |
| **Modify** | `crates/mcp-server/src/handler.rs` | Integrate `RootGuard`, eliminate panics, consolidate tools (B1, B4, B13). |
| **Modify** | `crates/engine/src/loader.rs` | Replace scanner with `ignore`, add size limits, fix prefix stripping. |
| **Modify** | `crates/engine/src/cache.rs` | Relocate cache outside repo, add bincode size limits, verify bytes. |
| **Modify** | `crates/engine/src/coedit.rs` | Update cache paths to use externalized directory storage. |
| **Modify** | `crates/{engine,cli,mcp-server}/Cargo.toml`| Inherit workspace README, add exclude rules, declare dependencies. |
| **Modify** | `Cargo.toml` (workspace root) | Add `mcp` keyword and configure shared package properties. |
| **Modify** | `.github/workflows/ci.yml` | Add feature matrix, `--locked` check, and full test execution on MSRV. |
| **Modify** | `.github/workflows/release.yml` | Add `aarch64` Linux target, checksums, and automated publishing. |
| **Modify** | `README.md` | Rewrite for rapid onboarding ($\le 8$ KB) with MCP client configs. |
| **Modify** | `CONTRIBUTING.md` | Document release workflow and link to `AGENTS.md` conventions. |
| **Move** | `docs/PHASE_PLAN.md` $\to$ `docs/internal/` | Relocate historical planning artifacts from public documentation. |
| **Move** | `docs/REPOTRIM_MATHEMATICAL_EVOLUTION.md` $\to$ `docs/internal/` | Relocate internal research notes. |
| **Move** | `docs/V1_EVOLUTION_PLAN.md` $\to$ `docs/internal/` | Relocate internal roadmaps. |

---

## 8. Master v1.0.0 Definition of Done

### Functionality & Feature Stability
- [ ] All 17 CLI commands documented and covered by integration tests.
- [ ] All public MCP tools documented with stable, frozen schemas.
- [ ] Token budgets strictly enforced across all context-generating operations.

### MCP Reliability & Efficiency
- [ ] Handshake `tools/list` payload verified at $\le 1,200$ tokens in CI.
- [ ] Zero unhandled panics; all failures return structured JSON-RPC error responses.
- [ ] Standard JSON-RPC conformance test suite passing cleanly.
- [ ] Documented SemVer compatibility policy for MCP tool schemas.

### Security & Sandboxing
- [ ] `RootGuard` enforces canonical path containment; unauthorized reads are blocked.
- [ ] Zero writes performed inside analyzed target repository directories.
- [ ] Cache stored in user cache directory with mandatory content hash validation.
- [ ] Subprocess arguments sanitized against flag injection.
- [ ] Symlink traversal loops terminate safely without duplicate ingestion.
- [ ] `SECURITY.md` published defining threat model and reporting channels.
- [ ] Automated supply-chain scanning via `cargo-deny` clean in CI.

### Test Coverage & CI Matrix
- [ ] CI passes cleanly across `--no-default-features`, default, and `--all-features`.
- [ ] Dependency resolution reproducibility verified with `cargo build --locked`.
- [ ] Minimum Supported Rust Version (1.90) verified with full `cargo test`.
- [ ] Security regression and protocol conformance test suites passing in CI.

### Packaging & Distribution
- [ ] Packages published to crates.io with properly rendered READMEs.
- [ ] Multi-platform release binaries (`x86_64` and `aarch64`) published with SHA-256 checksums.
- [ ] `rust-toolchain.toml` committed; clean clones build without manual toolchain steps.

### Documentation & Onboarding
- [ ] Streamlined README ($\le 8$ KB) providing quickstart and copy-pasteable MCP configs.
- [ ] Complete reference guides: `docs/MCP_TOOLS.md`, `docs/INSTALL.md`, `docs/TROUBLESHOOTING.md`.
- [ ] Public engine API documented with `#![warn(missing_docs)]`.
- [ ] Internal planning documents organized into `docs/internal/`.

---

## 9. Master Execution Summary & Step Tracking

| Step | Milestone | Action Target | Command / Artifact | Status |
| :--- | :--- | :--- | :--- | :---: |
| **1** | Milestone 7 | Implement `RootGuard` path isolation | `crates/engine/src/security.rs` | `[PLANNED]` |
| **2** | Milestone 7 | Wire `RootGuard` into MCP request handlers | `crates/mcp-server/src/handler.rs` | `[PLANNED]` |
| **3** | Milestone 7 | Add security path escape regression tests | `crates/engine/tests/security_test.rs` | `[PLANNED]` |
| **4** | Milestone 7 | Relocate cache storage outside target trees | `crates/engine/src/cache.rs` | `[PLANNED]` |
| **5** | Milestone 7 | Enforce bincode buffer limits and byte checks | `crates/engine/src/cache.rs` | `[PLANNED]` |
| **6** | Milestone 7 | Eliminate unhandled panics across MCP handler | `crates/mcp-server/src/handler.rs` | `[PLANNED]` |
| **7** | Milestone 7 | Replace unwrap calls across engine crates | `crates/engine/src/*.rs` | `[PLANNED]` |
| **8** | Milestone 7 | Enable workspace-wide unwrap deny lints | `crates/{engine,mcp-server}/src/lib.rs` | `[PLANNED]` |
| **9** | Milestone 7 | Sanitize git subprocess revision arguments | `crates/engine/src/diff.rs` | `[PLANNED]` |
| **10**| Milestone 7 | Publish repository `SECURITY.md` | `SECURITY.md` | `[PLANNED]` |
| **11**| Milestone 7 | Migrate traversal to `ignore::WalkBuilder` | `crates/engine/src/loader.rs` | `[PLANNED]` |
| **12**| Milestone 7 | Add file size bounds and symlink cycle guards | `crates/engine/src/loader.rs` | `[PLANNED]` |
| **13**| Milestone 8 | Build MCP whole-session token test harness | `crates/mcp-server/tests/session_cost_test.rs` | `[PLANNED]` |
| **14**| Milestone 8 | Document MCP session token benchmarks | `docs/benchmarks/mcp_session_cost.md` | `[PLANNED]` |
| **15**| Milestone 8 | Consolidate redundant MCP context tools | `crates/mcp-server/src/handler.rs` | `[PLANNED]` |
| **16**| Milestone 8 | Compress MCP tool docstrings and schemas | `crates/mcp-server/src/handler.rs` | `[PLANNED]` |
| **17**| Milestone 8 | Author `docs/MCP_TOOLS.md` reference guide | `docs/MCP_TOOLS.md` | `[PLANNED]` |
| **18**| Milestone 8 | Add snapshot tests for public tool schemas | `crates/mcp-server/tests/conformance_test.rs` | `[PLANNED]` |
| **19**| Milestone 8 | Add budget constraints to unbounded tools | `crates/mcp-server/src/handler.rs` | `[PLANNED]` |
| **20**| Milestone 8 | Add structured tracing across engine and MCP | `crates/{engine,mcp-server}/src/lib.rs` | `[PLANNED]` |
| **21**| Milestone 9 | Configure package manifests and workspace README | `crates/*/Cargo.toml` | `[PLANNED]` |
| **22**| Milestone 9 | Commit workspace `rust-toolchain.toml` | `rust-toolchain.toml` | `[PLANNED]` |
| **23**| Milestone 9 | Add supply-chain scanning to CI | `.github/workflows/security.yml` | `[PLANNED]` |
| **24**| Milestone 9 | Expand CI feature matrix and `--locked` check | `.github/workflows/ci.yml` | `[PLANNED]` |
| **25**| Milestone 9 | Rewrite README with quickstart and MCP configs | `README.md` | `[PLANNED]` |
| **26**| Milestone 9 | Author `INSTALL.md` and `TROUBLESHOOTING.md` | `docs/INSTALL.md` | `[PLANNED]` |
| **27**| Milestone 9 | Reorganize internal notes to `docs/internal/` | `docs/internal/` | `[PLANNED]` |
| **28**| Milestone 10| Automate release workflow with checksums | `.github/workflows/release.yml` | `[PLANNED]` |
