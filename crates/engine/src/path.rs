//! Task-Relevant Causal Path Inference & Multi-Hop Traversal Engine.
//!
//! # Theoretical Formulation (Dijkstra, 1959; Yen, 1971; De Domenico et al., 2013)
//! In code context synthesis under token budgets, isolated code symbols fail to capture
//! end-to-end execution flows and cause LLM hallucination. This module discovers continuous
//! multi-hop causal dependency paths connecting seed entrypoints (such as modified diff hunks
//! or query targets) to candidate definitions across the typed `MultiplexCsrGraph`.
//!
//! # Academic Citations
//! - Dijkstra, E. W. (1959). "A note on two problems in connexion with graphs".
//!   *Numerische Mathematik*, 1(1), 269–271.
//! - Yen, J. Y. (1971). "Finding the K Shortest Loopless Paths in a Network".
//!   *Management Science*, 17(11), 712–716.
//! - De Domenico, M., Solé-Ribalta, A., Cozzo, E., Kivelä, M., Moreno, Y., Porter, M. A.,
//!   Gómez, S., & Arenas, A. (2013). "Mathematical Formulation of Multilayer Networks".
//!   *Physical Review X*, 3(4), 041022.

use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap, HashSet};

use crate::multiplex::{MultiplexCsrGraph, RelationWeights};
use crate::symbol::{RelationType, SymbolId, SymbolNode};
use crate::task::TaskContext;

/// An ordered sequence of symbols and typed relations representing an execution or dependency path.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExecutionPath {
    /// Sequence of symbols along the path from source to target.
    pub nodes: Vec<SymbolId>,
    /// Directed relation types connecting consecutive symbol pairs $(v_i \to v_{i+1})$.
    pub relations: Vec<RelationType>,
    /// Edge weights along the path.
    pub edge_weights: Vec<f32>,
    /// Evaluated path energy score $\mathrm{Score}(p \mid q)$.
    pub score: f32,
    /// Normalized Boltzmann probability $P(p \mid q, G) \in [0, 1]$.
    pub probability: f32,
}

impl ExecutionPath {
    /// Constructs a new `ExecutionPath`.
    pub fn new(
        nodes: Vec<SymbolId>,
        relations: Vec<RelationType>,
        edge_weights: Vec<f32>,
    ) -> Self {
        assert_eq!(
            nodes.len().saturating_sub(1),
            relations.len(),
            "Relation count must match node transitions"
        );
        assert_eq!(
            relations.len(),
            edge_weights.len(),
            "Edge weight count must match relation count"
        );
        Self {
            nodes,
            relations,
            edge_weights,
            score: 0.0,
            probability: 0.0,
        }
    }

    /// Returns the number of symbols along this path.
    #[inline]
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// Returns the number of transitions/hops along this path ($|V| - 1$).
    #[inline]
    pub fn num_hops(&self) -> usize {
        self.relations.len()
    }

    /// Returns true if the path contains zero symbols.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Returns the source (first) symbol of this path, if any.
    #[inline]
    pub fn source(&self) -> Option<SymbolId> {
        self.nodes.first().copied()
    }

    /// Returns the target (last) symbol of this path, if any.
    #[inline]
    pub fn target(&self) -> Option<SymbolId> {
        self.nodes.last().copied()
    }

    /// Returns true if the path visits symbol `sym`.
    #[inline]
    pub fn contains(&self, sym: SymbolId) -> bool {
        self.nodes.contains(&sym)
    }

    /// Returns an iterator over directed edge transitions `(src, dst, relation, weight)`.
    pub fn edges(&self) -> impl Iterator<Item = (SymbolId, SymbolId, RelationType, f32)> + '_ {
        self.nodes
            .windows(2)
            .zip(self.relations.iter().zip(self.edge_weights.iter()))
            .map(|(w, (&rel, &weight))| (w[0], w[1], rel, weight))
    }

    /// Formats a human-readable diagnostic trace string for this path.
    pub fn format_trace(&self, symbols: &[SymbolNode]) -> String {
        if self.nodes.is_empty() {
            return String::new();
        }
        let mut out = String::new();
        for (i, &node) in self.nodes.iter().enumerate() {
            let name = symbols
                .get(node.0 as usize)
                .map(|s| s.name.as_str())
                .unwrap_or("unknown");
            if i > 0 {
                let rel = self.relations[i - 1];
                out.push_str(&format!(" --[{:?}]--> ", rel));
            }
            out.push_str(name);
        }
        out
    }
}

/// Configuration parameters for constrained multi-hop path search over `MultiplexCsrGraph`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PathFinderConfig {
    /// Maximum graph traversal radius (number of hops) allowed for any path.
    pub max_depth: usize,
    /// Maximum number of paths discovered per target symbol.
    pub max_paths_per_target: usize,
    /// Maximum total paths discovered across all targets.
    pub max_total_paths: usize,
    /// Allowed relation types for edge traversal. If empty, all relations are traversed.
    pub allowed_relations: Vec<RelationType>,
    /// Minimum non-zero edge weight threshold to traverse an edge.
    pub min_edge_weight: f32,
}

impl Default for PathFinderConfig {
    fn default() -> Self {
        Self {
            max_depth: 4,
            max_paths_per_target: 3,
            max_total_paths: 50,
            allowed_relations: vec![
                RelationType::Calls,
                RelationType::References,
                RelationType::Inherits,
                RelationType::Implements,
                RelationType::Contains,
                RelationType::BelongsTo,
                RelationType::IsTestedBy,
                RelationType::Reads,
                RelationType::Writes,
                RelationType::CoChangesWith,
            ],
            min_edge_weight: 0.05,
        }
    }
}

/// Priority queue search item for best-first branch traversal.
#[derive(Clone)]
struct SearchItem {
    current: SymbolId,
    nodes: Vec<SymbolId>,
    relations: Vec<RelationType>,
    edge_weights: Vec<f32>,
    cumulative_score: f32,
}

impl PartialEq for SearchItem {
    fn eq(&self, other: &Self) -> bool {
        self.cumulative_score == other.cumulative_score && self.nodes == other.nodes
    }
}

impl Eq for SearchItem {}

impl PartialOrd for SearchItem {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for SearchItem {
    fn cmp(&self, other: &Self) -> Ordering {
        self.cumulative_score
            .partial_cmp(&other.cumulative_score)
            .unwrap_or(Ordering::Equal)
    }
}

/// Multi-hop path search engine operating over `MultiplexCsrGraph`.
///
/// Implements constrained loopless path extraction connecting source entrypoints to candidate targets.
#[derive(Debug, Clone)]
pub struct PathFinder {
    config: PathFinderConfig,
}

impl Default for PathFinder {
    fn default() -> Self {
        Self::new(PathFinderConfig::default())
    }
}

impl PathFinder {
    /// Creates a new `PathFinder` with the specified configuration.
    pub fn new(config: PathFinderConfig) -> Self {
        Self { config }
    }

    /// Returns a reference to the active path search configuration.
    pub fn config(&self) -> &PathFinderConfig {
        &self.config
    }

    /// Discovers top-$k$ loopless paths connecting `source` to `target`.
    pub fn find_paths_between(
        &self,
        graph: &MultiplexCsrGraph,
        source: SymbolId,
        target: SymbolId,
    ) -> Vec<ExecutionPath> {
        if source == target || graph.is_empty() {
            return Vec::new();
        }
        self.find_paths(graph, &[source], &[target])
    }

    /// Discovers top paths connecting any seed `sources` to any candidate `targets`.
    pub fn find_paths(
        &self,
        graph: &MultiplexCsrGraph,
        sources: &[SymbolId],
        targets: &[SymbolId],
    ) -> Vec<ExecutionPath> {
        if graph.is_empty() || sources.is_empty() || targets.is_empty() {
            return Vec::new();
        }

        let target_set: HashSet<SymbolId> = targets.iter().copied().collect();
        let active_relations = if self.config.allowed_relations.is_empty() {
            RelationType::ALL.to_vec()
        } else {
            self.config.allowed_relations.clone()
        };

        let mut queue = BinaryHeap::new();
        for &src in sources {
            if (src.0 as usize) < graph.num_symbols() {
                queue.push(SearchItem {
                    current: src,
                    nodes: vec![src],
                    relations: Vec::new(),
                    edge_weights: Vec::new(),
                    cumulative_score: 0.0,
                });
            }
        }

        let mut paths_per_target: HashMap<SymbolId, usize> = HashMap::new();
        let mut results = Vec::new();

        while let Some(item) = queue.pop() {
            if results.len() >= self.config.max_total_paths {
                break;
            }

            // Check if current item reached a non-trivial target
            if item.nodes.len() > 1 && target_set.contains(&item.current) {
                let count = paths_per_target.entry(item.current).or_insert(0);
                if *count < self.config.max_paths_per_target {
                    *count += 1;
                    results.push(ExecutionPath::new(
                        item.nodes.clone(),
                        item.relations.clone(),
                        item.edge_weights.clone(),
                    ));
                }
            }

            // Continue expanding if within max_depth limit
            if item.nodes.len() > self.config.max_depth {
                continue;
            }

            for &rel in &active_relations {
                let neighbors = graph.neighbors_for(item.current, rel);
                let weights = graph.weights_for(item.current, rel);

                for (&next_idx, &weight) in neighbors.iter().zip(weights.iter()) {
                    let next_sym = SymbolId(next_idx);
                    if weight < self.config.min_edge_weight {
                        continue;
                    }

                    // Prevent cycles in simple paths
                    if item.nodes.contains(&next_sym) {
                        continue;
                    }

                    let mut next_nodes = item.nodes.clone();
                    next_nodes.push(next_sym);

                    let mut next_rels = item.relations.clone();
                    next_rels.push(rel);

                    let mut next_weights = item.edge_weights.clone();
                    next_weights.push(weight);

                    // Length-penalized edge gain
                    let step_score = weight - 0.10;
                    let next_score = item.cumulative_score + step_score;

                    queue.push(SearchItem {
                        current: next_sym,
                        nodes: next_nodes,
                        relations: next_rels,
                        edge_weights: next_weights,
                        cumulative_score: next_score,
                    });
                }
            }
        }

        results
    }

    /// Explores forward paths originating from `sources` up to `max_depth` hops.
    pub fn find_candidate_paths(
        &self,
        graph: &MultiplexCsrGraph,
        sources: &[SymbolId],
        max_depth: Option<usize>,
    ) -> Vec<ExecutionPath> {
        if graph.is_empty() || sources.is_empty() {
            return Vec::new();
        }

        let depth_limit = max_depth.unwrap_or(self.config.max_depth);
        let active_relations = if self.config.allowed_relations.is_empty() {
            RelationType::ALL.to_vec()
        } else {
            self.config.allowed_relations.clone()
        };

        let mut queue = BinaryHeap::new();
        for &src in sources {
            if (src.0 as usize) < graph.num_symbols() {
                queue.push(SearchItem {
                    current: src,
                    nodes: vec![src],
                    relations: Vec::new(),
                    edge_weights: Vec::new(),
                    cumulative_score: 0.0,
                });
            }
        }

        let mut results = Vec::new();
        let mut visited_paths: HashSet<Vec<SymbolId>> = HashSet::new();

        while let Some(item) = queue.pop() {
            if results.len() >= self.config.max_total_paths {
                break;
            }

            if item.nodes.len() > 1 && visited_paths.insert(item.nodes.clone()) {
                results.push(ExecutionPath::new(
                    item.nodes.clone(),
                    item.relations.clone(),
                    item.edge_weights.clone(),
                ));
            }

            if item.nodes.len() > depth_limit {
                continue;
            }

            for &rel in &active_relations {
                let neighbors = graph.neighbors_for(item.current, rel);
                let weights = graph.weights_for(item.current, rel);

                for (&next_idx, &weight) in neighbors.iter().zip(weights.iter()) {
                    let next_sym = SymbolId(next_idx);
                    if weight < self.config.min_edge_weight {
                        continue;
                    }

                    if item.nodes.contains(&next_sym) {
                        continue;
                    }

                    let mut next_nodes = item.nodes.clone();
                    next_nodes.push(next_sym);

                    let mut next_rels = item.relations.clone();
                    next_rels.push(rel);

                    let mut next_weights = item.edge_weights.clone();
                    next_weights.push(weight);

                    let step_score = weight - 0.10;
                    let next_score = item.cumulative_score + step_score;

                    queue.push(SearchItem {
                        current: next_sym,
                        nodes: next_nodes,
                        relations: next_rels,
                        edge_weights: next_weights,
                        cumulative_score: next_score,
                    });
                }
            }
        }

        results
    }
}

/// Configuration for Boltzmann path energy evaluation and probabilistic distribution.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PathScorerConfig {
    /// Boltzmann thermodynamic temperature $T > 0$ controlling entropy vs exploitation.
    pub temperature: f32,
    /// Path length penalty coefficient $\kappa \ge 0$ penalizing overly long hop chains.
    pub length_penalty: f32,
}

impl Default for PathScorerConfig {
    fn default() -> Self {
        Self {
            temperature: 1.0,
            length_penalty: 0.25,
        }
    }
}

/// Evaluator computing energy scores and Boltzmann probability distributions over execution paths.
///
/// # Theoretical Formulation (Boltzmann, 1868; Ziebart et al., 2008; Kappen, 2005)
/// The path energy score combines additive node relevance, task-conditioned edge weights,
/// and a length penalty:
/// $$\mathrm{Score}(p \mid q) = \sum_{v \in p} r(v \mid q) + \sum_{e \in p} \omega_{\tau(e)}(q) \cdot w_e - \kappa \cdot \mathrm{Length}(p)$$
///
/// The probability distribution over candidate paths $\mathcal{P}_q$ follows the Boltzmann/Gibbs law:
/// $$P(p \mid q, G) = \frac{\exp((\mathrm{Score}(p \mid q) - M) / T)}{\sum_{p' \in \mathcal{P}_q} \exp((\mathrm{Score}(p' \mid q) - M) / T)}$$
/// where $M = \max_{p'} \mathrm{Score}(p')$ enforces numerical stability.
#[derive(Debug, Clone)]
pub struct PathScorer {
    config: PathScorerConfig,
    relation_weights: RelationWeights,
    relevance_scores: Vec<f32>,
}

impl Default for PathScorer {
    fn default() -> Self {
        Self::new(PathScorerConfig::default())
    }
}

impl PathScorer {
    /// Creates a new `PathScorer` with default uniform relation weights and zero relevance scores.
    pub fn new(config: PathScorerConfig) -> Self {
        Self {
            config,
            relation_weights: RelationWeights::default(),
            relevance_scores: Vec::new(),
        }
    }

    /// Creates a `PathScorer` conditioned on the operational intent of a `TaskContext`.
    pub fn with_task(config: PathScorerConfig, task: &TaskContext) -> Self {
        Self {
            config,
            relation_weights: RelationWeights::from_task(task),
            relevance_scores: Vec::new(),
        }
    }

    /// Sets the task-conditioned relation weights $\boldsymbol{\omega}(q)$.
    pub fn with_relation_weights(mut self, weights: RelationWeights) -> Self {
        self.relation_weights = weights;
        self
    }

    /// Sets the symbol relevance vector $r(v \mid q)$ indexed by `SymbolId`.
    pub fn with_relevance_scores(mut self, scores: Vec<f32>) -> Self {
        self.relevance_scores = scores;
        self
    }

    /// Returns a reference to the active scorer configuration.
    pub fn config(&self) -> &PathScorerConfig {
        &self.config
    }

    /// Evaluates the unnormalized path energy score $\mathrm{Score}(p \mid q)$.
    pub fn score_path(&self, path: &ExecutionPath) -> f32 {
        if path.is_empty() {
            return 0.0;
        }

        let node_relevance: f32 = path
            .nodes
            .iter()
            .map(|&sym| {
                let idx = sym.0 as usize;
                self.relevance_scores.get(idx).copied().unwrap_or(0.0)
            })
            .sum();

        let edge_score: f32 = path
            .relations
            .iter()
            .zip(path.edge_weights.iter())
            .map(|(&rel, &weight)| {
                let rel_w = self.relation_weights.weight_for(rel);
                rel_w * weight
            })
            .sum();

        let hops = path.num_hops() as f32;
        let length_pen = self.config.length_penalty * hops;

        node_relevance + edge_score - length_pen
    }

    /// Computes path scores and normalized Boltzmann probabilities $P(p \mid q)$ in-place.
    pub fn score_and_normalize(&self, paths: &mut [ExecutionPath]) {
        if paths.is_empty() {
            return;
        }

        for path in paths.iter_mut() {
            path.score = self.score_path(path);
        }

        let temp = self.config.temperature.max(1e-5);
        let max_score = paths
            .iter()
            .map(|p| p.score)
            .fold(f32::NEG_INFINITY, f32::max);

        let mut exp_weights = Vec::with_capacity(paths.len());
        let mut sum_exp = 0.0_f32;

        for path in paths.iter() {
            let unnorm = ((path.score - max_score) / temp).exp();
            exp_weights.push(unnorm);
            sum_exp += unnorm;
        }

        if sum_exp > 0.0 && !sum_exp.is_nan() {
            for (path, &unnorm) in paths.iter_mut().zip(exp_weights.iter()) {
                path.probability = unnorm / sum_exp;
            }
        } else {
            let uniform = 1.0 / (paths.len() as f32);
            for path in paths.iter_mut() {
                path.probability = uniform;
            }
        }
    }

    /// Scores all candidate paths, computes normalized Boltzmann probabilities, and returns
    /// them sorted in descending order of probability.
    pub fn score_paths(&self, mut paths: Vec<ExecutionPath>) -> Vec<ExecutionPath> {
        self.score_and_normalize(&mut paths);
        paths.sort_by(|a, b| {
            b.probability
                .partial_cmp(&a.probability)
                .unwrap_or(Ordering::Equal)
        });
        paths
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::symbol::{SymbolKind, TextSpan};
    use std::path::PathBuf;

    fn make_test_symbol(id: u32, name: &str) -> SymbolNode {
        SymbolNode {
            id: SymbolId(id),
            name: name.to_string(),
            kind: SymbolKind::Function,
            file_path: PathBuf::from("src/lib.rs"),
            span: TextSpan::new(0, 50, 1, 3),
            signature: format!("pub fn {}()", name),
            docstring: None,
            token_cost: 30,
            ast_hash: [0u8; 32],
            container_name: None,
            trait_name: None,
        }
    }

    #[test]
    fn test_execution_path_basics() {
        let p = ExecutionPath::new(
            vec![SymbolId(0), SymbolId(1), SymbolId(2)],
            vec![RelationType::Calls, RelationType::References],
            vec![0.9, 0.8],
        );

        assert_eq!(p.len(), 3);
        assert_eq!(p.num_hops(), 2);
        assert_eq!(p.source(), Some(SymbolId(0)));
        assert_eq!(p.target(), Some(SymbolId(2)));
        assert!(p.contains(SymbolId(1)));
        assert!(!p.contains(SymbolId(3)));

        let syms = vec![
            make_test_symbol(0, "entrypoint"),
            make_test_symbol(1, "service"),
            make_test_symbol(2, "db"),
        ];
        let trace = p.format_trace(&syms);
        assert_eq!(trace, "entrypoint --[Calls]--> service --[References]--> db");
    }

    #[test]
    fn test_path_finder_multi_hop_traversal() {
        let syms = vec![
            make_test_symbol(0, "entry"),
            make_test_symbol(1, "router"),
            make_test_symbol(2, "handler"),
            make_test_symbol(3, "db"),
        ];

        // 0 -> 1 -> 2 -> 3 via Calls
        let edges = vec![
            (0, 1, RelationType::Calls, 0.95),
            (1, 2, RelationType::Calls, 0.90),
            (2, 3, RelationType::Calls, 0.85),
            // Direct reference 0 -> 3 with lower weight
            (0, 3, RelationType::References, 0.40),
        ];

        let graph = MultiplexCsrGraph::from_typed_edges(syms, &edges);
        let finder = PathFinder::default();

        let paths = finder.find_paths_between(&graph, SymbolId(0), SymbolId(3));
        assert!(!paths.is_empty());

        // Should find the direct path and the multi-hop path
        let has_multi_hop = paths
            .iter()
            .any(|p| p.nodes == vec![SymbolId(0), SymbolId(1), SymbolId(2), SymbolId(3)]);
        let has_direct = paths
            .iter()
            .any(|p| p.nodes == vec![SymbolId(0), SymbolId(3)]);

        assert!(has_multi_hop, "Should discover multi-hop causal chain");
        assert!(has_direct, "Should discover direct reference path");
    }

    #[test]
    fn test_path_finder_prevents_cycles() {
        let syms = vec![
            make_test_symbol(0, "a"),
            make_test_symbol(1, "b"),
            make_test_symbol(2, "c"),
        ];

        // Cycle 0 -> 1 -> 2 -> 0
        let edges = vec![
            (0, 1, RelationType::Calls, 0.9),
            (1, 2, RelationType::Calls, 0.9),
            (2, 0, RelationType::Calls, 0.9),
        ];

        let graph = MultiplexCsrGraph::from_typed_edges(syms, &edges);
        let finder = PathFinder::new(PathFinderConfig {
            max_depth: 5,
            ..Default::default()
        });

        // Path search should terminate safely without infinite looping
        let paths = finder.find_paths_between(&graph, SymbolId(0), SymbolId(2));
        assert_eq!(paths.len(), 1);
        assert_eq!(paths[0].nodes, vec![SymbolId(0), SymbolId(1), SymbolId(2)]);
    }

    #[test]
    fn test_path_scorer_boltzmann_normalization() {
        let mut paths = vec![
            ExecutionPath::new(
                vec![SymbolId(0), SymbolId(1)],
                vec![RelationType::Calls],
                vec![0.9],
            ),
            ExecutionPath::new(
                vec![SymbolId(0), SymbolId(2), SymbolId(3)],
                vec![RelationType::References, RelationType::Calls],
                vec![0.7, 0.8],
            ),
            ExecutionPath::new(
                vec![SymbolId(0), SymbolId(4)],
                vec![RelationType::Imports],
                vec![0.4],
            ),
        ];

        let scorer = PathScorer::new(PathScorerConfig {
            temperature: 1.0,
            length_penalty: 0.25,
        })
        .with_relevance_scores(vec![0.5, 0.8, 0.4, 0.3, 0.1]);

        scorer.score_and_normalize(&mut paths);

        let sum_prob: f32 = paths.iter().map(|p| p.probability).sum();
        assert!(
            (sum_prob - 1.0).abs() < 1e-5,
            "Boltzmann probabilities must sum to 1.0, got {}",
            sum_prob
        );

        // Path 0 has high relevance and strong edge, should have highest probability
        assert!(paths[0].probability > paths[1].probability);
        assert!(paths[1].probability > paths[2].probability);
    }

    #[test]
    fn test_path_scorer_temperature_scaling() {
        let paths_template = vec![
            ExecutionPath::new(
                vec![SymbolId(0), SymbolId(1)],
                vec![RelationType::Calls],
                vec![0.9],
            ),
            ExecutionPath::new(
                vec![SymbolId(0), SymbolId(2)],
                vec![RelationType::Calls],
                vec![0.5],
            ),
        ];

        // Low temperature (exploitation / sharp argmax)
        let cold_scorer = PathScorer::new(PathScorerConfig {
            temperature: 0.05,
            length_penalty: 0.0,
        });
        let mut cold_paths = paths_template.clone();
        cold_scorer.score_and_normalize(&mut cold_paths);
        assert!(
            cold_paths[0].probability > 0.99,
            "Low temperature should concentrate probability on max score"
        );

        // High temperature (exploration / uniform)
        let hot_scorer = PathScorer::new(PathScorerConfig {
            temperature: 100.0,
            length_penalty: 0.0,
        });
        let mut hot_paths = paths_template;
        hot_scorer.score_and_normalize(&mut hot_paths);
        let diff = (hot_paths[0].probability - hot_paths[1].probability).abs();
        assert!(
            diff < 0.05,
            "High temperature should approach uniform distribution"
        );
    }
}
