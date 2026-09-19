# Troubleshooting & Frequently Asked Questions (FAQ)

This guide provides actionable remedies for common configuration, environmental, and runtime issues encountered when running RepoTrim CLI or the MCP server.

---

## 1. Toolchain & Compilation Errors

### Issue: `error[E0658]: use of unstable library feature` or `edition2024` errors

**Symptom:**
Compilation fails when attempting to compile dependencies or crates on older Rust versions (< 1.90.0).

**Root Cause:**
RepoTrim's Minimum Supported Rust Version (MSRV) is **Rust 1.90.0**. Older compilers lack support for required workspace features and edition idioms.

**Solution:**
Update your Rust toolchain to the latest stable release:
```bash
rustup update stable
rustup default stable
```
Alternatively, ensure `rustup` detects the pinned toolchain:
```bash
rustup show
```

---

### Issue: `fatal error: tree_sitter/parser.h: No such file or directory` or `cc: command not found`

**Symptom:**
`cargo build` fails while compiling `tree-sitter`, `tree-sitter-rust`, or other language parser crates.

**Root Cause:**
Tree-sitter requires a local C compiler (`cc`, `gcc`, `clang`, or MSVC `cl.exe`) to compile native C grammars.

**Solution:**
Install the required platform C build tools:
- **Ubuntu / Debian:** `sudo apt-get install build-essential`
- **Fedora / RHEL:** `sudo dnf groupinstall "Development Tools"`
- **macOS:** `xcode-select --install`
- **Windows:** Install *"Desktop development with C++"* via Visual Studio Installer.

---

## 2. Model Context Protocol (MCP) Issues

### Issue: `JSON-RPC error -32602: Path ... is outside allowed roots`

**Symptom:**
An AI assistant invokes an MCP tool (e.g., `trim_context`, `find_symbols`) and receives:
```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "error": {
    "code": -32602,
    "message": "Path /path/to/repo is outside allowed roots [/...]"
  }
}
```

**Root Cause:**
RepoTrim enforces strict filesystem sandboxing via `RootGuard`. The MCP server rejects any request targeting a path not explicitly permitted at launch.

**Solution:**
Configure the `--allow-root` flag in your MCP client settings to encompass your workspace directory.

**Claude Desktop Configuration (`claude_desktop_config.json`):**
```json
{
  "mcpServers": {
    "repotrim": {
      "command": "repotrim-mcp",
      "args": [
        "--allow-root", "/Users/yourname/projects/my-repo"
      ]
    }
  }
}
```

**Multiple Projects:**
You can specify multiple `--allow-root` arguments:
```json
"args": [
  "--allow-root", "/Users/yourname/work",
  "--allow-root", "/Users/yourname/personal"
]
```
Or set the environment variable:
```bash
export REPOTRIM_ALLOW_ROOT="/Users/yourname/projects"
```

---

### Issue: MCP Server Hangs or AI Client Reports "Failed to connect to stdio transport"

**Symptom:**
The MCP client times out during initialization or disconnects unexpectedly.

**Root Cause:**
In stdio transport, stdout is reserved strictly for JSON-RPC messages. If any logging or debugging messages are emitted to stdout, the JSON-RPC pipe becomes corrupted.

**Verification & Diagnosis:**
1. Test running the server in isolation from the terminal:
   ```bash
   repotrim-mcp --help
   ```
2. Enable structured tracing on stderr (never pollutes stdout):
   ```bash
   RUST_LOG=repotrim_mcp=debug repotrim-mcp
   ```
3. Send a test `initialize` JSON-RPC handshake over stdin:
   ```json
   {"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"test","version":"1.0"}}}
   ```

---

## 3. Ingestion & Cache Issues

### Issue: Modifications to Source Code Not Reflected in Output

**Symptom:**
You edited a function or interface, but `repotrim select` or MCP context queries return outdated signatures.

**Root Cause:**
RepoTrim stores incremental parsed graphs in the OS user cache directory (`dirs::cache_dir()/repotrim/<root-blake3>/`). While cache verification uses both mtimes and blake3 hashes, external tools modifying file timestamps or git branch resets can occasionally retain stale cached state.

**Solution:**
1. Clean the incremental cache for your project:
   ```bash
   repotrim clean
   ```
2. Or run without using cached state:
   ```bash
   repotrim select --seed <Symbol> --budget 500 --no-cache
   ```

---

### Issue: Oversized Files Skipped During Analysis

**Symptom:**
Warning in stderr: `Skipping oversized file ... (exceeds limit of 2097152 bytes)`.

**Root Cause:**
To prevent memory exhaustion and runaway token costs, RepoTrim skips files larger than 2 MB (`DEFAULT_MAX_FILE_BYTES`). Minified JavaScript bundles or large generated datasets are automatically excluded from the Code Property Graph.

**Solution:**
Ensure generated files or minified bundles are listed in your project's `.gitignore`. RepoTrim automatically honors `.gitignore` rules during traversal.

---

## 4. Git & Diff Integration Issues

### Issue: `Diff error: invalid git revision`

**Symptom:**
`repotrim select --from-diff` or `analyze_impact` fails with an invalid revision error.

**Root Cause:**
To eliminate subprocess argument injection hazards, revision strings are validated against a strict alphanumeric pattern (`^[A-Za-z0-9._/~^@{}-]+$`). Flags starting with `-` (such as `--output`) are rejected.

**Solution:**
Ensure revisions are standard branch names, tags, or commit hashes:
```bash
# Valid revision arguments:
repotrim select --from-diff --base HEAD~1
repotrim select --from-diff --base origin/main
repotrim select --from-diff --base v0.11.0
```
