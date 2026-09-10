use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap, HashSet};
use std::path::PathBuf;

use crate::graph::MultiplexGraph;
use crate::symbol::SymbolId;

/// Configuration parameters for the CELF Submodular Knapsack optimizer.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CelfConfig {
    /// Anti-clustering diversity weight ($\lambda$). Default: `0.4`.
    pub lambda_diversity: f32,
    /// Neighborhood coverage weight ($\mu$). Default: `0.2`.
    pub neighbor_coverage_weight: f32,
    /// Minimum PPR relevance score required to be considered for knapsack selection. Default: `1e-5`.
    pub min_relevance_threshold: f32,
}

impl Default for CelfConfig {
    fn default() -> Self {
        Self {
            lambda_diversity: 0.4,
            neighbor_coverage_weight: 0.2,
            min_relevance_threshold: 1e-5,
        }
    }
}

/// Element stored in the CELF max-priority queue.
#[derive(Debug, Clone)]
struct CelfItem {
    symbol_id: SymbolId,
    marginal_gain_per_token: f32,
    last_iteration: usize,
}

impl PartialEq for CelfItem {
    fn eq(&self, other: &Self) -> bool {
        self.marginal_gain_per_token == other.marginal_gain_per_token
    }
}

impl Eq for CelfItem {}

impl PartialOrd for CelfItem {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for CelfItem {
    fn cmp(&self, other: &Self) -> Ordering {
        self.marginal_gain_per_token
            .partial_cmp(&other.marginal_gain_per_token)
            .unwrap_or(Ordering::Equal)
    }
}

/// Diagnostic trace step recorded during CELF submodular knapsack optimization.
#[derive(Debug, Clone, PartialEq)]
pub struct CelfTraceStep {
    /// Identifier of the accepted symbol.
    pub symbol_id: SymbolId,
    /// Token cost of the symbol.
    pub symbol_cost: usize,
    /// Cumulative token cost after including this symbol.
    pub cumulative_tokens: usize,
    /// Marginal utility gain provided by this symbol.
    pub marginal_gain: f32,
    /// Cumulative utility score after including this symbol.
    pub cumulative_utility: f32,
}

/// Cost-Effective Lazy Forward (CELF) submodular knapsack optimizer.
///
/// Selects an optimal subset of code symbols $S \subseteq V$ maximizing information coverage
/// and module diversity subject to a strict token budget $\sum_{v \in S} c(v) \le B$.
///
/// Exploits submodular diminishing returns to achieve $(1 - 1/e)$ approximation of the NP-hard
/// knapsack problem in $O(|V| \log |V|)$ time.
#[derive(Debug, Clone, Default)]
pub struct CelfOptimizer {
    config: CelfConfig,
}

impl CelfOptimizer {
    /// Creates a new `CelfOptimizer` with custom configuration.
    pub fn new(config: CelfConfig) -> Self {
        Self { config }
    }

    /// Selects the optimal subset of symbols that fit within the token `budget`.
    ///
    /// Combines PPR relevance scores with an anti-clustering file diversity penalty.
    ///
    /// Returns the ordered list of selected `SymbolId`s.
    pub fn optimize(
        &self,
        graph: &MultiplexGraph,
        ppr_scores: &HashMap<SymbolId, f32>,
        budget: usize,
    ) -> Vec<SymbolId> {
        self.optimize_with_trace(graph, ppr_scores, budget).0
    }

    /// Selects the optimal subset of symbols and records the cumulative utility trajectory.
    ///
    /// Returns a tuple containing the ordered list of selected `SymbolId`s and
    /// the discrete `CelfTraceStep` trajectory used for knee-point detection.
    pub fn optimize_with_trace(
        &self,
        graph: &MultiplexGraph,
        ppr_scores: &HashMap<SymbolId, f32>,
        budget: usize,
    ) -> (Vec<SymbolId>, Vec<CelfTraceStep>) {
        if budget == 0 || graph.is_empty() {
            return (Vec::new(), Vec::new());
        }

        let mut selected_set: HashSet<SymbolId> = HashSet::new();
        let mut selected_list: Vec<SymbolId> = Vec::new();
        let mut trace: Vec<CelfTraceStep> = Vec::new();
        let mut covered_nodes: HashSet<SymbolId> = HashSet::new();
        let mut file_token_costs: HashMap<PathBuf, usize> = HashMap::new();
        let mut current_tokens: usize = 0;
        let mut cumulative_utility: f32 = 0.0;
        let mut current_iteration: usize = 0;

        let mut heap: BinaryHeap<CelfItem> = BinaryHeap::new();

        // Initialize candidate pool from graph symbols
        for sym in graph.symbols() {
            let score = ppr_scores.get(&sym.id).copied().unwrap_or(0.0);
            if score < self.config.min_relevance_threshold && !ppr_scores.is_empty() {
                continue;
            }

            let cost = sym.token_cost.max(1);
            if cost > budget {
                continue;
            }

            let initial_marginal_gain = self.compute_marginal_gain(
                sym.id,
                graph,
                ppr_scores,
                &covered_nodes,
                &file_token_costs,
            );

            heap.push(CelfItem {
                symbol_id: sym.id,
                marginal_gain_per_token: initial_marginal_gain / (cost as f32),
                last_iteration: 0,
            });
        }

        // CELF lazy evaluation loop
        while let Some(mut top) = heap.pop() {
            let sym = match graph.symbol(top.symbol_id) {
                Some(s) => s,
                None => continue,
            };

            let cost = sym.token_cost.max(1);

            // Skip if it doesn't fit in remaining budget
            if current_tokens + cost > budget {
                continue;
            }

            // If this element was evaluated in the current iteration, it is guaranteed
            // by submodularity to have the maximum marginal gain per token.
            if top.last_iteration == current_iteration {
                let marginal_gain = (top.marginal_gain_per_token * (cost as f32)).max(0.0);
                cumulative_utility += marginal_gain;

                selected_set.insert(top.symbol_id);
                selected_list.push(top.symbol_id);
                current_tokens += cost;

                trace.push(CelfTraceStep {
                    symbol_id: top.symbol_id,
                    symbol_cost: cost,
                    cumulative_tokens: current_tokens,
                    marginal_gain,
                    cumulative_utility,
                });

                // Update covered nodes (self + immediate neighbors)
                covered_nodes.insert(top.symbol_id);
                for &neighbor in graph.neighbors(top.symbol_id) {
                    covered_nodes.insert(SymbolId(neighbor));
                }

                // Update file token density
                *file_token_costs.entry(sym.file_path.clone()).or_default() += cost;

                current_iteration += 1;

                if current_tokens >= budget {
                    break;
                }
            } else {
                // Lazily recompute marginal gain with updated state
                let new_gain = self.compute_marginal_gain(
                    top.symbol_id,
                    graph,
                    ppr_scores,
                    &covered_nodes,
                    &file_token_costs,
                );

                if new_gain > 0.0 {
                    top.marginal_gain_per_token = new_gain / (cost as f32);
                    top.last_iteration = current_iteration;
                    heap.push(top);
                }
            }
        }

        (selected_list, trace)
    }

    /// Evaluates the marginal gain $\Delta(v \mid S)$ of adding symbol `id` to the current set $S$.
    fn compute_marginal_gain(
        &self,
        id: SymbolId,
        graph: &MultiplexGraph,
        ppr_scores: &HashMap<SymbolId, f32>,
        covered_nodes: &HashSet<SymbolId>,
        file_token_costs: &HashMap<PathBuf, usize>,
    ) -> f32 {
        let sym = match graph.symbol(id) {
            Some(s) => s,
            None => return 0.0,
        };

        // 1. Direct relevance coverage
        let direct_relevance = ppr_scores.get(&id).copied().unwrap_or(0.01);

        // 2. 1-hop neighbor coverage gain (diminishing returns if neighbors already covered)
        let mut neighbor_gain = 0.0_f32;
        let neighbors = graph.neighbors(id);
        for &n in neighbors {
            let neighbor_id = SymbolId(n);
            if !covered_nodes.contains(&neighbor_id) {
                let n_score = ppr_scores.get(&neighbor_id).copied().unwrap_or(0.005);
                neighbor_gain += self.config.neighbor_coverage_weight * n_score;
            }
        }

        // 3. Module/File Diversity gain: log(1 + c_v / (1 + C_file))
        // Concavity penalizes adding more symbols from already saturated files
        let current_file_cost = file_token_costs.get(&sym.file_path).copied().unwrap_or(0) as f32;
        let sym_cost = sym.token_cost.max(1) as f32;

        let diversity_gain =
            ((1.0 + (sym_cost / (1.0 + current_file_cost))).ln()) * self.config.lambda_diversity;

        direct_relevance + neighbor_gain + diversity_gain
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::LayerWeights;
    use crate::symbol::{SymbolKind, SymbolNode, TextSpan};

    fn make_test_node(id: u32, name: &str, file: &str, cost: usize) -> SymbolNode {
        SymbolNode {
            id: SymbolId(id),
            name: name.to_string(),
            kind: SymbolKind::Function,
            file_path: PathBuf::from(file),
            span: TextSpan::new(0, 10, 0, 1),
            signature: format!("fn {}()", name),
            docstring: None,
            token_cost: cost,
            ast_hash: [0u8; 32],
            container_name: None,
            trait_name: None,
        }
    }

    #[test]
    fn test_celf_budget_adherence() {
        let s0 = make_test_node(0, "f0", "a.rs", 20);
        let s1 = make_test_node(1, "f1", "a.rs", 30);
        let s2 = make_test_node(2, "f2", "b.rs", 40);
        let s3 = make_test_node(3, "f3", "b.rs", 50);

        let graph = MultiplexGraph::build(vec![s0, s1, s2, s3], &[], LayerWeights::default());

        let mut ppr_scores = HashMap::new();
        ppr_scores.insert(SymbolId(0), 0.5);
        ppr_scores.insert(SymbolId(1), 0.3);
        ppr_scores.insert(SymbolId(2), 0.15);
        ppr_scores.insert(SymbolId(3), 0.05);

        let optimizer = CelfOptimizer::default();

        // Budget of 60 tokens: can fit f0 (20) + f1 (30) = 50 tokens, or f0 (20) + f2 (40) = 60 tokens
        let selected = optimizer.optimize(&graph, &ppr_scores, 60);

        let total_cost: usize = selected
            .iter()
            .map(|&id| graph.symbol(id).unwrap().token_cost)
            .sum();

        assert!(total_cost <= 60, "Cost exceeded budget: {}", total_cost);
        assert!(!selected.is_empty());
        // Highest relevance symbol f0 should definitely be included
        assert!(selected.contains(&SymbolId(0)));
    }

    #[test]
    fn test_celf_diversity_anti_clustering() {
        // File A has 5 small functions
        // File B has 1 function
        let s0 = make_test_node(0, "a0", "file_a.rs", 10);
        let s1 = make_test_node(1, "a1", "file_a.rs", 10);
        let s2 = make_test_node(2, "a2", "file_a.rs", 10);
        let s3 = make_test_node(3, "b0", "file_b.rs", 10);

        let graph = MultiplexGraph::build(vec![s0, s1, s2, s3], &[], LayerWeights::default());

        // PPR scores where file_a functions are slightly higher than file_b
        let mut ppr_scores = HashMap::new();
        ppr_scores.insert(SymbolId(0), 0.30);
        ppr_scores.insert(SymbolId(1), 0.28);
        ppr_scores.insert(SymbolId(2), 0.26);
        ppr_scores.insert(SymbolId(3), 0.25);

        // Budget allows 2 functions (20 tokens)
        // With diversity enabled, file_b should be prioritized over file_a's 2nd or 3rd function
        let optimizer = CelfOptimizer::new(CelfConfig {
            lambda_diversity: 1.0,
            neighbor_coverage_weight: 0.0,
            min_relevance_threshold: 0.0,
        });

        let selected = optimizer.optimize(&graph, &ppr_scores, 20);
        assert_eq!(selected.len(), 2);
        assert!(selected.contains(&SymbolId(0))); // Top score from file_a
        assert!(selected.contains(&SymbolId(3))); // Diverse pick from file_b
    }

    #[test]
    fn test_celf_empty_and_zero_budget() {
        let s0 = make_test_node(0, "f0", "a.rs", 20);
        let graph = MultiplexGraph::build(vec![s0], &[], LayerWeights::default());

        let optimizer = CelfOptimizer::default();
        let selected = optimizer.optimize(&graph, &HashMap::new(), 0);
        assert!(selected.is_empty());
    }

    #[test]
    fn test_celf_optimize_with_trace() {
        let s0 = make_test_node(0, "f0", "a.rs", 20);
        let s1 = make_test_node(1, "f1", "a.rs", 30);
        let s2 = make_test_node(2, "f2", "b.rs", 40);
        let graph = MultiplexGraph::build(vec![s0, s1, s2], &[], LayerWeights::default());

        let mut ppr_scores = HashMap::new();
        ppr_scores.insert(SymbolId(0), 0.5);
        ppr_scores.insert(SymbolId(1), 0.3);
        ppr_scores.insert(SymbolId(2), 0.15);

        let optimizer = CelfOptimizer::default();
        let (selected, trace) = optimizer.optimize_with_trace(&graph, &ppr_scores, 100);

        assert_eq!(selected.len(), trace.len());
        assert!(!trace.is_empty());

        let mut prev_tokens = 0;
        let mut prev_util = 0.0;

        for step in &trace {
            assert!(step.cumulative_tokens > prev_tokens);
            assert!(step.cumulative_tokens <= 100);
            assert!(step.cumulative_utility >= prev_util);
            prev_tokens = step.cumulative_tokens;
            prev_util = step.cumulative_utility;
        }
    }
}
