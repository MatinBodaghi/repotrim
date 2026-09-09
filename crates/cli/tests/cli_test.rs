use std::path::PathBuf;
use std::process::Command;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[test]
fn test_cli_help() {
    let output = Command::new(env!("CARGO_BIN_EXE_repotrim"))
        .arg("--help")
        .output()
        .expect("Failed to execute repotrim --help");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("repotrim"));
    assert!(stdout.contains("select"));
    assert!(stdout.contains("stats"));
    assert!(stdout.contains("inspect"));
    assert!(stdout.contains("clean"));
    assert!(stdout.contains("mcp"));
}

#[test]
fn test_cli_stats() {
    let output = Command::new(env!("CARGO_BIN_EXE_repotrim"))
        .args(["stats", "--path"])
        .arg(repo_root())
        .output()
        .expect("Failed to execute repotrim stats");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("REPOTRIM CODEBASE GRAPH STATISTICS"));
    assert!(stdout.contains("Total Symbols:"));
    assert!(stdout.contains("Resolved Edges:"));
    assert!(stdout.contains("Top Architectural Hubs"));
}

#[test]
fn test_cli_inspect_symbol() {
    let output = Command::new(env!("CARGO_BIN_EXE_repotrim"))
        .args(["inspect", "--symbol", "ContextSelector", "--path"])
        .arg(repo_root())
        .output()
        .expect("Failed to execute repotrim inspect");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("ContextSelector"));
    assert!(stdout.contains("Outgoing Graph Dependencies"));
    assert!(stdout.contains("Incoming Callers & References"));
}

#[test]
fn test_cli_select_markdown() {
    let output = Command::new(env!("CARGO_BIN_EXE_repotrim"))
        .args([
            "select",
            "--seed",
            "ContextSelector",
            "--budget",
            "300",
            "--path",
        ])
        .arg(repo_root())
        .output()
        .expect("Failed to execute repotrim select");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("### File:"));
    assert!(stdout.contains("```rust"));
    assert!(stdout.contains("ContextSelector"));
}

#[test]
fn test_cli_select_json() {
    let output = Command::new(env!("CARGO_BIN_EXE_repotrim"))
        .args([
            "select",
            "--seed",
            "ContextSelector",
            "--budget",
            "200",
            "--format",
            "json",
            "--path",
        ])
        .arg(repo_root())
        .output()
        .expect("Failed to execute repotrim select with json");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: serde_json::Value = serde_json::from_str(&stdout).expect("Valid JSON output");
    assert!(parsed.get("budget").is_some());
    assert!(parsed.get("symbols").is_some());
    assert!(parsed.get("markdown").is_some());
}

#[test]
fn test_cli_clean_and_incremental_cache() {
    let temp_dir =
        std::env::temp_dir().join(format!("repotrim_cli_cache_test_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&temp_dir);
    std::fs::create_dir_all(temp_dir.join("src")).unwrap();

    let file_path = temp_dir.join("src/main.rs");
    std::fs::write(
        &file_path,
        "pub fn compute_sum(a: i32, b: i32) -> i32 { a + b }",
    )
    .unwrap();

    // 1. Cold stats run
    let output_cold = Command::new(env!("CARGO_BIN_EXE_repotrim"))
        .args(["stats", "--path"])
        .arg(&temp_dir)
        .output()
        .expect("Failed to execute stats cold");
    assert!(output_cold.status.success());
    let _stderr_cold = String::from_utf8_lossy(&output_cold.stderr);
    let stdout_cold = String::from_utf8_lossy(&output_cold.stdout);
    assert!(stdout_cold.contains("Incremental Cache:"));
    assert!(temp_dir.join(".repotrim/cache.bin").exists());

    // 2. Warm stats run (cache hit)
    let output_warm = Command::new(env!("CARGO_BIN_EXE_repotrim"))
        .args(["stats", "--path"])
        .arg(&temp_dir)
        .output()
        .expect("Failed to execute stats warm");
    assert!(output_warm.status.success());
    let stdout_warm = String::from_utf8_lossy(&output_warm.stdout);
    assert!(stdout_warm.contains("1/1 files cached (100.0% warm hit)"));

    // 3. No-cache stats run
    let output_nocache = Command::new(env!("CARGO_BIN_EXE_repotrim"))
        .args(["stats", "--no-cache", "--path"])
        .arg(&temp_dir)
        .output()
        .expect("Failed to execute stats --no-cache");
    assert!(output_nocache.status.success());
    let stdout_nocache = String::from_utf8_lossy(&output_nocache.stdout);
    assert!(stdout_nocache.contains("Disabled (--no-cache)"));

    // 4. Select with warm cache
    let output_select = Command::new(env!("CARGO_BIN_EXE_repotrim"))
        .args(["select", "--seed", "compute_sum", "--path"])
        .arg(&temp_dir)
        .output()
        .expect("Failed to execute select");
    assert!(output_select.status.success());
    let stdout_select = String::from_utf8_lossy(&output_select.stdout);
    assert!(stdout_select.contains("pub fn compute_sum"));

    // 5. Clean cache command
    let output_clean = Command::new(env!("CARGO_BIN_EXE_repotrim"))
        .args(["clean", "--path"])
        .arg(&temp_dir)
        .output()
        .expect("Failed to execute clean");
    assert!(output_clean.status.success());
    assert!(!temp_dir.join(".repotrim").exists());

    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_cli_select_query() {
    let output = Command::new(env!("CARGO_BIN_EXE_repotrim"))
        .args([
            "select",
            "--query",
            "estimate tokens",
            "--budget",
            "300",
            "--path",
        ])
        .arg(repo_root())
        .output()
        .expect("Failed to execute repotrim select --query");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("estimate_tokens") || stdout.contains("tokens"));
}

#[test]
fn test_cli_select_missing_args_error() {
    let output = Command::new(env!("CARGO_BIN_EXE_repotrim"))
        .args(["select", "--path"])
        .arg(repo_root())
        .output()
        .expect("Failed to execute repotrim select with no seed or query");

    // Should fail with error message
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("At least one seed source must be provided"));
}

#[test]
fn test_cli_blueprint() {
    let temp_file =
        std::env::temp_dir().join(format!("repotrim_bp_test_{}.md", std::process::id()));
    let _ = std::fs::remove_file(&temp_file);

    let output = Command::new(env!("CARGO_BIN_EXE_repotrim"))
        .args(["blueprint", "estimate tokens with fast BPE", "--path"])
        .arg(repo_root())
        .args(["--output", temp_file.to_str().unwrap()])
        .output()
        .expect("Failed to execute repotrim blueprint");

    assert!(output.status.success());
    assert!(temp_file.exists());

    let content = std::fs::read_to_string(&temp_file).expect("Read blueprint file");
    assert!(content.contains("# Feature Blueprint: estimate tokens with fast BPE"));
    assert!(content.contains("Key Symbol Targets"));
    assert!(content.contains("estimate_tokens"));
    assert!(content.contains("trim_context"));

    let _ = std::fs::remove_file(&temp_file);
}

#[test]
fn test_cli_architecture() {
    let temp_file =
        std::env::temp_dir().join(format!("repotrim_arch_test_{}.md", std::process::id()));
    let _ = std::fs::remove_file(&temp_file);

    let output = Command::new(env!("CARGO_BIN_EXE_repotrim"))
        .args(["architecture", "--path"])
        .arg(repo_root())
        .args(["--output", temp_file.to_str().unwrap()])
        .output()
        .expect("Failed to execute repotrim architecture");

    assert!(output.status.success());
    assert!(temp_file.exists());

    let content = std::fs::read_to_string(&temp_file).expect("Read architecture file");
    assert!(content.contains("# Repository Architecture & Subsystem Specification"));
    assert!(content.contains("## 1. Executive Summary & Graph Modularity"));
    assert!(content.contains("## 2. Architectural Dependency Graph"));
    assert!(content.contains("```mermaid\nflowchart TD"));
    assert!(content.contains("## 3. Subsystem Community Catalog"));
    assert!(content.contains("## 4. Architectural Layers & Responsibilities"));
    assert!(content.contains("## 5. Central Architectural Hubs"));
    assert!(content.contains("## 6. Public API & Key Interface Catalog"));

    let _ = std::fs::remove_file(&temp_file);

    // Also test stdout output with `--output -`
    let stdout_output = Command::new(env!("CARGO_BIN_EXE_repotrim"))
        .args(["architecture", "--path"])
        .arg(repo_root())
        .args(["--output", "-"])
        .output()
        .expect("Failed to execute repotrim architecture to stdout");

    assert!(stdout_output.status.success());
    let stdout_str = String::from_utf8_lossy(&stdout_output.stdout);
    assert!(stdout_str.contains("# Repository Architecture & Subsystem Specification"));
}
