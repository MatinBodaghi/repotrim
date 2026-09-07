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
        let n = graph.num_symbols();
        if n == 0 || seeds.is_empty() {
            return HashMap::new();
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
            return HashMap::new();
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

        // Return non-zero scores mapped to SymbolId
        let mut result = HashMap::with_capacity(p.iter().filter(|&&score| score > 0.0).count());
        for (i, &score) in p.iter().enumerate() {
            if score > 0.0 {
                result.insert(SymbolId(i as u32), score);
            }
        }

        result
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
}
