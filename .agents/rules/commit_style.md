# Commit Message Style Guide

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
1. **Subject Line**:
   - The first line of a commit message serves as a summary.
   - Capitalize after the type prefix (e.g. `feat: Implement AST symbol extractor`).
   - Omit any trailing punctuation.
   - Aim for about 50 characters (give or take).
   - Write in the imperative tense: e.g. "Fix bug" and not "Fixed bug" or "Fixes bug".

2. **Body**:
   - When a body is needed, add a blank line after the subject line.
   - Hard wrap one or more paragraphs to 72 characters per line.

3. **Author Rules**:
   - DO NOT mention co-author (no `Co-authored-by` or similar co-author lines).

### Git Push Policy
- DO NOT automatically push commits to remote (`git push`).
- Keep all commits strictly local unless the user explicitly requests a push.

### Code Quality & Atomic Commits
- Keep commits logical, modular, and self-contained.
- Ensure every commit compiles cleanly and passes its unit tests.

