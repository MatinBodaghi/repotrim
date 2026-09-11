//! Exact Branch-and-Bound Combinatorial Knapsack Oracle.
//!
//! # Theoretical Formulation (Nemhauser et al., 1978; Wolsey, 1999; Sviridenko, 2004)
//! In the research evaluation of approximation algorithms for constrained submodular maximization,
//! bounding the approximation gap requires access to the exact global optimum $S^*$:
//!
//! $$S^* = \arg\max_{S \subseteq V} F(S) \quad \text{subject to} \quad \sum_{v \in S} c(v) \le B$$
//!
//! While maximizing a submodular function under a knapsack constraint is NP-hard, for benchmark
//! evaluation and parameter calibration on candidate sets with $|V_{\text{cand}}| \le 32$, an exact
//! solution can be found via Depth-First Branch-and-Bound with submodular upper-bound pruning.
//!
//! # Submodular Admissible Upper Bound
//! For any search node with selected subset $S$ and remaining candidate set $R$, by submodularity
//! diminishing returns ($\Delta_F(u \mid S') \le \Delta_F(u \mid S)$ for $S \subseteq S'$):
//!
//! $$F(S \cup R') \le F(S) + \sum_{u \in R'} \Delta_F(u \mid S)$$
//!
//! Solving the continuous (fractional) knapsack problem over profits $p(u) = \Delta_F(u \mid S)$
//! and costs $c(u)$ with residual capacity $B - c(S)$ provides an admissible upper bound $UB$:
//!
//! $$UB = F(S) + \mathrm{FractionalKnapsack}\left(\{(\Delta_F(u \mid S), c(u))\}_{u \in R}, B - c(S)\right)$$
//!
//! If $UB \le F(S^*_{\text{best}})$, the subtree is pruned without loss of optimality.
//!
//! # Academic Citations
//! - Nemhauser, G. L., Wolsey, L. A., & Fisher, M. L. (1978). "An analysis of approximations for
//!   maximizing submodular set functions—I". *Mathematical Programming*, 14(1), 265–294.
//! - Wolsey, L. A. (1999). *Integer Programming*. John Wiley & Sons.
//! - Sviridenko, M. (2004). "A note on maximizing a submodular set function subject to a knapsack
//!   constraint". *Operations Research Letters*, 32(1), 41–43.

use serde::{Deserialize, Serialize};

use crate::submodular::{SubmodularUtility, UtilityState};
use crate::symbol::SymbolId;

/// Configuration parameters for the exact knapsack oracle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExactKnapsackConfig {
    /// Maximum allowed candidate instances to prevent exponential search (defaults to 32).
    pub max_candidates: usize,
    /// Maximum search tree nodes explored before halting (defaults to 100,000).
    pub max_explored_nodes: usize,
    /// Whether to enable fractional knapsack linear relaxation pruning.
    pub enable_pruning: bool,
}

impl Default for ExactKnapsackConfig {
    fn default() -> Self {
        Self {
            max_candidates: 32,
            max_explored_nodes: 100_000,
            enable_pruning: true,
        }
    }
}

/// Exact branch-and-bound combinatorial knapsack solver.
///
/// Computes the exact ground-truth global optimum $S^*$ maximizing `SubmodularUtility`
/// under token budget constraint $\sum_{v \in S} c(v) \le B$.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExactKnapsackOracle {
    config: ExactKnapsackConfig,
}

impl Default for ExactKnapsackOracle {
    fn default() -> Self {
        Self::new(ExactKnapsackConfig::default())
    }
}

impl ExactKnapsackOracle {
    /// Creates a new `ExactKnapsackOracle` with given configuration.
    pub fn new(config: ExactKnapsackConfig) -> Self {
        Self { config }
    }

    /// Returns a reference to the active configuration.
    #[inline]
    pub fn config(&self) -> &ExactKnapsackConfig {
        &self.config
    }

    /// Solves for the exact optimal subset $S^*$ and returns the optimal `UtilityState`.
    ///
    /// # Panics
    /// Panics if `candidates.len() > self.config.max_candidates` to prevent runaway factorial search.
    pub fn solve(
        &self,
        utility: &SubmodularUtility,
        candidates: &[SymbolId],
        costs: &[usize],
        budget: usize,
    ) -> (Vec<SymbolId>, UtilityState) {
        assert!(
            candidates.len() <= self.config.max_candidates,
            "ExactKnapsackOracle candidate set size ({}) exceeds maximum configured limit ({})",
            candidates.len(),
            self.config.max_candidates
        );
        assert_eq!(
            candidates.len(),
            costs.len(),
            "Candidate count must match costs count"
        );

        let empty_state = utility.new_state();
        if candidates.is_empty() || budget == 0 {
            return (Vec::new(), empty_state);
        }

        // 1. Filter out infeasible candidates with cost > budget
        let mut feasible_pairs: Vec<(SymbolId, usize)> = candidates
            .iter()
            .copied()
            .zip(costs.iter().copied())
            .filter(|&(_, c)| c <= budget)
            .collect();

        if feasible_pairs.is_empty() {
            return (Vec::new(), empty_state);
        }

        // 2. Pre-sort candidates by initial singleton marginal density descending
        let initial_state = utility.new_state();
        feasible_pairs.sort_by(|&(id_a, cost_a), &(id_b, cost_b)| {
            let gain_a = utility.marginal_gain(&initial_state, id_a);
            let gain_b = utility.marginal_gain(&initial_state, id_b);
            let density_a = if cost_a > 0 {
                gain_a / (cost_a as f32)
            } else {
                gain_a
            };
            let density_b = if cost_b > 0 {
                gain_b / (cost_b as f32)
            } else {
                gain_b
            };
            density_b
                .partial_cmp(&density_a)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // 3. Initialize best solution with best feasible singleton
        let mut best_state = empty_state;
        let mut best_utility = 0.0_f32;

        for &(id, cost) in &feasible_pairs {
            let mut s = utility.new_state();
            let gain = utility.add_symbol(&mut s, id, cost);
            if gain > best_utility {
                best_utility = gain;
                best_state = s;
            }
        }

        let mut nodes_explored = 0usize;
        let current_state = utility.new_state();

        self.branch_and_bound(
            utility,
            &feasible_pairs,
            0,
            current_state,
            budget,
            &mut best_state,
            &mut best_utility,
            &mut nodes_explored,
        );

        let selected = best_state.selected.clone();
        (selected, best_state)
    }

    #[allow(clippy::too_many_arguments)]
    fn branch_and_bound(
        &self,
        utility: &SubmodularUtility,
        items: &[(SymbolId, usize)],
        idx: usize,
        current_state: UtilityState,
        remaining_budget: usize,
        best_state: &mut UtilityState,
        best_utility: &mut f32,
        nodes_explored: &mut usize,
    ) {
        *nodes_explored += 1;
        if *nodes_explored > self.config.max_explored_nodes {
            return;
        }

        if idx >= items.len() || remaining_budget == 0 {
            return;
        }

        // Submodular Linear Relaxation Pruning
        if self.config.enable_pruning {
            let upper_bound =
                self.compute_upper_bound(utility, items, idx, &current_state, remaining_budget);
            if upper_bound <= *best_utility {
                return;
            }
        }

        let (candidate_id, candidate_cost) = items[idx];

        // Branch 1: Include candidate if within budget
        if candidate_cost <= remaining_budget {
            let mut next_state = current_state.clone();
            utility.add_symbol(&mut next_state, candidate_id, candidate_cost);

            if next_state.total_utility > *best_utility {
                *best_utility = next_state.total_utility;
                *best_state = next_state.clone();
            }

            self.branch_and_bound(
                utility,
                items,
                idx + 1,
                next_state,
                remaining_budget - candidate_cost,
                best_state,
                best_utility,
                nodes_explored,
            );
        }

        // Branch 2: Exclude candidate
        self.branch_and_bound(
            utility,
            items,
            idx + 1,
            current_state,
            remaining_budget,
            best_state,
            best_utility,
            nodes_explored,
        );
    }

    /// Computes an admissible upper bound via submodularity and fractional knapsack relaxation.
    fn compute_upper_bound(
        &self,
        utility: &SubmodularUtility,
        items: &[(SymbolId, usize)],
        from_idx: usize,
        current_state: &UtilityState,
        remaining_capacity: usize,
    ) -> f32 {
        let mut potential_gains = Vec::with_capacity(items.len() - from_idx);
        for &(id, cost) in &items[from_idx..] {
            let g = utility.marginal_gain(current_state, id);
            if g > 0.0 && cost > 0 {
                potential_gains.push((g, cost, g / (cost as f32)));
            }
        }

        // Sort remaining items by profit-to-cost ratio descending
        potential_gains.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));

        let mut capacity_left = remaining_capacity as f32;
        let mut fractional_bound = current_state.total_utility;

        for (gain, cost, _) in potential_gains {
            let c = cost as f32;
            if c <= capacity_left {
                fractional_bound += gain;
                capacity_left -= c;
            } else {
                // Fractional item split (Dantzig 1957)
                fractional_bound += gain * (capacity_left / c);
                break;
            }
        }

        fractional_bound
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::evidence::{ProbabilisticCoverage, SparseKernelMatrix};
    use crate::submodular::SubmodularConfig;

    fn build_test_utility(n: usize) -> (SubmodularUtility, Vec<SymbolId>, Vec<usize>) {
        let mut rows = Vec::new();
        for i in 0..n {
            let mut row = Vec::new();
            for j in 0..n {
                let p = if i == j {
                    1.0
                } else {
                    0.2 / ((i as f32 - j as f32).abs() + 1.0)
                };
                row.push((SymbolId(j as u32), p));
            }
            rows.push(row);
        }
        let kernel = SparseKernelMatrix::from_row_entries(n, &rows);
        let cov = ProbabilisticCoverage::new(kernel, vec![1.0; n]);
        let rel: Vec<f32> = (0..n).map(|i| 1.0 / (i as f32 + 1.0)).collect();

        let config = SubmodularConfig {
            alpha: 0.4,
            beta: 0.6,
            delta: 0.0,
            lambda: 0.0,
            budget: 100,
            normalize_components: true,
        };
        let utility = SubmodularUtility::new(config, cov, rel);
        let candidates: Vec<SymbolId> = (0..n).map(|i| SymbolId(i as u32)).collect();
        let costs: Vec<usize> = (0..n).map(|i| (i + 1) * 10).collect();
        (utility, candidates, costs)
    }

    #[test]
    fn test_exact_oracle_small_instance() {
        let (utility, candidates, costs) = build_test_utility(5);
        let oracle = ExactKnapsackOracle::default();
        let budget = 35; // Can take candidate 0 (cost 10) + candidate 1 (cost 20) = 30 <= 35

        let (selected, state) = oracle.solve(&utility, &candidates, &costs, budget);
        assert!(!selected.is_empty());
        assert!(state.total_tokens <= budget);
        assert!(state.total_utility > 0.0);

        // Verification: ensure no other feasible subset in 2^5 = 32 subsets has higher utility
        let mut max_brute_force_util = 0.0_f32;
        for mask in 0..(1u32 << candidates.len()) {
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
                if s.total_utility > max_brute_force_util {
                    max_brute_force_util = s.total_utility;
                }
            }
        }

        assert!(
            (state.total_utility - max_brute_force_util).abs() < 1e-4,
            "Oracle result {} must match brute force optimal {}",
            state.total_utility,
            max_brute_force_util
        );
    }
}
