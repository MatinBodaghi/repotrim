use proptest::prelude::*;
use repotrim_engine::{
    EvidenceKernel, LodLevel, MultiplexCsrGraph, ProbabilisticCoverage, RelationType,
    SparseKernelMatrix, SubmodularConfig, SubmodularUtility, SymbolId, SymbolKind, SymbolNode,
    TextSpan, TokenCostConfig, TokenCostEstimator, TokenizerModel,
};
use std::path::PathBuf;

fn mock_symbol(
    id: u32,
    name: &str,
    file: &str,
    container: Option<&str>,
    cost: usize,
) -> SymbolNode {
    SymbolNode {
        id: SymbolId(id),
        name: name.to_string(),
        kind: SymbolKind::Function,
        file_path: PathBuf::from(file),
        span: TextSpan::new(0, 80, 1, 8),
        signature: format!("pub fn {}() -> Result<(), EngineError>", name),
        docstring: Some(format!("Documentation for symbol {}.", name)),
        token_cost: cost,
        ast_hash: [0u8; 32],
        container_name: container.map(|s| s.to_string()),
        trait_name: None,
    }
}

#[test]
fn test_multifactor_cost_breakdown() {
    let sym = mock_symbol(0, "execute_query", "src/engine.rs", Some("QueryEngine"), 60);
    let config = TokenCostConfig {
        model: TokenizerModel::FastHeuristic,
        base_format_overhead: 5,
        tokens_per_relation: 4,
        amortized_header_overhead: 3,
        min_cost_floor: 1,
    };
    let estimator = TokenCostEstimator::with_config(config);

    let b_sig = estimator.estimate_breakdown(&sym, LodLevel::SignatureOnly, None, 2);
    let b_full = estimator.estimate_breakdown(&sym, LodLevel::FullBody, None, 2);

    assert_eq!(b_sig.rel_cost, 2 * 4);
    assert_eq!(b_sig.format_cost, 5 + 3);
    assert!(b_sig.meta_cost > 0);
    assert!(b_sig.text_cost > 0);
    assert_eq!(
        b_sig.total(),
        b_sig.text_cost + b_sig.meta_cost + b_sig.rel_cost + b_sig.format_cost
    );

    assert!(b_full.text_cost >= b_sig.text_cost);
    assert!(b_full.total() >= b_sig.total());
    assert_eq!(
        estimator.estimate_total(&sym, LodLevel::FullBody, None, 2),
        b_full.total()
    );
}

#[test]
fn test_submodular_utility_end_to_end() {
    let syms = vec![
        mock_symbol(0, "entrypoint", "src/main.rs", None, 30),
        mock_symbol(1, "parse_config", "src/config.rs", Some("Config"), 40),
        mock_symbol(2, "init_storage", "src/storage.rs", Some("Storage"), 50),
        mock_symbol(3, "read_record", "src/storage.rs", Some("Storage"), 45),
        mock_symbol(4, "write_record", "src/storage.rs", Some("Storage"), 45),
    ];

    let edges = vec![
        (0, 1, RelationType::Calls, 1.0),
        (0, 2, RelationType::Calls, 1.0),
        (2, 3, RelationType::Contains, 1.0),
        (2, 4, RelationType::Contains, 1.0),
        (3, 4, RelationType::References, 1.0),
    ];

    let graph = MultiplexCsrGraph::from_typed_edges(syms.clone(), &edges);
    let kernel = EvidenceKernel::default().compute(&graph, None);
    let weights = vec![1.0; 5];
    let cov = ProbabilisticCoverage::new(kernel, weights);
    let rel = vec![0.8, 0.6, 0.7, 0.4, 0.4];

    let config = SubmodularConfig {
        alpha: 0.35,
        beta: 0.45,
        delta: 0.0,
        gamma: 0.0,
        lambda: 0.10,
        budget: 4096,
        normalize_components: true,
    };

    let utility = SubmodularUtility::new(config, cov, rel).with_symbols(&syms);
    let mut state = utility.new_state();

    assert_eq!(state.len(), 0);
    assert_eq!(state.total_tokens, 0);
    assert_eq!(state.total_utility, 0.0);

    // Add entrypoint
    let gain0 = utility.add_symbol(&mut state, SymbolId(0), 30);
    assert!(gain0 > 0.0);
    assert_eq!(state.len(), 1);
    assert_eq!(state.total_tokens, 30);
    assert!((state.total_utility - gain0).abs() < 1e-5);

    // Re-adding symbol yields zero marginal gain
    let gain_dup = utility.marginal_gain(&state, SymbolId(0));
    assert_eq!(gain_dup, 0.0);
    let added_dup = utility.add_symbol(&mut state, SymbolId(0), 30);
    assert_eq!(added_dup, 0.0);
    assert_eq!(state.len(), 1);

    // Add storage symbols
    let gain2 = utility.add_symbol(&mut state, SymbolId(2), 50);
    assert!(gain2 > 0.0);
    assert_eq!(state.len(), 2);
    assert_eq!(state.total_tokens, 80);
    assert!(state.total_utility > gain0);
}

#[test]
fn test_test_verification_coverage() {
    let syms = vec![
        mock_symbol(0, "compute_hash", "src/crypto.rs", None, 30),
        mock_symbol(1, "test_compute_hash", "tests/crypto_test.rs", None, 35),
    ];

    let kernel = SparseKernelMatrix::from_row_entries(
        2,
        &[
            vec![(SymbolId(0), 1.0), (SymbolId(1), 0.8)],
            vec![(SymbolId(0), 0.8), (SymbolId(1), 1.0)],
        ],
    );
    let cov = ProbabilisticCoverage::new(kernel.clone(), vec![1.0, 0.2]);
    let test_cov = ProbabilisticCoverage::new(kernel, vec![0.0, 1.0]);

    let config_no_test = SubmodularConfig {
        delta: 0.0,
        ..Default::default()
    };
    let config_with_test = SubmodularConfig {
        delta: 0.30,
        ..Default::default()
    };

    let util_no = SubmodularUtility::new(config_no_test, cov.clone(), vec![0.5, 0.5])
        .with_test_coverage(test_cov.clone())
        .with_symbols(&syms);
    let util_yes = SubmodularUtility::new(config_with_test, cov, vec![0.5, 0.5])
        .with_test_coverage(test_cov)
        .with_symbols(&syms);

    let s_no = util_no.new_state();
    let s_yes = util_yes.new_state();

    let gain_no = util_no.marginal_gain(&s_no, SymbolId(1));
    let gain_yes = util_yes.marginal_gain(&s_yes, SymbolId(1));

    assert!(
        gain_yes > gain_no,
        "Candidate providing test coverage must have higher marginal gain under delta > 0: {} vs {}",
        gain_yes,
        gain_no
    );
}

#[test]
fn test_redundancy_penalty_effect() {
    let syms = vec![
        mock_symbol(0, "query_user", "src/db.rs", Some("UserStore"), 40),
        mock_symbol(1, "find_user", "src/db.rs", Some("UserStore"), 40),
    ];

    let kernel = SparseKernelMatrix::from_row_entries(
        2,
        &[
            vec![(SymbolId(0), 1.0), (SymbolId(1), 0.7)],
            vec![(SymbolId(0), 0.7), (SymbolId(1), 1.0)],
        ],
    );
    let cov = ProbabilisticCoverage::new(kernel, vec![1.0, 1.0]);
    let rel = vec![0.6, 0.6];

    let util_unpenalized = SubmodularUtility::new(
        SubmodularConfig {
            lambda: 0.0,
            ..Default::default()
        },
        cov.clone(),
        rel.clone(),
    )
    .with_symbols(&syms);

    let util_penalized = SubmodularUtility::new(
        SubmodularConfig {
            lambda: 0.5,
            ..Default::default()
        },
        cov,
        rel,
    )
    .with_symbols(&syms);

    let mut s_unpen = util_unpenalized.new_state();
    let mut s_pen = util_penalized.new_state();

    util_unpenalized.add_symbol(&mut s_unpen, SymbolId(0), 40);
    util_penalized.add_symbol(&mut s_pen, SymbolId(0), 40);

    let gain_unpen = util_unpenalized.marginal_gain(&s_unpen, SymbolId(1));
    let gain_pen = util_penalized.marginal_gain(&s_pen, SymbolId(1));

    assert!(
        gain_pen < gain_unpen,
        "Redundancy penalty must discount marginal gain of shared container symbol: {} vs {}",
        gain_pen,
        gain_unpen
    );
}

// ============================================================================
// Proptest Invariant Proofs
// ============================================================================

proptest! {
    /// Property: Utility Monotonicity
    /// For any sequence of symbol additions, the total utility F(S) is monotonically non-decreasing.
    #[test]
    fn prop_utility_monotonicity_under_sequential_selection(
        n in 3usize..8,
        seed in 1u64..1000,
    ) {
        let mut rows = Vec::new();
        for i in 0..n {
            let mut row = Vec::new();
            for j in 0..n {
                let p = if i == j {
                    1.0
                } else {
                    let h = ((seed.wrapping_mul(31) + i as u64 * 17 + j as u64 * 13) % 100) as f32 / 100.0;
                    h * 0.4
                };
                if p > 0.05 {
                    row.push((SymbolId(j as u32), p));
                }
            }
            rows.push(row);
        }

        let kernel = SparseKernelMatrix::from_row_entries(n, &rows);
        let weights = vec![1.0; n];
        let cov = ProbabilisticCoverage::new(kernel, weights);
        let rel = vec![0.5; n];

        let config = SubmodularConfig {
            alpha: 0.35,
            beta: 0.50,
            delta: 0.0,
            gamma: 0.0,
            lambda: 0.05, // Monotone non-negative guarantee holds via max(0.0)
            budget: 4096,
            normalize_components: true,
        };

        let utility = SubmodularUtility::new(config, cov, rel);
        let mut state = utility.new_state();
        let mut prev_util = 0.0_f32;

        for i in 0..n {
            let cand = SymbolId(i as u32);
            let gain = utility.add_symbol(&mut state, cand, 20);
            prop_assert!(gain >= 0.0, "Marginal gain must be non-negative: {}", gain);
            prop_assert!(
                state.total_utility >= prev_util - 1e-5,
                "Utility must be non-decreasing: {} vs {}",
                state.total_utility,
                prev_util
            );
            prev_util = state.total_utility;
        }
    }

    /// Property: Submodularity Diminishing Returns (Nemhauser et al. 1978)
    /// Under pure coverage + relevance (lambda = 0), for nested subsets A <= B and candidate x not in B:
    /// Delta_F(x | A) >= Delta_F(x | B)
    #[test]
    fn prop_diminishing_returns_submodularity(
        n in 4usize..9,
        seed in 1u64..1000,
    ) {
        let mut rows = Vec::new();
        for i in 0..n {
            let mut row = Vec::new();
            for j in 0..n {
                let p = if i == j {
                    1.0
                } else {
                    let h = ((seed.wrapping_mul(47) + i as u64 * 23 + j as u64 * 19) % 100) as f32 / 100.0;
                    h * 0.5
                };
                if p > 0.05 {
                    row.push((SymbolId(j as u32), p));
                }
            }
            rows.push(row);
        }

        let kernel = SparseKernelMatrix::from_row_entries(n, &rows);
        let weights = vec![1.0; n];
        let cov = ProbabilisticCoverage::new(kernel, weights);
        let rel = vec![0.5; n];

        let config = SubmodularConfig {
            alpha: 0.30,
            beta: 0.70,
            delta: 0.0,
            gamma: 0.0,
            lambda: 0.0, // Monotone submodular regime
            budget: 4096,
            normalize_components: true,
        };

        let utility = SubmodularUtility::new(config, cov, rel);

        // Build nested sets A <= B
        let mut state_a = utility.new_state();
        let mut state_b = utility.new_state();

        utility.add_symbol(&mut state_a, SymbolId(0), 20);
        utility.add_symbol(&mut state_b, SymbolId(0), 20);
        utility.add_symbol(&mut state_b, SymbolId(1), 20);

        // Evaluate candidate x = SymbolId(2) not in B
        let cand = SymbolId(2);
        let gain_a = utility.marginal_gain(&state_a, cand);
        let gain_b = utility.marginal_gain(&state_b, cand);

        prop_assert!(
            gain_a >= gain_b - 1e-5,
            "Submodular diminishing returns violated: Delta(x|A) = {}, Delta(x|B) = {}",
            gain_a,
            gain_b
        );
    }
}
