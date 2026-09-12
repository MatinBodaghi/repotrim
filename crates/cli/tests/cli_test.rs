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
    assert!(stdout.contains("coedit"));
    assert!(stdout.contains("community"));
    assert!(stdout.contains("mcp"));
    assert!(stdout.contains("watch"));
}

#[test]
fn test_cli_watch_help() {
    let output = Command::new(env!("CARGO_BIN_EXE_repotrim"))
        .args(["watch", "--help"])
        .output()
        .expect("Failed to execute repotrim watch --help");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("--debounce"));
    assert!(stdout.contains("--path"));
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

#[test]
fn test_cli_select_auto_budget_markdown() {
    let output = Command::new(env!("CARGO_BIN_EXE_repotrim"))
        .args([
            "select",
            "--seed",
            "ContextSelector",
            "--budget",
            "auto",
            "--model",
            "claude",
            "--path",
        ])
        .arg(repo_root())
        .output()
        .expect("Failed to execute repotrim select --budget auto");

    assert!(output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Auto-budget tuned to"));
    assert!(stderr.contains("via Knee-Curve"));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("ContextSelector"));
}

#[test]
fn test_cli_select_auto_budget_json() {
    let output = Command::new(env!("CARGO_BIN_EXE_repotrim"))
        .args([
            "select",
            "--seed",
            "ContextSelector",
            "--budget",
            "auto",
            "--model",
            "gpt-4o",
            "--format",
            "json",
            "--path",
        ])
        .arg(repo_root())
        .output()
        .expect("Failed to execute repotrim select --budget auto with json");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: serde_json::Value = serde_json::from_str(&stdout).expect("Valid JSON output");
    assert!(parsed.get("budget").is_some());
    assert!(parsed.get("auto_budget").is_some());
    let auto_budget = &parsed["auto_budget"];
    assert_eq!(auto_budget["model"], "gpt-4o");
    assert!(auto_budget["knee_tokens"].as_u64().unwrap() > 0);
    assert!(auto_budget["knee_utility_ratio"].as_f64().unwrap() >= 0.0);
}

#[test]
fn test_cli_blueprint_with_auto_budget() {
    let output = Command::new(env!("CARGO_BIN_EXE_repotrim"))
        .args([
            "blueprint",
            "add submodular scoring",
            "--budget",
            "auto",
            "--model",
            "deepseek",
            "--output",
            "-",
            "--path",
        ])
        .arg(repo_root())
        .output()
        .expect("Failed to execute repotrim blueprint with auto budget");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("--budget auto"));
    assert!(stdout.contains("--model deepseek"));
    assert!(stdout.contains("\"budget\": \"auto\""));
    assert!(stdout.contains("\"model\": \"deepseek\""));
}

#[test]
fn test_cli_select_with_exact_tokenizer() {
    let output = Command::new(env!("CARGO_BIN_EXE_repotrim"))
        .args([
            "select",
            "--seed",
            "estimate_tokens",
            "--budget",
            "500",
            "--tokenizer",
            "exact",
            "--format",
            "json",
            "--path",
        ])
        .arg(repo_root())
        .output()
        .expect("Failed to execute repotrim select --tokenizer exact");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: serde_json::Value = serde_json::from_str(&stdout).expect("Valid JSON output");
    assert!(parsed.get("tokenizer").is_some());
    assert!(parsed["tokenizer"].as_str().unwrap().contains("Exact BPE"));
    assert!(parsed["tokens_used"].as_u64().unwrap() > 0);
}

#[test]
fn test_cli_blueprint_with_tokenizer() {
    let output = Command::new(env!("CARGO_BIN_EXE_repotrim"))
        .args([
            "blueprint",
            "exact token accounting",
            "--budget",
            "2000",
            "--tokenizer",
            "exact",
            "--output",
            "-",
            "--path",
        ])
        .arg(repo_root())
        .output()
        .expect("Failed to execute repotrim blueprint with tokenizer");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("--tokenizer exact"));
    assert!(stdout.contains("\"tokenizer\": \"exact\""));
}

#[test]
fn test_cli_select_with_joint_lod() {
    let output = Command::new(env!("CARGO_BIN_EXE_repotrim"))
        .args([
            "select",
            "--seed",
            "ContextSelector",
            "--budget",
            "500",
            "--joint-lod",
            "--format",
            "json",
            "--path",
        ])
        .arg(repo_root())
        .output()
        .expect("Failed to execute repotrim select --joint-lod");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: serde_json::Value = serde_json::from_str(&stdout).expect("Valid JSON output");
    assert!(parsed.get("joint_lod").is_some());
    assert_eq!(parsed["joint_lod"]["enabled"], true);
    assert!(parsed["joint_lod"]["total_tokens"].as_u64().unwrap() <= 500);
}

#[test]
fn test_cli_coedit_help() {
    let output = Command::new(env!("CARGO_BIN_EXE_repotrim"))
        .args(["coedit", "--help"])
        .output()
        .expect("Failed to execute repotrim coedit --help");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("--max-commits"));
    assert!(stdout.contains("--half-life-days"));
    assert!(stdout.contains("--min-support"));
    assert!(stdout.contains("--min-confidence"));
    assert!(stdout.contains("--json"));
}

#[test]
fn test_cli_coedit_run() {
    let output = Command::new(env!("CARGO_BIN_EXE_repotrim"))
        .args(["coedit", "--max-commits", "30", "--path"])
        .arg(repo_root())
        .output()
        .expect("Failed to execute repotrim coedit");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("REPOTRIM GIT CO-EDIT MINING & WEIGHT LEARNING"));
    assert!(stdout.contains("Commits Analyzed:"));
    assert!(stdout.contains("Layer Empirical Correlation & Learned Weights:"));
    assert!(stdout.contains("Empirical Ranking Validation (MRR):"));
}

#[test]
fn test_cli_coedit_json() {
    let output = Command::new(env!("CARGO_BIN_EXE_repotrim"))
        .args(["coedit", "--max-commits", "30", "--json", "--path"])
        .arg(repo_root())
        .output()
        .expect("Failed to execute repotrim coedit --json");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: serde_json::Value =
        serde_json::from_str(&stdout).expect("Valid JSON from repotrim coedit");
    assert!(parsed.get("head_hash").is_some());
    assert!(parsed.get("commits_analyzed").is_some());
    assert!(parsed.get("layer_stats").is_some());
    assert!(parsed.get("learned_weights").is_some());
}

#[test]
fn test_cli_select_with_coedit_and_learned_weights() {
    let output = Command::new(env!("CARGO_BIN_EXE_repotrim"))
        .args([
            "select",
            "--seed",
            "ContextSelector",
            "--budget",
            "500",
            "--coedit",
            "--learn-weights",
            "--path",
        ])
        .arg(repo_root())
        .output()
        .expect("Failed to execute repotrim select with coedit and learned weights");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("ContextSelector"));
    assert!(stdout.contains("### File:"));
}

#[test]
fn test_cli_community_help() {
    let output = Command::new(env!("CARGO_BIN_EXE_repotrim"))
        .args(["community", "--help"])
        .output()
        .expect("Failed to execute repotrim community --help");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("--resolution"));
    assert!(stdout.contains("--hierarchy"));
    assert!(stdout.contains("--drift"));
}

#[test]
fn test_cli_community_run() {
    let output = Command::new(env!("CARGO_BIN_EXE_repotrim"))
        .args(["community", "--path"])
        .arg(repo_root())
        .output()
        .expect("Failed to execute repotrim community");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Community Name"));
    assert!(stdout.contains("Density"));
    assert!(stdout.contains("Dominant Dir"));
    assert!(stdout.contains("Purity"));
}

#[test]
fn test_cli_community_json() {
    let output = Command::new(env!("CARGO_BIN_EXE_repotrim"))
        .args(["community", "--json", "--path"])
        .arg(repo_root())
        .output()
        .expect("Failed to execute repotrim community --json");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: serde_json::Value =
        serde_json::from_str(&stdout).expect("Valid JSON from repotrim community");
    assert!(parsed.get("resolution").is_some());
    assert!(parsed.get("modularity").is_some());
    assert!(parsed.get("communities").is_some());
}

#[test]
fn test_cli_community_hierarchy() {
    let output = Command::new(env!("CARGO_BIN_EXE_repotrim"))
        .args(["community", "--hierarchy", "--path"])
        .arg(repo_root())
        .output()
        .expect("Failed to execute repotrim community --hierarchy");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Macro Subsystems"));
    assert!(stdout.contains("Meso Modules"));
    assert!(stdout.contains("Micro Components"));
}

#[test]
fn test_cli_community_drift() {
    let output = Command::new(env!("CARGO_BIN_EXE_repotrim"))
        .args(["community", "--drift", "--path"])
        .arg(repo_root())
        .output()
        .expect("Failed to execute repotrim community --drift");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Architectural Drift Analysis"));
}

#[test]
fn test_cli_select_with_community_boost() {
    let output = Command::new(env!("CARGO_BIN_EXE_repotrim"))
        .args([
            "select",
            "--seed",
            "ContextSelector",
            "--budget",
            "500",
            "--community-boost",
            "0.35",
            "--path",
        ])
        .arg(repo_root())
        .output()
        .expect("Failed to execute repotrim select with community boost");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("ContextSelector"));
    assert!(stdout.contains("### File:"));
}

#[test]
fn test_cli_architecture_with_resolution() {
    let output = Command::new(env!("CARGO_BIN_EXE_repotrim"))
        .args([
            "architecture",
            "--resolution",
            "1.5",
            "--output",
            "-",
            "--path",
        ])
        .arg(repo_root())
        .output()
        .expect("Failed to execute repotrim architecture --resolution");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("# Repository Architecture & Subsystem Specification"));
    assert!(stdout.contains("Resolution"));
}

#[test]
fn test_cli_query_help() {
    let output = Command::new(env!("CARGO_BIN_EXE_repotrim"))
        .args(["query", "--help"])
        .output()
        .expect("Failed to execute repotrim query --help");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout
        .contains("Search codebase symbols via hybrid BM25+ and dense subword semantic retrieval"));
    assert!(stdout.contains("--mode"));
    assert!(stdout.contains("--expand"));
    assert!(stdout.contains("--explain"));
}

#[test]
fn test_cli_query_basic_tabular() {
    let output = Command::new(env!("CARGO_BIN_EXE_repotrim"))
        .args(["query", "estimate tokens bpe", "--limit", "3", "--path"])
        .arg(repo_root())
        .output()
        .expect("Failed to execute repotrim query");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Rank"));
    assert!(stdout.contains("Score"));
    assert!(stdout.contains("Symbol Name"));
    assert!(stdout.contains("estimate_tokens"));
}

#[test]
fn test_cli_query_json() {
    let output = Command::new(env!("CARGO_BIN_EXE_repotrim"))
        .args([
            "query",
            "estimate tokens",
            "--limit",
            "2",
            "--json",
            "--path",
        ])
        .arg(repo_root())
        .output()
        .expect("Failed to execute repotrim query --json");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: serde_json::Value =
        serde_json::from_str(&stdout).expect("Valid JSON from repotrim query");
    assert_eq!(parsed["query"], "estimate tokens");
    assert_eq!(parsed["mode"], "hybrid");
    assert!(parsed.get("results").is_some());
    let results = parsed["results"].as_array().unwrap();
    assert_eq!(results.len(), 2);
    assert!(results[0].get("bm25_score").is_some());
    assert!(results[0].get("dense_score").is_some());
    assert!(results[0].get("rrf_score").is_some());
}

#[test]
fn test_cli_query_explain() {
    let output = Command::new(env!("CARGO_BIN_EXE_repotrim"))
        .args([
            "query",
            "estimate tokens",
            "--limit",
            "2",
            "--explain",
            "--path",
        ])
        .arg(repo_root())
        .output()
        .expect("Failed to execute repotrim query --explain");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("BM25:"));
    assert!(stdout.contains("Dense:"));
    assert!(stdout.contains("RRF:"));
    assert!(stdout.contains("Matched:"));
}

#[test]
fn test_cli_query_lexical_and_dense_modes() {
    // 1. Lexical Mode
    let out_lex = Command::new(env!("CARGO_BIN_EXE_repotrim"))
        .args([
            "query",
            "estimate tokens",
            "--mode",
            "lexical",
            "--limit",
            "2",
            "--path",
        ])
        .arg(repo_root())
        .output()
        .expect("Failed to execute repotrim query --mode lexical");
    assert!(out_lex.status.success());

    // 2. Dense Mode
    let out_dense = Command::new(env!("CARGO_BIN_EXE_repotrim"))
        .args([
            "query",
            "estimate tokens",
            "--mode",
            "dense",
            "--limit",
            "2",
            "--path",
        ])
        .arg(repo_root())
        .output()
        .expect("Failed to execute repotrim query --mode dense");
    assert!(out_dense.status.success());
}

#[test]
fn test_cli_query_with_expansion() {
    let output = Command::new(env!("CARGO_BIN_EXE_repotrim"))
        .args(["query", "jwt token", "--expand", "--limit", "3", "--path"])
        .arg(repo_root())
        .output()
        .expect("Failed to execute repotrim query --expand");

    assert!(output.status.success());
}

#[test]
fn test_cli_select_with_retrieval_mode() {
    let output = Command::new(env!("CARGO_BIN_EXE_repotrim"))
        .args([
            "select",
            "--query",
            "token estimation",
            "--retrieval-mode",
            "hybrid",
            "--query-expand",
            "--budget",
            "500",
            "--path",
        ])
        .arg(repo_root())
        .output()
        .expect("Failed to execute repotrim select with retrieval-mode");

    assert!(output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Inferred seeds from query 'token estimation' (mode: Hybrid)"));
}

#[test]
fn test_cli_benchmark_help() {
    let output = Command::new(env!("CARGO_BIN_EXE_repotrim"))
        .args(["benchmark", "--help"])
        .output()
        .expect("Failed to execute repotrim benchmark --help");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("--budget"));
    assert!(stdout.contains("--scenario"));
    assert!(stdout.contains("--strategies"));
    assert!(stdout.contains("--format"));
    assert!(stdout.contains("--json"));

    // Verify alias 'eval' also works
    let eval_output = Command::new(env!("CARGO_BIN_EXE_repotrim"))
        .args(["eval", "--help"])
        .output()
        .expect("Failed to execute repotrim eval --help");
    assert!(eval_output.status.success());
}

#[test]
fn test_cli_benchmark_table_scenario() {
    let output = Command::new(env!("CARGO_BIN_EXE_repotrim"))
        .args([
            "benchmark",
            "--scenario",
            "context_selector",
            "--strategies",
            "aider,full",
            "--budget",
            "600",
            "--path",
        ])
        .arg(repo_root())
        .output()
        .expect("Failed to execute repotrim benchmark");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Scenario: ContextSelector"));
    assert!(stdout.contains("Aider Repo Map"));
    assert!(stdout.contains("RepoTrim Full"));
    assert!(stdout.contains("Tokens"));
    assert!(stdout.contains("Reduct%"));
    assert!(stdout.contains("DirRec%"));
    assert!(stdout.contains("Aggregate Benchmark Summary"));
}

#[test]
fn test_cli_benchmark_json_output() {
    let output = Command::new(env!("CARGO_BIN_EXE_repotrim"))
        .args([
            "eval",
            "--scenario",
            "ppr_solver",
            "--strategies",
            "vanilla,full",
            "--json",
            "--path",
        ])
        .arg(repo_root())
        .output()
        .expect("Failed to execute repotrim eval --json");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let v: serde_json::Value = serde_json::from_str(&stdout).expect("Valid JSON report");
    assert_eq!(v["scenarios_evaluated"], 1);
    assert!(v["metrics"].as_array().unwrap().len() >= 2);
    assert!(v["summary"].as_array().unwrap().len() >= 2);
}

#[test]
fn test_cli_locate_text_and_json() {
    let output = Command::new(env!("CARGO_BIN_EXE_repotrim"))
        .args(["locate", "context selector", "--limit", "3", "--path"])
        .arg(repo_root())
        .output()
        .expect("Failed to execute repotrim locate");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("REPOTRIM TASK-CONDITIONED ENTRYPOINT DISCOVERY"));
    assert!(stdout.contains("select"));

    let json_output = Command::new(env!("CARGO_BIN_EXE_repotrim"))
        .args([
            "locate",
            "context selector",
            "--limit",
            "3",
            "--json",
            "--path",
        ])
        .arg(repo_root())
        .output()
        .expect("Failed to execute repotrim locate --json");

    assert!(json_output.status.success());
    let json_stdout = String::from_utf8_lossy(&json_output.stdout);
    let v: serde_json::Value = serde_json::from_str(&json_stdout).expect("Valid JSON report");
    assert_eq!(v["query"], "context selector");
    assert!(!v["entrypoints"].as_array().unwrap().is_empty());
}

#[test]
fn test_cli_trace_text_and_json() {
    let output = Command::new(env!("CARGO_BIN_EXE_repotrim"))
        .args([
            "trace",
            "select_structured_context",
            "format_markdown",
            "--path",
        ])
        .arg(repo_root())
        .output()
        .expect("Failed to execute repotrim trace");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("REPOTRIM CAUSAL PATH TRACE"));
    assert!(stdout.contains("sequenceDiagram"));

    let json_output = Command::new(env!("CARGO_BIN_EXE_repotrim"))
        .args([
            "trace",
            "select_structured_context",
            "format_markdown",
            "--json",
            "--path",
        ])
        .arg(repo_root())
        .output()
        .expect("Failed to execute repotrim trace --json");

    assert!(json_output.status.success());
    let json_stdout = String::from_utf8_lossy(&json_output.stdout);
    let v: serde_json::Value = serde_json::from_str(&json_stdout).expect("Valid JSON report");
    assert_eq!(v["source"]["name"], "select_structured_context");
    assert_eq!(v["target"]["name"], "format_markdown");
    assert!(!v["paths"].as_array().unwrap().is_empty());
}

#[test]
fn test_cli_expand_text_and_json() {
    let output = Command::new(env!("CARGO_BIN_EXE_repotrim"))
        .args(["expand", "ContextSelector", "--budget", "300", "--path"])
        .arg(repo_root())
        .output()
        .expect("Failed to execute repotrim expand");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("REPOTRIM LOCAL SUBMODULAR CONTEXT EXPANSION"));
    assert!(stdout.contains("ContextSelector"));

    let json_output = Command::new(env!("CARGO_BIN_EXE_repotrim"))
        .args([
            "expand",
            "ContextSelector",
            "--budget",
            "300",
            "--format",
            "json",
            "--path",
        ])
        .arg(repo_root())
        .output()
        .expect("Failed to execute repotrim expand --format json");

    assert!(json_output.status.success());
    let json_stdout = String::from_utf8_lossy(&json_output.stdout);
    let v: serde_json::Value = serde_json::from_str(&json_stdout).expect("Valid JSON report");
    assert_eq!(v["focal_symbol"]["name"], "ContextSelector");
    assert!(v["tokens_used"].as_u64().unwrap() <= 300);
}
