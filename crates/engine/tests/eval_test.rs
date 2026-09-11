use repotrim_engine::{
    BenchmarkMetrics, BenchmarkRunner, BenchmarkScenario, BenchmarkSummary, ContextStrategy,
    LoadedRepository,
};
use std::path::PathBuf;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[test]
fn test_eval_strategies_and_scenarios() {
    let strats = ContextStrategy::all();
    assert_eq!(strats.len(), 5);
    assert_eq!(ContextStrategy::WholeFile.to_string(), "Whole-File Dump");
    assert_eq!(
        ContextStrategy::AiderRepoMap.to_string(),
        "Aider Repo Map (Global PR)"
    );
    assert_eq!(
        ContextStrategy::RepoTrimFull.to_string(),
        "RepoTrim Full (Modern)"
    );

    let scenarios = BenchmarkRunner::canonical_scenarios();
    assert_eq!(scenarios.len(), 5);
    assert!(scenarios.iter().any(|s| s.id == "context_selector"));
    assert!(scenarios.iter().any(|s| s.id == "ppr_solver"));
    assert!(scenarios.iter().any(|s| s.id == "diff_resolver"));
    assert!(scenarios.iter().any(|s| s.id == "multi_seed_subsystem"));
    assert!(scenarios.iter().any(|s| s.id == "query_intent_random_walk"));
}

#[test]
fn test_eval_runner_on_workspace_repo() {
    let root = repo_root();
    let mut repo = LoadedRepository::load(&root).expect("Failed to load repo");
    repo.load_all_sources().expect("Failed to load sources");

    let runner = BenchmarkRunner::new();
    let scenario = BenchmarkScenario::with_seeds(
        "context_selector",
        "ContextSelector Test",
        vec!["ContextSelector"],
        800,
        "Test evaluating ContextSelector under 800 tokens",
    );

    let metrics = runner.evaluate_scenario(&repo, &scenario, ContextStrategy::all());
    assert_eq!(metrics.len(), 5);

    let dump = metrics
        .iter()
        .find(|m| m.strategy == ContextStrategy::WholeFile)
        .unwrap();
    let grep = metrics
        .iter()
        .find(|m| m.strategy == ContextStrategy::NaiveGrep)
        .unwrap();
    let aider = metrics
        .iter()
        .find(|m| m.strategy == ContextStrategy::AiderRepoMap)
        .unwrap();
    let repotrim_vanilla = metrics
        .iter()
        .find(|m| m.strategy == ContextStrategy::RepoTrimVanilla)
        .unwrap();
    let repotrim_full = metrics
        .iter()
        .find(|m| m.strategy == ContextStrategy::RepoTrimFull)
        .unwrap();

    for m in &metrics {
        eprintln!(
            "STRATEGY: {:<25} | Tokens: {:>4} | Direct Recall: {:>5.1}% | Precision: {:>5.1}% | Cohesion: {:>5.1}% | Orphans: {:>5.1}% | Count: {}",
            m.strategy_name, m.tokens_used, m.direct_dep_recall_pct, m.context_precision_pct, m.community_cohesion_pct, m.orphan_rate_pct, m.symbol_count
        );
    }

    // 1. Whole file baseline properties
    assert_eq!(dump.token_reduction_pct, 0.0);
    assert!(dump.tokens_used > 800, "Whole file should exceed budget");

    // 2. Budget adherence
    assert!(repotrim_full.budget_adherence);
    assert!(repotrim_full.tokens_used <= 800);
    assert!(repotrim_vanilla.tokens_used <= 800);
    assert!(grep.tokens_used <= 800);
    assert!(aider.tokens_used <= 800);

    // 3. Significant token reduction (> 60%)
    assert!(
        repotrim_full.token_reduction_pct > 60.0,
        "RepoTrim should reduce tokens by >60%"
    );

    // 4. RepoTrim Full vs Aider and Grep: Recall and Cohesion dominance
    assert!(
        repotrim_full.direct_dep_recall_pct >= aider.direct_dep_recall_pct,
        "RepoTrim direct recall ({:.1}%) should be >= Aider ({:.1}%)",
        repotrim_full.direct_dep_recall_pct,
        aider.direct_dep_recall_pct
    );
    assert!(
        repotrim_full.community_cohesion_pct >= aider.community_cohesion_pct,
        "RepoTrim community cohesion ({:.1}%) should be >= Aider ({:.1}%)",
        repotrim_full.community_cohesion_pct,
        aider.community_cohesion_pct
    );

    // 5. RepoTrim Full precision and packing dominance over Aider
    assert!(
        repotrim_full.context_precision_pct >= aider.context_precision_pct,
        "RepoTrim precision ({:.1}%) should be >= Aider ({:.1}%)",
        repotrim_full.context_precision_pct,
        aider.context_precision_pct
    );
    assert!(
        repotrim_full.symbol_count > aider.symbol_count,
        "RepoTrim MCKP should pack more symbols than Aider under equal budget"
    );
    assert!(repotrim_full.orphan_rate_pct <= 100.0);

    // 6. Sub-millisecond or sub-100ms execution
    assert!(
        repotrim_full.execution_latency_us < 100_000,
        "Execution latency should be under 100ms"
    );
}

#[test]
fn test_eval_multi_seed_and_query_scenarios() {
    let root = repo_root();
    let mut repo = LoadedRepository::load(&root).expect("Failed to load repo");
    repo.load_all_sources().expect("Failed to load sources");

    let runner = BenchmarkRunner::new();

    // Multi-seed scenario
    let multi_scenario = BenchmarkScenario::with_seeds(
        "multi",
        "Multi-Seed Test",
        vec!["ContextSelector", "PprSolver"],
        1000,
        "Cross-module test",
    );
    let multi_metrics = runner.evaluate_scenario(
        &repo,
        &multi_scenario,
        &[ContextStrategy::AiderRepoMap, ContextStrategy::RepoTrimFull],
    );
    assert_eq!(multi_metrics.len(), 2);
    let full_m = &multi_metrics[1];
    assert!(full_m.budget_adherence);
    assert!(full_m.direct_dep_recall_pct > 0.0);

    // Query-based scenario
    let query_scenario = BenchmarkScenario::with_query(
        "query",
        "Query Intent Test",
        "PageRank diffusion",
        800,
        "Natural language intent query",
    );
    let query_metrics = runner.evaluate_scenario(
        &repo,
        &query_scenario,
        &[ContextStrategy::NaiveGrep, ContextStrategy::RepoTrimFull],
    );
    assert_eq!(query_metrics.len(), 2);
    assert!(query_metrics[1].budget_adherence);
}

#[test]
fn test_eval_summary_aggregation() {
    let mock_metrics = vec![
        BenchmarkMetrics {
            scenario: "S1".to_string(),
            strategy: ContextStrategy::RepoTrimFull,
            strategy_name: "RepoTrim Full".to_string(),
            budget: 800,
            tokens_used: 600,
            budget_adherence: true,
            token_reduction_pct: 75.0,
            direct_dep_recall_pct: 80.0,
            transitive_dep_recall_pct: 60.0,
            context_precision_pct: 85.0,
            community_cohesion_pct: 90.0,
            orphan_rate_pct: 0.0,
            execution_latency_us: 1200,
            symbol_count: 8,
        },
        BenchmarkMetrics {
            scenario: "S2".to_string(),
            strategy: ContextStrategy::RepoTrimFull,
            strategy_name: "RepoTrim Full".to_string(),
            budget: 500,
            tokens_used: 400,
            budget_adherence: true,
            token_reduction_pct: 80.0,
            direct_dep_recall_pct: 70.0,
            transitive_dep_recall_pct: 50.0,
            context_precision_pct: 80.0,
            community_cohesion_pct: 95.0,
            orphan_rate_pct: 5.0,
            execution_latency_us: 800,
            symbol_count: 6,
        },
    ];

    let summaries = BenchmarkSummary::summarize(&mock_metrics);
    assert_eq!(summaries.len(), 1);
    let s = &summaries[0];
    assert_eq!(s.strategy, ContextStrategy::RepoTrimFull);
    assert_eq!(s.mean_tokens, 500);
    assert_eq!(s.mean_token_reduction_pct, 77.5);
    assert_eq!(s.mean_direct_recall_pct, 75.0);
    assert_eq!(s.mean_transitive_recall_pct, 55.0);
    assert_eq!(s.mean_precision_pct, 82.5);
    assert_eq!(s.mean_cohesion_pct, 92.5);
    assert_eq!(s.mean_orphan_rate_pct, 2.5);
    assert_eq!(s.mean_latency_us, 1000);
}
