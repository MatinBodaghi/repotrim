//! Unified Submodular Utility Model for Constrained Codebase Intelligence.
//!
//! # Theoretical Formulation (Nemhauser et al., 1978; Lin & Bilmes, 2011; Krause & Golovin, 2014)
//! In code context extraction under token budget constraints, optimal subset selection
//! balances multiple competing criteria: direct task relevance, multi-hop evidence coverage,
//! test verification assurance, and redundancy elimination:
//!
//! $$F(S; q, G) = \alpha \mathrm{Rel}(S, q) + \beta \mathrm{Cov}(S, q, G) + \delta \mathrm{Test}(S, q, G) - \lambda \mathrm{Red}(S)$$
//!
//! Where:
//! - $\mathrm{Rel}(S, q) = \sum_{v \in S} r(v, q)$: Direct lexical, dense semantic, and seed relevance.
//! - $\mathrm{Cov}(S, q, G)$: Monotone submodular evidence coverage over the typed code property graph.
//! - $\mathrm{Test}(S, q, G)$: Submodular verification coverage ensuring modified logic includes corresponding tests.
//! - $\mathrm{Red}(S)$: Pairwise redundancy penalty penalizing overlapping or duplicate code contexts.
//!
//! # Incremental Marginal Gain Evaluation
//! During greedy and lazy CELF knapsack selection, the marginal gain $\Delta_F(x \mid S) = F(S \cup \{x\}) - F(S)$
//! is computed in $O(|\mathrm{Supp}(K(x, \cdot))|)$ sparse time:
//! $$\Delta_F(x \mid S) = \alpha \Delta_{\mathrm{Rel}}(x) + \beta \Delta_{\mathrm{Cov}}(x \mid S) + \delta \Delta_{\mathrm{Test}}(x \mid S) - \lambda \Delta_{\mathrm{Red}}(x \mid S)$$
//!
//! # Academic Citations
//! - Nemhauser, G. L., Wolsey, L. A., & Fisher, M. L. (1978). "An analysis of approximations for
//!   maximizing submodular set functions—I". *Mathematical Programming*, 14(1), 265–294.
//! - Lin, H., & Bilmes, J. (2011). "A Class of Submodular Functions for Document Summarization".
//!   In *ACL/HLT 2011*, pp. 510–520.
//! - Krause, A., & Golovin, D. (2014). "Submodular Function Maximization". In *Tractability:
//!   Practical Approaches to Hard Problems*, Cambridge University Press, pp. 71–104.

use serde::{Deserialize, Serialize};
use std::collections::HashSet;

use crate::evidence::{CoverageState, ProbabilisticCoverage, SparseKernelMatrix};
use crate::symbol::{SymbolId, SymbolNode};

/// Hyperparameter configuration for the multi-objective submodular utility model.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SubmodularConfig {
    /// Direct task relevance weight $\alpha \ge 0$.
    pub alpha: f32,
    /// Probabilistic evidence coverage weight $\beta \ge 0$.
    pub beta: f32,
    /// Verification test linkage weight $\delta \ge 0$.
    pub delta: f32,
    /// Redundancy / mutual overlap penalty weight $\lambda \ge 0$.
    pub lambda: f32,
    /// Target token budget constraint for the knapsack.
    pub budget: usize,
    /// Whether to normalize components to $[0, 1]$ relative to total domain weights.
    pub normalize_components: bool,
}

impl Default for SubmodularConfig {
    fn default() -> Self {
        Self {
            alpha: 0.35,
            beta: 0.40,
            delta: 0.15,
            lambda: 0.10,
            budget: 4096,
            normalize_components: true,
        }
    }
}

/// Dynamic state tracking selected symbols, coverage log-potentials, and token consumption.
#[derive(Debug, Clone, PartialEq)]
pub struct UtilityState {
    /// Evidence coverage log-uncovered potentials.
    pub coverage_state: CoverageState,
    /// Optional test verification coverage log-uncovered potentials.
    pub test_state: Option<CoverageState>,
    /// Chronological list of selected symbols $S$.
    pub selected: Vec<SymbolId>,
    /// Fast set membership lookup for selected symbols.
    pub selected_set: HashSet<SymbolId>,
    /// Accumulated total utility $F(S)$.
    pub total_utility: f32,
    /// Accumulated direct relevance $\mathrm{Rel}(S)$.
    pub total_relevance: f32,
    /// Accumulated evidence coverage $\mathrm{Cov}(S)$.
    pub total_coverage: f32,
    /// Accumulated test coverage $\mathrm{Test}(S)$.
    pub total_test: f32,
    /// Accumulated redundancy penalty $\mathrm{Red}(S)$.
    pub total_redundancy: f32,
    /// Total tokens consumed by selected symbols under active cost estimator.
    pub total_tokens: usize,
}

impl UtilityState {
    /// Creates an empty utility state for a given symbol domain size.
    pub fn empty(num_symbols: usize, has_test_coverage: bool) -> Self {
        Self {
            coverage_state: CoverageState::empty(num_symbols),
            test_state: if has_test_coverage {
                Some(CoverageState::empty(num_symbols))
            } else {
                None
            },
            selected: Vec::new(),
            selected_set: HashSet::new(),
            total_utility: 0.0,
            total_relevance: 0.0,
            total_coverage: 0.0,
            total_test: 0.0,
            total_redundancy: 0.0,
            total_tokens: 0,
        }
    }

    /// Returns true if symbol $v$ has already been selected into set $S$.
    #[inline]
    pub fn contains(&self, v: SymbolId) -> bool {
        self.selected_set.contains(&v)
    }

    /// Returns the number of symbols currently in set $S$.
    #[inline]
    pub fn len(&self) -> usize {
        self.selected.len()
    }

    /// Returns true if set $S$ is empty.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.selected.is_empty()
    }
}

/// Unified multi-objective submodular utility evaluator.
///
/// Implements $F(S) = \alpha \mathrm{Rel}(S) + \beta \mathrm{Cov}(S) + \delta \mathrm{Test}(S) - \lambda \mathrm{Red}(S)$
/// with exact $O(|\mathrm{Supp}(K(x, \cdot))|)$ marginal gain evaluations.
#[derive(Debug, Clone)]
pub struct SubmodularUtility {
    config: SubmodularConfig,
    evidence_coverage: ProbabilisticCoverage,
    test_coverage: Option<ProbabilisticCoverage>,
    relevance_scores: Vec<f32>,
    total_relevance: f32,
    similarity_matrix: Option<SparseKernelMatrix>,
    file_assignments: Vec<u32>,
    container_assignments: Vec<Option<u32>>,
}

impl SubmodularUtility {
    /// Constructs a new `SubmodularUtility` evaluator.
    pub fn new(
        config: SubmodularConfig,
        evidence_coverage: ProbabilisticCoverage,
        relevance_scores: Vec<f32>,
    ) -> Self {
        let n = evidence_coverage.num_symbols();
        let mut aligned_rel = relevance_scores;
        if aligned_rel.len() < n {
            aligned_rel.resize(n, 0.0);
        }
        let total_relevance: f32 = aligned_rel.iter().sum();

        Self {
            config,
            evidence_coverage,
            test_coverage: None,
            relevance_scores: aligned_rel,
            total_relevance: total_relevance.max(1e-6),
            similarity_matrix: None,
            file_assignments: vec![0; n],
            container_assignments: vec![None; n],
        }
    }

    /// Attaches an optional test verification coverage evaluator.
    pub fn with_test_coverage(mut self, test_coverage: ProbabilisticCoverage) -> Self {
        self.test_coverage = Some(test_coverage);
        self
    }

    /// Attaches an optional pairwise similarity matrix for redundancy calculation.
    pub fn with_similarity_matrix(mut self, similarity: SparseKernelMatrix) -> Self {
        self.similarity_matrix = Some(similarity);
        self
    }

    /// Configures structural file and container mappings from extracted symbols
    /// for fast heuristic redundancy estimation.
    pub fn with_symbols(mut self, symbols: &[SymbolNode]) -> Self {
        let n = self.evidence_coverage.num_symbols();
        let mut file_map = std::collections::HashMap::new();
        let mut container_map = std::collections::HashMap::new();
        let mut next_file_id = 0u32;
        let mut next_container_id = 0u32;

        self.file_assignments = vec![0; n];
        self.container_assignments = vec![None; n];

        for sym in symbols {
            let idx = sym.id.0 as usize;
            if idx < n {
                let f_id = *file_map.entry(&sym.file_path).or_insert_with(|| {
                    let id = next_file_id;
                    next_file_id += 1;
                    id
                });
                self.file_assignments[idx] = f_id;

                if let Some(c) = &sym.container_name {
                    let c_id = *container_map.entry((f_id, c.clone())).or_insert_with(|| {
                        let id = next_container_id;
                        next_container_id += 1;
                        id
                    });
                    self.container_assignments[idx] = Some(c_id);
                }
            }
        }
        self
    }

    /// Returns a reference to the configuration.
    #[inline]
    pub fn config(&self) -> &SubmodularConfig {
        &self.config
    }

    /// Returns the total number of symbols in the evaluation domain.
    #[inline]
    pub fn num_symbols(&self) -> usize {
        self.evidence_coverage.num_symbols()
    }

    /// Creates a fresh empty `UtilityState`.
    pub fn new_state(&self) -> UtilityState {
        UtilityState::empty(self.num_symbols(), self.test_coverage.is_some())
    }

    /// Computes the exact marginal utility gain $\Delta_F(x \mid S) = F(S \cup \{x\}) - F(S)$.
    pub fn marginal_gain(&self, state: &UtilityState, candidate: SymbolId) -> f32 {
        if state.contains(candidate) {
            return 0.0;
        }

        let idx = candidate.0 as usize;
        if idx >= self.num_symbols() {
            return 0.0;
        }

        // 1. Direct relevance marginal gain $\Delta_{\mathrm{Rel}}(x)$
        let raw_rel = self.relevance_scores.get(idx).copied().unwrap_or(0.0);
        let rel_gain = if self.config.normalize_components && self.total_relevance > 0.0 {
            raw_rel / self.total_relevance
        } else {
            raw_rel
        };

        // 2. Probabilistic evidence coverage marginal gain $\Delta_{\mathrm{Cov}}(x \mid S)$
        let cov_gain = self
            .evidence_coverage
            .marginal_gain(&state.coverage_state, candidate);
        let norm_cov_gain =
            if self.config.normalize_components && self.evidence_coverage.total_weight() > 0.0 {
                cov_gain / self.evidence_coverage.total_weight()
            } else {
                cov_gain
            };

        // 3. Test verification marginal gain $\Delta_{\mathrm{Test}}(x \mid S)$
        let test_gain = match (&self.test_coverage, &state.test_state) {
            (Some(tc), Some(ts)) => {
                let tg = tc.marginal_gain(ts, candidate);
                if self.config.normalize_components && tc.total_weight() > 0.0 {
                    tg / tc.total_weight()
                } else {
                    tg
                }
            }
            _ => 0.0,
        };

        // 4. Redundancy penalty $\Delta_{\mathrm{Red}}(x \mid S)$
        let red_penalty = self.redundancy_penalty(state, candidate);
        let norm_red_penalty = if self.config.normalize_components && !state.selected.is_empty() {
            red_penalty / (state.selected.len() as f32)
        } else {
            red_penalty
        };

        let net_gain = self.config.alpha * rel_gain
            + self.config.beta * norm_cov_gain
            + self.config.delta * test_gain
            - self.config.lambda * norm_red_penalty;

        // Monotone non-negative guarantee: net marginal gain is bounded at 0.0
        net_gain.max(0.0)
    }

    /// Computes pairwise redundancy penalty $\sum_{v \in S} \mathrm{Sim}(x, v)$.
    fn redundancy_penalty(&self, state: &UtilityState, candidate: SymbolId) -> f32 {
        if state.selected.is_empty() {
            return 0.0;
        }

        let cand_idx = candidate.0 as usize;

        // Fast path: use pairwise similarity matrix if available
        if let Some(sim_mat) = &self.similarity_matrix {
            let mut penalty = 0.0_f32;
            for (target, sim_val) in sim_mat.row_entries(candidate) {
                if state.contains(target) {
                    penalty += sim_val;
                }
            }
            return penalty;
        }

        // Structural fallback: file and container sharing heuristics
        let cand_file = self.file_assignments.get(cand_idx).copied().unwrap_or(0);
        let cand_container = self.container_assignments.get(cand_idx).copied().flatten();

        let mut penalty = 0.0_f32;
        for &v in &state.selected {
            let v_idx = v.0 as usize;
            if v == candidate {
                penalty += 1.0;
                continue;
            }

            let v_file = self.file_assignments.get(v_idx).copied().unwrap_or(0);
            if cand_file == v_file {
                penalty += 0.20; // Shared file baseline

                let v_container = self.container_assignments.get(v_idx).copied().flatten();
                if cand_container.is_some() && cand_container == v_container {
                    penalty += 0.40; // Shared class/struct container
                }
            }
        }

        penalty
    }

    /// Adds a symbol to the utility state, updating all log-potentials, accumulated totals,
    /// and token consumption.
    ///
    /// Returns the exact marginal gain achieved by the addition.
    pub fn add_symbol(
        &self,
        state: &mut UtilityState,
        candidate: SymbolId,
        token_cost: usize,
    ) -> f32 {
        if state.contains(candidate) {
            return 0.0;
        }

        let idx = candidate.0 as usize;
        let gain = self.marginal_gain(state, candidate);

        // Update component totals
        let raw_rel = self.relevance_scores.get(idx).copied().unwrap_or(0.0);
        let rel_val = if self.config.normalize_components && self.total_relevance > 0.0 {
            raw_rel / self.total_relevance
        } else {
            raw_rel
        };
        state.total_relevance += rel_val;

        let prev_cov = state.coverage_state.total_coverage();
        self.evidence_coverage
            .add_candidate(&mut state.coverage_state, candidate);
        let cov_marginal = state.coverage_state.total_coverage() - prev_cov;
        let norm_cov_marginal =
            if self.config.normalize_components && self.evidence_coverage.total_weight() > 0.0 {
                cov_marginal / self.evidence_coverage.total_weight()
            } else {
                cov_marginal
            };
        state.total_coverage += norm_cov_marginal;

        if let (Some(tc), Some(ts)) = (&self.test_coverage, &mut state.test_state) {
            let prev_test = ts.total_coverage();
            tc.add_candidate(ts, candidate);
            let test_marginal = ts.total_coverage() - prev_test;
            let norm_test_marginal = if self.config.normalize_components && tc.total_weight() > 0.0
            {
                test_marginal / tc.total_weight()
            } else {
                test_marginal
            };
            state.total_test += norm_test_marginal;
        }

        let red_penalty = self.redundancy_penalty(state, candidate);
        let norm_red_penalty = if self.config.normalize_components && !state.selected.is_empty() {
            red_penalty / (state.selected.len() as f32)
        } else {
            red_penalty
        };
        state.total_redundancy += norm_red_penalty;

        state.selected.push(candidate);
        state.selected_set.insert(candidate);
        state.total_tokens += token_cost;
        state.total_utility += gain;

        gain
    }

    /// Optimizes candidate selection using the Lazy CELF algorithm with best-singleton correction.
    pub fn optimize_celf(
        &self,
        candidates: &[SymbolId],
        costs: &[usize],
        budget: usize,
    ) -> (Vec<SymbolId>, UtilityState) {
        crate::celf::CelfOptimizer::default().optimize_submodular(self, candidates, costs, budget)
    }

    /// Solves for the exact ground-truth global optimum using branch-and-bound (for instances <= 32).
    pub fn optimize_exact(
        &self,
        candidates: &[SymbolId],
        costs: &[usize],
        budget: usize,
    ) -> (Vec<SymbolId>, UtilityState) {
        crate::oracle::ExactKnapsackOracle::default().solve(self, candidates, costs, budget)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::evidence::SparseKernelMatrix;

    fn build_test_setup() -> (ProbabilisticCoverage, Vec<f32>) {
        let kernel_entries = vec![
            vec![(SymbolId(0), 1.0), (SymbolId(1), 0.6), (SymbolId(2), 0.2)],
            vec![(SymbolId(0), 0.6), (SymbolId(1), 1.0), (SymbolId(2), 0.5)],
            vec![(SymbolId(0), 0.2), (SymbolId(1), 0.5), (SymbolId(2), 1.0)],
        ];
        let kernel = SparseKernelMatrix::from_row_entries(3, &kernel_entries);
        let weights = vec![1.0, 1.0, 1.0];
        let coverage = ProbabilisticCoverage::new(kernel, weights);
        let relevance = vec![0.9, 0.5, 0.3];
        (coverage, relevance)
    }

    #[test]
    fn test_utility_initialization_and_first_addition() {
        let (cov, rel) = build_test_setup();
        let config = SubmodularConfig::default();
        let utility = SubmodularUtility::new(config, cov, rel);
        let mut state = utility.new_state();

        assert_eq!(state.len(), 0);
        assert_eq!(state.total_tokens, 0);
        assert_eq!(state.total_utility, 0.0);

        let gain = utility.marginal_gain(&state, SymbolId(0));
        assert!(gain > 0.0);

        let added_gain = utility.add_symbol(&mut state, SymbolId(0), 50);
        assert!((gain - added_gain).abs() < 1e-5);
        assert_eq!(state.len(), 1);
        assert_eq!(state.total_tokens, 50);
        assert!(!state.is_empty());
        assert!(state.contains(SymbolId(0)));
    }

    #[test]
    fn test_diminishing_returns_monotonicity() {
        let (cov, rel) = build_test_setup();
        let config = SubmodularConfig {
            lambda: 0.0, // Pure submodular components
            ..Default::default()
        };
        let utility = SubmodularUtility::new(config, cov, rel);

        let mut state_empty = utility.new_state();
        let mut state_one = utility.new_state();
        utility.add_symbol(&mut state_one, SymbolId(1), 30);

        // Marginal gain of SymbolId(0) on empty set vs. set containing SymbolId(1)
        let gain_on_empty = utility.marginal_gain(&state_empty, SymbolId(0));
        let gain_on_one = utility.marginal_gain(&state_one, SymbolId(0));

        // Diminishing returns: Delta(x | empty) >= Delta(x | {1})
        assert!(
            gain_on_empty >= gain_on_one,
            "gain_on_empty {} should be >= gain_on_one {}",
            gain_on_empty,
            gain_on_one
        );

        // Adding candidate to state_empty
        utility.add_symbol(&mut state_empty, SymbolId(0), 40);
        assert!(state_empty.total_utility > 0.0);
    }

    #[test]
    fn test_redundancy_discount() {
        let (cov, rel) = build_test_setup();
        let config_no_pen = SubmodularConfig {
            lambda: 0.0,
            ..Default::default()
        };
        let config_with_pen = SubmodularConfig {
            lambda: 0.5,
            ..Default::default()
        };

        let util_no_pen = SubmodularUtility::new(config_no_pen, cov.clone(), rel.clone());
        let util_with_pen = SubmodularUtility::new(config_with_pen, cov, rel);

        let mut s1 = util_no_pen.new_state();
        let mut s2 = util_with_pen.new_state();

        util_no_pen.add_symbol(&mut s1, SymbolId(0), 40);
        util_with_pen.add_symbol(&mut s2, SymbolId(0), 40);

        let gain_no_pen = util_no_pen.marginal_gain(&s1, SymbolId(1));
        let gain_with_pen = util_with_pen.marginal_gain(&s2, SymbolId(1));

        assert!(
            gain_with_pen <= gain_no_pen,
            "Redundancy penalty must discount marginal gain"
        );
    }

    #[test]
    fn test_optimize_celf_knapsack_and_best_singleton() {
        let (cov, rel) = build_test_setup();
        let config = SubmodularConfig {
            alpha: 0.5,
            beta: 0.5,
            delta: 0.0,
            lambda: 0.0,
            budget: 60,
            normalize_components: true,
        };
        let util = SubmodularUtility::new(config, cov, rel);

        let candidates = vec![SymbolId(0), SymbolId(1), SymbolId(2)];
        let costs = vec![30, 25, 55];

        let (selected, state) = util.optimize_celf(&candidates, &costs, 60);

        assert!(!selected.is_empty());
        assert!(state.total_tokens <= 60);
        assert!(state.total_utility > 0.0);

        // Zero budget yields empty selection
        let (empty_sel, empty_state) = util.optimize_celf(&candidates, &costs, 0);
        assert!(empty_sel.is_empty());
        assert_eq!(empty_state.total_tokens, 0);
        assert_eq!(empty_state.total_utility, 0.0);
    }
}
