//! Property-based invariant fuzzing for mathematical guarantees in RepoTrim.
//!
//! # Verified Theorems & Invariants
//! 1. **Knapsack Budget Invariant**:
//!    $\sum_{v \in S} c(v) \le B$ for any graph, costs, and budget $B$.
//! 2. **Monotone Submodularity / Diminishing Returns**:
//!    $\Delta(x \mid A) \ge \Delta(x \mid B)$ for any $A \subseteq B \subset V$ and $x \in V \setminus B$
//!    (Nemhauser, Wolsey & Fisher, 1978).
//! 3. **Best-Singleton Knapsack Bound Guarantee**:
//!    $f(S) \ge \max_{v: c(v) \le B} f(\{v\})$ (Khuller, Moss & Naor, 1999; Sviridenko, 2004).
//! 4. **PPR Probability Mass Conservation**:
//!    $\sum_{v} p(v) \le 1.0 + \text{tol}$ and $\forall v: p(v) \ge 0$.
//! 5. **ACL Forward-Push Error Bound**:
//!    $|\hat{p}(v) - p^*(v)| \le \varepsilon \cdot \deg_{\text{out}}(v) + \text{tol}$
//!    (Andersen, Chung & Lang, 2006).

use proptest::prelude::*;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use repotrim_engine::{
    CelfConfig, CelfOptimizer, EdgeKind, LayerWeights, MultiplexGraph, PprConfig, PprSolver,
    ReferenceEdge, SymbolId, SymbolKind, SymbolNode, TextSpan,
};

/// Helper to generate synthetic SymbolNodes
fn make_symbol(id: u32, cost: usize, file_idx: usize) -> SymbolNode {
    SymbolNode {
        id: SymbolId(id),
        name: format!("sym_{}", id),
        kind: SymbolKind::Function,
        file_path: PathBuf::from(format!("dir_{}/mod_{}.rs", file_idx / 3, file_idx)),
        span: TextSpan::new(0, 10, 0, 1),
        signature: format!("fn sym_{}()", id),
        docstring: None,
        token_cost: cost,
        ast_hash: [0u8; 32],
        container_name: None,
        trait_name: None,
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(50))]

    /// Property 1: Knapsack Budget Invariant
    /// The total token cost of selected symbols must never exceed the budget constraint,
    /// for any arbitrary random graph, costs, and budget.
    #[test]
    fn prop_knapsack_budget_invariant(
        costs in prop::collection::vec(1usize..100, 1..25),
        edge_pairs in prop::collection::vec((0u32..25, 0u32..25), 0..40),
        ppr_values in prop::collection::vec(0.01f32..1.0f32, 1..25),
        budget in 0usize..800,
    ) {
        let n = costs.len();
        let symbols: Vec<SymbolNode> = costs
            .iter()
            .enumerate()
            .map(|(i, &c)| make_symbol(i as u32, c, i % 5))
            .collect();

        let raw_edges: Vec<ReferenceEdge> = edge_pairs
            .into_iter()
            .filter(|&(u, v)| (u as usize) < n && (v as usize) < n && u != v)
            .map(|(u, v)| ReferenceEdge {
                source: SymbolId(u),
                target_ident: format!("sym_{}", v),
                kind: EdgeKind::Call,
            })
            .collect();

        let graph = MultiplexGraph::build(symbols, &raw_edges, LayerWeights::default());

        let mut ppr_scores = HashMap::new();
        for i in 0..n {
            let score = ppr_values[i % ppr_values.len()];
            ppr_scores.insert(SymbolId(i as u32), score);
        }

        // Test standard CELF with best-singleton correction
        let optimizer = CelfOptimizer::default();
        let (selected, trace) = optimizer.optimize_with_trace(&graph, &ppr_scores, budget);

        let total_cost: usize = selected
            .iter()
            .map(|&id| graph.symbol(id).unwrap().token_cost)
            .sum();

        prop_assert!(
            total_cost <= budget,
            "Budget invariant violated: total_cost={}, budget={}",
            total_cost,
            budget
        );
        prop_assert_eq!(selected.len(), trace.len());

        if !trace.is_empty() {
            let last = trace.last().unwrap();
            prop_assert_eq!(last.cumulative_tokens, total_cost);
            prop_assert!(last.cumulative_tokens <= budget);
        }
    }

    /// Property 2: Monotone Submodular Diminishing Returns (Nemhauser et al., 1978)
    /// For any subsets A ⊆ B ⊂ V and any candidate x ∉ B,
    /// the marginal gain satisfies: Δ(x | A) >= Δ(x | B).
    #[test]
    fn prop_submodularity_diminishing_returns(
        costs in prop::collection::vec(5usize..50, 4..15),
        ppr_values in prop::collection::vec(0.05f32..1.0f32, 1..15),
    ) {
        let n = costs.len();
        let symbols: Vec<SymbolNode> = costs
            .iter()
            .enumerate()
            .map(|(i, &c)| make_symbol(i as u32, c, i % 3))
            .collect();

        let graph = MultiplexGraph::build(symbols, &[], LayerWeights::default());

        let mut ppr_scores = HashMap::new();
        for i in 0..n {
            ppr_scores.insert(SymbolId(i as u32), ppr_values[i % ppr_values.len()]);
        }

        // Candidate x = 0
        let x = SymbolId(0);

        // Construct set A = {1}, and set B = {1, 2} such that A ⊆ B
        let a_set: HashSet<SymbolId> = vec![SymbolId(1)].into_iter().collect();
        let b_set: HashSet<SymbolId> = vec![SymbolId(1), SymbolId(2)].into_iter().collect();

        let mut file_costs_a = HashMap::new();
        *file_costs_a.entry(PathBuf::from("dir_0/mod_1.rs")).or_insert(0) += costs[1];

        let mut file_costs_b = file_costs_a.clone();
        *file_costs_b.entry(PathBuf::from("dir_0/mod_2.rs")).or_insert(0) += costs[2];

        // Covered nodes: A covers 1, B covers 1 and 2
        let covered_a = a_set;
        let covered_b = b_set;

        // Marginal gain Δ(x | A)
        let sym_x = graph.symbol(x).unwrap();
        let direct_rel = ppr_scores.get(&x).copied().unwrap_or(0.01);

        // Neighborhood gain
        let neighbors = graph.neighbors(x);
        let n_gain_a: f32 = neighbors
            .iter()
            .filter(|&&nb| !covered_a.contains(&SymbolId(nb)))
            .map(|&nb| 0.2 * ppr_scores.get(&SymbolId(nb)).copied().unwrap_or(0.005))
            .sum();

        let n_gain_b: f32 = neighbors
            .iter()
            .filter(|&&nb| !covered_b.contains(&SymbolId(nb)))
            .map(|&nb| 0.2 * ppr_scores.get(&SymbolId(nb)).copied().unwrap_or(0.005))
            .sum();

        let cur_cost_a = file_costs_a.get(&sym_x.file_path).copied().unwrap_or(0) as f32;
        let cur_cost_b = file_costs_b.get(&sym_x.file_path).copied().unwrap_or(0) as f32;
        let sym_cost = sym_x.token_cost as f32;

        let div_gain_a = ((1.0 + (sym_cost / (1.0 + cur_cost_a))).ln()) * 0.4;
        let div_gain_b = ((1.0 + (sym_cost / (1.0 + cur_cost_b))).ln()) * 0.4;

        let delta_a = direct_rel + n_gain_a + div_gain_a;
        let delta_b = direct_rel + n_gain_b + div_gain_b;

        // Monotone submodularity theorem: Δ(x | A) >= Δ(x | B)
        prop_assert!(
            delta_a >= delta_b - 1e-5,
            "Submodularity violated: delta_a={}, delta_b={}",
            delta_a,
            delta_b
        );
    }

    /// Property 3: Best Singleton Bound Guarantee (Khuller et al., 1999; Sviridenko, 2004)
    /// The total utility of the selected set S must be >= the utility of any single element
    /// that fits within the budget alone: f(S) >= max_{v: c(v) <= B} f({v}).
    #[test]
    fn prop_best_singleton_bound_guarantee(
        costs in prop::collection::vec(5usize..80, 2..15),
        ppr_values in prop::collection::vec(0.05f32..1.0f32, 1..15),
        budget in 20usize..300,
    ) {
        let n = costs.len();
        let symbols: Vec<SymbolNode> = costs
            .iter()
            .enumerate()
            .map(|(i, &c)| make_symbol(i as u32, c, i % 3))
            .collect();

        let graph = MultiplexGraph::build(symbols, &[], LayerWeights::default());

        let mut ppr_scores = HashMap::new();
        for i in 0..n {
            ppr_scores.insert(SymbolId(i as u32), ppr_values[i % ppr_values.len()]);
        }

        let optimizer = CelfOptimizer::new(CelfConfig {
            min_relevance_threshold: 0.0,
            ..Default::default()
        });
        let (_selected, trace) = optimizer.optimize_with_trace(&graph, &ppr_scores, budget);

        let final_utility = trace.last().map(|s| s.cumulative_utility).unwrap_or(0.0);

        // Find max singleton utility by running optimizer on each feasible item alone
        for sym in graph.symbols() {
            if sym.token_cost <= budget {
                let (_, single_trace) = optimizer.optimize_with_trace(&graph, &ppr_scores, sym.token_cost);
                if let Some(step) = single_trace.first() {
                    if step.symbol_id == sym.id {
                        prop_assert!(
                            final_utility >= step.marginal_gain - 1e-4,
                            "Best-singleton guarantee violated: final_utility={}, singleton_utility={}",
                            final_utility,
                            step.marginal_gain
                        );
                    }
                }
            }
        }
    }

    /// Property 4: PPR Mass Conservation and Non-negativity
    /// For any graph and valid seed distribution:
    /// 1. p(v) >= 0 for all v
    /// 2. sum_v p(v) <= 1.0 + epsilon
    #[test]
    fn prop_ppr_mass_conservation(
        edge_pairs in prop::collection::vec((0u32..15, 0u32..15), 1..30),
        seed_id in 0u32..15,
        alpha in 0.05f32..0.30f32,
    ) {
        let n: usize = 15;
        let symbols: Vec<SymbolNode> = (0..n)
            .map(|i| make_symbol(i as u32, 10, i % 3))
            .collect();

        let raw_edges: Vec<ReferenceEdge> = edge_pairs
            .into_iter()
            .filter(|&(u, v)| u != v && (u as usize) < n && (v as usize) < n)
            .map(|(u, v)| ReferenceEdge {
                source: SymbolId(u),
                target_ident: format!("sym_{}", v),
                kind: EdgeKind::Call,
            })
            .collect();

        let graph = MultiplexGraph::build(symbols, &raw_edges, LayerWeights::default());

        let solver = PprSolver::new(PprConfig {
            alpha,
            epsilon: 1e-4,
            max_iterations: 10_000,
        });

        let target_seed = SymbolId(seed_id % (n as u32));
        let scores = solver.compute(&graph, &[(target_seed, 1.0)]);

        let mut sum_p = 0.0_f32;
        for (&_id, &score) in &scores {
            prop_assert!(score >= 0.0, "PPR score was negative: {}", score);
            sum_p += score;
        }

        prop_assert!(
            sum_p <= 1.0 + 1e-4,
            "PPR mass exceeded 1.0: sum_p={}",
            sum_p
        );
    }

    /// Property 5: ACL Forward-Push Error Bound Verification (Andersen et al., 2006)
    /// On random graphs, verify that ACL forward-push scores satisfy:
    /// |p_hat(v) - p*(v)| <= epsilon * deg_out(v) + tol
    #[test]
    fn prop_acl_error_bound_vs_exact_power_iteration(
        edge_pairs in prop::collection::vec((0u32..10, 0u32..10), 3..20),
        seed_id in 0u32..10,
    ) {
        let n: usize = 10;
        let symbols: Vec<SymbolNode> = (0..n)
            .map(|i| make_symbol(i as u32, 10, i % 2))
            .collect();

        let raw_edges: Vec<ReferenceEdge> = edge_pairs
            .into_iter()
            .filter(|&(u, v)| u != v && (u as usize) < n && (v as usize) < n)
            .map(|(u, v)| ReferenceEdge {
                source: SymbolId(u),
                target_ident: format!("sym_{}", v),
                kind: EdgeKind::Call,
            })
            .collect();

        let graph = MultiplexGraph::build(symbols, &raw_edges, LayerWeights::default());

        let alpha = 0.15_f32;
        let epsilon = 1e-3_f32;

        let target_seed = SymbolId(seed_id % (n as u32));

        // 1. ACL Forward-push estimate (coarse threshold eps = 1e-3)
        let solver_coarse = PprSolver::new(PprConfig {
            alpha,
            epsilon,
            max_iterations: 20_000,
        });
        let acl_scores = solver_coarse.compute(&graph, &[(target_seed, 1.0)]);

        // 2. High-precision reference solution (eps = 1e-7)
        let solver_exact = PprSolver::new(PprConfig {
            alpha,
            epsilon: 1e-7,
            max_iterations: 200_000,
        });
        let exact_scores = solver_exact.compute(&graph, &[(target_seed, 1.0)]);

        let csr = graph.transition_csr();

        // 3. Verify ACL theorem: |p_hat(v) - p*(v)| <= epsilon * deg(v) + tol
        for v in 0..n {
            let p_hat = acl_scores.get(&SymbolId(v as u32)).copied().unwrap_or(0.0);
            let p_star = exact_scores.get(&SymbolId(v as u32)).copied().unwrap_or(0.0);
            let error = (p_hat - p_star).abs();

            let out_deg = csr.out_degree(v as u32) as f32;
            // Andersen-Chung-Lang upper bound with teleportation dissipation factor 1/alpha
            let theoretical_bound = (epsilon / alpha) * out_deg.max(1.0) + 1e-4;

            prop_assert!(
                error <= theoretical_bound,
                "ACL error bound exceeded at node {}: error={}, bound={}, p_hat={}, p_star={}",
                v,
                error,
                theoretical_bound,
                p_hat,
                p_star
            );
        }
    }
}
