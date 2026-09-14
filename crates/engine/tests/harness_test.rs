use repotrim_engine::{
    AgentSessionTrace, AgentTraceRecorder, HarnessBenchmarkRunner, HarnessComparison,
    HarnessComparisonReport, InvocationStatus, LoadedRepository,
};
use std::path::PathBuf;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}

#[test]
fn test_agent_trace_recorder_lifecycle_and_json() {
    let mut recorder = AgentTraceRecorder::new(
        "test-session-123",
        "antigravity",
        "Refactor ContextSelector to support custom submodular knapsack coefficients",
        true,
    );

    recorder.record_tool_call(
        "locate_entrypoints",
        "query: \"ContextSelector\"",
        60,
        350,
        1500,
        &["ContextSelector".to_string(), "PprSolver".to_string()],
        &["crates/engine/src/selector.rs".to_string()],
        InvocationStatus::Success,
    );

    recorder.record_tool_call(
        "expand_symbol",
        "symbol: \"ContextSelector\", budget: 600",
        80,
        420,
        2800,
        &[
            "ContextSelector".to_string(),
            "CsrMatrix".to_string(),
            "LodLevel".to_string(),
        ],
        &[
            "crates/engine/src/selector.rs".to_string(),
            "crates/engine/src/csr.rs".to_string(),
        ],
        InvocationStatus::Success,
    );

    let trace = recorder.finalize(true);

    assert_eq!(trace.session_id, "test-session-123");
    assert_eq!(trace.harness_type, "antigravity");
    assert!(trace.with_repotrim);
    assert!(trace.task_success);
    assert_eq!(trace.tool_call_count(), 2);
    assert_eq!(trace.total_input_tokens, 140);
    assert_eq!(trace.total_output_tokens, 770);
    assert_eq!(trace.total_tokens(), 910);
    assert_eq!(trace.unique_files_accessed, 2);
    assert_eq!(trace.unique_symbols_accessed, 4); // ContextSelector, PprSolver, CsrMatrix, LodLevel
    assert!(trace.information_density_spt > 4.0);

    // Test JSON roundtrip
    let json = trace.to_json().expect("Serialization failed");
    let deserialized = AgentSessionTrace::from_json(&json).expect("Deserialization failed");
    assert_eq!(trace, deserialized);
}

#[test]
fn test_harness_comparison_metrics_and_invariants() {
    // 1. Synthetic Baseline Session (e.g. naive file reads & grepping)
    let mut base_recorder =
        AgentTraceRecorder::new("base-1", "swe_bench", "Fix cache invalidation bug", false);
    base_recorder.record_tool_call(
        "list_dir",
        "path: .",
        40,
        200,
        10000,
        &[],
        &["crates/engine/src/lib.rs".to_string()],
        InvocationStatus::Success,
    );
    base_recorder.record_tool_call(
        "grep_search",
        "query: \"RepositoryCache\"",
        60,
        500,
        20000,
        &["RepositoryCache".to_string()],
        &[],
        InvocationStatus::Success,
    );
    base_recorder.record_tool_call(
        "read_file",
        "path: crates/engine/src/cache.rs",
        3500,
        3500,
        15000,
        &[
            "RepositoryCache".to_string(),
            "compute_blake3_hash".to_string(),
        ],
        &["crates/engine/src/cache.rs".to_string()],
        InvocationStatus::Success,
    );
    base_recorder.record_tool_call(
        "read_file",
        "path: crates/engine/src/loader.rs",
        4200,
        4200,
        18000,
        &["LoadedRepository".to_string()],
        &["crates/engine/src/loader.rs".to_string()],
        InvocationStatus::Success,
    );
    let baseline_trace = base_recorder.finalize(true);

    // 2. Synthetic RepoTrim Session (targeted intelligence tool calls)
    let mut trim_recorder =
        AgentTraceRecorder::new("trim-1", "swe_bench", "Fix cache invalidation bug", true);
    trim_recorder.record_tool_call(
        "locate_entrypoints",
        "query: \"RepositoryCache\"",
        50,
        180,
        1200,
        &["RepositoryCache".to_string()],
        &["crates/engine/src/cache.rs".to_string()],
        InvocationStatus::Success,
    );
    trim_recorder.record_tool_call(
        "expand_symbol",
        "symbol: \"RepositoryCache\", budget: 400",
        60,
        380,
        2100,
        &[
            "RepositoryCache".to_string(),
            "compute_blake3_hash".to_string(),
        ],
        &["crates/engine/src/cache.rs".to_string()],
        InvocationStatus::Success,
    );
    let repotrim_trace = trim_recorder.finalize(true);

    // 3. Compare Pairwise
    let comp = HarnessComparison::compare(
        "Cache Invalidation Scenario",
        &baseline_trace,
        &repotrim_trace,
    );

    assert_eq!(comp.baseline_tool_calls, 4);
    assert_eq!(comp.repotrim_tool_calls, 2);
    assert_eq!(comp.tool_call_reduction_pct, 50.0);
    assert!(comp.token_reduction_pct > 90.0); // 670 vs ~16000
    assert!(comp.spt_multiplier > 3.0); // Much higher information density
    assert!(comp.baseline_success);
    assert!(comp.repotrim_success);

    // 4. Summarize Report
    let report = HarnessComparisonReport::summarize(vec![comp]);
    assert_eq!(report.comparisons.len(), 1);
    assert!(report.mean_token_reduction_pct > 90.0);
    assert_eq!(report.mean_tool_call_reduction_pct, 50.0);
    assert_eq!(report.baseline_success_rate_pct, 100.0);
    assert_eq!(report.repotrim_success_rate_pct, 100.0);

    let table_str = report.render_table();
    assert!(table_str.contains("Agent Exploration Harness Study"));
    assert!(table_str.contains("Cache Invalidation Scenario"));

    let md_str = report.render_markdown();
    assert!(md_str.contains("## Agent Exploration Harness Study"));
    assert!(md_str.contains("| Scenario | Baseline Tokens | RepoTrim Tokens |"));
}

#[test]
fn test_harness_benchmark_runner_on_workspace() {
    let root = repo_root();
    let mut repo = LoadedRepository::load(&root).expect("Failed to load repo");
    repo.load_all_sources().expect("Failed to load sources");

    let runner = HarnessBenchmarkRunner::new();
    assert!(!runner.scenarios.is_empty());

    let scenario = &runner.scenarios[0];
    let (baseline, repotrim, comp) = runner.evaluate_scenario(&repo, scenario);

    assert!(!baseline.with_repotrim);
    assert!(repotrim.with_repotrim);
    assert!(baseline.total_tokens() > repotrim.total_tokens());
    assert!(comp.token_reduction_pct > 50.0);
    assert!(comp.repotrim_spt > comp.baseline_spt);
}
