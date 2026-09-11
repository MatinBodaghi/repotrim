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
}
