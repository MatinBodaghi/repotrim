# Agent Guidelines for repotrim

## Git Commit Message Style Guide
All commit messages in this repository must strictly adhere to the following style:

### Commit Types Guide
* `feat`: Add a new feature to the codebase
* `fix`: Fix a bug or error
* `docs`: Add or update documentation
* `style`: Changes that do not affect the meaning of the code (formatting, white-space, etc.)
* `refactor`: Code changes that neither fix a bug nor add a feature
* `perf`: A code change that improves performance
* `test`: Add missing tests or correct existing tests
* `chore`: Changes to the build process, auxiliary tools, or dependencies

### Formatting & Style Rules
- **Summary / First Line**:
  - Capitalize the summary (e.g. `feat: Add AST symbol extraction`).
  - Omit any trailing punctuation.
  - Aim for ~50 characters.
  - Write in the imperative tense: e.g. "Add feature" and not "Added feature" or "Adds feature".
- **Body**:
  - Add a blank line between the summary and the body.
  - Hard wrap paragraphs to 72 characters.
- **Constraints**:
  - DO NOT mention co-author (never add `Co-authored-by`).

## Git Push Policy
- DO NOT automatically push commits to remote (`git push`).
- Keep commits local and only push when explicitly instructed by the user.

## Code Quality & Atomic Commits
- Keep commits logical, modular, and self-contained.
- Every commit must compile cleanly, have zero clippy warnings (`cargo clippy --workspace --all-targets -- -D warnings`), and pass all unit/integration tests.

