# Installation Guide

RepoTrim provides multiple installation methods depending on your development environment, workflow, and deployment preferences.

---

## 1. Quick Installation via Cargo

If you have Rust and Cargo installed, install the latest stable version directly from crates.io:

```bash
cargo install repotrim
```

To install the standalone dedicated Model Context Protocol (MCP) server:

```bash
cargo install repotrim-mcp
```

*(Note: The main `repotrim` binary also includes the MCP server via `repotrim mcp`)*.

---

## 2. Pre-Compiled Binary Releases

Pre-compiled release binaries are built with full optimizations and published for all major operating systems on [GitHub Releases](https://github.com/matinbodaghi/repotrim/releases).

### Supported Platforms & Targets

| Operating System | Architecture | Target Triple | Archive Format |
| :--- | :--- | :--- | :--- |
| **Linux** | x86_64 | `x86_64-unknown-linux-gnu` | `.tar.gz` |
| **Linux** | ARM64 / AArch64 | `aarch64-unknown-linux-gnu` | `.tar.gz` |
| **macOS** | Intel x86_64 | `x86_64-apple-darwin` | `.tar.gz` |
| **macOS** | Apple Silicon (M1/M2/M3/M4) | `aarch64-apple-darwin` | `.tar.gz` |
| **Windows** | x86_64 | `x86_64-pc-windows-msvc` | `.zip` |

### Verifying SHA-256 Checksums

Every release asset is accompanied by an official `.sha256` checksum file. Verify the integrity of your download:

#### On Linux / macOS
```bash
# Verify downloaded archive matches checksum
sha256sum -c repotrim-v0.12.0-x86_64-unknown-linux-gnu.tar.gz.sha256
```

#### On Windows (PowerShell)
```powershell
# Compare calculated hash against checksum file
(Get-FileHash -Path .\repotrim-v0.12.0-x86_64-pc-windows-msvc.zip -Algorithm SHA256).Hash
Get-Content .\repotrim-v0.12.0-x86_64-pc-windows-msvc.zip.sha256
```

---

## 3. Building from Source

To compile the latest bleeding-edge version directly from the git repository:

### Prerequisites

- **Rust Toolchain:** Rust `1.90.0` or later (managed via [rustup](https://rustup.rs/)).
- **C Compiler:** Required by tree-sitter grammars:
  - **Linux:** `gcc` or `clang` (`sudo apt install build-essential` or equivalent).
  - **macOS:** Xcode Command Line Tools (`xcode-select --install`).
  - **Windows:** MSVC C++ Build Tools (via Visual Studio Installer).
- **Git:** Required for change-set diff resolution and repository cloning.

### Build Steps

```bash
# 1. Clone the repository
git clone https://github.com/matinbodaghi/repotrim.git
cd repotrim

# 2. Compile release binaries with all workspace targets
cargo build --release --workspace

# 3. Binaries will be generated in:
#    - target/release/repotrim       (CLI tool & subcommands)
#    - target/release/repotrim-mcp   (Dedicated MCP server)
```

To install the locally compiled binaries to your Cargo bin directory:

```bash
cargo install --path crates/cli
cargo install --path crates/mcp-server
```

---

## 4. Minimum Supported Rust Version (MSRV)

RepoTrim guarantees compatibility with **Rust 1.90.0** and above across all workspace crates.

The repository root includes a [`rust-toolchain.toml`](../rust-toolchain.toml) file:
```toml
[toolchain]
channel = "1.90"
components = ["rustfmt", "clippy"]
profile = "minimal"
```

When building inside a cloned repository, `rustup` will automatically synchronize and use Rust 1.90.

---

## 5. Verifying Installation

Verify that `repotrim` is correctly available on your `PATH`:

```bash
# Check version and help
repotrim --version
repotrim --help

# Verify MCP server binary
repotrim-mcp --version
```
