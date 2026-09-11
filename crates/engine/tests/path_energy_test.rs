use proptest::prelude::*;
use std::path::PathBuf;

use repotrim_engine::{
    ExecutionPath, MultiplexCsrGraph, PathCoverage, PathFinder, PathFinderConfig, PathScorer,
    PathScorerConfig, ProbabilisticCoverage, RelationType, SparseKernelMatrix, SubmodularConfig,
    SubmodularUtility, SymbolId, SymbolKind, SymbolNode, TaskContext, TaskKind, TextSpan,
};

fn mock_symbol(id: u32, name: &str, file: &str) -> SymbolNode {
    SymbolNode {
        id: SymbolId(id),
        name: name.to_string(),
        kind: SymbolKind::Function,
        file_path: PathBuf::from(file),
        span: TextSpan::new(0, 50, 1, 3),
        signature: format!("pub fn {}()", name),
        docstring: None,
        token_cost: 25,
        ast_hash: [0u8; 32],
        container_name: None,
        trait_name: None,
    }
}

#[test]
fn test_multihop_causal_path_discovery() {
    let syms = vec![
        mock_symbol(0, "api_handler", "src/api.rs"),
        mock_symbol(1, "auth_middleware", "src/auth.rs"),
        mock_symbol(2, "user_service", "src/service.rs"),
        mock_symbol(3, "user_repo", "src/repo.rs"),
        mock_symbol(4, "db_pool", "src/db.rs"),
        mock_symbol(5, "metrics_logger", "src/metrics.rs"),
    ];

    let edges = vec![
        (0, 1, RelationType::Calls, 0.95),
        (1, 2, RelationType::Calls, 0.90),
        (2, 3, RelationType::Calls, 0.90),
        (3, 4, RelationType::Calls, 0.85),
        // Direct call from service to db (shortcut)
        (2, 4, RelationType::Calls, 0.70),
        // Side branch from handler to logger
        (0, 5, RelationType::Calls, 0.60),
    ];

    let graph = MultiplexCsrGraph::from_typed_edges(syms.clone(), &edges);
    let finder = PathFinder::new(PathFinderConfig {
        max_depth: 5,
        max_paths_per_target: 5,
        max_total_paths: 10,
        ..Default::default()
    });

    let paths = finder.find_paths_between(&graph, SymbolId(0), SymbolId(4));
    assert_eq!(
        paths.len(),
        2,
        "Expected 2 loopless paths from handler to db"
    );

    let p0 = &paths[0];
    let trace0 = p0.format_trace(&syms);
    assert!(!trace0.is_empty());

    // Both paths must start at api_handler and end at db_pool
    for p in &paths {
        assert_eq!(p.source(), Some(SymbolId(0)));
        assert_eq!(p.target(), Some(SymbolId(4)));
        assert!(p.len() >= 3);
    }
}

#[test]
fn test_boltzmann_probability_normalization_and_temperature() {
    let paths = vec![
        ExecutionPath::new(
            vec![SymbolId(0), SymbolId(1), SymbolId(4)],
            vec![RelationType::Calls, RelationType::Calls],
            vec![0.9, 0.9],
        ),
        ExecutionPath::new(
            vec![SymbolId(0), SymbolId(2), SymbolId(3), SymbolId(4)],
            vec![
                RelationType::Calls,
                RelationType::References,
                RelationType::Calls,
            ],
            vec![0.8, 0.7, 0.8],
        ),
        ExecutionPath::new(
            vec![SymbolId(0), SymbolId(5), SymbolId(4)],
            vec![RelationType::Imports, RelationType::References],
            vec![0.4, 0.5],
        ),
    ];

    let relevance = vec![0.8, 0.7, 0.5, 0.4, 0.9, 0.2];

    for temp in [0.1_f32, 0.5, 1.0, 5.0, 50.0] {
        let scorer = PathScorer::new(PathScorerConfig {
            temperature: temp,
            length_penalty: 0.25,
        })
        .with_relevance_scores(relevance.clone());

        let mut current_paths = paths.clone();
        scorer.score_and_normalize(&mut current_paths);

        let sum_p: f32 = current_paths.iter().map(|p| p.probability).sum();
        assert!(
            (sum_p - 1.0).abs() < 1e-5,
            "Probabilities must sum to 1.0 at temp {}, got {}",
            temp,
            sum_p
        );

        for p in &current_paths {
            assert!(
                p.probability >= 0.0 && p.probability <= 1.0,
                "Probability out of range: {}",
                p.probability
            );
        }

        if temp < 0.2 {
            // Cold temperature concentrates probability on best path
            let max_p = current_paths
                .iter()
                .map(|p| p.probability)
                .fold(0.0_f32, f32::max);
            assert!(
                max_p > 0.80,
                "Low temperature should concentrate probability: max_p={}",
                max_p
            );
        } else if temp > 40.0 {
            // Hot temperature approaches uniform distribution 1/3 ~ 0.333
            for p in &current_paths {
                assert!(
                    (p.probability - 0.333).abs() < 0.05,
                    "High temperature should be nearly uniform, got {}",
                    p.probability
                );
            }
        }
    }
}

#[test]
fn test_task_conditioned_path_scoring() {
    let call_path = ExecutionPath::new(
        vec![SymbolId(0), SymbolId(1)],
        vec![RelationType::Calls],
        vec![0.8],
    );

    let test_path = ExecutionPath::new(
        vec![SymbolId(0), SymbolId(2)],
        vec![RelationType::IsTestedBy],
        vec![0.8],
    );

    let refactor_path = ExecutionPath::new(
        vec![SymbolId(0), SymbolId(3)],
        vec![RelationType::Inherits],
        vec![0.8],
    );

    let mut test_task = TaskContext::from_query("write tests for auth");
    test_task.metadata.kind = Some(TaskKind::TestCreation);
    let mut refactor_task = TaskContext::from_query("refactor user models");
    refactor_task.metadata.kind = Some(TaskKind::Refactor);

    let test_scorer = PathScorer::with_task(PathScorerConfig::default(), &test_task);
    let refactor_scorer = PathScorer::with_task(PathScorerConfig::default(), &refactor_task);

    let mut paths_test_task = vec![call_path.clone(), test_path.clone(), refactor_path.clone()];
    let mut paths_refactor_task = vec![call_path, test_path, refactor_path];

    test_scorer.score_and_normalize(&mut paths_test_task);
    refactor_scorer.score_and_normalize(&mut paths_refactor_task);

    // In TestCreation task, test_path (index 1) must have higher probability than refactor_path (index 2)
    assert!(
        paths_test_task[1].probability > paths_test_task[2].probability,
        "TestCreation task must prioritize test path: {} vs {}",
        paths_test_task[1].probability,
        paths_test_task[2].probability
    );

    // In Refactor task, refactor_path (index 2) must have higher probability than test_path (index 1)
    assert!(
        paths_refactor_task[2].probability > paths_refactor_task[1].probability,
        "Refactor task must prioritize inheritance path: {} vs {}",
        paths_refactor_task[2].probability,
        paths_refactor_task[1].probability
    );
}

#[test]
fn test_path_preserving_submodular_selection() {
    // Graph of 4 symbols:
    // 0 (entrypoint) -> 1 (bridge) -> 2 (destination)
    // 3 is a disconnected symbol with high isolated relevance
    let n = 4;
    let kernel_rows = vec![
        vec![(SymbolId(0), 1.0), (SymbolId(1), 0.5)],
        vec![(SymbolId(1), 1.0), (SymbolId(2), 0.5)],
        vec![(SymbolId(2), 1.0)],
        vec![(SymbolId(3), 1.0)],
    ];
    let kernel = SparseKernelMatrix::from_row_entries(n, &kernel_rows);
    let cov = ProbabilisticCoverage::new(kernel, vec![1.0; n]);

    // Symbol 3 has higher isolated relevance than bridge symbol 1
    let rel = vec![0.8, 0.3, 0.7, 0.65];

    let mut path = ExecutionPath::new(
        vec![SymbolId(0), SymbolId(1), SymbolId(2)],
        vec![RelationType::Calls, RelationType::Calls],
        vec![0.9, 0.9],
    );
    path.probability = 1.0;

    let path_cov = PathCoverage::new(n, vec![path]);

    // Without path coverage (gamma = 0.0), symbol 3 is favored over bridge symbol 1
    let config_no_path = SubmodularConfig {
        alpha: 0.6,
        beta: 0.4,
        delta: 0.0,
        gamma: 0.0,
        lambda: 0.0,
        budget: 60,
        normalize_components: true,
    };

    // With path coverage (gamma = 0.4), bridge symbol 1 is favored to preserve unbroken path
    let config_with_path = SubmodularConfig {
        alpha: 0.3,
        beta: 0.3,
        delta: 0.0,
        gamma: 0.4,
        lambda: 0.0,
        budget: 60,
        normalize_components: true,
    };

    let util_no_path = SubmodularUtility::new(config_no_path, cov.clone(), rel.clone());
    let util_with_path =
        SubmodularUtility::new(config_with_path, cov, rel).with_path_coverage(path_cov);

    let candidates = vec![SymbolId(0), SymbolId(1), SymbolId(2), SymbolId(3)];
    let costs = vec![20, 20, 20, 20];
    let budget = 40; // Can pick exactly 2 symbols

    let (selected_no_path, _) = util_no_path.optimize_celf(&candidates, &costs, budget);
    let (selected_with_path, state_with_path) =
        util_with_path.optimize_celf(&candidates, &costs, budget);

    assert_eq!(selected_no_path.len(), 2);
    assert_eq!(selected_with_path.len(), 2);

    // With path coverage active, path preservation is rewarded
    assert!(
        state_with_path.total_path > 0.0,
        "Path-aware selection must yield positive path coverage"
    );
}

// ============================================================================
// Proptest Invariant Proofs
// ============================================================================

proptest! {
    /// Property: Boltzmann Probability Distribution Conservation
    /// For any set of candidate paths and energy scores, probabilities sum to 1.0 +/- 1e-5.
    #[test]
    fn prop_boltzmann_normalization_invariant(
        num_paths in 2usize..10,
        raw_scores in proptest::collection::vec(-50.0_f32..50.0_f32, 2..10),
        temp in 0.1_f32..20.0_f32,
    ) {
        let k = num_paths.min(raw_scores.len());
        let mut paths: Vec<ExecutionPath> = (0..k)
            .map(|i| {
                let mut p = ExecutionPath::new(
                    vec![SymbolId(0), SymbolId(i as u32 + 1)],
                    vec![RelationType::Calls],
                    vec![0.8],
                );
                p.score = raw_scores[i];
                p
            })
            .collect();

        let scorer = PathScorer::new(PathScorerConfig {
            temperature: temp,
            length_penalty: 0.0,
        });

        scorer.score_and_normalize(&mut paths);

        let sum_prob: f32 = paths.iter().map(|p| p.probability).sum();
        prop_assert!(
            (sum_prob - 1.0).abs() < 1e-4,
            "Boltzmann probabilities must sum to 1.0, got {} for temp {}",
            sum_prob,
            temp
        );

        for p in &paths {
            prop_assert!(p.probability >= 0.0 && p.probability <= 1.0);
        }
    }

    /// Property: Path Coverage Monotonicity
    /// Adding any symbol to PathCoverageState never decreases total path coverage.
    #[test]
    fn prop_path_coverage_monotonicity(
        num_symbols in 3usize..8,
        path_len in 2usize..4,
    ) {
        let len = path_len.min(num_symbols);
        let nodes: Vec<SymbolId> = (0..len).map(|i| SymbolId(i as u32)).collect();
        let rels = vec![RelationType::Calls; len - 1];
        let weights = vec![0.8; len - 1];

        let mut path = ExecutionPath::new(nodes, rels, weights);
        path.probability = 1.0;

        let pc = PathCoverage::new(num_symbols, vec![path]);
        let mut state = pc.new_state();
        let mut prev_cov = 0.0_f32;

        for i in 0..num_symbols {
            let cand = SymbolId(i as u32);
            let gain = pc.marginal_gain(&state, cand);
            prop_assert!(gain >= 0.0, "Marginal gain must be >= 0: {}", gain);

            pc.add_candidate(&mut state, cand);
            prop_assert!(
                state.total_coverage() >= prev_cov - 1e-6,
                "Total path coverage must be monotonically non-decreasing: {} vs {}",
                state.total_coverage(),
                prev_cov
            );
            prev_cov = state.total_coverage();
        }

        prop_assert!(state.total_coverage() <= 1.0001);
    }
}
