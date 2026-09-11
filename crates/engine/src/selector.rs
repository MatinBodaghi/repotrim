use std::collections::HashMap;
use std::path::PathBuf;

use crate::celf::{CelfConfig, CelfOptimizer, MckpResult, SensitivityReport};
use crate::community::{CommunityConfig, CommunityDetector};
use crate::formatter::ContextFormatter;
use crate::graph::MultiplexGraph;
use crate::knee::KneedleDetector;
use crate::model::ModelProfile;
use crate::ppr::{PprConfig, PprSolver};
use crate::symbol::{SymbolId, SymbolNode};
use crate::tokens::TokenizerModel;

/// Diagnostic report produced when auto-budgeting context selection.
#[derive(Debug, Clone, PartialEq)]
pub struct AutoBudgetReport {
    /// Name of the target model profile used.
    pub model_name: String,
    /// Automatically computed optimal budget threshold in tokens.
    pub optimal_budget: usize,
    /// Exact token cost at the Kneedle knee point.
    pub knee_tokens: usize,
    /// Ratio of cumulative utility captured at the knee point relative to total evaluated graph utility.
    pub knee_utility_ratio: f32,
    /// Total number of candidate symbols considered up to the profile ceiling.
    pub candidate_count: usize,
    /// Final number of symbols selected at the knee point.
    pub selected_count: usize,
}

/// End-to-end context selection pipeline.
///
/// Orchestrates Personalized PageRank (PPR) relevance diffusion and CELF submodular
/// knapsack selection to extract the mathematically optimal code context for AI coding agents.
#[derive(Debug, Clone, Default)]
pub struct ContextSelector {
    ppr: PprSolver,
    celf: CelfOptimizer,
    tokenizer_model: TokenizerModel,
}

impl ContextSelector {
    /// Creates a new `ContextSelector` with custom PPR and CELF configurations.
    pub fn new(ppr_config: PprConfig, celf_config: CelfConfig) -> Self {
        Self {
            ppr: PprSolver::new(ppr_config),
            celf: CelfOptimizer::new(celf_config),
            tokenizer_model: TokenizerModel::default(),
        }
    }

    /// Sets the tokenizer model for token counting and LOD resolution.
    pub fn with_tokenizer(mut self, model: TokenizerModel) -> Self {
        self.tokenizer_model = model;
        self
    }

    /// Returns the active tokenizer model.
    pub fn tokenizer_model(&self) -> TokenizerModel {
        self.tokenizer_model
    }

    /// Returns a borrowed reference to the inner PPR solver.
    pub fn ppr(&self) -> &PprSolver {
        &self.ppr
    }

    /// Returns a borrowed reference to the inner CELF optimizer.
    pub fn celf(&self) -> &CelfOptimizer {
        &self.celf
    }

    /// Selects the optimal set of symbols matching the `budget` (in tokens), seeded by `seed_ids`.
    ///
    /// The resulting `SymbolNode`s are sorted in canonical file and line order
    /// (`file_path`, then `start_row`, then `start_byte`) to produce natural, coherent reading
    /// order for language model prompts.
    pub fn select_context(
        &self,
        graph: &MultiplexGraph,
        seed_ids: &[SymbolId],
        budget: usize,
    ) -> Vec<SymbolNode> {
        let seeds: Vec<(SymbolId, f32)> = seed_ids.iter().map(|&id| (id, 1.0)).collect();
        self.select_context_weighted(graph, &seeds, budget)
    }

    /// Selects the optimal set of symbols matching the `budget` (in tokens), seeded by `weighted_seeds`.
    pub fn select_context_weighted(
        &self,
        graph: &MultiplexGraph,
        weighted_seeds: &[(SymbolId, f32)],
        budget: usize,
    ) -> Vec<SymbolNode> {
        if budget == 0 || graph.is_empty() || weighted_seeds.is_empty() {
            return Vec::new();
        }

        // 1. Compute local PPR relevance diffusion with weighted seeds
        let mut ppr_scores = self.ppr.compute(graph, weighted_seeds);

        // 2. Anchor boost: prioritize user focus seeds as roots of the context tree proportional to weights
        for &(seed_id, weight) in weighted_seeds {
            *ppr_scores.entry(seed_id).or_default() += weight;
        }

        // 3. Select optimal subset using CELF submodular knapsack
        let selected_ids = self.celf.optimize(graph, &ppr_scores, budget);

        // 4. Collect symbol nodes
        let mut selected_symbols: Vec<SymbolNode> = selected_ids
            .into_iter()
            .filter_map(|id| graph.symbol(id).cloned())
            .collect();

        // 5. Sort canonically: by file path, then line number, then start byte
        selected_symbols.sort_by(|a, b| {
            a.file_path
                .cmp(&b.file_path)
                .then_with(|| a.span.start_row.cmp(&b.span.start_row))
                .then_with(|| a.span.start_byte.cmp(&b.span.start_byte))
        });

        selected_symbols
    }

    /// Automatically tunes the token budget using the Kneedle algorithm and
    /// selects the optimal context tailored to the target `ModelProfile`.
    pub fn select_context_auto(
        &self,
        graph: &MultiplexGraph,
        seed_ids: &[SymbolId],
        profile: ModelProfile,
    ) -> (Vec<SymbolNode>, AutoBudgetReport) {
        let seeds: Vec<(SymbolId, f32)> = seed_ids.iter().map(|&id| (id, 1.0)).collect();
        self.select_context_auto_weighted(graph, &seeds, profile)
    }

    /// Automatically tunes the token budget using the Kneedle algorithm and
    /// selects the optimal context tailored to the target `ModelProfile` with weighted seeds.
    pub fn select_context_auto_weighted(
        &self,
        graph: &MultiplexGraph,
        weighted_seeds: &[(SymbolId, f32)],
        profile: ModelProfile,
    ) -> (Vec<SymbolNode>, AutoBudgetReport) {
        if graph.is_empty() || weighted_seeds.is_empty() {
            return (
                Vec::new(),
                AutoBudgetReport {
                    model_name: profile.name.to_string(),
                    optimal_budget: 0,
                    knee_tokens: 0,
                    knee_utility_ratio: 0.0,
                    candidate_count: 0,
                    selected_count: 0,
                },
            );
        }

        // 1. Compute local PPR relevance diffusion with weighted seeds
        let mut ppr_scores = self.ppr.compute(graph, weighted_seeds);

        // 2. Anchor boost
        for &(seed_id, weight) in weighted_seeds {
            *ppr_scores.entry(seed_id).or_default() += weight;
        }

        // 3. Optimize with trace up to profile ceiling
        let (all_selected, trace) =
            self.celf
                .optimize_with_trace(graph, &ppr_scores, profile.max_budget);

        if trace.is_empty() {
            return (
                Vec::new(),
                AutoBudgetReport {
                    model_name: profile.name.to_string(),
                    optimal_budget: 0,
                    knee_tokens: 0,
                    knee_utility_ratio: 0.0,
                    candidate_count: 0,
                    selected_count: 0,
                },
            );
        }

        // 4. Extract points (cumulative_tokens, cumulative_utility)
        let points: Vec<(usize, f32)> = trace
            .iter()
            .map(|s| (s.cumulative_tokens, s.cumulative_utility))
            .collect();

        // 5. Run KneedleDetector with profile sensitivity
        let detector = KneedleDetector::with_params(profile.sensitivity, 0.35);
        let knee = detector.find_knee(&points);

        let (knee_tokens, knee_ratio, knee_idx) = match knee {
            Some(k) => (k.token_cost, k.utility_ratio, k.index),
            None => {
                let last = trace.last().unwrap();
                (last.cumulative_tokens, 1.0, trace.len().saturating_sub(1))
            }
        };

        // Clamp the optimal budget between [min_budget, max_budget]
        let optimal_budget = knee_tokens.clamp(profile.min_budget, profile.max_budget);

        // Retain symbols up to the optimal budget
        let mut selected_ids: Vec<SymbolId> = trace
            .iter()
            .take_while(|s| s.cumulative_tokens <= optimal_budget)
            .map(|s| s.symbol_id)
            .collect();

        if selected_ids.is_empty() && !all_selected.is_empty() {
            selected_ids = trace
                .iter()
                .take(knee_idx + 1)
                .map(|s| s.symbol_id)
                .collect();
        }

        let mut selected_symbols: Vec<SymbolNode> = selected_ids
            .into_iter()
            .filter_map(|id| graph.symbol(id).cloned())
            .collect();

        selected_symbols.sort_by(|a, b| {
            a.file_path
                .cmp(&b.file_path)
                .then_with(|| a.span.start_row.cmp(&b.span.start_row))
                .then_with(|| a.span.start_byte.cmp(&b.span.start_byte))
        });

        let report = AutoBudgetReport {
            model_name: profile.name.to_string(),
            optimal_budget,
            knee_tokens,
            knee_utility_ratio: knee_ratio,
            candidate_count: all_selected.len(),
            selected_count: selected_symbols.len(),
        };

        (selected_symbols, report)
    }

    /// End-to-end pipeline: selects mathematically optimal symbols and formats them
    /// into structured Markdown context with dynamic Level-of-Detail (LOD).
    pub fn select_and_format_context(
        &self,
        graph: &MultiplexGraph,
        seed_ids: &[SymbolId],
        budget: usize,
        file_sources: &HashMap<PathBuf, String>,
    ) -> (Vec<SymbolNode>, String) {
        let seeds: Vec<(SymbolId, f32)> = seed_ids.iter().map(|&id| (id, 1.0)).collect();
        self.select_and_format_context_weighted(graph, &seeds, budget, file_sources)
    }

    /// End-to-end pipeline with weighted seeds: selects mathematically optimal symbols and formats
    /// them into structured Markdown context with dynamic Level-of-Detail (LOD).
    pub fn select_and_format_context_weighted(
        &self,
        graph: &MultiplexGraph,
        weighted_seeds: &[(SymbolId, f32)],
        budget: usize,
        file_sources: &HashMap<PathBuf, String>,
    ) -> (Vec<SymbolNode>, String) {
        if budget == 0 || graph.is_empty() || weighted_seeds.is_empty() {
            return (Vec::new(), String::new());
        }

        // 1. Compute local PPR relevance diffusion with weighted seeds
        let mut ppr_scores = self.ppr.compute(graph, weighted_seeds);

        // 2. Anchor boost: prioritize user focus seeds as roots of the context tree proportional to weights
        for &(seed_id, weight) in weighted_seeds {
            *ppr_scores.entry(seed_id).or_default() += weight;
        }

        // 3. Select optimal subset using CELF submodular knapsack
        let selected_ids = self.celf.optimize(graph, &ppr_scores, budget);

        // 4. Collect and canonically sort symbol nodes
        let mut selected_symbols: Vec<SymbolNode> = selected_ids
            .into_iter()
            .filter_map(|id| graph.symbol(id).cloned())
            .collect();

        selected_symbols.sort_by(|a, b| {
            a.file_path
                .cmp(&b.file_path)
                .then_with(|| a.span.start_row.cmp(&b.span.start_row))
                .then_with(|| a.span.start_byte.cmp(&b.span.start_byte))
        });

        // 5. Dynamically assign LOD and format into Markdown
        let seed_id_list: Vec<SymbolId> = weighted_seeds.iter().map(|&(id, _)| id).collect();
        let lod_map = ContextFormatter::assign_lod_with_model(
            &selected_symbols,
            &ppr_scores,
            &seed_id_list,
            budget,
            file_sources,
            self.tokenizer_model,
        );
        let markdown = ContextFormatter::format_markdown(&selected_symbols, &lod_map, file_sources);

        (selected_symbols, markdown)
    }

    /// End-to-end pipeline with auto-budgeting: selects optimal symbols and formats
    /// them into structured Markdown context with dynamic Level-of-Detail (LOD).
    pub fn select_and_format_context_auto(
        &self,
        graph: &MultiplexGraph,
        seed_ids: &[SymbolId],
        profile: ModelProfile,
        file_sources: &HashMap<PathBuf, String>,
    ) -> (Vec<SymbolNode>, String, AutoBudgetReport) {
        let seeds: Vec<(SymbolId, f32)> = seed_ids.iter().map(|&id| (id, 1.0)).collect();
        self.select_and_format_context_auto_weighted(graph, &seeds, profile, file_sources)
    }

    /// End-to-end pipeline with auto-budgeting and weighted seeds: selects optimal symbols
    /// and formats them into structured Markdown context with dynamic Level-of-Detail (LOD).
    pub fn select_and_format_context_auto_weighted(
        &self,
        graph: &MultiplexGraph,
        weighted_seeds: &[(SymbolId, f32)],
        profile: ModelProfile,
        file_sources: &HashMap<PathBuf, String>,
    ) -> (Vec<SymbolNode>, String, AutoBudgetReport) {
        let (selected_symbols, report) =
            self.select_context_auto_weighted(graph, weighted_seeds, profile);

        if selected_symbols.is_empty() {
            return (Vec::new(), String::new(), report);
        }

        let mut ppr_scores = self.ppr.compute(graph, weighted_seeds);
        for &(seed_id, weight) in weighted_seeds {
            *ppr_scores.entry(seed_id).or_default() += weight;
        }

        let seed_id_list: Vec<SymbolId> = weighted_seeds.iter().map(|&(id, _)| id).collect();
        let lod_map = ContextFormatter::assign_lod_with_model(
            &selected_symbols,
            &ppr_scores,
            &seed_id_list,
            report.optimal_budget,
            file_sources,
            self.tokenizer_model,
        );
        let markdown = ContextFormatter::format_markdown(&selected_symbols, &lod_map, file_sources);

        (selected_symbols, markdown, report)
    }

    /// End-to-end pipeline: selects mathematically optimal symbols and formats them
    /// into structured Markdown context with dynamic Level-of-Detail (LOD) and
    /// knapsack numerical stability and sensitivity diagnostics.
    pub fn select_and_format_context_with_sensitivity(
        &self,
        graph: &MultiplexGraph,
        seed_ids: &[SymbolId],
        budget: usize,
        file_sources: &HashMap<PathBuf, String>,
    ) -> (Vec<SymbolNode>, String, SensitivityReport) {
        let seeds: Vec<(SymbolId, f32)> = seed_ids.iter().map(|&id| (id, 1.0)).collect();
        self.select_and_format_context_weighted_with_sensitivity(
            graph,
            &seeds,
            budget,
            file_sources,
        )
    }

    /// End-to-end pipeline with weighted seeds: selects optimal symbols and formats
    /// them with LOD and knapsack sensitivity diagnostics.
    pub fn select_and_format_context_weighted_with_sensitivity(
        &self,
        graph: &MultiplexGraph,
        weighted_seeds: &[(SymbolId, f32)],
        budget: usize,
        file_sources: &HashMap<PathBuf, String>,
    ) -> (Vec<SymbolNode>, String, SensitivityReport) {
        if budget == 0 || graph.is_empty() || weighted_seeds.is_empty() {
            let report = SensitivityReport {
                epsilon: self.ppr.config().epsilon,
                alpha: self.ppr.config().alpha,
                max_error_bound: 0.0,
                mean_error_bound: 0.0,
                total_candidates: 0,
                selected_count: 0,
                stable_count: 0,
                stability_index: 1.0,
                borderline_pairs: Vec::new(),
            };
            return (Vec::new(), String::new(), report);
        }

        // 1. Compute detailed PPR relevance diffusion with weighted seeds
        let mut ppr_result = self.ppr.compute_detailed(graph, weighted_seeds);

        // 2. Anchor boost
        for &(seed_id, weight) in weighted_seeds {
            *ppr_result.scores.entry(seed_id).or_default() += weight;
        }

        // 3. Select optimal subset using CELF submodular knapsack
        let selected_ids = self.celf.optimize(graph, &ppr_result.scores, budget);

        // 4. Analyze numerical sensitivity
        let sensitivity = self.celf.analyze_sensitivity(
            graph,
            &ppr_result,
            &selected_ids,
            budget,
            self.ppr.config().alpha,
            self.ppr.config().epsilon,
        );

        // 5. Collect and canonically sort symbol nodes
        let mut selected_symbols: Vec<SymbolNode> = selected_ids
            .into_iter()
            .filter_map(|id| graph.symbol(id).cloned())
            .collect();

        selected_symbols.sort_by(|a, b| {
            a.file_path
                .cmp(&b.file_path)
                .then_with(|| a.span.start_row.cmp(&b.span.start_row))
                .then_with(|| a.span.start_byte.cmp(&b.span.start_byte))
        });

        // 6. Dynamically assign LOD and format into Markdown
        let seed_id_list: Vec<SymbolId> = weighted_seeds.iter().map(|&(id, _)| id).collect();
        let lod_map = ContextFormatter::assign_lod_with_model(
            &selected_symbols,
            &ppr_result.scores,
            &seed_id_list,
            budget,
            file_sources,
            self.tokenizer_model,
        );
        let markdown = ContextFormatter::format_markdown(&selected_symbols, &lod_map, file_sources);

        (selected_symbols, markdown, sensitivity)
    }

    /// End-to-end pipeline using Multiple-Choice Knapsack (MCKP) joint symbol selection
    /// and Level-of-Detail (LOD) optimization (Kellerer et al., 2004; Dyer, 1984).
    pub fn select_and_format_context_joint_lod(
        &self,
        graph: &MultiplexGraph,
        seed_ids: &[SymbolId],
        budget: usize,
        file_sources: &HashMap<PathBuf, String>,
    ) -> (Vec<SymbolNode>, String, MckpResult) {
        let seeds: Vec<(SymbolId, f32)> = seed_ids.iter().map(|&id| (id, 1.0)).collect();
        self.select_and_format_context_weighted_joint_lod(graph, &seeds, budget, file_sources)
    }

    /// End-to-end pipeline with weighted seeds using Multiple-Choice Knapsack (MCKP)
    /// joint symbol selection and Level-of-Detail (LOD) optimization.
    pub fn select_and_format_context_weighted_joint_lod(
        &self,
        graph: &MultiplexGraph,
        weighted_seeds: &[(SymbolId, f32)],
        budget: usize,
        file_sources: &HashMap<PathBuf, String>,
    ) -> (Vec<SymbolNode>, String, MckpResult) {
        if budget == 0 || graph.is_empty() || weighted_seeds.is_empty() {
            return (
                Vec::new(),
                String::new(),
                MckpResult {
                    selected_lods: HashMap::new(),
                    total_tokens: 0,
                    cumulative_utility: 0.0,
                    trace: Vec::new(),
                },
            );
        }

        // 1. Compute local PPR relevance diffusion with weighted seeds
        let mut ppr_scores = self.ppr.compute(graph, weighted_seeds);

        // 2. Anchor boost: prioritize user focus seeds proportional to weights
        for &(seed_id, weight) in weighted_seeds {
            *ppr_scores.entry(seed_id).or_default() += weight;
        }

        // 3. Solve Multiple-Choice Knapsack Problem (MCKP) jointly
        let mckp_result = self.celf.optimize_mckp(
            graph,
            &ppr_scores,
            budget,
            file_sources,
            self.tokenizer_model,
        );

        // 4. Collect and canonically sort selected symbol nodes
        let mut selected_symbols: Vec<SymbolNode> = mckp_result
            .selected_lods
            .keys()
            .filter_map(|&id| graph.symbol(id).cloned())
            .collect();

        selected_symbols.sort_by(|a, b| {
            a.file_path
                .cmp(&b.file_path)
                .then_with(|| a.span.start_row.cmp(&b.span.start_row))
                .then_with(|| a.span.start_byte.cmp(&b.span.start_byte))
        });

        // 5. Format into Markdown using the exact MCKP-selected LOD mapping
        let markdown = ContextFormatter::format_markdown(
            &selected_symbols,
            &mckp_result.selected_lods,
            file_sources,
        );

        (selected_symbols, markdown, mckp_result)
    }

    /// End-to-end pipeline with auto-budgeting using Multiple-Choice Knapsack (MCKP)
    /// joint symbol selection and Level-of-Detail (LOD) optimization.
    pub fn select_and_format_context_auto_joint_lod(
        &self,
        graph: &MultiplexGraph,
        seed_ids: &[SymbolId],
        profile: ModelProfile,
        file_sources: &HashMap<PathBuf, String>,
    ) -> (Vec<SymbolNode>, String, AutoBudgetReport, MckpResult) {
        let seeds: Vec<(SymbolId, f32)> = seed_ids.iter().map(|&id| (id, 1.0)).collect();
        self.select_and_format_context_auto_weighted_joint_lod(graph, &seeds, profile, file_sources)
    }

    /// End-to-end pipeline with auto-budgeting and weighted seeds using Multiple-Choice Knapsack (MCKP)
    /// joint symbol selection and Level-of-Detail (LOD) optimization.
    pub fn select_and_format_context_auto_weighted_joint_lod(
        &self,
        graph: &MultiplexGraph,
        weighted_seeds: &[(SymbolId, f32)],
        profile: ModelProfile,
        file_sources: &HashMap<PathBuf, String>,
    ) -> (Vec<SymbolNode>, String, AutoBudgetReport, MckpResult) {
        if graph.is_empty() || weighted_seeds.is_empty() {
            let report = AutoBudgetReport {
                model_name: profile.name.to_string(),
                optimal_budget: 0,
                knee_tokens: 0,
                knee_utility_ratio: 0.0,
                candidate_count: 0,
                selected_count: 0,
            };
            let mckp = MckpResult {
                selected_lods: HashMap::new(),
                total_tokens: 0,
                cumulative_utility: 0.0,
                trace: Vec::new(),
            };
            return (Vec::new(), String::new(), report, mckp);
        }

        // 1. Compute local PPR relevance diffusion with weighted seeds
        let mut ppr_scores = self.ppr.compute(graph, weighted_seeds);
        for &(seed_id, weight) in weighted_seeds {
            *ppr_scores.entry(seed_id).or_default() += weight;
        }

        // 2. Run MCKP optimization up to max_budget to generate the incremental utility-cost curve
        let full_mckp = self.celf.optimize_mckp(
            graph,
            &ppr_scores,
            profile.max_budget,
            file_sources,
            self.tokenizer_model,
        );

        if full_mckp.trace.is_empty() {
            let report = AutoBudgetReport {
                model_name: profile.name.to_string(),
                optimal_budget: 0,
                knee_tokens: 0,
                knee_utility_ratio: 0.0,
                candidate_count: 0,
                selected_count: 0,
            };
            return (Vec::new(), String::new(), report, full_mckp);
        }

        // 3. Extract curve points: (cumulative_tokens, cumulative_utility)
        let points: Vec<(usize, f32)> = full_mckp
            .trace
            .iter()
            .map(|s| (s.cumulative_tokens, s.cumulative_utility))
            .collect();

        // 4. Run KneedleDetector
        let detector = KneedleDetector::with_params(profile.sensitivity, 0.35);
        let knee = detector.find_knee(&points);
        let (knee_tokens, knee_ratio, _) = match knee {
            Some(k) => (k.token_cost, k.utility_ratio, k.index),
            None => {
                let last = full_mckp.trace.last().unwrap();
                (
                    last.cumulative_tokens,
                    1.0,
                    full_mckp.trace.len().saturating_sub(1),
                )
            }
        };

        let optimal_budget = knee_tokens.clamp(profile.min_budget, profile.max_budget);

        // 5. Re-run MCKP at optimal_budget
        let mckp_result = self.celf.optimize_mckp(
            graph,
            &ppr_scores,
            optimal_budget,
            file_sources,
            self.tokenizer_model,
        );

        // 6. Collect and sort selected symbols
        let mut selected_symbols: Vec<SymbolNode> = mckp_result
            .selected_lods
            .keys()
            .filter_map(|&id| graph.symbol(id).cloned())
            .collect();

        selected_symbols.sort_by(|a, b| {
            a.file_path
                .cmp(&b.file_path)
                .then_with(|| a.span.start_row.cmp(&b.span.start_row))
                .then_with(|| a.span.start_byte.cmp(&b.span.start_byte))
        });

        let markdown = ContextFormatter::format_markdown(
            &selected_symbols,
            &mckp_result.selected_lods,
            file_sources,
        );

        let report = AutoBudgetReport {
            model_name: profile.name.to_string(),
            optimal_budget,
            knee_tokens,
            knee_utility_ratio: knee_ratio,
            candidate_count: full_mckp.selected_lods.len(),
            selected_count: selected_symbols.len(),
        };

        (selected_symbols, markdown, report, mckp_result)
    }

    /// Applies an intra-community cohesion boost to PPR relevance scores.
    ///
    /// Symbols belonging to the same topological community as any focus seed receive
    /// a utility multiplier $(1.0 + \text{boost})$, reducing context drift into tangential hubs.
    pub fn apply_community_boost(
        graph: &MultiplexGraph,
        ppr_scores: &mut HashMap<SymbolId, f32>,
        weighted_seeds: &[(SymbolId, f32)],
        boost: f32,
    ) {
        if boost <= 0.0 || weighted_seeds.is_empty() || graph.is_empty() {
            return;
        }

        let comm_res = CommunityDetector::detect(graph, &CommunityConfig::meso_scale());
        let seed_comms: std::collections::HashSet<usize> = weighted_seeds
            .iter()
            .filter_map(|&(sid, _)| comm_res.membership.get(&sid).copied())
            .collect();

        if !seed_comms.is_empty() {
            for (sid, score) in ppr_scores.iter_mut() {
                if let Some(&comm_id) = comm_res.membership.get(sid) {
                    if seed_comms.contains(&comm_id) {
                        *score *= 1.0 + boost;
                    }
                }
            }
        }
    }

    /// Selects context with intra-community cohesion boosting.
    pub fn select_context_with_community(
        &self,
        graph: &MultiplexGraph,
        weighted_seeds: &[(SymbolId, f32)],
        budget: usize,
        community_boost: f32,
    ) -> Vec<SymbolNode> {
        if budget == 0 || graph.is_empty() || weighted_seeds.is_empty() {
            return Vec::new();
        }

        let mut ppr_scores = self.ppr.compute(graph, weighted_seeds);
        for &(seed_id, weight) in weighted_seeds {
            *ppr_scores.entry(seed_id).or_default() += weight;
        }

        Self::apply_community_boost(graph, &mut ppr_scores, weighted_seeds, community_boost);

        let selected_ids = self.celf.optimize(graph, &ppr_scores, budget);
        let mut selected_symbols: Vec<SymbolNode> = selected_ids
            .into_iter()
            .filter_map(|id| graph.symbol(id).cloned())
            .collect();

        selected_symbols.sort_by(|a, b| {
            a.file_path
                .cmp(&b.file_path)
                .then_with(|| a.span.start_row.cmp(&b.span.start_row))
                .then_with(|| a.span.start_byte.cmp(&b.span.start_byte))
        });

        selected_symbols
    }

    /// End-to-end pipeline with community cohesion boosting and formatted context.
    pub fn select_and_format_context_with_community(
        &self,
        graph: &MultiplexGraph,
        weighted_seeds: &[(SymbolId, f32)],
        budget: usize,
        community_boost: f32,
        file_sources: &HashMap<PathBuf, String>,
    ) -> (Vec<SymbolNode>, String) {
        if budget == 0 || graph.is_empty() || weighted_seeds.is_empty() {
            return (Vec::new(), String::new());
        }

        let mut ppr_scores = self.ppr.compute(graph, weighted_seeds);
        for &(seed_id, weight) in weighted_seeds {
            *ppr_scores.entry(seed_id).or_default() += weight;
        }

        Self::apply_community_boost(graph, &mut ppr_scores, weighted_seeds, community_boost);

        let selected_ids = self.celf.optimize(graph, &ppr_scores, budget);

        let mut selected_symbols: Vec<SymbolNode> = selected_ids
            .into_iter()
            .filter_map(|id| graph.symbol(id).cloned())
            .collect();

        selected_symbols.sort_by(|a, b| {
            a.file_path
                .cmp(&b.file_path)
                .then_with(|| a.span.start_row.cmp(&b.span.start_row))
                .then_with(|| a.span.start_byte.cmp(&b.span.start_byte))
        });

        let seed_id_list: Vec<SymbolId> = weighted_seeds.iter().map(|&(id, _)| id).collect();
        let lod_map = ContextFormatter::assign_lod_with_model(
            &selected_symbols,
            &ppr_scores,
            &seed_id_list,
            budget,
            file_sources,
            self.tokenizer_model,
        );
        let markdown = ContextFormatter::format_markdown(&selected_symbols, &lod_map, file_sources);

        (selected_symbols, markdown)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::LayerWeights;
    use crate::symbol::{EdgeKind, ReferenceEdge, SymbolKind, TextSpan};
    use std::path::PathBuf;

    fn make_test_symbol(id: u32, name: &str, file: &str, row: usize, cost: usize) -> SymbolNode {
        SymbolNode {
            id: SymbolId(id),
            name: name.to_string(),
            kind: SymbolKind::Function,
            file_path: PathBuf::from(file),
            span: TextSpan::new(row * 20, (row + 1) * 20, row, row + 1),
            signature: format!("fn {}()", name),
            docstring: None,
            token_cost: cost,
            ast_hash: [0u8; 32],
            container_name: None,
            trait_name: None,
        }
    }

    #[test]
    fn test_context_selector_budget_and_canonical_sorting() {
        // Module with 3 functions in geometry.rs and 1 in main.rs
        let s0 = make_test_symbol(0, "main", "src/main.rs", 10, 25);
        let s1 = make_test_symbol(1, "distance", "src/geometry.rs", 20, 30);
        let s2 = make_test_symbol(2, "sqrt", "src/geometry.rs", 5, 15);
        let s3 = make_test_symbol(3, "area", "src/geometry.rs", 40, 40);

        let edges = vec![
            ReferenceEdge {
                source: SymbolId(0),
                target_ident: "distance".to_string(),
                kind: EdgeKind::Call,
            },
            ReferenceEdge {
                source: SymbolId(1),
                target_ident: "sqrt".to_string(),
                kind: EdgeKind::Call,
            },
        ];

        let graph = MultiplexGraph::build(vec![s0, s1, s2, s3], &edges, LayerWeights::default());
        let selector = ContextSelector::default();

        // Query with seed = main (SymbolId 0), budget = 75 tokens
        // main (25) + distance (30) + sqrt (15) = 70 tokens <= 75 tokens
        let context = selector.select_context(&graph, &[SymbolId(0)], 75);

        assert_eq!(context.len(), 3);
        let names: Vec<&str> = context.iter().map(|s| s.name.as_str()).collect();

        // Canonical ordering should place geometry.rs first (sorted by file path),
        // and inside geometry.rs, sqrt (row 5) before distance (row 20)!
        assert_eq!(names, vec!["sqrt", "distance", "main"]);

        let total_tokens: usize = context.iter().map(|s| s.token_cost).sum();
        assert!(total_tokens <= 75);
    }

    #[test]
    fn test_context_selector_strict_budget_pruning() {
        let s0 = make_test_symbol(0, "entry", "src/lib.rs", 0, 50);
        let s1 = make_test_symbol(1, "helper", "src/lib.rs", 10, 50);

        let edges = vec![ReferenceEdge {
            source: SymbolId(0),
            target_ident: "helper".to_string(),
            kind: EdgeKind::Call,
        }];

        let graph = MultiplexGraph::build(vec![s0, s1], &edges, LayerWeights::default());
        let selector = ContextSelector::default();

        // Budget of 60 tokens: only entry (50 tokens) can fit; helper (50) would exceed 60
        let context = selector.select_context(&graph, &[SymbolId(0)], 60);
        assert_eq!(context.len(), 1);
        assert_eq!(context[0].name, "entry");
    }

    #[test]
    fn test_context_selector_select_and_format_pipeline() {
        let s0 = make_test_symbol(0, "entry", "src/lib.rs", 0, 10);
        let s1 = make_test_symbol(1, "helper", "src/lib.rs", 10, 10);

        let edges = vec![ReferenceEdge {
            source: SymbolId(0),
            target_ident: "helper".to_string(),
            kind: EdgeKind::Call,
        }];

        let graph = MultiplexGraph::build(vec![s0, s1], &edges, LayerWeights::default());
        let selector = ContextSelector::default();

        let mut sources = HashMap::new();
        sources.insert(
            PathBuf::from("src/lib.rs"),
            "fn entry() {\n    helper();\n}\n\nfn helper() {\n}\n".to_string(),
        );

        let (symbols, markdown) =
            selector.select_and_format_context(&graph, &[SymbolId(0)], 100, &sources);

        assert_eq!(symbols.len(), 2);
        assert!(markdown.contains("### File: `src/lib.rs`"));
        assert!(markdown.contains("```rust"));
        assert!(markdown.contains("fn entry()"));
        assert!(markdown.contains("fn helper()"));
    }

    #[test]
    fn test_context_selector_weighted_seeds() {
        let s0 = make_test_symbol(0, "heavy_seed", "src/lib.rs", 0, 10);
        let s1 = make_test_symbol(1, "light_seed", "src/lib.rs", 10, 10);
        let s2 = make_test_symbol(2, "leaf", "src/lib.rs", 20, 10);

        let edges = vec![
            ReferenceEdge {
                source: SymbolId(0),
                target_ident: "leaf".to_string(),
                kind: EdgeKind::Call,
            },
            ReferenceEdge {
                source: SymbolId(1),
                target_ident: "leaf".to_string(),
                kind: EdgeKind::Call,
            },
        ];

        let graph = MultiplexGraph::build(vec![s0, s1, s2], &edges, LayerWeights::default());
        let selector = ContextSelector::default();

        let weighted_seeds = vec![(SymbolId(0), 10.0), (SymbolId(1), 0.1)];
        let selected = selector.select_context_weighted(&graph, &weighted_seeds, 25);

        // Budget of 25 fits 2 symbols (each is 10 tokens).
        // heavy_seed (weight 10.0) should be prioritized.
        assert_eq!(selected.len(), 2);
        assert!(selected.iter().any(|s| s.name == "heavy_seed"));
    }

    #[test]
    fn test_context_selector_auto_budgeting() {
        let s0 = make_test_symbol(0, "entry", "src/entry.rs", 0, 100);
        let s1 = make_test_symbol(1, "service", "src/service.rs", 10, 150);
        let s2 = make_test_symbol(2, "db", "src/db.rs", 20, 200);
        let s3 = make_test_symbol(3, "cache", "src/cache.rs", 30, 250);
        let s4 = make_test_symbol(4, "metrics", "src/metrics.rs", 40, 300);

        let edges = vec![
            ReferenceEdge {
                source: SymbolId(0),
                target_ident: "service".to_string(),
                kind: EdgeKind::Call,
            },
            ReferenceEdge {
                source: SymbolId(1),
                target_ident: "db".to_string(),
                kind: EdgeKind::Call,
            },
            ReferenceEdge {
                source: SymbolId(1),
                target_ident: "cache".to_string(),
                kind: EdgeKind::Call,
            },
            ReferenceEdge {
                source: SymbolId(2),
                target_ident: "metrics".to_string(),
                kind: EdgeKind::Call,
            },
        ];

        let graph =
            MultiplexGraph::build(vec![s0, s1, s2, s3, s4], &edges, LayerWeights::default());
        let selector = ContextSelector::default();

        let profile = ModelProfile::local_ollama();
        let (selected, report) = selector.select_context_auto(&graph, &[SymbolId(0)], profile);

        assert!(!selected.is_empty());
        assert_eq!(report.model_name, "local-ollama");
        assert!(report.optimal_budget >= profile.min_budget);
        assert!(report.optimal_budget <= profile.max_budget);
        assert!(report.knee_utility_ratio > 0.0);
        assert_eq!(report.selected_count, selected.len());

        let mut sources = HashMap::new();
        sources.insert(PathBuf::from("src/entry.rs"), "fn entry() {}".to_string());
        sources.insert(
            PathBuf::from("src/service.rs"),
            "fn service() {}".to_string(),
        );

        let (syms, md, rep) =
            selector.select_and_format_context_auto(&graph, &[SymbolId(0)], profile, &sources);
        assert!(!syms.is_empty());
        assert!(!md.is_empty());
        assert_eq!(rep.optimal_budget, report.optimal_budget);
    }

    #[test]
    fn test_context_selector_with_sensitivity() {
        let s0 = make_test_symbol(0, "entry", "src/entry.rs", 1, 10);
        let s1 = make_test_symbol(1, "service", "src/service.rs", 1, 10);

        let edges = vec![ReferenceEdge {
            source: SymbolId(0),
            target_ident: "service".to_string(),
            kind: EdgeKind::Call,
        }];

        let graph = MultiplexGraph::build(vec![s0, s1], &edges, LayerWeights::default());
        let selector = ContextSelector::default();

        let mut sources = HashMap::new();
        sources.insert(PathBuf::from("src/entry.rs"), "fn entry() {}".to_string());
        sources.insert(
            PathBuf::from("src/service.rs"),
            "fn service() {}".to_string(),
        );

        let (syms, md, report) = selector.select_and_format_context_with_sensitivity(
            &graph,
            &[SymbolId(0)],
            100,
            &sources,
        );

        assert!(!syms.is_empty());
        assert!(!md.is_empty());
        assert_eq!(report.selected_count, syms.len());
        assert!(report.stability_index >= 0.0 && report.stability_index <= 1.0);
    }

    #[test]
    fn test_context_selector_joint_lod() {
        let s0 = make_test_symbol(0, "entry", "src/entry.rs", 1, 10);
        let mut s1 = make_test_symbol(1, "service", "src/service.rs", 1, 15);
        s1.docstring = Some("Handles backend business transactions".to_string());

        let edges = vec![ReferenceEdge {
            source: SymbolId(0),
            target_ident: "service".to_string(),
            kind: EdgeKind::Call,
        }];

        let graph = MultiplexGraph::build(vec![s0, s1], &edges, LayerWeights::default());
        let selector = ContextSelector::default();

        let mut sources = HashMap::new();
        sources.insert(
            PathBuf::from("src/entry.rs"),
            "fn entry() { service(); }".to_string(),
        );
        sources.insert(
            PathBuf::from("src/service.rs"),
            "/// Handles backend business transactions\nfn service() { do_work(); }".to_string(),
        );

        // Run joint LOD selection with modest budget
        let (syms, md, mckp) =
            selector.select_and_format_context_joint_lod(&graph, &[SymbolId(0)], 50, &sources);

        assert!(!syms.is_empty());
        assert!(!md.is_empty());
        assert!(mckp.total_tokens <= 50);
        assert_eq!(syms.len(), mckp.selected_lods.len());

        // Run auto joint LOD selection
        let (auto_syms, auto_md, report, auto_mckp) = selector
            .select_and_format_context_auto_joint_lod(
                &graph,
                &[SymbolId(0)],
                ModelProfile::claude_3_5_sonnet(),
                &sources,
            );

        assert!(!auto_syms.is_empty());
        assert!(!auto_md.is_empty());
        assert!(auto_mckp.total_tokens <= report.optimal_budget);
    }
}
