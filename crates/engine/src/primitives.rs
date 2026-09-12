//! Codebase Intelligence Primitives (Phase 38).
//!
//! # Theoretical Formulation & Architecture
//! Transitions RepoTrim from a single-shot context pruning engine into an active,
//! modular codebase intelligence service grounded in typed multiplex graph theory
//! (De Domenico et al., 2013), submodular knapsack selection (Nemhauser et al., 1978),
//! and Boltzmann energy-scored path inference (Ziebart et al., 2008).
//!
//! Provides six formal intelligence primitives:
//! 1. `locate(task, top_k)`: Task-conditioned entrypoint discovery combining BM25,
//!    trigram fuzzy similarity, diff hunks, and teleportation priors.
//! 2. `neighbors(symbol, max_hops)`: Weighted typed relation subgraphs across orthogonal
//!    relation layers (Calls, References, Inherits, Implements, Contains, TestedBy, CoEdits).
//! 3. `trace(source, target, task)`: Constrained multi-hop causal path discovery with
//!    Boltzmann probability scoring and Mermaid sequence/flowchart diagram generation.
//! 4. `expand(symbol, budget, task)`: Localized submodular CELF knapsack packing centered
//!    on a focal symbol.
//! 5. `impact(symbol, budget)`: Semantic change impact analysis, forward reachability blast radius,
//!    and regression test identification.
//! 6. `context(task, budget)`: End-to-end budgeted evidence synthesis returning
//!    `StructuredContext` with causal traces and omission diagnostics.

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::OnceLock;

use crate::context_object::StructuredContext;
use crate::error::EngineError;
use crate::formatter::ContextFormatter;
use crate::graph::MultiplexGraph;
use crate::impact::{ImpactAnalyzer, ImpactReport};
use crate::intent::IntentResolver;
use crate::multiplex::MultiplexCsrGraph;
use crate::path::{ExecutionPath, PathFinder, PathScorer, PathScorerConfig};
use crate::selector::ContextSelector;
use crate::symbol::{LodLevel, RelationType, SymbolId, SymbolNode};
use crate::task::TaskContext;

/// Directed edge orientation relative to a focal symbol.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EdgeDirection {
    /// Outgoing edge ($v \to u$, e.g., focal symbol invokes target).
    Outgoing,
    /// Incoming edge ($u \to v$, e.g., caller invokes focal symbol).
    Incoming,
}

impl EdgeDirection {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Outgoing => "OUTGOING",
            Self::Incoming => "INCOMING",
        }
    }
}

/// A directed, typed connection between a focal symbol and an adjacent neighbor.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NeighborEdge {
    /// The adjacent symbol node.
    pub target: SymbolNode,
    /// The semantic relation type connecting the symbols.
    pub relation: RelationType,
    /// Scalar connection weight / transition probability.
    pub weight: f32,
    /// Directed edge orientation relative to the focal symbol.
    pub direction: EdgeDirection,
    /// Graph geodesic hop distance from the focal symbol ($k \ge 1$).
    pub hop: usize,
}

/// The structured multi-layer neighborhood surrounding a focal symbol.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SymbolNeighborhood {
    /// The central symbol node under inspection.
    pub focal_symbol: SymbolNode,
    /// Directed edges pointing to callee / referenced symbols.
    pub outgoing: Vec<NeighborEdge>,
    /// Directed edges pointing from caller / dependent symbols.
    pub incoming: Vec<NeighborEdge>,
    /// Distribution of connections categorized by relation type.
    pub layer_counts: HashMap<RelationType, usize>,
    /// Total unique neighbor symbols discovered.
    pub total_neighbors: usize,
}

/// A ranked candidate entrypoint for a task with attribution justification.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RankedEntrypoint {
    /// Resolved symbol node in the codebase.
    pub symbol: SymbolNode,
    /// Quantitative relevance / confidence score in `(0.0, 1.0]`.
    pub score: f32,
    /// Qualitative heuristic rationale for why this symbol was selected.
    pub reason: String,
}

/// The outcome of causal path discovery connecting source and target symbols.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CausalTraceResult {
    /// Source entrypoint symbol.
    pub source: SymbolNode,
    /// Target destination symbol.
    pub target: SymbolNode,
    /// All discovered paths sorted by Boltzmann probability descending.
    pub paths: Vec<ExecutionPath>,
    /// The single most probable / lowest energy causal path.
    pub best_path: Option<ExecutionPath>,
    /// Rendered Mermaid sequence diagram illustrating the causal flow.
    pub mermaid_diagram: String,
}

/// A localized submodular context cluster grown around a focal symbol.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LocalExpansion {
    /// The focal symbol at the center of the expansion.
    pub focal_symbol: SymbolNode,
    /// Target token budget limit allocated for the expansion.
    pub budget: usize,
    /// Total tokens consumed by the expanded symbols.
    pub tokens_used: usize,
    /// Clustered symbols selected into the localized context.
    pub symbols: Vec<SymbolNode>,
    /// Causal execution paths connecting the focal symbol to members of the expansion.
    pub paths: Vec<ExecutionPath>,
    /// Contextual markdown formatted code blocks for the expansion.
    pub formatted_code: String,
}

/// Core Codebase Intelligence Service.
///
/// Unifies retrieval, structural graph traversals, causal path evaluation,
/// and submodular knapsack optimization over an active repository.
pub struct CodebaseIntelligence<'a> {
    graph: &'a MultiplexGraph,
    file_sources: &'a HashMap<PathBuf, String>,
    selector: ContextSelector,
    path_finder: PathFinder,
    multiplex_csr: OnceLock<MultiplexCsrGraph>,
    transposed_csr: OnceLock<MultiplexCsrGraph>,
}

impl<'a> CodebaseIntelligence<'a> {
    /// Constructs a new `CodebaseIntelligence` service over a `MultiplexGraph` and file sources.
    pub fn new(graph: &'a MultiplexGraph, file_sources: &'a HashMap<PathBuf, String>) -> Self {
        Self {
            graph,
            file_sources,
            selector: ContextSelector::default(),
            path_finder: PathFinder::default(),
            multiplex_csr: OnceLock::new(),
            transposed_csr: OnceLock::new(),
        }
    }

    /// Customizes the underlying `ContextSelector` configuration.
    pub fn with_selector(mut self, selector: ContextSelector) -> Self {
        self.selector = selector;
        self
    }

    /// Customizes the underlying `PathFinder` configuration.
    pub fn with_path_finder(mut self, path_finder: PathFinder) -> Self {
        self.path_finder = path_finder;
        self
    }

    /// Returns a reference to the active `MultiplexGraph`.
    #[inline]
    pub fn graph(&self) -> &MultiplexGraph {
        self.graph
    }

    /// Returns a reference to the repository file sources.
    #[inline]
    pub fn file_sources(&self) -> &HashMap<PathBuf, String> {
        self.file_sources
    }

    fn get_multiplex_csr(&self) -> &MultiplexCsrGraph {
        self.multiplex_csr
            .get_or_init(|| self.graph.to_multiplex_csr())
    }

    fn get_transposed_csr(&self) -> &MultiplexCsrGraph {
        self.transposed_csr
            .get_or_init(|| self.get_multiplex_csr().transpose())
    }

    // -------------------------------------------------------------------------
    // Primitive 1: locate
    // -------------------------------------------------------------------------

    /// Discovers top-$k$ ranked entrypoint symbols for a structured task context.
    ///
    /// Evaluates explicit seed hints, concept keywords, BM25 text match, and target file constraints.
    pub fn locate(&self, task: &TaskContext, top_k: usize) -> Vec<RankedEntrypoint> {
        if self.graph.is_empty() || top_k == 0 {
            return Vec::new();
        }

        let mut candidate_scores: HashMap<SymbolId, (f32, String)> = HashMap::new();
        let symbols = self.graph.symbols();

        // 1. Match explicit seed hints (highest priority: score 1.0)
        for hint in &task.seed_hints {
            let clean_hint = hint.trim();
            if clean_hint.is_empty() {
                continue;
            }

            for sym in symbols {
                if sym.name.eq_ignore_ascii_case(clean_hint) {
                    candidate_scores
                        .insert(sym.id, (1.0, format!("Exact seed match: '{}'", clean_hint)));
                } else if sym.name.to_lowercase().contains(&clean_hint.to_lowercase()) {
                    candidate_scores.entry(sym.id).or_insert_with(|| {
                        (0.85, format!("Sub-string seed match: '{}'", clean_hint))
                    });
                }
            }
        }

        // 2. Resolve query string via hybrid lexical & trigram retriever
        if !task.query.trim().is_empty() {
            let search_results = IntentResolver::resolve_query(symbols, &task.query, top_k * 3);

            for (sym_id, score) in search_results {
                let reason = format!("Lexical & concept affinity: {:.2}", score);
                let entry = candidate_scores
                    .entry(sym_id)
                    .or_insert((score, reason.clone()));
                if score > entry.0 {
                    *entry = (score, reason);
                }
            }
        }

        // 3. Resolve extracted concept keywords
        for concept in &task.concepts {
            let concept_clean = concept.trim();
            if concept_clean.is_empty() {
                continue;
            }

            let concept_hits = IntentResolver::resolve_query(symbols, concept_clean, 3);
            for (sym_id, raw_score) in concept_hits {
                let score = raw_score * 0.75;
                let reason = format!("Concept keyword match: '{}' ({:.2})", concept_clean, score);
                let entry = candidate_scores
                    .entry(sym_id)
                    .or_insert((score, reason.clone()));
                if score > entry.0 {
                    *entry = (score, reason);
                }
            }
        }

        // 4. Boost target files if constraints are present
        if !task.metadata.target_files.is_empty() {
            let target_set: HashSet<&PathBuf> = task.metadata.target_files.iter().collect();
            for (id, (score, reason)) in candidate_scores.iter_mut() {
                if let Some(sym) = self.graph.symbol(*id) {
                    if target_set
                        .iter()
                        .any(|p| sym.file_path.ends_with(p.as_path()))
                    {
                        *score = (*score * 1.30).min(1.0);
                        reason.push_str(" [Target file boost]");
                    }
                }
            }
        }

        // 5. Fallback: if no candidates discovered, backfill with top architectural hubs
        if candidate_scores.is_empty() {
            let mut top_degrees: Vec<(SymbolId, usize)> = symbols
                .iter()
                .map(|s| (s.id, self.graph.out_degree(s.id)))
                .collect();
            top_degrees.sort_by_key(|b| std::cmp::Reverse(b.1));

            for (sym_id, degree) in top_degrees.into_iter().take(top_k) {
                candidate_scores.insert(
                    sym_id,
                    (
                        0.30,
                        format!("Top architectural hub fallback (degree {})", degree),
                    ),
                );
            }
        }

        // 6. Sort descending by score and format output
        let mut sorted: Vec<(SymbolId, (f32, String))> = candidate_scores.into_iter().collect();
        sorted.sort_by(|a, b| {
            b.1 .0
                .partial_cmp(&a.1 .0)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        sorted
            .into_iter()
            .take(top_k)
            .filter_map(|(id, (score, reason))| {
                self.graph
                    .symbol(id)
                    .cloned()
                    .map(|symbol| RankedEntrypoint {
                        symbol,
                        score,
                        reason,
                    })
            })
            .collect()
    }

    /// Discovers top-$k$ ranked entrypoint symbols for a raw query string.
    pub fn locate_query(&self, query: &str, top_k: usize) -> Vec<RankedEntrypoint> {
        let task = TaskContext::from_query(query);
        self.locate(&task, top_k)
    }

    // -------------------------------------------------------------------------
    // Primitive 2: neighbors
    // -------------------------------------------------------------------------

    /// Extracts the multi-layer typed neighborhood surrounding a focal symbol (default 1-hop).
    pub fn neighbors(&self, symbol_id: SymbolId) -> Result<SymbolNeighborhood, EngineError> {
        self.neighbors_with_hops(symbol_id, 1)
    }

    /// Extracts the multi-layer typed neighborhood surrounding a focal symbol up to `max_hops`.
    pub fn neighbors_with_hops(
        &self,
        symbol_id: SymbolId,
        max_hops: usize,
    ) -> Result<SymbolNeighborhood, EngineError> {
        let focal_symbol = self
            .graph
            .symbol(symbol_id)
            .cloned()
            .ok_or(EngineError::SymbolNotFound(symbol_id.0))?;

        let mut outgoing = Vec::new();
        let mut incoming = Vec::new();
        let mut layer_counts: HashMap<RelationType, usize> = HashMap::new();
        let mut unique_neighbors: HashSet<SymbolId> = HashSet::new();

        let multiplex_csr = self.get_multiplex_csr();
        let transposed_csr = self.get_transposed_csr();

        // Level 1: Immediate outgoing edges
        for rel in RelationType::ALL {
            let (neighbors, weights) = multiplex_csr.row_slice_for(symbol_id, rel);
            for (&dst, &w) in neighbors.iter().zip(weights.iter()) {
                let target_id = SymbolId(dst);
                if target_id == symbol_id {
                    continue;
                }
                if let Some(target) = self.graph.symbol(target_id).cloned() {
                    unique_neighbors.insert(target_id);
                    *layer_counts.entry(rel).or_default() += 1;
                    outgoing.push(NeighborEdge {
                        target,
                        relation: rel,
                        weight: w,
                        direction: EdgeDirection::Outgoing,
                        hop: 1,
                    });
                }
            }
        }

        // Level 1: Immediate incoming edges
        for rel in RelationType::ALL {
            let (callers, weights) = transposed_csr.row_slice_for(symbol_id, rel);
            for (&caller_id_raw, &w) in callers.iter().zip(weights.iter()) {
                let caller_id = SymbolId(caller_id_raw);
                if caller_id == symbol_id {
                    continue;
                }
                if let Some(target) = self.graph.symbol(caller_id).cloned() {
                    unique_neighbors.insert(caller_id);
                    *layer_counts.entry(rel).or_default() += 1;
                    incoming.push(NeighborEdge {
                        target,
                        relation: rel,
                        weight: w,
                        direction: EdgeDirection::Incoming,
                        hop: 1,
                    });
                }
            }
        }

        // If max_hops > 1, expand to level 2 neighbors
        if max_hops > 1 {
            let level1_out: Vec<SymbolId> = outgoing.iter().map(|e| e.target.id).collect();
            for out_id in level1_out {
                for rel in RelationType::ALL {
                    let (neighbors, weights) = multiplex_csr.row_slice_for(out_id, rel);
                    for (&dst, &w) in neighbors.iter().zip(weights.iter()) {
                        let target_id = SymbolId(dst);
                        if target_id == symbol_id || unique_neighbors.contains(&target_id) {
                            continue;
                        }
                        if let Some(target) = self.graph.symbol(target_id).cloned() {
                            unique_neighbors.insert(target_id);
                            *layer_counts.entry(rel).or_default() += 1;
                            outgoing.push(NeighborEdge {
                                target,
                                relation: rel,
                                weight: w * 0.5,
                                direction: EdgeDirection::Outgoing,
                                hop: 2,
                            });
                        }
                    }
                }
            }
        }

        // Sort edges deterministically by weight descending, then symbol name
        outgoing.sort_by(|a, b| {
            b.weight
                .partial_cmp(&a.weight)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.target.name.cmp(&b.target.name))
        });
        incoming.sort_by(|a, b| {
            b.weight
                .partial_cmp(&a.weight)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.target.name.cmp(&b.target.name))
        });

        let total_neighbors = unique_neighbors.len();

        Ok(SymbolNeighborhood {
            focal_symbol,
            outgoing,
            incoming,
            layer_counts,
            total_neighbors,
        })
    }

    // -------------------------------------------------------------------------
    // Primitive 3: trace
    // -------------------------------------------------------------------------

    /// Resolves the most probable causal execution paths connecting `source` to `target`.
    pub fn trace(
        &self,
        source: SymbolId,
        target: SymbolId,
        task: Option<&TaskContext>,
    ) -> Result<CausalTraceResult, EngineError> {
        let src_sym = self
            .graph
            .symbol(source)
            .cloned()
            .ok_or(EngineError::SymbolNotFound(source.0))?;
        let tgt_sym = self
            .graph
            .symbol(target)
            .cloned()
            .ok_or(EngineError::SymbolNotFound(target.0))?;

        if source == target {
            let mut trivial_path = ExecutionPath::new(vec![source], Vec::new(), Vec::new());
            trivial_path.score = 1.0;
            trivial_path.probability = 1.0;
            let paths = vec![trivial_path];
            let mermaid_diagram =
                Self::generate_trace_mermaid(&src_sym, &tgt_sym, &paths, self.graph.symbols());
            return Ok(CausalTraceResult {
                source: src_sym,
                target: tgt_sym,
                best_path: paths.first().cloned(),
                paths,
                mermaid_diagram,
            });
        }

        let multiplex_csr = self.get_multiplex_csr();
        let paths = self
            .path_finder
            .find_paths_between(multiplex_csr, source, target);

        let scorer = if let Some(t) = task {
            PathScorer::with_task(PathScorerConfig::default(), t)
        } else {
            PathScorer::default()
        };
        let mut paths = scorer.score_paths(paths);

        paths.sort_by(|a, b| {
            b.probability
                .partial_cmp(&a.probability)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        let best_path = paths.first().cloned();
        let mermaid_diagram =
            Self::generate_trace_mermaid(&src_sym, &tgt_sym, &paths, self.graph.symbols());

        Ok(CausalTraceResult {
            source: src_sym,
            target: tgt_sym,
            paths,
            best_path,
            mermaid_diagram,
        })
    }

    /// Generates a Mermaid sequence diagram for candidate causal paths.
    pub fn generate_trace_mermaid(
        source: &SymbolNode,
        target: &SymbolNode,
        paths: &[ExecutionPath],
        symbols: &[SymbolNode],
    ) -> String {
        if paths.is_empty() {
            let mut out = String::from("```mermaid\nsequenceDiagram\n");
            out.push_str(&format!(
                "    participant s{} as {}\n",
                source.id.0, source.name
            ));
            out.push_str(&format!(
                "    participant s{} as {}\n",
                target.id.0, target.name
            ));
            out.push_str(&format!(
                "    s{}--x s{}: No direct path found\n",
                source.id.0, target.id.0
            ));
            out.push_str("```\n");
            return out;
        }

        let mut out = String::from("```mermaid\nsequenceDiagram\n");
        let mut participants = HashSet::new();

        participants.insert(source.id);
        out.push_str(&format!(
            "    participant s{} as {}\n",
            source.id.0, source.name
        ));
        participants.insert(target.id);
        out.push_str(&format!(
            "    participant s{} as {}\n",
            target.id.0, target.name
        ));

        for path in paths.iter().take(3) {
            for &node in &path.nodes {
                if participants.insert(node) {
                    let name = symbols
                        .get(node.0 as usize)
                        .map(|s| s.name.as_str())
                        .unwrap_or("Unknown");
                    out.push_str(&format!("    participant s{} as {}\n", node.0, name));
                }
            }

            for (i, pair) in path.nodes.windows(2).enumerate() {
                let rel = path
                    .relations
                    .get(i)
                    .copied()
                    .unwrap_or(RelationType::Calls);
                out.push_str(&format!("    s{}->>s{}: {:?}\n", pair[0].0, pair[1].0, rel));
            }
        }
        out.push_str("```\n");
        out
    }

    // -------------------------------------------------------------------------
    // Primitive 4: expand
    // -------------------------------------------------------------------------

    /// Expands a localized submodular context cluster centered around a focal symbol up to `budget`.
    pub fn expand(
        &self,
        symbol_id: SymbolId,
        budget: usize,
        _task: Option<&TaskContext>,
    ) -> Result<LocalExpansion, EngineError> {
        let focal_symbol = self
            .graph
            .symbol(symbol_id)
            .cloned()
            .ok_or(EngineError::SymbolNotFound(symbol_id.0))?;

        if budget == 0 || self.graph.is_empty() {
            return Ok(LocalExpansion {
                focal_symbol,
                budget,
                tokens_used: 0,
                symbols: Vec::new(),
                paths: Vec::new(),
                formatted_code: String::new(),
            });
        }

        // Localized knapsack optimization anchored at focal_symbol
        let weighted_seeds = [(symbol_id, 1.0)];
        let symbols = self
            .selector
            .select_context_weighted(self.graph, &weighted_seeds, budget);
        let tokens_used: usize = symbols.iter().map(|s| s.token_cost).sum();
        let lod_map: HashMap<SymbolId, LodLevel> =
            symbols.iter().map(|s| (s.id, LodLevel::FullBody)).collect();
        let formatted_code =
            ContextFormatter::format_markdown(&symbols, &lod_map, self.file_sources);

        // Discover paths connecting focal symbol to selected expansion symbols
        let other_targets: Vec<SymbolId> = symbols
            .iter()
            .map(|s| s.id)
            .filter(|&id| id != symbol_id)
            .collect();

        let paths = if other_targets.is_empty() {
            Vec::new()
        } else {
            let p =
                self.path_finder
                    .find_paths(self.get_multiplex_csr(), &[symbol_id], &other_targets);
            let scorer = PathScorer::default();
            let mut p = scorer.score_paths(p);
            p.sort_by(|a, b| {
                b.probability
                    .partial_cmp(&a.probability)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            p
        };

        Ok(LocalExpansion {
            focal_symbol,
            budget,
            tokens_used,
            symbols,
            paths,
            formatted_code,
        })
    }

    // -------------------------------------------------------------------------
    // Primitive 5: impact
    // -------------------------------------------------------------------------

    /// Evaluates the architectural blast radius and downstream ripple effects for a single symbol.
    pub fn impact(&self, symbol: SymbolId, budget: usize) -> Result<ImpactReport, EngineError> {
        self.impact_set(&[symbol], budget)
    }

    /// Evaluates the architectural blast radius and downstream ripple effects for a set of mutated symbols.
    pub fn impact_set(
        &self,
        symbols: &[SymbolId],
        budget: usize,
    ) -> Result<ImpactReport, EngineError> {
        for &sym in symbols {
            if self.graph.symbol(sym).is_none() {
                return Err(EngineError::SymbolNotFound(sym.0));
            }
        }

        Ok(ImpactAnalyzer::analyze_symbols(
            self.graph,
            symbols,
            budget,
            self.file_sources,
        ))
    }

    // -------------------------------------------------------------------------
    // Primitive 6: context
    // -------------------------------------------------------------------------

    /// Generates an end-to-end budgeted evidence package tailored to a `TaskContext`.
    ///
    /// Automatically performs entrypoint discovery via `locate`, computes task-conditioned
    /// multiplex Personalized PageRank diffusion, and packs optimal context via CELF knapsack
    /// with causal path reconstruction and omission diagnostics.
    pub fn context(&self, task: &TaskContext, budget: usize) -> StructuredContext {
        let ranked = self.locate(task, 10);
        let seeds: Vec<(SymbolId, f32)> =
            ranked.into_iter().map(|r| (r.symbol.id, r.score)).collect();

        self.selector.select_structured_context_weighted(
            self.graph,
            &seeds,
            budget,
            self.file_sources,
            Some(task.clone()),
        )
    }

    /// Generates an end-to-end budgeted evidence package with explicit weighted seeds.
    pub fn context_with_seeds(
        &self,
        seeds: &[(SymbolId, f32)],
        task: Option<&TaskContext>,
        budget: usize,
    ) -> StructuredContext {
        self.selector.select_structured_context_weighted(
            self.graph,
            seeds,
            budget,
            self.file_sources,
            task.cloned(),
        )
    }
}
