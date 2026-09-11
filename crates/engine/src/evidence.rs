//! Probabilistic Evidence Coverage & Submodular Kernel Engine.
//!
//! # Theoretical Formulation (Nemhauser et al., 1978; Krause & Golovin, 2014)
//! In code intelligence and context optimization, isolated relevance scores fail to capture
//! information overlap and mutual dependency evidence. This module models code elements
//! as information transmitters covering other entities in the Code Property Graph according
//! to an evidence distribution kernel $K(v, u; q) \in [0, 1]$.
//!
//! The total context utility is evaluated via a monotone probabilistic coverage function:
//! $$\mathrm{Cov}(S; q, G) = \sum_{u \in V} \omega(u, q) \left[ 1 - \prod_{v \in S} (1 - K(v, u; q)) \right]$$
//!
//! # Academic Citations
//! - Nemhauser, G. L., Wolsey, L. A., & Fisher, M. L. (1978). "An analysis of approximations
//!   for maximizing submodular set functions—I". *Mathematical Programming*, 14(1), 265-294.
//! - Krause, A., & Golovin, D. (2014). "Submodular Function Maximization". In *Tractability:
//!   Practical Approaches to Hard Problems*, Cambridge University Press, pp. 71-104.
//! - Kempe, D., Kleinberg, J., & Tardos, É. (2003). "Maximizing the spread of influence
//!   through a social network". *KDD 2003*, pp. 137-146.

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};

use crate::multiplex::{MultiplexCsrGraph, RelationWeights};
use crate::symbol::{RelationType, SymbolId};
use crate::task::TaskContext;

/// Configuration parameters for the pairwise evidence distribution kernel $K(v, u; q)$.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct EvidenceKernelConfig {
    /// Exponential distance decay rate $\lambda_{\text{decay}}$ in $K(v, u) \propto \exp(-\lambda \cdot d)$.
    pub decay_rate: f32,
    /// Maximum graph traversal radius $h_{\max}$ (number of hops) for non-zero evidence.
    pub max_hops: usize,
    /// Minimum non-zero coverage probability threshold $\epsilon_{\text{kernel}}$.
    pub min_coverage_threshold: f32,
    /// Proximity multiplier for entities sharing the same file or container scope.
    pub containment_boost: f32,
    /// Self-coverage probability $K(v, v)$ (defaults to 1.0).
    pub self_coverage: f32,
}

impl Default for EvidenceKernelConfig {
    fn default() -> Self {
        Self {
            decay_rate: 0.5,
            max_hops: 2,
            min_coverage_threshold: 1e-4,
            containment_boost: 1.25,
            self_coverage: 1.0,
        }
    }
}

/// Compressed Sparse Row (CSR) representation of the pairwise evidence kernel matrix.
///
/// Stores non-zero pairwise coverage probabilities $K(v, u) \in (0, 1]$ in contiguous arrays
/// enabling zero-allocation iteration over covered entities for any source symbol $v$.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SparseKernelMatrix {
    /// Number of symbols $|V|$ in the graph.
    num_symbols: usize,
    /// Row offsets of length $|V| + 1$. Row $v$ occupies entries `row_ptrs[v]..row_ptrs[v+1]`.
    row_ptrs: Vec<usize>,
    /// Column indices (target covered symbols $u$).
    col_indices: Vec<SymbolId>,
    /// Non-zero coverage probabilities $K(v, u)$.
    values: Vec<f32>,
}

impl SparseKernelMatrix {
    /// Constructs a `SparseKernelMatrix` from pre-sorted row entries.
    ///
    /// Each row list must contain sorted, deduplicated `(SymbolId, f32)` pairs.
    pub fn from_row_entries(num_symbols: usize, rows: &[Vec<(SymbolId, f32)>]) -> Self {
        let mut row_ptrs = Vec::with_capacity(num_symbols + 1);
        let total_entries: usize = rows.iter().map(|r| r.len()).sum();
        let mut col_indices = Vec::with_capacity(total_entries);
        let mut values = Vec::with_capacity(total_entries);

        row_ptrs.push(0);
        for row in rows {
            for &(col, val) in row {
                col_indices.push(col);
                values.push(val.clamp(0.0, 1.0));
            }
            row_ptrs.push(col_indices.len());
        }

        Self {
            num_symbols,
            row_ptrs,
            col_indices,
            values,
        }
    }

    /// Returns the total number of symbols in the kernel matrix.
    #[inline]
    pub fn num_symbols(&self) -> usize {
        self.num_symbols
    }

    /// Returns the total number of non-zero coverage entries in the matrix.
    #[inline]
    pub fn num_entries(&self) -> usize {
        self.values.len()
    }

    /// Returns the slice of covered target symbols for a given source symbol $v$.
    #[inline]
    pub fn row_targets(&self, v: SymbolId) -> &[SymbolId] {
        let v_idx = v.0 as usize;
        if v_idx >= self.num_symbols {
            return &[];
        }
        let start = self.row_ptrs[v_idx];
        let end = self.row_ptrs[v_idx + 1];
        &self.col_indices[start..end]
    }

    /// Returns the slice of coverage probabilities $K(v, \cdot)$ for a given source symbol $v$.
    #[inline]
    pub fn row_values(&self, v: SymbolId) -> &[f32] {
        let v_idx = v.0 as usize;
        if v_idx >= self.num_symbols {
            return &[];
        }
        let start = self.row_ptrs[v_idx];
        let end = self.row_ptrs[v_idx + 1];
        &self.values[start..end]
    }

    /// Iterates over covered pairs `(target, K(v, target))` for a given source symbol $v$.
    #[inline]
    pub fn row_entries(&self, v: SymbolId) -> impl Iterator<Item = (SymbolId, f32)> + '_ {
        let targets = self.row_targets(v);
        let vals = self.row_values(v);
        targets.iter().copied().zip(vals.iter().copied())
    }

    /// Queries the exact pairwise coverage probability $K(v, u)$ via binary search.
    pub fn get(&self, v: SymbolId, u: SymbolId) -> f32 {
        let targets = self.row_targets(v);
        match targets.binary_search(&u) {
            Ok(idx) => self.row_values(v)[idx],
            Err(_) => 0.0,
        }
    }
}

/// Pairwise evidence distribution kernel synthesizer.
///
/// Computes sparse coverage probabilities $K(v, u; q)$ measuring how effectively including
/// symbol $v$ satisfies the informational evidence requirement for symbol $u$ in task $q$.
#[derive(Debug, Clone)]
pub struct EvidenceKernel {
    config: EvidenceKernelConfig,
}

impl Default for EvidenceKernel {
    fn default() -> Self {
        Self::new(EvidenceKernelConfig::default())
    }
}

impl EvidenceKernel {
    /// Creates a new `EvidenceKernel` with the specified configuration parameters.
    pub fn new(config: EvidenceKernelConfig) -> Self {
        Self { config }
    }

    /// Returns a reference to the active kernel configuration.
    pub fn config(&self) -> &EvidenceKernelConfig {
        &self.config
    }

    /// Computes the sparse evidence kernel matrix over a `MultiplexCsrGraph` with optional task context.
    pub fn compute(
        &self,
        graph: &MultiplexCsrGraph,
        task: Option<&TaskContext>,
    ) -> SparseKernelMatrix {
        let n = graph.num_symbols();
        if n == 0 {
            return SparseKernelMatrix::from_row_entries(0, &[]);
        }

        let rel_weights = match task {
            Some(t) => RelationWeights::from_task(t),
            None => RelationWeights::default(),
        };

        let mut rows: Vec<Vec<(SymbolId, f32)>> = Vec::with_capacity(n);

        // Precompute file path and parent container hashes for O(1) containment proximity
        let symbols = graph.symbols();

        for v_idx in 0..n {
            let v_id = SymbolId(v_idx as u32);
            let v_sym = &symbols[v_idx];
            let v_file = &v_sym.file_path;
            let v_container = v_sym.container_name.as_deref();

            // Distance map: target_id -> maximal coverage probability K(v, target)
            let mut cov_map: HashMap<SymbolId, f32> = HashMap::with_capacity(32);
            cov_map.insert(v_id, self.config.self_coverage.clamp(0.0, 1.0));

            // Breadth-First Search up to max_hops tracking cumulative path decay
            let mut queue: VecDeque<(SymbolId, usize, f32)> = VecDeque::with_capacity(32);
            queue.push_back((v_id, 0, 1.0));

            while let Some((curr_id, hops, path_prob)) = queue.pop_front() {
                if hops >= self.config.max_hops {
                    continue;
                }

                let next_hops = hops + 1;
                let hop_decay = (-self.config.decay_rate).exp();

                // Explore all 14 relation layers in the Multiplex CPG
                for rel in RelationType::ALL {
                    let w_rel = rel_weights.weight_for(rel);
                    if w_rel <= 0.0 {
                        continue;
                    }
                    let rel_factor = (w_rel / 2.0).clamp(0.1, 1.0);

                    for &target_u32 in graph.neighbors_for(curr_id, rel) {
                        let target_idx = target_u32 as usize;
                        if target_idx >= n {
                            continue;
                        }
                        let target_id = SymbolId(target_u32);

                        let target_sym = &symbols[target_idx];
                        let same_file = v_file == &target_sym.file_path;
                        let same_container = v_container.is_some()
                            && v_container == target_sym.container_name.as_deref();

                        let containment = if same_container {
                            self.config.containment_boost * 1.15
                        } else if same_file {
                            self.config.containment_boost
                        } else {
                            1.0
                        };

                        let candidate_prob =
                            (path_prob * hop_decay * rel_factor * containment).clamp(0.0, 1.0);

                        if candidate_prob < self.config.min_coverage_threshold {
                            continue;
                        }

                        // Update with maximum path evidence probability
                        let prev_entry = cov_map.entry(target_id).or_insert(0.0);
                        if candidate_prob > *prev_entry {
                            *prev_entry = candidate_prob;
                            queue.push_back((target_id, next_hops, candidate_prob));
                        }
                    }
                }
            }

            // Convert to sorted (SymbolId, f32) row slice
            let mut row: Vec<(SymbolId, f32)> = cov_map.into_iter().collect();
            row.sort_by_key(|&(id, _)| id);
            rows.push(row);
        }

        SparseKernelMatrix::from_row_entries(n, &rows)
    }
}

/// Numerical epsilon for log-space clamping to prevent $\ln(0) = -\infty$.
const LOG_EPSILON: f32 = 1e-7;

/// Dynamic coverage state maintaining log-space uncoverage potentials for incremental updates.
#[derive(Debug, Clone, PartialEq)]
pub struct CoverageState {
    /// Accumulated uncoverage log-products: $L_u(S) = \sum_{v \in S} \ln(1 - K(v, u; q))$.
    log_uncovered: Vec<f32>,
    /// Current evaluated coverage value $\mathrm{Cov}(S; q, G)$.
    total_coverage: f32,
    /// Indices of selected symbols in set $S$.
    selected: Vec<SymbolId>,
}

impl CoverageState {
    /// Creates a fresh coverage state for an empty selection set $S = \emptyset$.
    pub fn empty(num_symbols: usize) -> Self {
        Self {
            log_uncovered: vec![0.0; num_symbols],
            total_coverage: 0.0,
            selected: Vec::new(),
        }
    }

    /// Returns the current total evaluated coverage $\mathrm{Cov}(S)$.
    #[inline]
    pub fn total_coverage(&self) -> f32 {
        self.total_coverage
    }

    /// Returns the slice of selected symbol identifiers in $S$.
    #[inline]
    pub fn selected(&self) -> &[SymbolId] {
        &self.selected
    }

    /// Returns the log-space uncoverage potential for a specific entity $u$.
    #[inline]
    pub fn log_uncovered(&self, u: SymbolId) -> f32 {
        let idx = u.0 as usize;
        if idx < self.log_uncovered.len() {
            self.log_uncovered[idx]
        } else {
            0.0
        }
    }

    /// Returns the individual coverage probability of entity $u$: $1 - \exp(L_u(S))$.
    #[inline]
    pub fn coverage_of(&self, u: SymbolId) -> f32 {
        let l = self.log_uncovered(u);
        (1.0 - l.exp()).clamp(0.0, 1.0)
    }
}

/// Monotone submodular probabilistic coverage evaluator.
///
/// Computes $\mathrm{Cov}(S; q, G) = \sum_{u \in V} \omega(u, q) [1 - \prod_{v \in S} (1 - K(v, u))]$
/// with provable submodularity and $O(|\mathrm{Supp}(K(x, \cdot))|)$ incremental delta evaluations.
#[derive(Debug, Clone)]
pub struct ProbabilisticCoverage {
    /// Sparse pairwise evidence distribution kernel matrix.
    kernel: SparseKernelMatrix,
    /// Entity importance weights $\omega(u, q) \ge 0$ (e.g. from task-conditioned PPR).
    weights: Vec<f32>,
    /// Sum of all entity weights $\sum_{u \in V} \omega(u, q)$.
    total_weight: f32,
}

impl ProbabilisticCoverage {
    /// Constructs a `ProbabilisticCoverage` evaluator from a sparse evidence kernel and node weights.
    pub fn new(kernel: SparseKernelMatrix, weights: Vec<f32>) -> Self {
        let n = kernel.num_symbols();
        let mut aligned_weights = weights;
        if aligned_weights.len() < n {
            aligned_weights.resize(n, 1.0);
        }
        let total_weight: f32 = aligned_weights.iter().sum();

        Self {
            kernel,
            weights: aligned_weights,
            total_weight,
        }
    }

    /// Returns the number of symbols in the evaluation domain.
    #[inline]
    pub fn num_symbols(&self) -> usize {
        self.kernel.num_symbols()
    }

    /// Returns the sum of all entity weights $\sum_u \omega_u$.
    #[inline]
    pub fn total_weight(&self) -> f32 {
        self.total_weight
    }

    /// Returns a reference to the sparse kernel matrix.
    #[inline]
    pub fn kernel(&self) -> &SparseKernelMatrix {
        &self.kernel
    }

    /// Returns a reference to the entity importance weights.
    #[inline]
    pub fn weights(&self) -> &[f32] {
        &self.weights
    }

    /// Initializes a new empty coverage state.
    pub fn new_state(&self) -> CoverageState {
        CoverageState::empty(self.num_symbols())
    }

    /// Computes the exact marginal gain $\Delta(x \mid S) = \mathrm{Cov}(S \cup \{x\}) - \mathrm{Cov}(S)$
    /// in $O(|\mathrm{Supp}(K(x, \cdot))|)$ sparse time.
    ///
    /// Explores only nodes covered by candidate $x$ via log-space potentials:
    /// $$\Delta(x \mid S) = \sum_{u \in \mathrm{Supp}(K(x, \cdot))} \omega_u \cdot \exp(L_u(S)) \cdot K(x, u)$$
    pub fn marginal_gain(&self, state: &CoverageState, candidate: SymbolId) -> f32 {
        let mut gain = 0.0_f32;

        for (target, k_val) in self.kernel.row_entries(candidate) {
            let t_idx = target.0 as usize;
            if t_idx >= self.weights.len() {
                continue;
            }

            let w_u = self.weights[t_idx];
            if w_u <= 0.0 {
                continue;
            }

            let l_u = state.log_uncovered(target);
            let uncov_prob = l_u.exp();
            let new_covered = uncov_prob * k_val;
            gain += w_u * new_covered;
        }

        gain
    }

    /// Updates the `CoverageState` in-place by including the newly selected symbol $x \in V$.
    ///
    /// Executes in $O(|\mathrm{Supp}(K(x, \cdot))|)$ sparse time.
    pub fn add_candidate(&self, state: &mut CoverageState, candidate: SymbolId) {
        for (target, k_val) in self.kernel.row_entries(candidate) {
            let t_idx = target.0 as usize;
            if t_idx >= self.weights.len() {
                continue;
            }

            let w_u = self.weights[t_idx];
            let k_clamped = k_val.clamp(0.0, 1.0 - LOG_EPSILON);
            let log_term = (1.0 - k_clamped).ln();

            let old_l_u = state.log_uncovered[t_idx];
            let new_l_u = old_l_u + log_term;
            state.log_uncovered[t_idx] = new_l_u;

            let old_cov = 1.0 - old_l_u.exp();
            let new_cov = 1.0 - new_l_u.exp();
            let delta = w_u * (new_cov - old_cov);
            state.total_coverage += delta;
        }

        state.selected.push(candidate);
    }

    /// Evaluates the full coverage $\mathrm{Cov}(S)$ from scratch (batch evaluation).
    pub fn evaluate_batch(&self, selected: &[SymbolId]) -> f32 {
        let mut state = self.new_state();
        for &sym in selected {
            self.add_candidate(&mut state, sym);
        }
        state.total_coverage()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::symbol::{SymbolKind, TextSpan};
    use std::path::PathBuf;

    fn mock_symbol(
        id: u32,
        name: &str,
        file: &str,
        container: Option<&str>,
    ) -> crate::symbol::SymbolNode {
        crate::symbol::SymbolNode {
            id: SymbolId(id),
            name: name.to_string(),
            kind: SymbolKind::Function,
            file_path: PathBuf::from(file),
            span: TextSpan::new(0, 100, 1, 10),
            signature: format!("fn {}()", name),
            docstring: None,
            token_cost: 20,
            ast_hash: [0u8; 32],
            container_name: container.map(|s| s.to_string()),
            trait_name: None,
        }
    }

    #[test]
    fn test_sparse_kernel_matrix_construction_and_query() {
        let rows = vec![
            vec![(SymbolId(0), 1.0), (SymbolId(1), 0.6)],
            vec![(SymbolId(1), 1.0), (SymbolId(2), 0.4)],
            vec![(SymbolId(2), 1.0)],
        ];
        let mat = SparseKernelMatrix::from_row_entries(3, &rows);

        assert_eq!(mat.num_symbols(), 3);
        assert_eq!(mat.num_entries(), 5);
        assert_eq!(mat.get(SymbolId(0), SymbolId(0)), 1.0);
        assert_eq!(mat.get(SymbolId(0), SymbolId(1)), 0.6);
        assert_eq!(mat.get(SymbolId(0), SymbolId(2)), 0.0);
        assert_eq!(mat.get(SymbolId(1), SymbolId(2)), 0.4);
    }

    #[test]
    fn test_evidence_kernel_self_coverage_and_containment() {
        let syms = vec![
            mock_symbol(0, "parent_func", "src/core.rs", None),
            mock_symbol(1, "child_func", "src/core.rs", Some("parent_func")),
            mock_symbol(2, "remote_func", "src/other.rs", None),
        ];

        let edges = vec![
            (0, 1, RelationType::Calls, 1.0),
            (1, 2, RelationType::Calls, 1.0),
        ];
        let graph = MultiplexCsrGraph::from_typed_edges(syms, &edges);

        let kernel = EvidenceKernel::default().compute(&graph, None);

        // Self-coverage must be 1.0
        assert_eq!(kernel.get(SymbolId(0), SymbolId(0)), 1.0);
        assert_eq!(kernel.get(SymbolId(1), SymbolId(1)), 1.0);
        assert_eq!(kernel.get(SymbolId(2), SymbolId(2)), 1.0);

        // 1-hop calls with same file containment boost > remote 1-hop
        let k_0_1 = kernel.get(SymbolId(0), SymbolId(1));
        let k_1_2 = kernel.get(SymbolId(1), SymbolId(2));
        assert!(k_0_1 > 0.0);
        assert!(k_1_2 > 0.0);
        assert!(
            k_0_1 > k_1_2,
            "Same file + containment boost must exceed remote file link"
        );
    }

    #[test]
    fn test_probabilistic_coverage_monotonicity_and_parity() {
        let rows = vec![
            vec![(SymbolId(0), 1.0), (SymbolId(1), 0.7), (SymbolId(2), 0.3)],
            vec![(SymbolId(0), 0.5), (SymbolId(1), 1.0), (SymbolId(2), 0.6)],
            vec![(SymbolId(0), 0.2), (SymbolId(1), 0.4), (SymbolId(2), 1.0)],
        ];
        let kernel = SparseKernelMatrix::from_row_entries(3, &rows);
        let weights = vec![1.0, 1.0, 1.0];
        let cov_eval = ProbabilisticCoverage::new(kernel, weights);

        let mut state = cov_eval.new_state();
        assert_eq!(state.total_coverage(), 0.0);

        // Incremental vs batch parity
        let gain0 = cov_eval.marginal_gain(&state, SymbolId(0));
        assert!(gain0 > 0.0);
        cov_eval.add_candidate(&mut state, SymbolId(0));
        assert!((state.total_coverage() - cov_eval.evaluate_batch(&[SymbolId(0)])).abs() < 1e-6);

        let gain1 = cov_eval.marginal_gain(&state, SymbolId(1));
        assert!(gain1 > 0.0);
        cov_eval.add_candidate(&mut state, SymbolId(1));
        assert!(
            (state.total_coverage() - cov_eval.evaluate_batch(&[SymbolId(0), SymbolId(1)])).abs()
                < 1e-6
        );

        // Monotonicity: Cov(S union {x}) >= Cov(S)
        assert!(state.total_coverage() >= gain0);
    }
}
