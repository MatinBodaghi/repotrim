## Description

<!-- Brief summary of changes, rationale, and problem addressed. -->

## Related Issues

<!-- Fixes #123 or Relates to #123 -->

## Type of Change

- [ ] `feat`: Add a new feature or MCP tool
- [ ] `fix`: Fix a bug or regression
- [ ] `docs`: Documentation updates or additions
- [ ] `perf`: Algorithmic performance or memory optimization
- [ ] `refactor`: Internal code structure change without behavioral changes
- [ ] `test`: Add missing tests or correct existing tests
- [ ] `chore`: Build process, dependencies, or CI changes

## Quality Checklist

Before submitting this pull request, please verify each of the following:

- [ ] **Commit Style Guide (`AGENTS.md`):**
  - [ ] Summary line is imperative, capitalized, $\le 50$ chars, and has no trailing period.
  - [ ] Blank line between summary and body.
  - [ ] Body paragraphs are hard-wrapped to 72 characters.
  - [ ] **Strictly zero co-authors** (never include `Co-authored-by`).
- [ ] **Formatting:** `cargo fmt --all -- --check` passes with zero diffs.
- [ ] **Lints:** `cargo clippy --workspace --all-targets -- -D warnings` passes with zero warnings.
- [ ] **Tests:** `cargo test --workspace --all-targets` passes 100%.
- [ ] **Documentation:** `cargo doc --workspace --no-deps` builds with zero warnings (engine enforces `#![warn(missing_docs)]`).
- [ ] **Academic Citations:** If introducing new algorithms, formulas, or heuristics, papers are cited in in-code docstrings and `README.md`.
