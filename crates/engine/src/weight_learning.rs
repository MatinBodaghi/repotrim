//! Principled Multiplex Edge Weight Learning & Optimization.
//!
//! # Academic Foundations & Citations
//! - **Backstrom, L., & Leskovec, J. (2011)**.
//!   *Supervised Random Walks: Predicting and Recommending Links in Social Networks*.
//!   Proceedings of the 4th ACM International Conference on Web Search and Data Mining (WSDM '11), 67-76.
//!   (Formulation of edge weight optimization for Personalized PageRank via link prediction).
//! - **Zimmermann, T., Weißgerber, P., Diehl, S., & Zeller, A. (2005)**.
//!   *Mining Version Histories to Guide Software Changes*.
//!   IEEE Transactions on Software Engineering, 31(6), 429-445.
//! - **Gall, H., Hajek, K., & Jazayeri, M. (1998)**.
//!   *Detection of Logical Coupling Based on Change Sets*.
//!   Proceedings of the 20th International Conference on Software Engineering (ICSE '98), 159-168.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::coedit::CoeditGraph;
use crate::graph::{LayerWeights, MultiplexGraph};
use crate::import::FileImport;
use crate::ppr::{PprConfig, PprSolver};
use crate::resolver::ScopedResolver;
use crate::symbol::{EdgeKind, ReferenceEdge, SymbolId, SymbolNode};

/// Empirical statistics and weight transitions for a single relationship layer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayerLearningStat {
    /// Total directed edges extracted in this layer.
    pub edge_count: usize,
    /// Number of edges whose endpoints historically co-changed.
    pub cochanged_edges: usize,
    /// Sum of time-decayed co-change mass across edges in this layer.
    pub total_cochange_mass: f32,
    /// Empirical co-change rate $R_k = \frac{\sum C(u, v)}{|E_k| + 1}$.
    pub empirical_rate: f32,
    /// Initial baseline heuristic weight.
    pub default_weight: f32,
    /// Calibrated data-driven weight.
    pub learned_weight: f32,
}

/// Comprehensive diagnostic report of edge weight learning.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WeightLearningReport {
    /// Total commits inspected in the mining window.
    pub commits_mined: usize,
    /// Commits contributing valid co-edit data after filtering.
    pub valid_commits: usize,
    /// Megacommits filtered out as bulk churn.
    pub megacommits_filtered: usize,
    /// Total unique co-edit pairs discovered.
    pub total_coedit_pairs: usize,
    /// Layer-by-layer empirical co-change metrics and weight transitions.
    pub layer_stats: HashMap<String, LayerLearningStat>,
    /// Default static heuristic weights.
    pub default_weights: LayerWeights,
    /// Data-driven calibrated weights.
    pub learned_weights: LayerWeights,
    /// Mean Reciprocal Rank (MRR) of co-change retrieval using default heuristic weights.
    pub baseline_mrr: f32,
    /// Mean Reciprocal Rank (MRR) using learned weights and co-edit graph fusion.
    pub learned_mrr: f32,
    /// Relative MRR percentage improvement (+X%).
    pub mrr_improvement_pct: f32,
}

impl WeightLearningReport {
    /// Generates a rich Markdown summary of empirical learning outcomes.
    pub fn to_markdown(&self) -> String {
        let mut out = String::new();
        out.push_str("### 📊 Git Co-Edit Mining & Principled Weight Learning Report\n\n");
        out.push_str(&format!(
            "- **Commits Mined**: {} (Valid: {}, Megacommits Filtered: {})\n",
            self.commits_mined, self.valid_commits, self.megacommits_filtered
        ));
        out.push_str(&format!(
            "- **Discovered Co-Edit Pairs**: {}\n",
            self.total_coedit_pairs
        ));
        out.push_str(&format!(
            "- **Retrieval MRR**: {:.4} → **{:.4}** ({:+.1}% improvement)\n\n",
            self.baseline_mrr, self.learned_mrr, self.mrr_improvement_pct
        ));

        out.push_str("| Layer | Edges | Co-Changed | Co-Change Rate | Default Weight | Learned Weight | Delta |\n");
        out.push_str("| :--- | :---: | :---: | :---: | :---: | :---: | :---: |\n");

        let mut keys: Vec<&String> = self.layer_stats.keys().collect();
        keys.sort();

        for key in keys {
            if let Some(stat) = self.layer_stats.get(key) {
                let delta = stat.learned_weight - stat.default_weight;
                out.push_str(&format!(
                    "| **{}** | {} | {} | {:.4} | {:.2} | **{:.2}** | {:+.2} |\n",
                    key,
                    stat.edge_count,
                    stat.cochanged_edges,
                    stat.empirical_rate,
                    stat.default_weight,
                    stat.learned_weight,
                    delta
                ));
            }
        }

        out
    }
}

/// Learner for estimating principled multiplex layer weights from Git co-edits.
pub struct EdgeWeightLearner;

impl EdgeWeightLearner {
    /// Learns data-driven `LayerWeights` and computes a full diagnostic validation report.
    pub fn learn_weights(
        symbols: &[SymbolNode],
        raw_edges: &[ReferenceEdge],
        imports: &[FileImport],
        coedit_graph: &CoeditGraph,
        default_weights: LayerWeights,
    ) -> (LayerWeights, WeightLearningReport) {
        let num_nodes = symbols.len();
        let resolver = ScopedResolver::with_imports(symbols, imports);

        // Classify and resolve directed edges per static layer
        let mut ast_edges: Vec<(SymbolId, SymbolId)> = Vec::new();
        let mut call_edges: Vec<(SymbolId, SymbolId)> = Vec::new();
        let mut type_edges: Vec<(SymbolId, SymbolId)> = Vec::new();
        let mut import_edges: Vec<(SymbolId, SymbolId)> = Vec::new();

        for edge in raw_edges {
            let src_idx = edge.source.0 as usize;
            if src_idx >= num_nodes {
                continue;
            }
            let src_node = &symbols[src_idx];
            if let Some((target_id, _confidence)) = resolver.resolve(src_node, &edge.target_ident) {
                if edge.source == target_id {
                    continue;
                }
                match edge.kind {
                    EdgeKind::AstParent => {
                        ast_edges.push((edge.source, target_id));
                        ast_edges.push((target_id, edge.source));
                    }
                    EdgeKind::Call => call_edges.push((edge.source, target_id)),
                    EdgeKind::TypeRef => type_edges.push((edge.source, target_id)),
                    EdgeKind::Import => import_edges.push((edge.source, target_id)),
                    EdgeKind::CoEdit => {}
                }
            }
        }

        // Empirical co-change analysis for each layer
        let (ast_cochanged, ast_mass) = Self::analyze_layer(&ast_edges, coedit_graph);
        let (call_cochanged, call_mass) = Self::analyze_layer(&call_edges, coedit_graph);
        let (type_cochanged, type_mass) = Self::analyze_layer(&type_edges, coedit_graph);
        let (import_cochanged, import_mass) = Self::analyze_layer(&import_edges, coedit_graph);

        let ast_rate = ast_mass / (ast_edges.len() as f32 + 1.0);
        let call_rate = call_mass / (call_edges.len() as f32 + 1.0);
        let type_rate = type_mass / (type_edges.len() as f32 + 1.0);
        let import_rate = import_mass / (import_edges.len() as f32 + 1.0);

        let max_rate = ast_rate.max(call_rate).max(type_rate).max(import_rate);

        // Bayesian shrinkage parameter lambda: shrinkage to prior decreases with more commits
        let lambda = (-((coedit_graph.valid_commits as f32) / 50.0))
            .exp()
            .clamp(0.10, 0.85);

        let calibrate_weight = |rate: f32, default_w: f32| -> f32 {
            let raw_empirical = if max_rate > 0.0 {
                rate / max_rate
            } else {
                default_w
            };
            // Shrinkage prior: w* = (1 - lambda) * empirical + lambda * default
            let blended = (1.0 - lambda) * raw_empirical + lambda * default_w;
            blended.max(0.15)
        };

        let raw_learned_ast = calibrate_weight(ast_rate, default_weights.ast_parent);
        let raw_learned_call = calibrate_weight(call_rate, default_weights.call);
        let raw_learned_type = calibrate_weight(type_rate, default_weights.type_ref);
        let raw_learned_import = calibrate_weight(import_rate, default_weights.import);

        // Normalize so highest static weight is 1.0
        let max_raw = raw_learned_ast
            .max(raw_learned_call)
            .max(raw_learned_type)
            .max(raw_learned_import)
            .max(0.01);

        let learned_ast = (raw_learned_ast / max_raw * 100.0).round() / 100.0;
        let learned_call = (raw_learned_call / max_raw * 100.0).round() / 100.0;
        let learned_type = (raw_learned_type / max_raw * 100.0).round() / 100.0;
        let learned_import = (raw_learned_import / max_raw * 100.0).round() / 100.0;

        // Calibrate co-edit layer weight based on average confidence of mined pairs
        let learned_coedit = if !coedit_graph.pairs.is_empty() {
            let sum_conf: f32 = coedit_graph.pairs.iter().map(|p| p.confidence).sum();
            let avg_conf = sum_conf / (coedit_graph.pairs.len() as f32);
            ((0.35 + 0.45 * avg_conf).clamp(0.20, 0.90) * 100.0).round() / 100.0
        } else {
            default_weights.co_edit
        };

        let learned_weights = LayerWeights {
            ast_parent: learned_ast,
            call: learned_call,
            type_ref: learned_type,
            import: learned_import,
            co_edit: learned_coedit,
        };

        let mut layer_stats = HashMap::new();
        layer_stats.insert(
            "AST Parent".to_string(),
            LayerLearningStat {
                edge_count: ast_edges.len(),
                cochanged_edges: ast_cochanged,
                total_cochange_mass: ast_mass,
                empirical_rate: ast_rate,
                default_weight: default_weights.ast_parent,
                learned_weight: learned_ast,
            },
        );
        layer_stats.insert(
            "Call".to_string(),
            LayerLearningStat {
                edge_count: call_edges.len(),
                cochanged_edges: call_cochanged,
                total_cochange_mass: call_mass,
                empirical_rate: call_rate,
                default_weight: default_weights.call,
                learned_weight: learned_call,
            },
        );
        layer_stats.insert(
            "Type Ref".to_string(),
            LayerLearningStat {
                edge_count: type_edges.len(),
                cochanged_edges: type_cochanged,
                total_cochange_mass: type_mass,
                empirical_rate: type_rate,
                default_weight: default_weights.type_ref,
                learned_weight: learned_type,
            },
        );
        layer_stats.insert(
            "Import".to_string(),
            LayerLearningStat {
                edge_count: import_edges.len(),
                cochanged_edges: import_cochanged,
                total_cochange_mass: import_mass,
                empirical_rate: import_rate,
                default_weight: default_weights.import,
                learned_weight: learned_import,
            },
        );
        layer_stats.insert(
            "Git Co-Edit".to_string(),
            LayerLearningStat {
                edge_count: coedit_graph.pairs.len(),
                cochanged_edges: coedit_graph.pairs.len(),
                total_cochange_mass: coedit_graph.pairs.iter().map(|p| p.support).sum(),
                empirical_rate: if !coedit_graph.pairs.is_empty() {
                    1.0
                } else {
                    0.0
                },
                default_weight: default_weights.co_edit,
                learned_weight: learned_coedit,
            },
        );

        // Empirical MRR ranking evaluation
        let (baseline_mrr, learned_mrr) = Self::evaluate_ranking_mrr(
            symbols,
            raw_edges,
            imports,
            coedit_graph,
            default_weights,
            learned_weights,
        );

        let mrr_improvement_pct = if baseline_mrr > 1e-6 {
            ((learned_mrr - baseline_mrr) / baseline_mrr) * 100.0
        } else if learned_mrr > 1e-6 {
            100.0
        } else {
            0.0
        };

        let report = WeightLearningReport {
            commits_mined: coedit_graph.total_commits_analyzed,
            valid_commits: coedit_graph.valid_commits,
            megacommits_filtered: coedit_graph.megacommits_filtered,
            total_coedit_pairs: coedit_graph.pairs.len(),
            layer_stats,
            default_weights,
            learned_weights,
            baseline_mrr,
            learned_mrr,
            mrr_improvement_pct,
        };

        (learned_weights, report)
    }

    /// Counts co-changed edges and sums co-change mass for a given layer.
    fn analyze_layer(edges: &[(SymbolId, SymbolId)], coedit_graph: &CoeditGraph) -> (usize, f32) {
        let mut cochanged_count = 0;
        let mut total_mass = 0.0;

        for &(u, v) in edges {
            let support = coedit_graph.coedit_support(u, v);
            if support > 0.0 {
                cochanged_count += 1;
                total_mass += support;
            }
        }

        (cochanged_count, total_mass)
    }

    /// Evaluates Mean Reciprocal Rank (MRR) of co-changed symbol retrieval
    /// comparing default heuristic weights vs learned weights + co-edit fusion.
    fn evaluate_ranking_mrr(
        symbols: &[SymbolNode],
        raw_edges: &[ReferenceEdge],
        imports: &[FileImport],
        coedit_graph: &CoeditGraph,
        default_weights: LayerWeights,
        learned_weights: LayerWeights,
    ) -> (f32, f32) {
        if coedit_graph.pairs.is_empty() || symbols.is_empty() {
            return (0.0, 0.0);
        }

        // Build baseline graph (no co-edits, default weights)
        let base_graph = MultiplexGraph::build_with_imports(
            symbols.to_vec(),
            raw_edges,
            imports,
            default_weights,
        );

        // Build learned graph (with co-edits, learned weights)
        let coedit_edges = coedit_graph.to_directed_edges(0.10);
        let learned_graph = MultiplexGraph::build_with_all(
            symbols.to_vec(),
            raw_edges,
            imports,
            &coedit_edges,
            learned_weights,
        );

        let ppr_solver = PprSolver::new(PprConfig {
            alpha: 0.85,
            epsilon: 1e-4,
            max_iterations: 100,
        });

        // Sample up to 15 unique source symbols that have co-edit targets
        let mut test_sources: Vec<SymbolId> = Vec::new();
        for p in &coedit_graph.pairs {
            if !test_sources.contains(&p.source) {
                test_sources.push(p.source);
            }
            if test_sources.len() >= 15 {
                break;
            }
        }

        if test_sources.is_empty() {
            return (0.0, 0.0);
        }

        let mut base_mrr_sum = 0.0;
        let mut learned_mrr_sum = 0.0;

        for &seed in &test_sources {
            // Target set: all symbols co-edited with seed
            let targets: Vec<SymbolId> = coedit_graph
                .couplings_for(seed)
                .into_iter()
                .map(|p| p.target)
                .collect();

            if targets.is_empty() {
                continue;
            }

            // Run baseline PPR
            let base_ppr = ppr_solver.compute(&base_graph, &[(seed, 1.0)]);
            let base_rank = Self::compute_target_reciprocal_rank(&base_ppr, &targets, seed);
            base_mrr_sum += base_rank;

            // Run learned PPR
            let learned_ppr = ppr_solver.compute(&learned_graph, &[(seed, 1.0)]);
            let learned_rank = Self::compute_target_reciprocal_rank(&learned_ppr, &targets, seed);
            learned_mrr_sum += learned_rank;
        }

        let count = test_sources.len() as f32;
        (base_mrr_sum / count, learned_mrr_sum / count)
    }

    /// Computes reciprocal rank of the top-ranking target symbol in the PPR stationary distribution.
    fn compute_target_reciprocal_rank(
        ppr_scores: &HashMap<SymbolId, f32>,
        targets: &[SymbolId],
        seed: SymbolId,
    ) -> f32 {
        let mut ranked: Vec<(SymbolId, f32)> = ppr_scores
            .iter()
            .filter(|(&id, _)| id != seed)
            .map(|(&id, &score)| (id, score))
            .collect();

        ranked.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        for (rank_0, &(id, _)) in ranked.iter().enumerate() {
            if targets.contains(&id) {
                return 1.0 / ((rank_0 + 1) as f32);
            }
        }

        0.0
    }
}
