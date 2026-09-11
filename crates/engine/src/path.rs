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

use crate::multiplex::MultiplexCsrGraph;
use crate::symbol::{RelationType, SymbolId, SymbolNode};

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
}
