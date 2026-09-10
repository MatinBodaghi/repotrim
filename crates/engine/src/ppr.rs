use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};

use crate::graph::MultiplexGraph;
use crate::symbol::SymbolId;

/// Configuration parameters for the Andersen-Chung-Lang (ACL) Forward-Push PPR solver.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PprConfig {
    /// Teleport / restart probability damping factor ($\alpha$). Default: `0.15`.
    pub alpha: f32,
    /// Residual push threshold ($\epsilon$). Default: `1e-4`.
    pub epsilon: f32,
    /// Maximum allowed push iterations to prevent unbounded execution. Default: `100_000`.
    pub max_iterations: usize,
}

impl Default for PprConfig {
    fn default() -> Self {
        Self {
            alpha: 0.15,
            epsilon: 1e-4,
            max_iterations: 100_000,
        }
    }
}

/// Detailed output from the Andersen-Chung-Lang (ACL) Forward-Push PPR solver,
/// including point-wise relevance scores, unallocated residuals, and theoretical error bounds.
///
/// # Mathematical Error Bounds (Andersen, Chung & Lang, 2006)
/// The stationary Personalized PageRank vector $p^*$ satisfies the matrix invariant:
/// $$p^* = p + (I - (1 - \alpha) P^T)^{-1} r$$
/// Because remaining residuals $r(u) \ge 0$ for all $u \in V$, the computed PageRank
/// vector $p$ is a strict monotone lower bound on the true stationary distribution ($p \le p^*$).
///
/// For any node $v \in V$, the point-wise approximation error $|p^*(v) - p(v)|$ is bounded by:
/// $$\delta(v) \le \frac{\max(\varepsilon, r_{\max})}{\alpha} \cdot \max(1.0, d_{\text{in}}(v))$$
/// where $r_{\max} = \max_{u \in V} r(u)$, $\alpha$ is the teleportation damping factor, and
/// $d_{\text{in}}(v)$ is the in-degree of $v$ in the graph.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PprResult {
    /// Personalized PageRank relevance scores p(v) for symbols with score > 0.
    pub scores: HashMap<SymbolId, f32>,
    /// Remaining unpushed residuals r(v) at algorithm termination.
    pub residuals: HashMap<SymbolId, f32>,
    /// Theoretical maximum point-wise error bounds delta(v) = |p*(v) - p(v)|.
    pub error_bounds: HashMap<SymbolId, f32>,
    /// Maximum residual across all nodes at termination: max_{u} r(u).
    pub max_residual: f32,
    /// Sum of all remaining residuals (unallocated probability mass): sum_{u} r(u).
    pub total_residual: f32,
    /// Total number of push iterations performed.
    pub iterations: usize,
    /// Whether the solver reached max_iterations before full convergence.
    pub truncated: bool,
}

impl Default for PprResult {
    fn default() -> Self {
        Self {
            scores: HashMap::new(),
            residuals: HashMap::new(),
            error_bounds: HashMap::new(),
            max_residual: 0.0,
            total_residual: 0.0,
            iterations: 0,
            truncated: false,
        }
    }
}

/// High-performance local Personalized PageRank (PPR) solver using the Andersen-Chung-Lang (ACL)
/// Forward-Push algorithm.
///
/// Achieves $O(1/\epsilon)$ local time complexity independent of repository size $|V|$,
/// computing sparse diffusion scores around seed symbols in $<2\text{ms}$.
#[derive(Debug, Clone, Default)]
pub struct PprSolver {
    config: PprConfig,
}

impl PprSolver {
    /// Creates a new `PprSolver` with custom configuration.
    pub fn new(config: PprConfig) -> Self {
        Self { config }
    }

    /// Returns the configuration parameters of this solver.
    pub fn config(&self) -> PprConfig {
        self.config
    }

    /// Computes Personalized PageRank diffusion scores seeded at the provided `(SymbolId, weight)` pairs.
    ///
    /// The input seed weights are automatically normalized so $\sum p_0(s) = 1.0$.
    ///
    /// Returns a map of `SymbolId -> f32` containing non-zero relevance scores.
    pub fn compute(
        &self,
        graph: &MultiplexGraph,
        seeds: &[(SymbolId, f32)],
    ) -> HashMap<SymbolId, f32> {
        self.compute_detailed(graph, seeds).scores
    }

    /// Computes Personalized PageRank diffusion scores and detailed error bounds seeded at `(SymbolId, weight)`.
    ///
    /// Evaluates the Andersen-Chung-Lang (2006) forward-push algorithm and computes point-wise
    /// theoretical error bounds $\delta(v) \le \frac{\max(\varepsilon, r_{\max})}{\alpha} \max(1, d_{\text{in}}(v))$.
    pub fn compute_detailed(&self, graph: &MultiplexGraph, seeds: &[(SymbolId, f32)]) -> PprResult {
        let n = graph.num_symbols();
        if n == 0 || seeds.is_empty() {
            return PprResult::default();
        }

        let alpha = self.config.alpha;
        let epsilon = self.config.epsilon;
        let csr = graph.transition_csr();

        let mut p = vec![0.0_f32; n];
        let mut r = vec![0.0_f32; n];
        let mut in_queue = vec![false; n];
        let mut queue = VecDeque::new();

        // Normalize seed distribution
        let total_seed_weight: f32 = seeds.iter().map(|s| s.1.max(0.0)).sum();
        if total_seed_weight <= 0.0 {
            return PprResult::default();
        }

        let inv_seed_sum = 1.0 / total_seed_weight;
        for &(seed_id, weight) in seeds {
            let idx = seed_id.0 as usize;
            if idx < n && weight > 0.0 {
                let initial_mass = weight * inv_seed_sum;
                r[idx] += initial_mass;
                if r[idx] >= epsilon && !in_queue[idx] {
                    queue.push_back(idx as u32);
                    in_queue[idx] = true;
                }
            }
        }

        let mut iterations = 0;
        let mut truncated = false;

        // Forward-Push iteration loop
        while let Some(u) = queue.pop_front() {
            let u_idx = u as usize;
            in_queue[u_idx] = false;

            let residual_u = r[u_idx];
            if residual_u < epsilon {
                continue;
            }

            iterations += 1;
            if iterations >= self.config.max_iterations {
                truncated = true;
                break;
            }

            // Convert fraction alpha * r(u) into real PageRank
            p[u_idx] += alpha * residual_u;
            r[u_idx] = 0.0;

            let out_degree = csr.out_degree(u);
            let push_mass = (1.0 - alpha) * residual_u;

            if out_degree == 0 {
                // Sink node: retain the push mass in p(u) to conserve probability
                p[u_idx] += push_mass;
            } else {
                // Push remaining mass (1 - alpha) * r(u) along out-edges
                let (neighbors, weights) = csr.row_slice(u);
                for (&v, &w) in neighbors.iter().zip(weights.iter()) {
                    let v_idx = v as usize;
                    r[v_idx] += push_mass * w;

                    if r[v_idx] >= epsilon && !in_queue[v_idx] {
                        queue.push_back(v);
                        in_queue[v_idx] = true;
                    }
                }
            }
        }

        // Compute in-degrees in O(|E|) time for error bounding
        let mut in_degrees = vec![0usize; n];
        for row in 0..n {
            for &target in csr.neighbors(row as u32) {
                if (target as usize) < n {
                    in_degrees[target as usize] += 1;
                }
            }
        }

        let max_residual = r.iter().copied().fold(0.0_f32, f32::max);
        let total_residual: f32 = r.iter().copied().sum();
        let effective_eps = if truncated {
            max_residual.max(epsilon)
        } else {
            epsilon
        };

        let mut scores = HashMap::with_capacity(p.iter().filter(|&&score| score > 0.0).count());
        let mut residuals = HashMap::new();
        let mut error_bounds = HashMap::with_capacity(n);

        for (i, &score) in p.iter().enumerate() {
            let id = SymbolId(i as u32);
            if score > 0.0 {
                scores.insert(id, score);
            }
            let res = r[i];
            if res > 0.0 {
                residuals.insert(id, res);
            }
            // Andersen-Chung-Lang (2006) theorem: |p*(v) - p(v)| <= (eps / alpha) * max(1, deg_in(v))
            let deg_in = in_degrees[i] as f32;
            let bound = (effective_eps / alpha) * deg_in.max(1.0);
            error_bounds.insert(id, bound);
        }

        PprResult {
            scores,
            residuals,
            error_bounds,
            max_residual,
            total_residual,
            iterations,
            truncated,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::LayerWeights;
    use crate::symbol::{EdgeKind, ReferenceEdge, SymbolKind, SymbolNode, TextSpan};
    use std::path::PathBuf;

    fn make_test_node(id: u32, name: &str, file: &str) -> SymbolNode {
        SymbolNode {
            id: SymbolId(id),
            name: name.to_string(),
            kind: SymbolKind::Function,
            file_path: PathBuf::from(file),
            span: TextSpan::new(0, 10, 0, 1),
            signature: format!("fn {}()", name),
            docstring: None,
            token_cost: 5,
            ast_hash: [0u8; 32],
            container_name: None,
            trait_name: None,
        }
    }

    #[test]
    fn test_ppr_probability_conservation() {
        // Line graph: 0 -> 1 -> 2 -> 3
        let symbols = vec![
            make_test_node(0, "n0", "a.rs"),
            make_test_node(1, "n1", "a.rs"),
            make_test_node(2, "n2", "a.rs"),
            make_test_node(3, "n3", "a.rs"),
        ];

        let edges = vec![
            ReferenceEdge {
                source: SymbolId(0),
                target_ident: "n1".to_string(),
                kind: EdgeKind::Call,
            },
            ReferenceEdge {
                source: SymbolId(1),
                target_ident: "n2".to_string(),
                kind: EdgeKind::Call,
            },
            ReferenceEdge {
                source: SymbolId(2),
                target_ident: "n3".to_string(),
                kind: EdgeKind::Call,
            },
        ];

        let graph = MultiplexGraph::build(symbols, &edges, LayerWeights::default());
        let solver = PprSolver::new(PprConfig {
            alpha: 0.15,
            epsilon: 1e-6,
            max_iterations: 100_000,
        });

        let seeds = vec![(SymbolId(0), 1.0)];
        let ppr = solver.compute(&graph, &seeds);

        // Sum of PageRank mass should be ~1.0
        let total_mass: f32 = ppr.values().sum();
        assert!(
            (total_mass - 1.0).abs() < 1e-3,
            "Total mass was {}",
            total_mass
        );

        // Seed 0 should have high score
        assert!(ppr.get(&SymbolId(0)).copied().unwrap_or(0.0) > 0.1);
        // Node 1 should be reachable
        assert!(ppr.get(&SymbolId(1)).copied().unwrap_or(0.0) > 0.0);
        // Node 3 (sink) accumulates remaining mass
        assert!(ppr.get(&SymbolId(3)).copied().unwrap_or(0.0) > 0.0);
    }

    #[test]
    fn test_ppr_cyclic_graph_convergence() {
        // Cycle: 0 -> 1 -> 0
        let symbols = vec![
            make_test_node(0, "loop_a", "a.rs"),
            make_test_node(1, "loop_b", "a.rs"),
        ];

        let edges = vec![
            ReferenceEdge {
                source: SymbolId(0),
                target_ident: "loop_b".to_string(),
                kind: EdgeKind::Call,
            },
            ReferenceEdge {
                source: SymbolId(1),
                target_ident: "loop_a".to_string(),
                kind: EdgeKind::Call,
            },
        ];

        let graph = MultiplexGraph::build(symbols, &edges, LayerWeights::default());
        let solver = PprSolver::default();

        let seeds = vec![(SymbolId(0), 1.0)];
        let ppr = solver.compute(&graph, &seeds);

        assert_eq!(ppr.len(), 2);
        let s0 = ppr.get(&SymbolId(0)).unwrap();
        let s1 = ppr.get(&SymbolId(1)).unwrap();

        // Seed node has higher score than neighbor
        assert!(s0 > s1);
        let total: f32 = s0 + s1;
        assert!((total - 1.0).abs() < 1e-3);
    }

    #[test]
    fn test_ppr_isolated_seed() {
        let symbols = vec![
            make_test_node(0, "alone", "a.rs"),
            make_test_node(1, "unconnected", "b.rs"),
        ];

        let graph = MultiplexGraph::build(symbols, &[], LayerWeights::default());
        let solver = PprSolver::default();

        let seeds = vec![(SymbolId(0), 1.0)];
        let ppr = solver.compute(&graph, &seeds);

        assert_eq!(ppr.len(), 1);
        assert_eq!(ppr.get(&SymbolId(0)), Some(&1.0));
    }

    #[test]
    fn test_ppr_detailed_residuals_and_bounds() {
        let symbols = vec![
            make_test_node(0, "root", "a.rs"),
            make_test_node(1, "middle", "a.rs"),
            make_test_node(2, "leaf", "a.rs"),
        ];

        let edges = vec![
            ReferenceEdge {
                source: SymbolId(0),
                target_ident: "middle".to_string(),
                kind: EdgeKind::Call,
            },
            ReferenceEdge {
                source: SymbolId(1),
                target_ident: "leaf".to_string(),
                kind: EdgeKind::Call,
            },
        ];

        let graph = MultiplexGraph::build(symbols, &edges, LayerWeights::default());
        let solver = PprSolver::new(PprConfig {
            alpha: 0.15,
            epsilon: 1e-4,
            max_iterations: 10_000,
        });

        let res = solver.compute_detailed(&graph, &[(SymbolId(0), 1.0)]);
        assert!(!res.truncated);
        assert!(res.iterations > 0);
        assert!(res.max_residual < 1e-4);
        assert!(res.total_residual >= 0.0);
        assert_eq!(res.error_bounds.len(), 3);

        // Every node must have a positive theoretical error bound
        for &bound in res.error_bounds.values() {
            assert!(bound > 0.0, "Theoretical error bound must be > 0.0");
        }

        // Middle has in-degree 1, root has in-degree 0
        let root_bound = res.error_bounds.get(&SymbolId(0)).copied().unwrap();
        let middle_bound = res.error_bounds.get(&SymbolId(1)).copied().unwrap();
        assert!(root_bound > 0.0);
        assert!(middle_bound > 0.0);
    }

    #[test]
    fn test_ppr_detailed_iteration_truncation() {
        let symbols = vec![
            make_test_node(0, "loop_a", "a.rs"),
            make_test_node(1, "loop_b", "a.rs"),
        ];

        let edges = vec![
            ReferenceEdge {
                source: SymbolId(0),
                target_ident: "loop_b".to_string(),
                kind: EdgeKind::Call,
            },
            ReferenceEdge {
                source: SymbolId(1),
                target_ident: "loop_a".to_string(),
                kind: EdgeKind::Call,
            },
        ];

        let graph = MultiplexGraph::build(symbols, &edges, LayerWeights::default());
        // Configure only 1 iteration so truncation triggers
        let solver = PprSolver::new(PprConfig {
            alpha: 0.15,
            epsilon: 1e-8,
            max_iterations: 1,
        });

        let res = solver.compute_detailed(&graph, &[(SymbolId(0), 1.0)]);
        assert!(res.truncated);
        assert_eq!(res.iterations, 1);
        assert!(res.max_residual >= 1e-8);
    }

    #[test]
    fn test_ppr_detailed_empty_graph_and_seeds() {
        let solver = PprSolver::default();
        let graph = MultiplexGraph::build(vec![], &[], LayerWeights::default());

        let res = solver.compute_detailed(&graph, &[]);
        assert_eq!(res, PprResult::default());
    }
}
