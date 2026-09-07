# Local-to-Server Deployment & Harness Execution Guide

This guide details how to transition RepoTrim and its agent harness workflows from a local development environment (e.g. Windows / macOS) to a remote server, cloud VM, or headless Docker container.

---

## 1. Architectural Differences: Local vs. Server

| Dimension | Local Development (Windows / macOS) | Remote Server / Container (Linux) |
| :--- | :--- | :--- |
| **Operating System** | Windows (`pwsh` / `cmd`) or macOS (`zsh`) | Linux (`bash` / Alpine / Debian) |
| **Path Conventions** | Windows backslashes (`\`) or mixed | POSIX forward slashes (`/`) |
| **Line Endings** | CRLF or LF (Git auto-conversion) | LF strictly |
| **Execution Mode** | Direct terminal CLI or IDE MCP plugin | Headless stdio MCP, SSH pipe, or Docker exec |
| **Memory / Cache** | Local temp / app data directories | `/tmp` or persistent mounted volumes |

> [!IMPORTANT]
> **RepoTrim Invariant:** RepoTrim's internal `SymbolNode.file_path` and tree-sitter path representations normalize paths to POSIX forward slashes (`/`) to ensure deterministic BLAKE3 hashes regardless of host OS.

---

## 2. Moving Code to the Server

### Option A: Via Git Remote
```bash
# On your local machine (when ready to push):
git push origin main

# On the remote server:
git clone https://github.com/matinbodaghi/repotrim.git
cd repotrim
```

### Option B: Via Direct Rsync / SSH (for Local Unpushed Development)
```bash
# Sync local repository to remote server excluding build artifacts
rsync -avz --exclude 'target/' --exclude '.git/' . user@server:/home/user/repotrim/
```

---

## 3. Server Environment Setup

Ensure the remote Linux environment has Rust and required build tools installed:

```bash
# Install Rust toolchain on Linux server
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
source $HOME/.cargo/env

# Ensure C compiler is available for tree-sitter C runtime
sudo apt-get update && sudo apt-get install -y build-essential
# (On RHEL/CentOS: sudo yum groupinstall "Development Tools")
# (On Alpine: apk add build-base)
```

---

## 4. Building the Production Binary

```bash
# Build optimized release binaries for engine and CLI
cargo build --release --workspace

# The binaries will be located at:
# target/release/repotrim       (CLI tool)
# target/release/repotrim-mcp   (Model Context Protocol server)
```

### Optional: Static Musl Build (Portable Single Binary)
If distributing across diverse Linux distributions without glibc dependency issues:
```bash
rustup target add x86_64-unknown-linux-musl
cargo build --release --target x86_64-unknown-linux-musl
```

---

## 5. Integrating with Remote Agent Harnesses

### A. Headless CLI Mode (SWE-bench / Shell Agents)
Inside remote automated agent loops, invoke RepoTrim directly to inject budget-aware context into the prompt:
```bash
# Extract optimal 4000-token context seeded at the target symbol or file
./target/release/repotrim select --budget 4000 --seed AstExtractor > context.md
```

### B. Remote Model Context Protocol (MCP) Mode
To connect local IDEs (Cursor, Antigravity, Claude Desktop) to a remote RepoTrim server via SSH:

Add the server configuration to your IDE's `mcpServers` JSON config:
```json
{
  "mcpServers": {
    "repotrim-remote": {
      "command": "ssh",
      "args": [
        "user@your-server-ip",
        "/home/user/repotrim/target/release/repotrim",
        "mcp",
        "--path",
        "/home/user/target-project"
      ]
    }
  }
}
```

### C. Dockerized Agent Harness
Run RepoTrim in an isolated container alongside the target codebase:
```dockerfile
FROM rust:1.80-slim as builder
WORKDIR /app
COPY . .
RUN cargo build --release

FROM debian:bookworm-slim
COPY --from=builder /app/target/release/repotrim /usr/local/bin/
COPY --from=builder /app/target/release/repotrim-mcp /usr/local/bin/
ENTRYPOINT ["repotrim", "mcp"]
```

---

## 6. Environment & Path Reference

| Configuration | Default | Purpose |
| :--- | :--- | :--- |
| Cache Directory | `.repotrim/` | Incremental BLAKE3 Merkle cache and bincode serialization |
| Stdio Protocol | JSON-RPC 2.0 | MCP server communication over `stdin`/`stdout` |
| Diagnostic Logs | `stderr` | All loader and indexing logs routed to avoid stdout pollution |

