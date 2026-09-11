# Contributing to RepoTrim

Thank you for your interest in contributing to **RepoTrim**! We welcome contributions ranging from algorithm design and performance optimizations to documentation and bug fixes.

RepoTrim is a high-performance, mathematically rigorous codebase context trimmer for AI coding agents. To maintain scientific integrity, code correctness, and sub-millisecond execution speeds, all contributions must adhere to the engineering and research guidelines described below.

---

## 1. Prerequisites & Toolchain

RepoTrim is written in Rust and uses Tree-sitter for polyglot Abstract Syntax Tree (AST) extraction.

- **Rust Toolchain:** **Rust 1.90.0 or higher** (due to `edition2024` Tree-sitter AST parser dependencies and Cargo lockfile format v4).
  ```bash
  rustup update stable
  rustc --version
  ```
- **C Compiler:** A working C/C++ compiler (`gcc`, `clang`, or MSVC `cl.exe`) is required to compile Tree-sitter language grammars.
  - On Ubuntu/Debian: `sudo apt-get install build-essential`
  - On macOS: `xcode-select --install`
  - On Windows: Visual Studio C++ Build Tools
- **Git:** Version 2.30+.

---

## 2. Branching & Development Workflow

RepoTrim utilizes a structured multi-tier branching strategy to keep production stable while allowing deep mathematical exploration:

```text
main (v0.5.0 stable) ───[v0.6.0 release]───[v0.7.0 release]───►
  │                           ▲                  ▲
  └───► dev (Integration) ────┴──────────────────┴────────────►
          │
          ├── research/*    (Formal math modeling, theory, specifications)
          ├── feature/*     (Isolated code implementations)
          └── experiment/*  (Ablations, oracle gaps, empirical benchmarks)
```

| Branch Prefix | Purpose | Activities & Scope |
| :--- | :--- | :--- |
| **`main`** | Stable production branch | Tagged releases (`v0.5.0`, `v0.6.0`, etc.). Kept production-ready at all times. |
| **`dev`** | Primary integration branch | Unifies completed feature, research, and experiment branches before release tagging. |
| **`research/*`** | Research & Mathematical Evolution | Formal problem formulations, submodular proof derivations, algorithm design docs. |
| **`feature/*`** | Isolated Implementation | Concrete engine traits, data structures, algorithms, CLI commands, and MCP tools. |
| **`experiment/*`** | Benchmarking & Hypothesis Testing | Empirical comparative studies, ablation ladders, oracle approximation gap analyses. |

### Development Cycle
1. Branch off `dev` using the appropriate prefix:
   ```bash
   git checkout dev
   git pull
   git checkout -b feature/my-enhancement
   ```
2. Develop atomically with comprehensive tests and zero warnings.
3. Verify all pre-commit quality gates pass.
4. Merge back into `dev` via clean, fast-forward or squash merges.

---

## 3. Git Commit Message Style Guide

All commit messages in this repository must strictly adhere to the following specification:

### Commit Types
- `feat`: Add a new feature or capability to the codebase.
- `fix`: Fix a bug, regression, or calculation error.
- `docs`: Add or update documentation, specifications, or citations.
- `style`: Changes that do not affect code semantics (formatting, white-space, etc.).
- `refactor`: Code changes that neither fix a bug nor add a feature.
- `perf`: A code change that improves execution speed, memory footprint, or cache efficiency.
- `test`: Add missing tests or correct existing test suites.
- `chore`: Changes to the build process, dependencies, auxiliary tools, or CI workflows.

### Formatting & Style Rules
- **Summary Line (First Line):**
  - Capitalize the summary (e.g. `feat: Add AST symbol extraction`).
  - Write in the imperative tense: `"Add feature"` and **not** `"Added feature"` or `"Adds feature"`.
  - Omit any trailing punctuation (no trailing period).
  - Aim for $\le 50$ characters (hard maximum 72 characters).
- **Body:**
  - Leave exactly one blank line between the summary and the body.
  - Hard-wrap all paragraphs to **72 characters**.
  - Explain *why* the change was made, technical trade-offs, and non-obvious design decisions.
- **Constraints:**
  - **DO NOT** mention co-authors (never include `Co-authored-by:` lines).
  - Commits should be atomic, self-contained, and compile cleanly.

---

## 4. Code Quality & Pre-Commit Verification

Before submitting any commit or opening a pull request, your working tree must satisfy the following zero-compromise quality gates:

### 1. Full Test Suite Pass
All unit, integration, and property-based invariant tests must pass:
```bash
cargo test --workspace --all-targets
```

### 2. Zero Clippy Warnings
RepoTrim enforces a strict `-D warnings` policy across all workspace targets:
```bash
cargo clippy --workspace --all-targets -- -D warnings
```
Code containing any clippy warning will fail CI and cannot be merged.

### 3. Code Formatting
All Rust code must strictly adhere to `rustfmt` formatting standards:
```bash
cargo fmt --all -- --check
```
To automatically format modified files:
```bash
cargo fmt --all
```

### 4. Regression Benchmarking
If you modify core algorithms (`ppr.rs`, `celf.rs`, `selector.rs`, `retrieval.rs`), verify that empirical performance has not regressed:
```bash
cargo run -p repotrim -- benchmark --scenario all
```

---

## 5. Research Attribution & Academic Citations

RepoTrim's mathematical superiority stems from rigorous theoretical grounding. Whenever algorithms, heuristics, data structures, or paradigms from external academic papers or technical articles are adopted or adapted, **explicit attribution is mandatory**:

1. **In-Code Docstrings:** Reference the author, paper title, year, publication venue/journal, and the specific algorithm name or equation number.
2. **`README.md`:** Add formal entries under `## References & Academic Citations` with DOI or arXiv URLs.
3. **Benchmark Documentation:** Include comparative analysis and algorithmic derivations under `docs/benchmarks/`.

---

## 6. Workspace Architecture

RepoTrim is organized as a Cargo workspace with three primary member crates:

```text
repotrim/
├── crates/
│   ├── engine/       # repotrim-engine: Core algorithms (PPR, CELF, CSR, AST, Slicing)
│   ├── cli/          # repotrim: CLI binary commands (select, query, benchmark, etc.)
│   └── mcp-server/   # repotrim-mcp: Model Context Protocol (stdio JSON-RPC server)
├── docs/             # Architecture specifications, benchmarks, and roadmap plans
├── skills/           # Turn-key agent skills for Antigravity, Claude Code, and Cursor
└── Cargo.toml        # Workspace manifest
```

- **`crates/engine`** must remain free of presentation logic and CLI dependencies. It should focus purely on AST analysis, graph algorithms, token accounting, and optimization solvers.
- **`crates/cli`** provides user-facing ANSI terminal output, formatting, and CLI parameter parsing.
- **`crates/mcp-server`** implements the standard JSON-RPC protocol exposing engine tools to AI agent harnesses.

---

## 7. Reporting Issues & Vulnerabilities

- **Bug Reports:** File an issue on GitHub with reproduction steps, the target codebase language, and the command output.
- **Security Inquiries:** For potential vulnerabilities or sensitive disclosures, please open a private GitHub security advisory.
