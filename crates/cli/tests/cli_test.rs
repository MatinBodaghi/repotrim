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
