use repotrim_engine::{estimate_tokens, ContextSelector, LoadedRepository, PprSolver, SymbolId};
use std::collections::HashSet;
use std::path::PathBuf;
use std::time::Instant;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[derive(Debug)]
pub struct BenchmarkResult {
    pub scenario: String,
    pub strategy: String,
    pub tokens_used: usize,
    pub token_reduction_pct: f32,
    pub dependency_recall_pct: f32,
    pub execution_latency_us: u128,
}

#[test]
fn test_empirical_baseline_comparisons() {
    let root = repo_root();
    let mut repo = LoadedRepository::load(&root).expect("Failed to load repo");
    repo.load_all_sources().expect("Failed to load sources");
    let graph = repo.build_graph();

    let scenarios = [
        ("ContextSelector", 500usize),
        ("RepositoryCache", 500usize),
        ("PprSolver", 300usize),
    ];

    let mut results: Vec<BenchmarkResult> = Vec::new();

    for (seed_name, budget) in scenarios {
        let seed_sym = graph
            .symbols()
            .iter()
            .find(|s| s.name == seed_name)
            .unwrap_or_else(|| panic!("Seed symbol '{}' not found", seed_name));
        let seed_id = seed_sym.id;

        // Ground truth: direct outgoing neighbors of seed
        let direct_neighbors: HashSet<u32> = graph.neighbors(seed_id).iter().copied().collect();

        // ---------------------------------------------------------------------
        // Strategy 1: Whole-File Dump (Naive baseline)
        // ---------------------------------------------------------------------
        let start_dump = Instant::now();
        let seed_file_source = repo
            .file_sources
            .get(&seed_sym.file_path)
            .map(|s| s.as_str())
            .unwrap_or("");
        let dump_tokens = estimate_tokens(seed_file_source);
        let dump_latency = start_dump.elapsed().as_micros();

        results.push(BenchmarkResult {
            scenario: seed_name.to_string(),
            strategy: "Whole-File Dump".to_string(),
            tokens_used: dump_tokens,
            token_reduction_pct: 0.0,
            dependency_recall_pct: 100.0, // Contains all symbols in file
            execution_latency_us: dump_latency,
        });

        // ---------------------------------------------------------------------
        // Strategy 2: Naive Keyword / Grep Search
        // Grabs symbols whose name contains keyword substring
        // ---------------------------------------------------------------------
        let start_grep = Instant::now();
        let keyword = &seed_name[..seed_name.len().min(6)];
        let grep_matches: Vec<_> = graph
            .symbols()
            .iter()
            .filter(|s| s.name.contains(keyword))
            .collect();
        let grep_tokens: usize = grep_matches.iter().map(|s| s.token_cost).sum();
        let grep_selected_ids: HashSet<u32> = grep_matches.iter().map(|s| s.id.0).collect();
        let grep_recall = if !direct_neighbors.is_empty() {
            let hits = direct_neighbors.intersection(&grep_selected_ids).count();
            (hits as f32 / direct_neighbors.len() as f32) * 100.0
        } else {
            100.0
        };
        let grep_latency = start_grep.elapsed().as_micros();

        results.push(BenchmarkResult {
            scenario: seed_name.to_string(),
            strategy: "Naive Keyword/Grep".to_string(),
            tokens_used: grep_tokens,
            token_reduction_pct: if dump_tokens > 0 {
                (1.0 - (grep_tokens as f32 / dump_tokens as f32)) * 100.0
            } else {
                0.0
            },
            dependency_recall_pct: grep_recall,
            execution_latency_us: grep_latency,
        });

        // ---------------------------------------------------------------------
        // Strategy 3: Unweighted Global PageRank (Aider-style)
        // ---------------------------------------------------------------------
        let start_global = Instant::now();
        let num_symbols = graph.num_symbols();
        let uniform_seeds: Vec<(SymbolId, f32)> = (0..num_symbols as u32)
            .map(|id| (SymbolId(id), 1.0 / num_symbols.max(1) as f32))
            .collect();
        let ppr = PprSolver::default();
        let scores = ppr.compute(&graph, &uniform_seeds);
        let mut sorted_hubs: Vec<_> = scores.into_iter().collect();
        sorted_hubs.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        // Pack top global hubs until budget
        let mut global_tokens = 0usize;
        let mut global_selected_ids: HashSet<u32> = HashSet::new();
        for (hub_id, _) in sorted_hubs {
            if let Some(sym) = graph.symbol(hub_id) {
                if global_tokens + sym.token_cost <= budget {
                    global_tokens += sym.token_cost;
                    global_selected_ids.insert(hub_id.0);
                }
            }
        }
        let global_recall = if !direct_neighbors.is_empty() {
            let hits = direct_neighbors.intersection(&global_selected_ids).count();
            (hits as f32 / direct_neighbors.len() as f32) * 100.0
        } else {
            100.0
        };
        let global_latency = start_global.elapsed().as_micros();

        results.push(BenchmarkResult {
            scenario: seed_name.to_string(),
            strategy: "Unweighted Global PageRank".to_string(),
            tokens_used: global_tokens,
            token_reduction_pct: if dump_tokens > 0 {
                (1.0 - (global_tokens as f32 / dump_tokens as f32)) * 100.0
            } else {
                0.0
            },
            dependency_recall_pct: global_recall,
            execution_latency_us: global_latency,
        });

        // ---------------------------------------------------------------------
        // Strategy 4: RepoTrim (Multiplex CPG + Forward-Push PPR + CELF Knapsack)
        // ---------------------------------------------------------------------
        let start_repotrim = Instant::now();
        let selector = ContextSelector::default();
        let (selected_symbols, _) =
            selector.select_and_format_context(&graph, &[seed_id], budget, &repo.file_sources);
        let repotrim_tokens: usize = selected_symbols.iter().map(|s| s.token_cost).sum();
        let repotrim_selected_ids: HashSet<u32> = selected_symbols.iter().map(|s| s.id.0).collect();
        let repotrim_recall = if !direct_neighbors.is_empty() {
            let hits = direct_neighbors
                .intersection(&repotrim_selected_ids)
                .count();
            (hits as f32 / direct_neighbors.len() as f32) * 100.0
        } else {
            100.0
        };
        let repotrim_latency = start_repotrim.elapsed().as_micros();

        results.push(BenchmarkResult {
            scenario: seed_name.to_string(),
            strategy: "RepoTrim (Ours)".to_string(),
            tokens_used: repotrim_tokens,
            token_reduction_pct: if dump_tokens > 0 {
                (1.0 - (repotrim_tokens as f32 / dump_tokens as f32)) * 100.0
            } else {
                0.0
            },
            dependency_recall_pct: repotrim_recall,
            execution_latency_us: repotrim_latency,
        });

        // Strict budget assertion
        assert!(repotrim_tokens <= budget, "RepoTrim exceeded budget!");
    }

    // Print comparative markdown table to stderr for dogfood inspection
    eprintln!("\n{}", "=".repeat(95));
    eprintln!("  EMPIRICAL COMPARATIVE BENCHMARK EVALUATION (REPOTRIM VS. BASELINES)");
    eprintln!("{}", "=".repeat(95));
    eprintln!(
        "{:<18} | {:<27} | {:>7} | {:>10} | {:>12} | {:>10}",
        "Scenario", "Strategy", "Tokens", "Reduction", "Dep Recall", "Latency"
    );
    eprintln!("{}", "-".repeat(95));

    for r in &results {
        eprintln!(
            "{:<18} | {:<27} | {:>7} | {:>9.1}% | {:>11.1}% | {:>8} µs",
            r.scenario,
            r.strategy,
            r.tokens_used,
            r.token_reduction_pct,
            r.dependency_recall_pct,
            r.execution_latency_us
        );
    }
    eprintln!("{}", "=".repeat(95));
}
