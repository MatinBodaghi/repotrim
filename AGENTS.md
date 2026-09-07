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

## Atomic Commits & Phase Granularity
- Split each development phase into multiple small, logical, self-contained atomic commits based on the sub-components/modules rather than one single commit for the whole phase.
- Each intermediate commit must compile cleanly and pass its respective unit tests.

## Post-Phase Dogfooding & Comparative Evolution Protocol
- After completing every development phase:
  1. Run/create a dedicated dogfood integration test benchmarking RepoTrim on its own repository.
  2. Record and preserve the dogfood results alongside the previous phase's results for historical comparison (e.g. `docs/benchmarks/dogfood_phaseX.md`).
  3. Compare the new metrics directly against the prior phase (latency, token reduction %, edge connectivity, struct/method coverage).
  4. Assess whether the empirical findings indicate necessary roadmap or phase adjustments and report recommendations to the user before starting the next phase.

