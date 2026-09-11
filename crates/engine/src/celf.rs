use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap, HashSet};
use std::path::PathBuf;

use crate::formatter::ContextFormatter;
use crate::graph::MultiplexGraph;
use crate::ppr::PprResult;
use crate::symbol::{LodLevel, SymbolId, SymbolNode};
use crate::tokens::{count_tokens, TokenizerModel};

/// Relative utility multipliers for discrete Level-of-Detail (LOD) representations.
///
/// In Multiple-Choice Knapsack (MCKP) joint optimization (Kellerer et al., 2004), each code symbol
/// can be rendered at one of four discrete resolution tiers, each providing an increasing fraction
/// of full semantic utility:
/// - `signature`: Basic interface signature contract (name, params, return type). Default: `0.35`.
/// - `doc`: Interface signature plus documentation comments. Default: `0.60`.
/// - `sliced`: Sliced control-flow skeleton (branches, loops, inter-procedural calls). Default: `0.85`.
/// - `full`: Complete unpruned source implementation. Default: `1.00`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct LodWeights {
    pub signature: f32,
    pub doc: f32,
    pub sliced: f32,
    pub full: f32,
}

impl Default for LodWeights {
    fn default() -> Self {
        Self {
            signature: 0.35,
            doc: 0.60,
            sliced: 0.85,
            full: 1.00,
        }
    }
}

impl LodWeights {
    /// Returns the utility multiplier corresponding to a given `LodLevel`.
    pub fn multiplier(&self, level: LodLevel) -> f32 {
        match level {
            LodLevel::SignatureOnly => self.signature,
            LodLevel::SignatureAndDoc => self.doc,
            LodLevel::SlicedBody => self.sliced,
            LodLevel::FullBody => self.full,
        }
    }
}

/// Concrete Level-of-Detail option for a symbol in Multiple-Choice Knapsack Optimization.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LodOption {
    /// The discrete LOD level, or `None` representing exclusion from context.
    pub level: Option<LodLevel>,
    /// Concrete token cost under the active tokenizer.
    pub cost: usize,
    /// Semantic utility multiplier $\mu \in [0.0, 1.0]$.
    pub utility_multiplier: f32,
}

/// Diagnostic trace step recorded during MCKP joint selection and LOD assignment.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MckpTraceStep {
    /// Identifier of the upgraded symbol.
    pub symbol_id: SymbolId,
    /// Previous LOD level before upgrade (`None` if newly admitted).
    pub from_level: Option<LodLevel>,
    /// New LOD level after upgrade.
    pub to_level: LodLevel,
    /// Incremental token cost of this upgrade step.
    pub incremental_cost: usize,
    /// Cumulative token cost after this upgrade step.
    pub cumulative_tokens: usize,
    /// Marginal utility gain provided by this upgrade step.
    pub marginal_gain: f32,
    /// Cumulative utility score after this upgrade step.
    pub cumulative_utility: f32,
}

/// Result of Multiple-Choice Knapsack (MCKP) joint symbol selection and Level-of-Detail assignment.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MckpResult {
    /// Mapping of selected symbol IDs to their chosen Level-of-Detail.
    pub selected_lods: HashMap<SymbolId, LodLevel>,
    /// Total tokens consumed by the selected configuration.
    pub total_tokens: usize,
    /// Total cumulative submodular utility achieved.
    pub cumulative_utility: f32,
    /// Diagnostic trace steps of upgrades performed.
    pub trace: Vec<MckpTraceStep>,
}

/// Configuration parameters for the CELF Submodular Knapsack optimizer.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CelfConfig {
    /// Anti-clustering diversity weight ($\lambda$). Default: `0.4`.
    pub lambda_diversity: f32,
    /// Neighborhood coverage weight ($\mu$). Default: `0.2`.
    pub neighbor_coverage_weight: f32,
    /// Minimum PPR relevance score required to be considered for knapsack selection. Default: `1e-5`.
    pub min_relevance_threshold: f32,
    /// Optional error tolerance ($\varepsilon$) for Badanidiyuru-Vondrák (2014) threshold-based greedy search.
    /// When `None` (default), uses lazy CELF with Khuller et al. (1999) best-singleton knapsack correction.
    pub threshold_epsilon: Option<f32>,
    /// Relative utility multipliers for discrete Level-of-Detail (LOD) representations.
    pub lod_weights: LodWeights,
    /// Tokenizer model used for estimating framing and symbol token costs.
    #[serde(default)]
    pub tokenizer_model: TokenizerModel,
    /// Whether to account for structural Markdown framing (file headers, container wrappers, line comments).
    #[serde(default)]
    pub framing_aware: bool,
}

impl Default for CelfConfig {
    fn default() -> Self {
        Self {
            lambda_diversity: 0.4,
            neighbor_coverage_weight: 0.2,
            min_relevance_threshold: 1e-5,
            threshold_epsilon: None,
            lod_weights: LodWeights::default(),
            tokenizer_model: TokenizerModel::default(),
            framing_aware: false,
        }
    }
}

impl CelfConfig {
    /// Sets custom Level-of-Detail utility multipliers.
    pub fn with_lod_weights(mut self, weights: LodWeights) -> Self {
        self.lod_weights = weights;
        self
    }

    /// Sets the tokenizer model for structural framing and cost calculations.
    pub fn with_tokenizer(mut self, model: TokenizerModel) -> Self {
        self.tokenizer_model = model;
        self
    }

    /// Sets whether the optimizer should account for structural Markdown framing.
    pub fn with_framing(mut self, framing_aware: bool) -> Self {
        self.framing_aware = framing_aware;
        self
    }
}

/// Builds the Pareto-efficient upper convex hull of Level-of-Detail options for a symbol.
///
/// Prunes dominated options:
/// 1. Prunes options with non-increasing costs or non-increasing utility.
/// 2. If two options have the same cost, retains only the one with strictly higher utility.
/// 3. Prunes non-convex points where the incremental slope $\frac{\Delta \mu}{\Delta c}$ does not decrease
///    (Dyer, 1984; Zemel, 1980; Kellerer et al., 2004, Chapter 11.1).
pub fn build_pareto_frontier(
    sym: &SymbolNode,
    file_source: Option<&str>,
    model: TokenizerModel,
    weights: LodWeights,
    framing_aware: bool,
) -> Vec<LodOption> {
    let mut raw_options = Vec::with_capacity(5);
    raw_options.push(LodOption {
        level: None,
        cost: 0,
        utility_multiplier: 0.0,
    });

    let levels = [
        LodLevel::SignatureOnly,
        LodLevel::SignatureAndDoc,
        LodLevel::SlicedBody,
        LodLevel::FullBody,
    ];

    let in_container = sym.container_name.is_some();
    let mut last_cost = 0;
    for &lvl in &levels {
        let cost = if framing_aware {
            ContextFormatter::rendered_symbol_tokens(sym, lvl, file_source, in_container, model).max(1)
        } else {
            let text = ContextFormatter::render_symbol(sym, lvl, file_source);
            count_tokens(&text, model).max(1)
        };
        let multiplier = weights.multiplier(lvl);
        raw_options.push(LodOption {
            level: Some(lvl),
            cost: cost.max(last_cost),
            utility_multiplier: multiplier,
        });
        last_cost = cost.max(last_cost);
    }

    // Pass 1: Deduplicate / monotonic filter
    // If two options have identical cost, keep the one with higher utility multiplier.
    let mut monotonic: Vec<LodOption> = Vec::new();
    for opt in raw_options {
        if let Some(last) = monotonic.last_mut() {
            if opt.cost == last.cost {
                if opt.utility_multiplier > last.utility_multiplier {
                    *last = opt;
                }
            } else if opt.cost > last.cost && opt.utility_multiplier > last.utility_multiplier {
                monotonic.push(opt);
            }
        } else {
            monotonic.push(opt);
        }
    }

    // Pass 2: Upper convex hull filter (LP dominance)
    // For three consecutive points A, B, C: slope(A, B) must be strictly greater than slope(B, C).
    // If slope(A, B) <= slope(B, C), point B is dominated and pruned.
    let mut hull: Vec<LodOption> = Vec::with_capacity(monotonic.len());
    for opt in monotonic {
        while hull.len() >= 2 {
            let n = hull.len();
            let a = &hull[n - 2];
            let b = &hull[n - 1];
            let c = &opt;

            let slope_ab =
                (b.utility_multiplier - a.utility_multiplier) / ((b.cost - a.cost) as f32);
            let slope_bc =
                (c.utility_multiplier - b.utility_multiplier) / ((c.cost - b.cost) as f32);

            if slope_ab <= slope_bc {
                hull.pop();
            } else {
                break;
            }
        }
        hull.push(opt);
    }

    hull
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

/// Element stored in the MCKP upgrade max-priority queue.
#[derive(Debug, Clone)]
struct MckpItem {
    symbol_id: SymbolId,
    current_level_idx: usize,
    marginal_density: f32,
    last_iteration: usize,
}

impl PartialEq for MckpItem {
    fn eq(&self, other: &Self) -> bool {
        self.marginal_density == other.marginal_density
    }
}

impl Eq for MckpItem {}

impl PartialOrd for MckpItem {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for MckpItem {
    fn cmp(&self, other: &Self) -> Ordering {
        self.marginal_density
            .partial_cmp(&other.marginal_density)
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

/// Pair of symbols (one selected in budget, one unselected) whose cost-normalized
/// marginal utility intervals overlap under theoretical ACL PageRank approximation error.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BorderlinePair {
    /// Identifier of the selected symbol.
    pub selected_symbol: SymbolId,
    /// Identifier of the unselected rival symbol.
    pub unselected_symbol: SymbolId,
    /// Name of the selected symbol.
    pub selected_name: String,
    /// Name of the unselected rival symbol.
    pub unselected_name: String,
    /// Lower bound on marginal utility per token for the selected symbol: $\Delta_{\min}(u) / c(u)$.
    pub selected_density_min: f32,
    /// Upper bound on marginal utility per token for the unselected rival: $\Delta_{\max}(v) / c(v)$.
    pub unselected_density_max: f32,
    /// Overlap magnitude: $\Delta_{\max}(v)/c(v) - \Delta_{\min}(u)/c(u)$.
    pub overlap: f32,
}

/// Numerical stability and sensitivity analysis of the CELF knapsack selection
/// under Andersen-Chung-Lang (ACL) PageRank approximation errors.
///
/// # Mathematical Formulation (Andersen et al., 2006; Leskovec et al., 2007)
/// Because ACL PageRank diffusion computes a monotone lower bound on stationary PageRank,
/// each symbol's true marginal gain $\Delta^*(v \mid S)$ is bounded within $[\Delta_{\min}(v), \Delta_{\max}(v)]$.
/// If for selected $u \in S$ and unselected $v \notin S$, $\Delta_{\max}(v)/c(v) \le \Delta_{\min}(u)/c(u)$,
/// then $u$'s selection over $v$ is unconditionally invariant to worst-case numerical approximation error.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SensitivityReport {
    /// Push threshold epsilon used by the ACL solver.
    pub epsilon: f32,
    /// Teleportation damping factor alpha used by the ACL solver.
    pub alpha: f32,
    /// Maximum theoretical PageRank error bound across all evaluated candidate symbols.
    pub max_error_bound: f32,
    /// Mean theoretical PageRank error bound across all evaluated candidate symbols.
    pub mean_error_bound: f32,
    /// Total number of candidate symbols considered for knapsack selection.
    pub total_candidates: usize,
    /// Total number of symbols selected within the token budget.
    pub selected_count: usize,
    /// Number of selected symbols that have zero borderline rival overlaps (unconditionally stable).
    pub stable_count: usize,
    /// Knapsack Stability Index in [0.0, 1.0]: ratio of unconditionally stable symbols to selected symbols.
    pub stability_index: f32,
    /// Detected borderline candidate pairs ranked by overlap severity in descending order.
    pub borderline_pairs: Vec<BorderlinePair>,
}

/// Cost-Effective Lazy Forward (CELF) submodular knapsack optimizer.
///
/// Selects an optimal subset of code symbols $S \subseteq V$ maximizing information coverage
/// and module diversity subject to a strict token budget $\sum_{v \in S} c(v) \le B$.
///
/// # Theoretical Guarantees & Academic Citations
///
/// - **Cardinality vs. Knapsack Constraints**:
///   Nemhauser, Wolsey & Fisher (1978) proved that standard greedy achieves a $(1 - 1/e) \approx 0.632$
///   approximation ratio for monotone submodular maximization under a uniform cardinality constraint ($|S| \le k$).
///   However, under general knapsack constraints ($\sum_{v \in S} c(v) \le B$ with variable token costs),
///   unaugmented density greedy has an unbounded worst-case approximation factor because high-cost, high-value
///   symbols can be permanently blocked by slightly denser runs of cheap symbols.
///
/// - **Best-Singleton Knapsack Correction (Khuller et al., 1999; Sviridenko, 2004)**:
///   By computing both the density-ordered greedy solution $S_{\text{greedy}}$ and the best feasible singleton
///   $v^* = \arg\max_{v \in V: c(v) \le B} f(\{v\})$, and returning $\arg\max \{ f(S_{\text{greedy}}), f(\{v^*\}) \}$,
///   the optimizer achieves a provable $\frac{1}{2}(1 - 1/e) \approx 0.316$ approximation guarantee in $O(|V| \log |V|)$ time.
///
/// - **Threshold-Based Greedy (Badanidiyuru & Vondrák, 2014)**:
///   Optionally, by sweeping geometrically decaying marginal density thresholds $\tau$, threshold greedy achieves
///   a $(1 - 1/e - \varepsilon)$ approximation guarantee in $O\left(\frac{|V|}{\varepsilon} \log \frac{|V|}{\varepsilon}\right)$ time.
///
/// Internal context bundle passed during MCKP lazy marginal gain computations.
struct MckpContext<'a> {
    graph: &'a MultiplexGraph,
    ppr_scores: &'a HashMap<SymbolId, f32>,
    covered_nodes: &'a HashSet<SymbolId>,
    file_token_costs: &'a HashMap<PathBuf, usize>,
}

/// - **Lazy Forward Evaluations (Leskovec et al., 2007)**:
///   Leverages submodular diminishing returns to avoid recomputing marginal gains for elements whose upper bounds
///   remain below the current queue maximum, executing in $<2\text{ms}$ on multi-thousand symbol graphs.
#[derive(Debug, Clone, Default)]
pub struct CelfOptimizer {
    config: CelfConfig,
}

impl CelfOptimizer {
    /// Creates a new `CelfOptimizer` with custom configuration.
    pub fn new(config: CelfConfig) -> Self {
        Self { config }
    }

    /// Sets the tokenizer model for structural framing and cost calculations.
    pub fn with_tokenizer(mut self, model: TokenizerModel) -> Self {
        self.config.tokenizer_model = model;
        self
    }

    /// Sets whether the optimizer should account for structural Markdown framing.
    pub fn with_framing(mut self, framing_aware: bool) -> Self {
        self.config.framing_aware = framing_aware;
        self
    }

    /// Computes the incremental token cost of admitting a symbol into context,
    /// accounting for symbol framing, file framing (if file not yet opened), and
    /// container framing (if container not yet opened) when framing_aware is true.
    fn symbol_incremental_cost(
        &self,
        sym: &SymbolNode,
        opened_files: &HashSet<PathBuf>,
        opened_containers: &HashSet<(PathBuf, String)>,
        model: TokenizerModel,
    ) -> usize {
        if !self.config.framing_aware {
            return sym.token_cost.max(1);
        }

        let in_container = sym.container_name.is_some();
        let mut cost = sym.token_cost.max(1);
        cost += ContextFormatter::symbol_framing_tokens(sym, in_container, model);

        if !opened_files.contains(&sym.file_path) {
            let lang_tag = ContextFormatter::language_tag_for_path(&sym.file_path);
            cost += ContextFormatter::file_framing_tokens(&sym.file_path, lang_tag, model);
        }

        if let Some(ref c_name) = sym.container_name {
            if !opened_containers.contains(&(sym.file_path.clone(), c_name.clone())) {
                let lang_tag = ContextFormatter::language_tag_for_path(&sym.file_path);
                cost += ContextFormatter::container_framing_tokens(
                    lang_tag,
                    c_name,
                    sym.trait_name.as_deref(),
                    model,
                );
            }
        }

        cost
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

        // If threshold greedy is configured, execute Badanidiyuru & Vondrák (2014)
        if let Some(eps) = self.config.threshold_epsilon {
            return self.optimize_threshold_greedy(graph, ppr_scores, budget, eps);
        }

        let mut selected_set: HashSet<SymbolId> = HashSet::new();
        let mut selected_list: Vec<SymbolId> = Vec::new();
        let mut trace: Vec<CelfTraceStep> = Vec::new();
        let mut covered_nodes: HashSet<SymbolId> = HashSet::new();
        let mut file_token_costs: HashMap<PathBuf, usize> = HashMap::new();
        let mut opened_files: HashSet<PathBuf> = HashSet::new();
        let mut opened_containers: HashSet<(PathBuf, String)> = HashSet::new();
        let model = self.config.tokenizer_model;
        let mut current_tokens: usize = 0;
        let mut cumulative_utility: f32 = 0.0;
        let mut current_iteration: usize = 0;

        let mut heap: BinaryHeap<CelfItem> = BinaryHeap::new();
        let mut best_singleton: Option<(SymbolId, usize, f32)> = None;

        // Initialize candidate pool from graph symbols
        for sym in graph.symbols() {
            let score = ppr_scores.get(&sym.id).copied().unwrap_or(0.0);
            if score < self.config.min_relevance_threshold && !ppr_scores.is_empty() {
                continue;
            }

            let initial_cost = self.symbol_incremental_cost(sym, &opened_files, &opened_containers, model);
            if initial_cost > budget {
                continue;
            }

            let initial_marginal_gain = self.compute_marginal_gain(
                sym.id,
                graph,
                ppr_scores,
                &covered_nodes,
                &file_token_costs,
            );

            // Track best singleton fitting budget: v* = argmax_{v: c(v) <= B} f({v})
            match &best_singleton {
                Some((_, _, best_val)) if *best_val >= initial_marginal_gain => {}
                _ => {
                    best_singleton = Some((sym.id, initial_cost, initial_marginal_gain));
                }
            }

            heap.push(CelfItem {
                symbol_id: sym.id,
                marginal_gain_per_token: initial_marginal_gain / (initial_cost as f32),
                last_iteration: 0,
            });
        }

        // CELF lazy evaluation loop
        while let Some(mut top) = heap.pop() {
            let sym = match graph.symbol(top.symbol_id) {
                Some(s) => s,
                None => continue,
            };

            let cost = self.symbol_incremental_cost(sym, &opened_files, &opened_containers, model);

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
                opened_files.insert(sym.file_path.clone());
                if let Some(ref c_name) = sym.container_name {
                    opened_containers.insert((sym.file_path.clone(), c_name.clone()));
                }

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

        // Khuller-Moss-Naor (1999) & Sviridenko (2004) knapsack guarantee correction:
        // Compare greedy solution against best feasible singleton. If the best singleton
        // outperforms the cumulative greedy solution, return max(Greedy, Singleton).
        if let Some((best_id, best_cost, best_val)) = best_singleton {
            if best_val > cumulative_utility {
                let singleton_step = CelfTraceStep {
                    symbol_id: best_id,
                    symbol_cost: best_cost,
                    cumulative_tokens: best_cost,
                    marginal_gain: best_val,
                    cumulative_utility: best_val,
                };
                return (vec![best_id], vec![singleton_step]);
            }
        }

        (selected_list, trace)
    }

    /// Threshold-based greedy submodular knapsack solver (Badanidiyuru & Vondrák, 2014).
    ///
    /// Achieves a $(1 - 1/e - \varepsilon)$ approximation guarantee in $O\left(\frac{|V|}{\varepsilon} \log \frac{|V|}{\varepsilon}\right)$ time.
    fn optimize_threshold_greedy(
        &self,
        graph: &MultiplexGraph,
        ppr_scores: &HashMap<SymbolId, f32>,
        budget: usize,
        epsilon: f32,
    ) -> (Vec<SymbolId>, Vec<CelfTraceStep>) {
        let eps = epsilon.clamp(0.01, 0.5);
        let empty_covered = HashSet::new();
        let empty_file_costs = HashMap::new();

        let mut best_singleton: Option<(SymbolId, usize, f32)> = None;
        let mut candidates = Vec::new();
        let mut opened_files: HashSet<PathBuf> = HashSet::new();
        let mut opened_containers: HashSet<(PathBuf, String)> = HashSet::new();
        let model = self.config.tokenizer_model;

        for sym in graph.symbols() {
            let score = ppr_scores.get(&sym.id).copied().unwrap_or(0.0);
            if score < self.config.min_relevance_threshold && !ppr_scores.is_empty() {
                continue;
            }
            let initial_cost = self.symbol_incremental_cost(sym, &opened_files, &opened_containers, model);
            if initial_cost > budget {
                continue;
            }
            let val = self.compute_marginal_gain(
                sym.id,
                graph,
                ppr_scores,
                &empty_covered,
                &empty_file_costs,
            );
            if val > 0.0 {
                candidates.push((sym.id, initial_cost));
                match &best_singleton {
                    Some((_, _, best_val)) if *best_val >= val => {}
                    _ => best_singleton = Some((sym.id, initial_cost, val)),
                }
            }
        }

        let m_val = match best_singleton {
            Some((_, _, val)) if val > 0.0 => val,
            _ => return (Vec::new(), Vec::new()),
        };

        let mut selected_set: HashSet<SymbolId> = HashSet::new();
        let mut selected_list: Vec<SymbolId> = Vec::new();
        let mut trace: Vec<CelfTraceStep> = Vec::new();
        let mut covered_nodes: HashSet<SymbolId> = HashSet::new();
        let mut file_token_costs: HashMap<PathBuf, usize> = HashMap::new();
        let mut current_tokens: usize = 0;
        let mut cumulative_utility: f32 = 0.0;

        let min_tau = (eps / (budget as f32).max(1.0)) * m_val;
        let mut tau = m_val;

        while tau >= min_tau && current_tokens < budget {
            for &(sym_id, _) in &candidates {
                if selected_set.contains(&sym_id) {
                    continue;
                }
                let sym = match graph.symbol(sym_id) {
                    Some(s) => s,
                    None => continue,
                };
                let cost = self.symbol_incremental_cost(sym, &opened_files, &opened_containers, model);
                if current_tokens + cost > budget {
                    continue;
                }
                let marginal_gain = self.compute_marginal_gain(
                    sym_id,
                    graph,
                    ppr_scores,
                    &covered_nodes,
                    &file_token_costs,
                );
                let density = marginal_gain / (cost as f32);
                if density >= tau {
                    selected_set.insert(sym_id);
                    selected_list.push(sym_id);
                    current_tokens += cost;
                    cumulative_utility += marginal_gain;
                    opened_files.insert(sym.file_path.clone());
                    if let Some(ref c_name) = sym.container_name {
                        opened_containers.insert((sym.file_path.clone(), c_name.clone()));
                    }

                    trace.push(CelfTraceStep {
                        symbol_id: sym_id,
                        symbol_cost: cost,
                        cumulative_tokens: current_tokens,
                        marginal_gain,
                        cumulative_utility,
                    });

                    covered_nodes.insert(sym_id);
                    for &neighbor in graph.neighbors(sym_id) {
                        covered_nodes.insert(SymbolId(neighbor));
                    }
                    if let Some(sym) = graph.symbol(sym_id) {
                        *file_token_costs.entry(sym.file_path.clone()).or_default() += cost;
                    }

                    if current_tokens >= budget {
                        break;
                    }
                }
            }
            tau *= 1.0 - eps;
        }

        // Compare threshold greedy against best singleton
        if let Some((best_id, best_cost, best_val)) = best_singleton {
            if best_val > cumulative_utility {
                let singleton_step = CelfTraceStep {
                    symbol_id: best_id,
                    symbol_cost: best_cost,
                    cumulative_tokens: best_cost,
                    marginal_gain: best_val,
                    cumulative_utility: best_val,
                };
                return (vec![best_id], vec![singleton_step]);
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

    /// Evaluates the numerical stability and sensitivity of the selected knapsack set
    /// against worst-case Andersen-Chung-Lang (ACL) PageRank approximation errors.
    ///
    /// Computes marginal density uncertainty intervals $[\Delta_{\min}(v)/c(v), \Delta_{\max}(v)/c(v)]$,
    /// identifies borderline candidate pairs whose selection could invert under numerical perturbation,
    /// and calculates the Knapsack Stability Index $\in [0.0, 1.0]$.
    pub fn analyze_sensitivity(
        &self,
        graph: &MultiplexGraph,
        ppr_result: &PprResult,
        selected_ids: &[SymbolId],
        budget: usize,
        alpha: f32,
        epsilon: f32,
    ) -> SensitivityReport {
        let max_err = ppr_result
            .error_bounds
            .values()
            .copied()
            .fold(0.0_f32, f32::max);
        let mean_err = if ppr_result.error_bounds.is_empty() {
            0.0
        } else {
            ppr_result.error_bounds.values().sum::<f32>() / ppr_result.error_bounds.len() as f32
        };

        if selected_ids.is_empty() || graph.is_empty() {
            return SensitivityReport {
                epsilon,
                alpha,
                max_error_bound: max_err,
                mean_error_bound: mean_err,
                total_candidates: 0,
                selected_count: 0,
                stable_count: 0,
                stability_index: 1.0,
                borderline_pairs: Vec::new(),
            };
        }

        let selected_set: HashSet<SymbolId> = selected_ids.iter().copied().collect();

        // 1. Compute marginal density intervals for selected items u in S
        // Evaluate u's marginal contribution with respect to S \ {u}
        struct SelectedInfo {
            id: SymbolId,
            name: String,
            density_min: f32,
        }

        let mut selected_info = Vec::with_capacity(selected_ids.len());

        for &u in selected_ids {
            let sym_u = match graph.symbol(u) {
                Some(s) => s,
                None => continue,
            };

            // Build covered set and file token costs for S \ {u}
            let mut covered_minus_u = HashSet::new();
            let mut file_costs_minus_u = HashMap::new();

            for &other_id in selected_ids {
                if other_id == u {
                    continue;
                }
                covered_minus_u.insert(other_id);
                for &neighbor in graph.neighbors(other_id) {
                    covered_minus_u.insert(SymbolId(neighbor));
                }
                if let Some(other_sym) = graph.symbol(other_id) {
                    let cost = other_sym.token_cost.max(1);
                    *file_costs_minus_u
                        .entry(other_sym.file_path.clone())
                        .or_default() += cost;
                }
            }

            let min_gain = self.compute_marginal_gain(
                u,
                graph,
                &ppr_result.scores,
                &covered_minus_u,
                &file_costs_minus_u,
            );

            let cost_u = sym_u.token_cost.max(1) as f32;
            let density_min = min_gain / cost_u;

            selected_info.push(SelectedInfo {
                id: u,
                name: sym_u.name.clone(),
                density_min,
            });
        }

        // 2. Build full covered set and file token costs for S
        let mut covered_s = HashSet::new();
        let mut file_costs_s = HashMap::new();
        for &s_id in selected_ids {
            covered_s.insert(s_id);
            for &neighbor in graph.neighbors(s_id) {
                covered_s.insert(SymbolId(neighbor));
            }
            if let Some(s_sym) = graph.symbol(s_id) {
                let cost = s_sym.token_cost.max(1);
                *file_costs_s.entry(s_sym.file_path.clone()).or_default() += cost;
            }
        }

        // 3. Compute marginal density intervals for unselected candidates v in V \ S
        struct UnselectedInfo {
            id: SymbolId,
            name: String,
            density_max: f32,
        }

        let mut unselected_info = Vec::new();
        let mut total_candidates = selected_ids.len();

        for sym in graph.symbols() {
            if selected_set.contains(&sym.id) {
                continue;
            }

            let score = ppr_result.scores.get(&sym.id).copied().unwrap_or(0.0);
            let err_bound = ppr_result.error_bounds.get(&sym.id).copied().unwrap_or(0.0);

            // Candidate filtering matching knapsack pool
            if (score < self.config.min_relevance_threshold && err_bound == 0.0)
                && !ppr_result.scores.is_empty()
            {
                continue;
            }

            let cost = sym.token_cost.max(1);
            if cost > budget {
                continue;
            }

            total_candidates += 1;

            let min_gain = self.compute_marginal_gain(
                sym.id,
                graph,
                &ppr_result.scores,
                &covered_s,
                &file_costs_s,
            );

            let mut neighbor_err = 0.0_f32;
            for &neighbor in graph.neighbors(sym.id) {
                let n_id = SymbolId(neighbor);
                if !covered_s.contains(&n_id) {
                    neighbor_err += self.config.neighbor_coverage_weight
                        * ppr_result.error_bounds.get(&n_id).copied().unwrap_or(0.0);
                }
            }

            let max_gain = min_gain + err_bound + neighbor_err;
            let density_max = max_gain / (cost as f32);

            unselected_info.push(UnselectedInfo {
                id: sym.id,
                name: sym.name.clone(),
                density_max,
            });
        }

        // 4. Cross-evaluate borderline rivals and compute stability index
        let mut borderline_pairs = Vec::new();
        let mut stable_count = 0;

        for sel in &selected_info {
            let mut sel_stable = true;
            for unsel in &unselected_info {
                if unsel.density_max > sel.density_min {
                    sel_stable = false;
                    borderline_pairs.push(BorderlinePair {
                        selected_symbol: sel.id,
                        unselected_symbol: unsel.id,
                        selected_name: sel.name.clone(),
                        unselected_name: unsel.name.clone(),
                        selected_density_min: sel.density_min,
                        unselected_density_max: unsel.density_max,
                        overlap: unsel.density_max - sel.density_min,
                    });
                }
            }
            if sel_stable {
                stable_count += 1;
            }
        }

        let stability_index = if selected_ids.is_empty() {
            1.0
        } else {
            stable_count as f32 / selected_ids.len() as f32
        };

        // Sort borderline pairs descending by overlap magnitude
        borderline_pairs
            .sort_by(|a, b| b.overlap.partial_cmp(&a.overlap).unwrap_or(Ordering::Equal));

        // Cap to top 25 to prevent memory explosion
        borderline_pairs.truncate(25);

        SensitivityReport {
            epsilon,
            alpha,
            max_error_bound: max_err,
            mean_error_bound: mean_err,
            total_candidates,
            selected_count: selected_ids.len(),
            stable_count,
            stability_index,
            borderline_pairs,
        }
    }

    /// Evaluates the incremental marginal gain $\Delta(v, j \to j+1 \mid S)$ of upgrading symbol `id`
    /// from frontier state $j$ to state $j+1$.
    fn compute_mckp_marginal_gain(
        &self,
        id: SymbolId,
        current_state_idx: usize,
        ctx: &MckpContext<'_>,
        frontier: &[LodOption],
    ) -> f32 {
        let sym = match ctx.graph.symbol(id) {
            Some(s) => s,
            None => return 0.0,
        };

        let cur_opt = &frontier[current_state_idx];
        let next_opt = &frontier[current_state_idx + 1];

        let delta_cost = next_opt.cost.saturating_sub(cur_opt.cost).max(1);
        let delta_mult = (next_opt.utility_multiplier - cur_opt.utility_multiplier).max(0.0);

        // 1. Direct relevance gain proportional to delta_mult
        let ppr_score = ctx.ppr_scores.get(&id).copied().unwrap_or(0.01);
        let direct_gain = delta_mult * ppr_score;

        // 2. 1-hop neighbor coverage gain (only awarded upon initial admission from None -> level)
        let mut neighbor_gain = 0.0_f32;
        if current_state_idx == 0 {
            let neighbors = ctx.graph.neighbors(id);
            for &n in neighbors {
                let neighbor_id = SymbolId(n);
                if !ctx.covered_nodes.contains(&neighbor_id) {
                    let n_score = ctx.ppr_scores.get(&neighbor_id).copied().unwrap_or(0.005);
                    neighbor_gain += self.config.neighbor_coverage_weight * n_score;
                }
            }
        }

        // 3. File diversity gain: log(1 + delta_cost / (1 + current_file_cost))
        let current_file_cost = ctx
            .file_token_costs
            .get(&sym.file_path)
            .copied()
            .unwrap_or(0) as f32;
        let diversity_gain = ((1.0 + (delta_cost as f32 / (1.0 + current_file_cost))).ln())
            * self.config.lambda_diversity;

        direct_gain + neighbor_gain + diversity_gain
    }

    /// Solves the joint Symbol Selection and Level-of-Detail (LOD) assignment problem
    /// as a Multiple-Choice Knapsack Problem (MCKP) with submodular objective (Kellerer et al., 2004; Dyer, 1984).
    ///
    /// Guarantees a $\frac{1}{2}(1 - 1/e)$ approximation factor via best-singleton correction
    /// (Khuller et al., 1999; Sviridenko, 2004) while eliminating stranded budget and
    /// maximizing semantic information density.
    pub fn optimize_mckp(
        &self,
        graph: &MultiplexGraph,
        ppr_scores: &HashMap<SymbolId, f32>,
        budget: usize,
        file_sources: &HashMap<PathBuf, String>,
        tokenizer_model: TokenizerModel,
    ) -> MckpResult {
        if budget == 0 || graph.is_empty() {
            return MckpResult {
                selected_lods: HashMap::new(),
                total_tokens: 0,
                cumulative_utility: 0.0,
                trace: Vec::new(),
            };
        }

        // 1. Build Pareto-efficient LOD frontiers for each candidate symbol
        let mut symbol_frontiers: HashMap<SymbolId, Vec<LodOption>> = HashMap::new();
        let mut current_state: HashMap<SymbolId, usize> = HashMap::new();
        let mut best_singleton: Option<(SymbolId, LodLevel, usize, f32)> = None;

        for sym in graph.symbols() {
            let score = ppr_scores.get(&sym.id).copied().unwrap_or(0.0);
            if score < self.config.min_relevance_threshold && !ppr_scores.is_empty() {
                continue;
            }

            let file_src = file_sources.get(&sym.file_path).map(|s| s.as_str());
            let frontier = build_pareto_frontier(
                sym,
                file_src,
                tokenizer_model,
                self.config.lod_weights,
                self.config.framing_aware,
            );

            let singleton_framing = if self.config.framing_aware {
                let lang_tag = ContextFormatter::language_tag_for_path(&sym.file_path);
                let mut f = ContextFormatter::file_framing_tokens(&sym.file_path, lang_tag, tokenizer_model);
                if let Some(ref c_name) = sym.container_name {
                    f += ContextFormatter::container_framing_tokens(
                        lang_tag,
                        c_name,
                        sym.trait_name.as_deref(),
                        tokenizer_model,
                    );
                }
                f
            } else {
                0
            };

            // If even the minimum option exceeds budget, symbol cannot fit
            if frontier.len() < 2 || frontier[1].cost + singleton_framing > budget {
                continue;
            }

            // Evaluate singleton candidate choices against empty set for Khuller-Sviridenko guarantee
            for opt in &frontier[1..] {
                let total_opt_cost = opt.cost + singleton_framing;
                if total_opt_cost <= budget {
                    if let Some(level) = opt.level {
                        let direct = opt.utility_multiplier * score;
                        let mut neighbor = 0.0_f32;
                        for &n in graph.neighbors(sym.id) {
                            let n_score = ppr_scores.get(&SymbolId(n)).copied().unwrap_or(0.005);
                            neighbor += self.config.neighbor_coverage_weight * n_score;
                        }
                        let diversity =
                            ((1.0 + (total_opt_cost as f32)).ln()) * self.config.lambda_diversity;
                        let standalone_util = direct + neighbor + diversity;

                        match &best_singleton {
                            Some((_, _, _, best_val)) if *best_val >= standalone_util => {}
                            _ => {
                                best_singleton = Some((sym.id, level, total_opt_cost, standalone_util));
                            }
                        }
                    }
                }
            }

            current_state.insert(sym.id, 0);
            symbol_frontiers.insert(sym.id, frontier);
        }

        // 2. Initialize CELF priority queue with initial upgrade (None -> P_1)
        let mut heap: BinaryHeap<MckpItem> = BinaryHeap::new();
        let mut covered_nodes: HashSet<SymbolId> = HashSet::new();
        let mut file_token_costs: HashMap<PathBuf, usize> = HashMap::new();

        let ctx = MckpContext {
            graph,
            ppr_scores,
            covered_nodes: &covered_nodes,
            file_token_costs: &file_token_costs,
        };

        for (&sym_id, frontier) in &symbol_frontiers {
            let delta_c = frontier[1].cost - frontier[0].cost;
            let initial_gain = self.compute_mckp_marginal_gain(sym_id, 0, &ctx, frontier);
            let density = initial_gain / (delta_c as f32);
            heap.push(MckpItem {
                symbol_id: sym_id,
                current_level_idx: 0,
                marginal_density: density,
                last_iteration: 0,
            });
        }

        let mut current_tokens: usize = 0;
        let mut cumulative_utility: f32 = 0.0;
        let mut current_iteration: usize = 0;
        let mut trace: Vec<MckpTraceStep> = Vec::new();
        let mut opened_files: HashSet<PathBuf> = HashSet::new();
        let mut opened_containers: HashSet<(PathBuf, String)> = HashSet::new();

        // 3. CELF Lazy Evaluation Loop
        while let Some(mut top) = heap.pop() {
            let frontier = match symbol_frontiers.get(&top.symbol_id) {
                Some(f) => f,
                None => continue,
            };
            let j = match current_state.get(&top.symbol_id) {
                Some(&idx) => idx,
                None => continue,
            };

            // Discard stale heap entries
            if top.current_level_idx != j || j + 1 >= frontier.len() {
                continue;
            }

            let sym = match graph.symbol(top.symbol_id) {
                Some(s) => s,
                None => continue,
            };

            let next_option = &frontier[j + 1];
            let mut delta_c = next_option.cost - frontier[j].cost;
            if self.config.framing_aware && j == 0 {
                if !opened_files.contains(&sym.file_path) {
                    let lang_tag = ContextFormatter::language_tag_for_path(&sym.file_path);
                    delta_c += ContextFormatter::file_framing_tokens(&sym.file_path, lang_tag, tokenizer_model);
                }
                if let Some(ref c_name) = sym.container_name {
                    if !opened_containers.contains(&(sym.file_path.clone(), c_name.clone())) {
                        let lang_tag = ContextFormatter::language_tag_for_path(&sym.file_path);
                        delta_c += ContextFormatter::container_framing_tokens(
                            lang_tag,
                            c_name,
                            sym.trait_name.as_deref(),
                            tokenizer_model,
                        );
                    }
                }
            }

            // Skip if this upgrade exceeds remaining budget
            if current_tokens + delta_c > budget {
                continue;
            }

            // If evaluated in current iteration, accept by submodularity
            if top.last_iteration == current_iteration {
                current_tokens += delta_c;
                let delta_gain = (top.marginal_density * (delta_c as f32)).max(0.0);
                cumulative_utility += delta_gain;
                *current_state.get_mut(&top.symbol_id).unwrap() = j + 1;

                if let Some(sym) = graph.symbol(top.symbol_id) {
                    *file_token_costs.entry(sym.file_path.clone()).or_default() += delta_c;
                }

                if j == 0 {
                    opened_files.insert(sym.file_path.clone());
                    if let Some(ref c_name) = sym.container_name {
                        opened_containers.insert((sym.file_path.clone(), c_name.clone()));
                    }
                    covered_nodes.insert(top.symbol_id);
                    for &n in graph.neighbors(top.symbol_id) {
                        covered_nodes.insert(SymbolId(n));
                    }
                }

                let from_level = frontier[j].level;
                let to_level = next_option.level.unwrap();

                trace.push(MckpTraceStep {
                    symbol_id: top.symbol_id,
                    from_level,
                    to_level,
                    incremental_cost: delta_c,
                    cumulative_tokens: current_tokens,
                    marginal_gain: delta_gain,
                    cumulative_utility,
                });

                current_iteration += 1;

                // Push next available upgrade for this symbol if one exists on Pareto frontier
                if j + 2 < frontier.len() {
                    let next_next = &frontier[j + 2];
                    let next_delta_c = next_next.cost - next_option.cost;
                    let current_ctx = MckpContext {
                        graph,
                        ppr_scores,
                        covered_nodes: &covered_nodes,
                        file_token_costs: &file_token_costs,
                    };
                    let next_gain = self.compute_mckp_marginal_gain(
                        top.symbol_id,
                        j + 1,
                        &current_ctx,
                        frontier,
                    );
                    let next_density = next_gain / (next_delta_c as f32);
                    heap.push(MckpItem {
                        symbol_id: top.symbol_id,
                        current_level_idx: j + 1,
                        marginal_density: next_density,
                        last_iteration: current_iteration,
                    });
                }
            } else {
                // Lazy re-evaluation
                let current_ctx = MckpContext {
                    graph,
                    ppr_scores,
                    covered_nodes: &covered_nodes,
                    file_token_costs: &file_token_costs,
                };
                let current_gain =
                    self.compute_mckp_marginal_gain(top.symbol_id, j, &current_ctx, frontier);
                let current_density = current_gain / (delta_c as f32);
                top.marginal_density = current_density;
                top.last_iteration = current_iteration;
                heap.push(top);
            }
        }

        // 4. Khuller et al. (1999) / Sviridenko (2004) Best-Singleton Correction
        if let Some((best_sym, best_lod, best_cost, best_util)) = best_singleton {
            if best_util > cumulative_utility && best_cost <= budget {
                let mut selected_lods = HashMap::new();
                selected_lods.insert(best_sym, best_lod);
                return MckpResult {
                    selected_lods,
                    total_tokens: best_cost,
                    cumulative_utility: best_util,
                    trace: vec![MckpTraceStep {
                        symbol_id: best_sym,
                        from_level: None,
                        to_level: best_lod,
                        incremental_cost: best_cost,
                        cumulative_tokens: best_cost,
                        marginal_gain: best_util,
                        cumulative_utility: best_util,
                    }],
                };
            }
        }

        // 5. Collect selected LOD levels
        let mut selected_lods = HashMap::new();
        for (&id, &state_idx) in &current_state {
            if state_idx > 0 {
                if let Some(level) = symbol_frontiers[&id][state_idx].level {
                    selected_lods.insert(id, level);
                }
            }
        }

        MckpResult {
            selected_lods,
            total_tokens: current_tokens,
            cumulative_utility,
            trace,
        }
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
            threshold_epsilon: None,
            lod_weights: LodWeights::default(),
            ..Default::default()
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

    #[test]
    fn test_knapsack_adversarial_singleton_beats_greedy() {
        // Canonical knapsack adversarial counterexample (Khuller et al., 1999):
        // Item 0 has higher density but lower total value, exhausting remaining budget.
        // Item 1 has slightly lower density, fits alone, and has much higher total value.
        let s0 = make_test_node(0, "cheap_dense", "a.rs", 6);
        let s1 = make_test_node(1, "valuable_singleton", "b.rs", 10);
        let s2 = make_test_node(2, "filler", "c.rs", 5);

        let graph = MultiplexGraph::build(vec![s0, s1, s2], &[], LayerWeights::default());

        let mut ppr_scores = HashMap::new();
        // Item 0: density 0.50 / 6 = 0.0833
        ppr_scores.insert(SymbolId(0), 0.50);
        // Item 1: density 0.70 / 10 = 0.0700
        ppr_scores.insert(SymbolId(1), 0.70);
        // Item 2: density 0.20 / 5 = 0.0400
        ppr_scores.insert(SymbolId(2), 0.20);

        // Turn off diversity penalty to isolate knapsack packing mechanics
        let optimizer = CelfOptimizer::new(CelfConfig {
            lambda_diversity: 0.0,
            neighbor_coverage_weight: 0.0,
            min_relevance_threshold: 0.0,
            threshold_epsilon: None,
            lod_weights: LodWeights::default(),
            ..Default::default()
        });

        // Budget = 10.
        // Plain density greedy would take s0 (cost 6, utility 0.50), leaving 4 tokens.
        // Neither s1 (10) nor s2 (5) fits in 4 tokens. Greedy result = {s0} (utility 0.50).
        // Best singleton is s1 (cost 10, utility 0.70).
        // Best-singleton correction ensures {s1} is returned!
        let (selected, trace) = optimizer.optimize_with_trace(&graph, &ppr_scores, 10);

        assert_eq!(selected, vec![SymbolId(1)]);
        assert_eq!(trace.len(), 1);
        assert_eq!(trace[0].symbol_id, SymbolId(1));
        assert_eq!(trace[0].symbol_cost, 10);
        assert!((trace[0].cumulative_utility - 0.70).abs() < 1e-4);
    }

    #[test]
    fn test_celf_threshold_greedy() {
        let s0 = make_test_node(0, "f0", "a.rs", 10);
        let s1 = make_test_node(1, "f1", "a.rs", 15);
        let s2 = make_test_node(2, "f2", "b.rs", 20);

        let graph = MultiplexGraph::build(vec![s0, s1, s2], &[], LayerWeights::default());

        let mut ppr_scores = HashMap::new();
        ppr_scores.insert(SymbolId(0), 0.6);
        ppr_scores.insert(SymbolId(1), 0.4);
        ppr_scores.insert(SymbolId(2), 0.2);

        let optimizer = CelfOptimizer::new(CelfConfig {
            lambda_diversity: 0.2,
            neighbor_coverage_weight: 0.0,
            min_relevance_threshold: 0.0,
            threshold_epsilon: Some(0.1),
            lod_weights: LodWeights::default(),
            ..Default::default()
        });

        let (selected, trace) = optimizer.optimize_with_trace(&graph, &ppr_scores, 30);
        assert!(!selected.is_empty());
        assert!(!trace.is_empty());
        let total_cost: usize = selected
            .iter()
            .map(|&id| graph.symbol(id).unwrap().token_cost)
            .sum();
        assert!(total_cost <= 30);
        assert!(selected.contains(&SymbolId(0)));
    }

    #[test]
    fn test_celf_sensitivity_analysis_stable() {
        let s0 = make_test_node(0, "f0", "a.rs", 10);
        let s1 = make_test_node(1, "f1", "a.rs", 10);
        let s2 = make_test_node(2, "f2", "b.rs", 10);

        let graph = MultiplexGraph::build(vec![s0, s1, s2], &[], LayerWeights::default());

        let mut scores = HashMap::new();
        scores.insert(SymbolId(0), 0.9);
        scores.insert(SymbolId(1), 0.05);
        scores.insert(SymbolId(2), 0.01);

        let mut error_bounds = HashMap::new();
        error_bounds.insert(SymbolId(0), 0.001);
        error_bounds.insert(SymbolId(1), 0.001);
        error_bounds.insert(SymbolId(2), 0.001);

        let ppr_result = PprResult {
            scores,
            residuals: HashMap::new(),
            error_bounds,
            max_residual: 0.0001,
            total_residual: 0.0001,
            iterations: 100,
            truncated: false,
        };

        let optimizer = CelfOptimizer::default();
        let selected = vec![SymbolId(0)];

        let report = optimizer.analyze_sensitivity(&graph, &ppr_result, &selected, 10, 0.15, 1e-4);

        assert_eq!(report.selected_count, 1);
        assert_eq!(report.stable_count, 1);
        assert_eq!(report.stability_index, 1.0);
        assert!(report.borderline_pairs.is_empty());
    }

    #[test]
    fn test_celf_sensitivity_borderline_detection() {
        // Two symbols with identical costs and nearly identical utilities
        let s0 = make_test_node(0, "selected_border", "a.rs", 10);
        let s1 = make_test_node(1, "unselected_border", "b.rs", 10);

        let graph = MultiplexGraph::build(vec![s0, s1], &[], LayerWeights::default());

        let mut scores = HashMap::new();
        scores.insert(SymbolId(0), 0.100);
        scores.insert(SymbolId(1), 0.098);

        let mut error_bounds = HashMap::new();
        // Error bound of 0.005 exceeds difference (0.002)
        error_bounds.insert(SymbolId(0), 0.005);
        error_bounds.insert(SymbolId(1), 0.005);

        let ppr_result = PprResult {
            scores,
            residuals: HashMap::new(),
            error_bounds,
            max_residual: 0.001,
            total_residual: 0.001,
            iterations: 50,
            truncated: false,
        };

        let optimizer = CelfOptimizer::new(CelfConfig {
            lambda_diversity: 0.0,
            neighbor_coverage_weight: 0.0,
            min_relevance_threshold: 0.0,
            threshold_epsilon: None,
            lod_weights: LodWeights::default(),
            ..Default::default()
        });

        let selected = vec![SymbolId(0)];

        let report = optimizer.analyze_sensitivity(&graph, &ppr_result, &selected, 10, 0.15, 1e-4);

        assert_eq!(report.selected_count, 1);
        assert_eq!(report.stable_count, 0);
        assert_eq!(report.stability_index, 0.0);
        assert_eq!(report.borderline_pairs.len(), 1);
        assert_eq!(report.borderline_pairs[0].selected_symbol, SymbolId(0));
        assert_eq!(report.borderline_pairs[0].unselected_symbol, SymbolId(1));
        assert!(report.borderline_pairs[0].overlap > 0.0);
    }

    #[test]
    fn test_pareto_frontier_deduplication_and_convex_hull() {
        let mut s0 = make_test_node(0, "compute_hash", "src/crypto.rs", 20);
        s0.docstring =
            Some("Computes BLAKE3 cryptographic digest.\nVerifies integrity.".to_string());

        let source = "/// Computes BLAKE3 cryptographic digest.\n/// Verifies integrity.\nfn compute_hash() {\n    let state = init();\n    let hash = finish(state);\n    return hash;\n}";

        let frontier = build_pareto_frontier(
            &s0,
            Some(source),
            TokenizerModel::FastHeuristic,
            LodWeights::default(),
            false,
        );

        // Frontier must start with None at cost 0
        assert_eq!(frontier[0].level, None);
        assert_eq!(frontier[0].cost, 0);
        assert_eq!(frontier[0].utility_multiplier, 0.0);

        // Every subsequent option must have strictly increasing cost and utility multiplier
        for i in 1..frontier.len() {
            assert!(
                frontier[i].cost > frontier[i - 1].cost,
                "Costs not strictly increasing: {} <= {}",
                frontier[i].cost,
                frontier[i - 1].cost
            );
            assert!(
                frontier[i].utility_multiplier > frontier[i - 1].utility_multiplier,
                "Utilities not strictly increasing"
            );
        }

        // Slopes must be strictly decreasing (upper convex hull condition)
        for i in 2..frontier.len() {
            let slope_prev = (frontier[i - 1].utility_multiplier
                - frontier[i - 2].utility_multiplier)
                / ((frontier[i - 1].cost - frontier[i - 2].cost) as f32);
            let slope_cur = (frontier[i].utility_multiplier - frontier[i - 1].utility_multiplier)
                / ((frontier[i].cost - frontier[i - 1].cost) as f32);
            assert!(
                slope_prev >= slope_cur,
                "Convex hull violation: prev slope {} < cur slope {}",
                slope_prev,
                slope_cur
            );
        }
    }

    #[test]
    fn test_pareto_frontier_prunes_empty_docstring() {
        let s0 = make_test_node(0, "empty_doc", "src/lib.rs", 10);
        let source = "fn empty_doc() {}";

        let frontier = build_pareto_frontier(
            &s0,
            Some(source),
            TokenizerModel::FastHeuristic,
            LodWeights::default(),
            false,
        );

        // Since s0 has no docstring, SignatureAndDoc should not appear in the frontier
        // because it provides identical text/cost as SignatureOnly
        let has_doc = frontier
            .iter()
            .any(|opt| opt.level == Some(LodLevel::SignatureAndDoc));
        assert!(
            !has_doc,
            "Redundant SignatureAndDoc was not pruned from Pareto frontier"
        );
    }

    #[test]
    fn test_mckp_joint_optimization_budget_invariant() {
        let s0 = make_test_node(0, "root_handler", "src/main.rs", 10);
        let mut s1 = make_test_node(1, "authenticate", "src/auth.rs", 15);
        s1.docstring = Some("Authenticates bearer token".to_string());
        let s2 = make_test_node(2, "log_event", "src/logger.rs", 8);

        let graph = MultiplexGraph::build(vec![s0, s1, s2], &[], LayerWeights::default());

        let mut ppr_scores = HashMap::new();
        ppr_scores.insert(SymbolId(0), 0.50);
        ppr_scores.insert(SymbolId(1), 0.35);
        ppr_scores.insert(SymbolId(2), 0.15);

        let mut sources = HashMap::new();
        sources.insert(
            PathBuf::from("src/main.rs"),
            "fn root_handler() { let x = 1; }".to_string(),
        );
        sources.insert(
            PathBuf::from("src/auth.rs"),
            "/// Authenticates bearer token\nfn authenticate() { let y = 2; }".to_string(),
        );
        sources.insert(
            PathBuf::from("src/logger.rs"),
            "fn log_event() { println!(); }".to_string(),
        );

        let optimizer = CelfOptimizer::default();
        let budgets = [15, 30, 60, 150];

        for &b in &budgets {
            let result = optimizer.optimize_mckp(
                &graph,
                &ppr_scores,
                b,
                &sources,
                TokenizerModel::FastHeuristic,
            );

            assert!(
                result.total_tokens <= b,
                "MCKP total tokens {} exceeded budget {}",
                result.total_tokens,
                b
            );

            // Verified selected LODs match selected symbols
            for (&sym_id, &lod) in &result.selected_lods {
                assert!(graph.symbol(sym_id).is_some());
                assert!(matches!(
                    lod,
                    LodLevel::SignatureOnly
                        | LodLevel::SignatureAndDoc
                        | LodLevel::SlicedBody
                        | LodLevel::FullBody
                ));
            }
        }
    }

    #[test]
    fn test_mckp_incremental_upgrade_progression() {
        let source = "/// Processes incoming encrypted payload in chunks\nfn process_payload() {\n    let decrypted = decrypt();\n    let verified = verify(decrypted);\n    return verified;\n}";
        let mut s0 = make_test_node(0, "process_payload", "src/pipeline.rs", 10);
        s0.docstring = Some("Processes incoming encrypted payload in chunks".to_string());
        s0.span = TextSpan::new(0, source.len(), 0, 5);

        let graph = MultiplexGraph::build(vec![s0.clone()], &[], LayerWeights::default());

        let mut ppr_scores = HashMap::new();
        ppr_scores.insert(SymbolId(0), 1.0);

        let mut sources = HashMap::new();
        sources.insert(PathBuf::from("src/pipeline.rs"), source.to_string());

        let optimizer = CelfOptimizer::default();

        // Very small budget: fits SignatureOnly
        let res_small = optimizer.optimize_mckp(
            &graph,
            &ppr_scores,
            15,
            &sources,
            TokenizerModel::FastHeuristic,
        );
        assert_eq!(res_small.selected_lods.len(), 1);
        let lod_small = res_small.selected_lods[&SymbolId(0)];
        assert_eq!(lod_small, LodLevel::SignatureOnly);

        // Generous budget: upgrades to FullBody
        let res_large = optimizer.optimize_mckp(
            &graph,
            &ppr_scores,
            200,
            &sources,
            TokenizerModel::FastHeuristic,
        );
        assert_eq!(res_large.selected_lods.len(), 1);
        let lod_large = res_large.selected_lods[&SymbolId(0)];
        assert_eq!(lod_large, LodLevel::FullBody);
        assert!(res_large.total_tokens > res_small.total_tokens);
        assert!(res_large.cumulative_utility > res_small.cumulative_utility);
    }

    #[test]
    fn test_mckp_best_singleton_supersedes_greedy() {
        // Adversarial singleton instance for MCKP:
        // Item 0: cheap signature (cost 6), low utility (0.01)
        // Item 1: high utility item whose full body costs 10 (utility 1.0)
        let s0 = make_test_node(0, "cheap_filler", "src/filler.rs", 6);
        let mut s1 = make_test_node(1, "heavy_core", "src/core.rs", 10);
        s1.span = TextSpan::new(0, 40, 0, 2);

        let graph = MultiplexGraph::build(vec![s0, s1], &[], LayerWeights::default());

        let mut ppr_scores = HashMap::new();
        ppr_scores.insert(SymbolId(0), 0.05);
        ppr_scores.insert(SymbolId(1), 10.0);

        let mut sources = HashMap::new();
        sources.insert(
            PathBuf::from("src/filler.rs"),
            "fn cheap_filler() {}".to_string(),
        );
        sources.insert(
            PathBuf::from("src/core.rs"),
            "fn heavy_core() { expensive_work(); }".to_string(),
        );

        let optimizer = CelfOptimizer::new(CelfConfig {
            lambda_diversity: 0.0,
            neighbor_coverage_weight: 0.0,
            min_relevance_threshold: 0.0,
            threshold_epsilon: None,
            lod_weights: LodWeights::default(),
            ..Default::default()
        });

        // With budget 10, the best singleton s1 at FullBody provides enormous utility
        let result = optimizer.optimize_mckp(
            &graph,
            &ppr_scores,
            10,
            &sources,
            TokenizerModel::FastHeuristic,
        );

        assert_eq!(result.selected_lods.len(), 1);
        assert!(result.selected_lods.contains_key(&SymbolId(1)));
    }

    #[test]
    fn test_celf_framing_aware_budget_adherence() {
        // Two symbols in two separate files
        let s0 = make_test_node(0, "fn_a", "src/a.rs", 10);
        let s1 = make_test_node(1, "fn_b", "src/b.rs", 10);
        let graph = MultiplexGraph::build(vec![s0, s1], &[], LayerWeights::default());

        let mut ppr_scores = HashMap::new();
        ppr_scores.insert(SymbolId(0), 1.0);
        ppr_scores.insert(SymbolId(1), 0.8);

        // Framing aware optimizer
        let optimizer = CelfOptimizer::default().with_framing(true);

        // Budget of 35 tokens:
        // One symbol in src/a.rs costs 10 (raw) + ~15 (file framing) + ~4 (comment) = ~29 tokens.
        // Admitting the second symbol from src/b.rs would cost another ~29 tokens (total ~58), which exceeds 35.
        let selected = optimizer.optimize(&graph, &ppr_scores, 35);
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0], SymbolId(0));
    }
}
