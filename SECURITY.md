# Security Policy

RepoTrim is committed to ensuring the safety, integrity, and privacy of developer workspaces and agentic coding environments. This document outlines our threat model, security architecture, and procedures for reporting security vulnerabilities.

---

## Supported Versions

Security updates and critical patches are actively provided for the following versions:

| Version | Supported |
| :--- | :---: |
| `v1.0.x` | :white_check_mark: |
| `v0.11.x` | :white_check_mark: |
| `< 0.11.0` | :x: |

---

## Reporting a Vulnerability

If you discover a security vulnerability or potential exploit in RepoTrim:

1. **Do NOT open a public GitHub issue.** Public issues disclose attack vectors before patches can be deployed.
2. **Submit via GitHub Private Vulnerability Reporting** directly on the [RepoTrim repository](https://github.com/matinbodaghi/repotrim/security/advisories/new).
3. Alternatively, report the issue via email to the maintainers at: `security@repotrim.dev` (or maintainer contact listed in `Cargo.toml`).

### Disclosure Timeline & SLA
- **Acknowledgment:** Within 48 hours of initial report receipt.
- **Triage & Assessment:** Within 5 business days with preliminary CVSS severity score.
- **Fix & Advisory:** Coordinated release and security advisory published within 30 days of verification.

---

## Threat Model & Security Architecture

RepoTrim is designed to analyze potentially untrusted third-party codebases while being invoked by AI agents (e.g. via Model Context Protocol or CLI). We assume that the target repository being analyzed may contain hostile file structures, forged metadata, or adversarial inputs.

```
       +-------------------------------------------------------+
       |                  AI Coding Agent                      |
       +-------------------------------------------------------+
                                  |
                                  | JSON-RPC (stdio)
                                  v
+---------------------------------------------------------------------+
| RepoTrim MCP Server                                                 |
|                                                                     |
|  [RootGuard Boundary]  ---> Rejects path traversal (-32602)        |
|  [Argument Validator]  ---> Sanitizes git revisions & flags         |
|  [Zero-Panic Dispatch] ---> Returns typed errors, never panics      |
+---------------------------------------------------------------------+
        |                                            |
        | Safe disk reads                            | Safe cache lookups
        v                                            v
+-----------------------+              +------------------------------+
| Analyzed Codebase     |              | Externalized User Cache      |
| (<target_repo>/)      |              | (~/.cache/repotrim/<hash>/)  |
| - Read-only           |              | - BLAKE3 hash verified       |
| - No writes inside    |              | - 64 MB bincode limit        |
| - .repotrim/ ignored  |              | - Relocated away from repo   |
+-----------------------+              +------------------------------+
```

### 1. Path Confinement & Traversal Protection (`RootGuard`)
- **Strict Root Boundaries:** All filesystem resolution passes through `RootGuard`. Targets must reside inside explicitly authorized directories.
- **Allowed Roots Configuration:** By default, the allowed root is restricted to the launch directory. Additional directories may be explicitly granted via `--allow-root <PATH>` CLI flags or the `REPOTRIM_ALLOWED_ROOTS` environment variable.
- **Traversal Prevention:** Lexical escapes (`..`), symlink redirections escaping authorized boundaries, or arbitrary absolute paths (e.g., `/etc/passwd` or `C:\Windows\System32`) are rejected prior to filesystem interaction with JSON-RPC error code `-32602` (`Invalid params`).
- **Path Normalization:** On Windows platforms, verbatim prefix paths (`\\?\`) are normalized to prevent boundary-check bypasses.

### 2. Cache Isolation & Untrusted Deserialization Hardening
- **Zero Unsolicited Codebase Writes:** RepoTrim never writes cache files or metadata inside the analyzed codebase directory.
- **Relocated Cache Storage:** Binary caches (`cache.bin`, `coedit.bin`) reside exclusively in the external user OS cache directory (`dirs::cache_dir() / repotrim / <blake3(root)>`).
- **Hostile Cache Fixture Isolation:** Any pre-existing or forged `.repotrim/` folder inside an analyzed repository is completely ignored.
- **Deserialization Buffer Limits:** Bincode deserialization enforces a strict 64 MB maximum buffer limit (`bincode::DefaultOptions::new().with_limit(64 * 1024 * 1024)`), neutralizing memory exhaustion attacks.
- **Cryptographic BLAKE3 Verification:** Modification-time-only cache trust is deprecated. Cache entries are accepted only if the disk bytes match the stored 32-byte cryptographic BLAKE3 content hash.

### 3. Git Subprocess Isolation & Argument Sanitization
- **Revision Whitelist:** Git revisions passed to differential analysis (`get_git_diff_against`, `analyze_impact`) are strictly validated against `^[A-Za-z0-9._/~^@{}-]+$`.
- **Flag Injection Rejection:** Any revision parameter starting with a dash (`-`) (e.g., `--output=/path` or `-o/path`) is rejected immediately before process creation.
- **Hook Neutralization:** All git subprocesses run with `-c core.hooksPath=/dev/null`, preventing arbitrary command execution via malicious repository hooks.
- **Interactive Pager Prevention:** Subprocesses run with `--no-pager` to prevent terminal or process hijacking.
- **Positional Argument Delimiters:** Revision arguments are isolated with `--` separators from subsequent arguments.

### 4. Panic Elimination & Denial-of-Service Defense
- **Zero Unhandled Panics:** Library crate roots enforce `#![deny(clippy::unwrap_used, clippy::expect_used)]`.
- **Structured Error Propagation:** All errors are represented as typed `EngineError` or structured JSON-RPC error responses. Malformed user inputs never crash the engine or server process.
