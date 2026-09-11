use proptest::prelude::*;
use repotrim_engine::{
    CelfOptimizer, ExactKnapsackOracle, MultiplexCsrGraph, ProbabilisticCoverage, RelationType,
    SparseKernelMatrix, SubmodularConfig, SubmodularUtility, SymbolId, SymbolKind, SymbolNode,
    TextSpan,
};
use std::path::PathBuf;

fn mock_symbol(id: u32, name: &str, file: &str, cost: usize) -> SymbolNode {
    SymbolNode {
        id: SymbolId(id),
        name: name.to_string(),
        kind: SymbolKind::Function,
        file_path: PathBuf::from(file),
        span: TextSpan::new(0, 100, 1, 10),
        signature: format!("pub fn {}()", name),
        docstring: None,
        token_cost: cost,
        ast_hash: [0u8; 32],
        container_name: None,
        trait_name: None,
    }
}

#[test]
fn test_oracle_exact_optimality_vs_brute_force() {
    let n = 6;
    let mut rows = Vec::new();
    for i in 0..n {
        let mut row = Vec::new();
        for j in 0..n {
            let p = if i == j {
                1.0
            } else {
                0.3 / ((i as f32 - j as f32).abs() + 1.0)
            };
            row.push((SymbolId(j as u32), p));
        }
        rows.push(row);
    }
    let kernel = SparseKernelMatrix::from_row_entries(n, &rows);
    let cov = ProbabilisticCoverage::new(kernel, vec![1.0; n]);
    let rel = vec![0.9, 0.7, 0.5, 0.4, 0.3, 0.2];

    let config = SubmodularConfig {
        alpha: 0.4,
        beta: 0.6,
        delta: 0.0,
        lambda: 0.0,
        budget: 50,
        normalize_components: true,
    };
    let utility = SubmodularUtility::new(config, cov, rel);

    let candidates: Vec<SymbolId> = (0..n).map(|i| SymbolId(i as u32)).collect();
    let costs = vec![20, 15, 25, 10, 30, 12];
    let budget = 45;

    let oracle = ExactKnapsackOracle::default();
    let (selected_oracle, state_oracle) = oracle.solve(&utility, &candidates, &costs, budget);

    assert!(state_oracle.total_tokens <= budget);
    assert!(!selected_oracle.is_empty());

    // Exhaustive brute force over all 2^6 = 64 subsets
    let mut max_util = 0.0_f32;
    for mask in 0..(1u32 << n) {
        let mut cost = 0;
        for (i, &c) in costs.iter().enumerate() {
            if (mask & (1 << i)) != 0 {
                cost += c;
            }
        }
        if cost <= budget {
            let mut s = utility.new_state();
            for (i, &cand) in candidates.iter().enumerate() {
                if (mask & (1 << i)) != 0 {
                    utility.add_symbol(&mut s, cand, costs[i]);
                }
            }
            if s.total_utility > max_util {
                max_util = s.total_utility;
            }
        }
    }

    assert!(
        (state_oracle.total_utility - max_util).abs() < 1e-4,
        "ExactKnapsackOracle utility {} must match brute force {}",
        state_oracle.total_utility,
        max_util
    );
}

#[test]
fn test_celf_adversarial_best_singleton_correction() {
    // Adversarial knapsack scenario (Khuller et al., 1999; Sviridenko, 2004):
    // Candidate 0: heavy singleton, cost = 100, utility = 0.95 (density = 0.0095)
    // Candidate 1: cheap item, cost = 60, utility = 0.60 (density = 0.0100)
    // Candidate 2: cheap item, cost = 45, utility = 0.40 (density = 0.0088)
    // Budget = 100.
    // Pure density greedy picks Candidate 1 (cost 60), leaving 40 capacity.
    // Candidate 0 (cost 100) and Candidate 2 (cost 45) cannot fit!
    // Greedy utility = 0.60.
    // Best singleton = Candidate 0 with utility = 0.95.
    // Best-singleton correction guarantees choosing Candidate 0!

    let n = 3;
    let kernel = SparseKernelMatrix::from_row_entries(
        n,
        &[
            vec![(SymbolId(0), 1.0)],
            vec![(SymbolId(1), 1.0)],
            vec![(SymbolId(2), 1.0)],
        ],
    );
    let cov = ProbabilisticCoverage::new(kernel, vec![1.0; n]);
    let rel = vec![0.95, 0.60, 0.40];

    let config = SubmodularConfig {
        alpha: 0.5,
        beta: 0.5,
        delta: 0.0,
        lambda: 0.0,
        budget: 100,
        normalize_components: false,
    };
    let utility = SubmodularUtility::new(config, cov, rel);

    let candidates = vec![SymbolId(0), SymbolId(1), SymbolId(2)];
    let costs = vec![100, 60, 45];
    let budget = 100;

    let optimizer = CelfOptimizer::default();
    let (selected, state) = optimizer.optimize_submodular(&utility, &candidates, &costs, budget);

    assert!(
        selected.contains(&SymbolId(0)),
        "Best-singleton correction must select candidate 0 over density trap: {:?}",
        selected
    );
    assert_eq!(state.total_tokens, 100);
    assert!(
        state.total_utility >= 0.95,
        "Achieved utility {} must match or exceed best singleton 0.95",
        state.total_utility
    );
}

#[test]
fn test_empirical_approximation_ratio_on_graph_topology() {
    let n = 10;
    let syms: Vec<SymbolNode> = (0..n)
        .map(|i| {
            mock_symbol(
                i as u32,
                &format!("sym_{}", i),
                if i < 5 { "src/a.rs" } else { "src/b.rs" },
                (i + 1) * 12,
            )
        })
        .collect();

    let edges = vec![
        (0, 1, RelationType::Calls, 1.0),
        (1, 2, RelationType::Calls, 1.0),
        (2, 3, RelationType::References, 1.0),
        (3, 4, RelationType::Contains, 1.0),
        (5, 6, RelationType::Calls, 1.0),
        (6, 7, RelationType::Calls, 1.0),
        (7, 8, RelationType::Contains, 1.0),
        (4, 9, RelationType::IsTestedBy, 1.0),
    ];

    let graph = MultiplexCsrGraph::from_typed_edges(syms.clone(), &edges);
    let kernel = repotrim_engine::EvidenceKernel::default().compute(&graph, None);
    let weights = vec![1.0; n];
    let cov = ProbabilisticCoverage::new(kernel, weights);
    let rel: Vec<f32> = (0..n).map(|i| 1.0 / (i as f32 * 0.5 + 1.0)).collect();

    let config = SubmodularConfig {
        alpha: 0.35,
        beta: 0.50,
        delta: 0.15,
        lambda: 0.0,
        budget: 120,
        normalize_components: true,
    };
    let utility = SubmodularUtility::new(config, cov, rel).with_symbols(&syms);

    let candidates: Vec<SymbolId> = (0..n).map(|i| SymbolId(i as u32)).collect();
    let costs: Vec<usize> = (0..n).map(|i| (i + 1) * 12).collect();
    let budget = 120;

    let oracle = ExactKnapsackOracle::default();
    let (exact_sel, exact_state) = oracle.solve(&utility, &candidates, &costs, budget);

    let optimizer = CelfOptimizer::default();
    let (celf_sel, celf_state) =
        optimizer.optimize_submodular(&utility, &candidates, &costs, budget);

    assert!(exact_state.total_tokens <= budget);
    assert!(celf_state.total_tokens <= budget);
    assert!(!exact_sel.is_empty());
    assert!(!celf_sel.is_empty());

    let ratio = celf_state.total_utility / exact_state.total_utility;

    // Theoretical worst-case guarantee: 0.5 * (1 - 1/e) ~= 0.316
    assert!(
        ratio >= 0.316,
        "Approximation ratio {} violates theoretical bound 0.316",
        ratio
    );

    // Empirical performance on code graph topology: typically >= 0.85
    assert!(
        ratio >= 0.85,
        "Empirical approximation ratio {} on code graph should exceed 0.85",
        ratio
    );
}

// ============================================================================
// Proptest Invariant Proofs
// ============================================================================

proptest! {
    /// Property: Khuller-Sviridenko Theoretical Approximation Guarantee
    /// For any randomized submodular knapsack instance:
    /// 1. Cost of CELF solution <= Budget
    /// 2. Utility of CELF <= Utility of Exact Oracle + epsilon
    /// 3. Approximation ratio F(S_celf) / F(S*) >= 0.5 * (1 - 1/e) - epsilon ~= 0.316
    #[test]
    fn prop_celf_knapsack_approximation_guarantee(
        n in 4usize..8,
        seed in 1u64..1000,
        budget in 40usize..150,
    ) {
        let mut rows = Vec::new();
        for i in 0..n {
            let mut row = Vec::new();
            for j in 0..n {
                let p = if i == j {
                    1.0
                } else {
                    let h = ((seed.wrapping_mul(43) + i as u64 * 19 + j as u64 * 11) % 100) as f32 / 100.0;
                    h * 0.4
                };
                if p > 0.05 {
                    row.push((SymbolId(j as u32), p));
                }
            }
            rows.push(row);
        }

        let kernel = SparseKernelMatrix::from_row_entries(n, &rows);
        let cov = ProbabilisticCoverage::new(kernel, vec![1.0; n]);
        let rel: Vec<f32> = (0..n).map(|i| {
            let r = ((seed.wrapping_mul(17) + i as u64 * 29) % 100) as f32 / 100.0;
            r.max(0.1)
        }).collect();

        let config = SubmodularConfig {
            alpha: 0.40,
            beta: 0.60,
            delta: 0.0,
            lambda: 0.0, // Monotone submodular regime
            budget,
            normalize_components: true,
        };
        let utility = SubmodularUtility::new(config, cov, rel);

        let candidates: Vec<SymbolId> = (0..n).map(|i| SymbolId(i as u32)).collect();
        let costs: Vec<usize> = (0..n)
            .map(|i| ((seed.wrapping_mul(23) + i as u64 * 13) % 40) as usize + 10)
            .collect();

        let oracle = ExactKnapsackOracle::default();
        let (_exact_sel, exact_state) = oracle.solve(&utility, &candidates, &costs, budget);

        let optimizer = CelfOptimizer::default();
        let (_celf_sel, celf_state) = optimizer.optimize_submodular(&utility, &candidates, &costs, budget);

        // Invariant 1: Feasibility
        prop_assert!(
            celf_state.total_tokens <= budget,
            "CELF tokens {} exceeds budget {}",
            celf_state.total_tokens,
            budget
        );

        // Invariant 2: Exact Oracle is truly optimal
        prop_assert!(
            celf_state.total_utility <= exact_state.total_utility + 1e-4,
            "CELF utility {} cannot exceed Oracle optimal {}",
            celf_state.total_utility,
            exact_state.total_utility
        );

        // Invariant 3: Theoretical approximation lower bound
        if exact_state.total_utility > 1e-5 {
            let ratio = celf_state.total_utility / exact_state.total_utility;
            let theoretical_lower_bound = 0.5 * (1.0 - (-1.0_f32).exp()) - 1e-4; // ~0.316
            prop_assert!(
                ratio >= theoretical_lower_bound,
                "Approximation ratio {} violates theoretical lower bound {}: CELF={}, Oracle={}",
                ratio,
                theoretical_lower_bound,
                celf_state.total_utility,
                exact_state.total_utility
            );
        }
    }
}
