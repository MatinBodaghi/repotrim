# Feature Blueprint: [Feature Name]

> **Purpose:** Provide a high-density, low-token specification for AI agent harnesses to execute this feature without exploratory file dumping.

---

## 1. Objective & Scope
- **Summary:** [1-2 sentences describing the core capability to be built]
- **Target Component(s):** [e.g. `crates/engine`, `crates/cli`, etc.]
- **Expected Outcome:** [Exact observable behavior, CLI flag, API type, or test case]

---

## 2. Seed Anchors (for RepoTrim Context Slicing)
Specify the key files or symbols the model should anchor to. RepoTrim uses these as seeds for Forward-Push PPR:
- **Primary Seed Files:**
  - `path/to/file1.rs`
  - `path/to/file2.rs`
- **Key Symbol Targets:**
  - `SymbolName` (e.g. `AstExtractor`, `SymbolNode`)

---

## 3. Technical Constraints & Invariants
- **Memory / Allocation:** [e.g. Zero-allocation, stack-only, or amortized Vec]
- **Error Handling:** [e.g. Add variants to `EngineError`, never unwrap in library code]
- **Dependencies:** [Permitted third-party crates or pure in-engine logic]
- **Commit / Git Policy:** [Local commit only, follow commit message guidelines]

---

## 4. Acceptance Criteria & Test Plan
- [ ] Automated Test 1: [Description of test case]
- [ ] Automated Test 2: [Description of test case]
- [ ] Verification Command: `cargo test -p [crate-name] --test [test_name]`
- [ ] Formatting / Linting: `cargo clippy --workspace -- -D warnings`
