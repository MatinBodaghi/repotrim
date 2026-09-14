//! Rigorous empirical evaluation harness and baseline comparative benchmarking.
//!
//! Provides mathematically principled evaluation of context extraction strategies
//! comparing RepoTrim against industry baselines including Aider's Global PageRank Repo Map,
//! Whole-File Dump, and Naive Keyword/Grep Search across quantitative metrics:
//! - Token Budget Adherence and Token Reduction %
//! - Target Direct (1st-order) and Transitive (2nd-order) Dependency Recall
//! - Context Precision and Subgraph Community Cohesion
//! - Induced Subgraph Orphan Symbol Rate
//! - Microsecond Execution Latency
//!
//! # References & Academic Citations
//! - Aider AI. (2023). "Repository Map: Global PageRank over Code Tags". aider.chat/docs/repomap.html
//! - Manning, C. D., Raghavan, P., & Schütze, H. (2008). "Introduction to Information Retrieval".
//!   Cambridge University Press. Chapters 8 & 21 (Evaluation in Information Retrieval).
//! - Andersen, R., Chung, F., & Lang, K. (2006). "Local Graph Partitioning using PageRank Vectors".
//!   FOCS '06.

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::time::Instant;

use serde::{Deserialize, Serialize};

use crate::community::{CommunityConfig, CommunityDetector};
use crate::formatter::ContextFormatter;
use crate::intent::IntentResolver;
use crate::loader::LoadedRepository;
use crate::navigation::{AdaptiveNavigator, NavigatorConfig};
use crate::ppr::PprSolver;
use crate::primitives::CodebaseIntelligence;
use crate::retrieval::{HybridRetriever, RetrievalConfig};
use crate::selector::ContextSelector;
use crate::symbol::{LodLevel, SymbolId, SymbolNode};
use crate::task::TaskContext;
use crate::tokens::{count_tokens, estimate_tokens, TokenizerModel};

/// Context selection strategy evaluated by the benchmark harness.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContextStrategy {
    // --- 7 Canonical Ablation Tiers ---
    /// Tier 1: Whole-File Dump (Full Context baseline).
    WholeFile,
    /// Tier 2: Lexical search (BM25+ keyword and token matching).
    Lexical,
    /// Tier 3: Graph-Only topology (degree centrality without semantic query priors).
    GraphOnly,
    /// Tier 4: PPR-Only diffusion (Personalized PageRank + density knapsack without submodular utility).
    PprOnly,
    /// Tier 5: Static Submodular utility (RepoTrim v0.7: relevance, coverage, redundancy, test verification).
    StaticSubmodular,
    /// Tier 6: Path-Aware context (RepoTrim v0.8: Boltzmann path energy + structured context).
    PathAware,
    /// Tier 7: Adaptive Navigation (RepoTrim v0.10: Sequential adaptive submodular greedy exploration).
    AdaptiveNavigation,

    // --- Legacy / External Baselines ---
    /// Aider-style Repo Map: uniform Global PageRank over reference graph, greedy definition packing.
    AiderRepoMap,
    /// Naive Grep/Substring Search (legacy keyword search baseline).
    NaiveGrep,
    /// RepoTrim Vanilla (v0.1: legacy alias for PPR diffusion).
    RepoTrimVanilla,
    /// RepoTrim Full (v0.5-v0.7: legacy alias for static submodular knapsack).
    RepoTrimFull,
}

impl ContextStrategy {
    /// Human-readable display label for the strategy.
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::WholeFile => "Whole-File Dump",
            Self::Lexical => "Lexical (BM25)",
            Self::GraphOnly => "Graph-Only (Topology)",
            Self::PprOnly => "PPR-Only (Diffusion)",
            Self::StaticSubmodular => "Static Submodular (v0.7)",
            Self::PathAware => "Path-Aware Context (v0.8)",
            Self::AdaptiveNavigation => "Adaptive Navigation (v0.10)",
            Self::AiderRepoMap => "Aider Repo Map (Global PR)",
            Self::NaiveGrep => "Naive Keyword/Grep",
            Self::RepoTrimVanilla => "RepoTrim Vanilla (v0.1)",
            Self::RepoTrimFull => "RepoTrim Full (Modern)",
        }
    }

    /// The 7 canonical ablation tiers representing RepoTrim's architectural progression.
    pub fn ablation_tiers() -> &'static [Self] {
        &[
            Self::WholeFile,
            Self::Lexical,
            Self::GraphOnly,
            Self::PprOnly,
            Self::StaticSubmodular,
            Self::PathAware,
            Self::AdaptiveNavigation,
        ]
    }

    /// Legacy baseline comparative set.
    pub fn baselines() -> &'static [Self] {
        &[
            Self::WholeFile,
            Self::NaiveGrep,
            Self::AiderRepoMap,
            Self::RepoTrimVanilla,
            Self::RepoTrimFull,
        ]
    }

    /// All canonical ablation strategies evaluated by default.
    pub fn all() -> &'static [Self] {
        Self::ablation_tiers()
    }

    /// All known strategies including ablation tiers and external baselines.
    pub fn all_known() -> &'static [Self] {
        &[
            Self::WholeFile,
            Self::Lexical,
            Self::GraphOnly,
            Self::PprOnly,
            Self::StaticSubmodular,
            Self::PathAware,
            Self::AdaptiveNavigation,
            Self::AiderRepoMap,
            Self::NaiveGrep,
            Self::RepoTrimVanilla,
            Self::RepoTrimFull,
        ]
    }
}

impl fmt::Display for ContextStrategy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.display_name())
    }
}

/// Evaluation metrics produced by evaluating a strategy on a benchmark scenario.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkMetrics {
    /// Scenario name.
    pub scenario: String,
    /// Evaluated strategy.
    pub strategy: ContextStrategy,
    /// Strategy display name.
    pub strategy_name: String,
    /// Target token budget constraint.
    pub budget: usize,
    /// Actual tokens consumed by the extracted context.
    pub tokens_used: usize,
    /// Whether the extracted context strictly respected the budget constraint.
    pub budget_adherence: bool,
    /// Token reduction percentage relative to Whole-File Dump: `(1 - T / T_dump) * 100%`.
    pub token_reduction_pct: f32,
    /// 1st-order direct dependency recall percentage: `|S ∩ N_1(q)| / |N_1(q)| * 100%`.
    pub direct_dep_recall_pct: f32,
    /// 2nd-order transitive dependency recall percentage: `|S ∩ N_2(q)| / |N_2(q)| * 100%`.
    pub transitive_dep_recall_pct: f32,
    /// Context precision: fraction of selected symbols that reside in `N_1 ∪ N_2 ∪ {seeds}`.
    pub context_precision_pct: f32,
    /// Community cohesion: fraction of selected symbols in the seed's dominant topological community.
    pub community_cohesion_pct: f32,
    /// Orphan symbol rate: fraction of selected symbols having degree 0 in the induced subgraph `G[S]`.
    pub orphan_rate_pct: f32,
    /// Execution time in microseconds.
    pub execution_latency_us: u128,
    /// Count of selected symbols.
    pub symbol_count: usize,
    /// Input tokens inspected during search and exploration.
    #[serde(default)]
    pub input_tokens: usize,
    /// Output context tokens returned.
    #[serde(default)]
    pub output_tokens: usize,
    /// Number of exploration action steps or tool calls executed.
    #[serde(default = "default_tool_calls")]
    pub tool_call_count: usize,
    /// Number of distinct files inspected during exploration.
    #[serde(default)]
    pub inspected_files: usize,
    /// Exploration Cost Reduction percentage relative to Whole-File baseline.
    #[serde(default)]
    pub ecr_pct: f32,
    /// Information density: Symbols Per Thousand Tokens (SPT).
    #[serde(default)]
    pub spt_ratio: f32,
}

fn default_tool_calls() -> usize {
    1
}

/// A standardized evaluation scenario representing a realistic agent coding task.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkScenario {
    /// Unique scenario identifier (e.g. "context_selector").
    pub id: String,
    /// Human-readable scenario title.
    pub name: String,
    /// Seed symbol names to anchor context around.
    pub seeds: Vec<String>,
    /// Natural language task description or search query.
    pub query: Option<String>,
    /// Target token budget for the scenario.
    pub budget: usize,
    /// Description of the scenario and task objective.
    pub description: String,
}

impl BenchmarkScenario {
    /// Creates a new scenario with explicit seeds.
    pub fn with_seeds(
        id: &str,
        name: &str,
        seeds: Vec<&str>,
        budget: usize,
        description: &str,
    ) -> Self {
        Self {
            id: id.to_string(),
            name: name.to_string(),
            seeds: seeds.into_iter().map(String::from).collect(),
            query: None,
            budget,
            description: description.to_string(),
        }
    }

    /// Creates a new scenario with a natural language query.
    pub fn with_query(id: &str, name: &str, query: &str, budget: usize, description: &str) -> Self {
        Self {
            id: id.to_string(),
            name: name.to_string(),
            seeds: Vec::new(),
            query: Some(query.to_string()),
            budget,
            description: description.to_string(),
        }
    }
}

/// Empirical benchmark runner executing evaluations over code property graphs.
pub struct BenchmarkRunner {
    pub scenarios: Vec<BenchmarkScenario>,
}

impl Default for BenchmarkRunner {
    fn default() -> Self {
        Self::new()
    }
}

impl BenchmarkRunner {
    /// Creates a new benchmark runner initialized with canonical evaluation scenarios.
    pub fn new() -> Self {
        Self {
            scenarios: Self::canonical_scenarios(),
        }
    }

    /// Returns the standard canonical evaluation scenarios representative of real development tasks.
    pub fn canonical_scenarios() -> Vec<BenchmarkScenario> {
        vec![
            BenchmarkScenario::with_seeds(
                "context_selector",
                "ContextSelector (Core CELF Knapsack)",
                vec!["ContextSelector"],
                800,
                "Core optimization engine coordinating forward-push diffusion and submodular knapsack",
            ),
            BenchmarkScenario::with_seeds(
                "ppr_solver",
                "PprSolver (Sparse Local Diffusion)",
                vec!["PprSolver"],
                500,
                "Sparse ACL forward-push Personalized PageRank solver and linear system routines",
            ),
            BenchmarkScenario::with_seeds(
                "diff_resolver",
                "DiffResolver (Change Mapping)",
                vec!["DiffResolver"],
                600,
                "Unified git diff parser mapping line-level mutations to AST symbols",
            ),
            BenchmarkScenario::with_seeds(
                "multi_seed_subsystem",
                "Multi-Seed (Selector + Solver Navigation)",
                vec!["ContextSelector", "PprSolver"],
                1200,
                "Cross-module navigation linking submodular selection orchestrator to sparse linear solver",
            ),
            BenchmarkScenario::with_query(
                "query_intent_random_walk",
                "Query Intent: 'PageRank random walk'",
                "PageRank random walk local diffusion",
                800,
                "Natural language intent query resolving relevant graph diffusion and linear algebra symbols",
            ),
        ]
    }

    /// Sets custom scenarios for the benchmark suite.
    pub fn with_scenarios(mut self, scenarios: Vec<BenchmarkScenario>) -> Self {
        self.scenarios = scenarios;
        self
    }

    /// Returns a reference to the active scenarios.
    pub fn scenarios(&self) -> &[BenchmarkScenario] {
        &self.scenarios
    }

    /// Evaluates a single scenario across the given strategies on a loaded repository.
    pub fn evaluate_scenario(
        &self,
        repo: &LoadedRepository,
        scenario: &BenchmarkScenario,
        strategies: &[ContextStrategy],
    ) -> Vec<BenchmarkMetrics> {
        let graph = repo.build_graph();

        // 1. Resolve seed symbol IDs
        let mut resolved_seed_ids: Vec<SymbolId> = Vec::new();
        for seed_name in &scenario.seeds {
            for sym in graph.symbols() {
                if sym.name == *seed_name {
                    resolved_seed_ids.push(sym.id);
                }
            }
        }

        // If seeds empty but query provided, resolve seeds via IntentResolver
        if resolved_seed_ids.is_empty() {
            if let Some(ref q) = scenario.query {
                let query_seeds = IntentResolver::resolve_query(&repo.symbols, q, 5);
                resolved_seed_ids.extend(query_seeds.into_iter().map(|(id, _)| id));
            }
        }

        if resolved_seed_ids.is_empty() {
            return Vec::new();
        }

        // 2. Compute Ground Truth Neighborhoods: N_1 (direct) and N_2 (transitive)
        let seed_id_set: HashSet<u32> = resolved_seed_ids.iter().map(|s| s.0).collect();
        let mut n1_set: HashSet<u32> = HashSet::new();
        let mut n2_set: HashSet<u32> = HashSet::new();

        // Forward and backward direct neighbors
        for &s_id in &resolved_seed_ids {
            for &nbr in graph.neighbors(s_id) {
                if !seed_id_set.contains(&nbr) {
                    n1_set.insert(nbr);
                }
            }
        }

        // Incoming callers as direct dependencies
        let num_symbols = graph.num_symbols();
        for other_id in 0..num_symbols as u32 {
            if seed_id_set.contains(&other_id) {
                continue;
            }
            let nbrs = graph.neighbors(SymbolId(other_id));
            if resolved_seed_ids.iter().any(|s| nbrs.contains(&s.0)) {
                n1_set.insert(other_id);
            }
        }

        // 2nd-order transitive neighbors (neighbors of N_1 excluding seeds and N_1)
        for &n1_id in &n1_set {
            for &nbr in graph.neighbors(SymbolId(n1_id)) {
                if !seed_id_set.contains(&nbr) && !n1_set.contains(&nbr) {
                    n2_set.insert(nbr);
                }
            }
        }

        // 3. Compute topological communities to evaluate community cohesion
        let community_res = CommunityDetector::detect(&graph, &CommunityConfig::default());
        let mut seed_comm_counts: HashMap<usize, usize> = HashMap::new();
        for &s_id in &resolved_seed_ids {
            if let Some(&comm_id) = community_res.membership.get(&s_id) {
                *seed_comm_counts.entry(comm_id).or_insert(0) += 1;
            }
        }
        let dominant_seed_community: Option<usize> = seed_comm_counts
            .into_iter()
            .max_by_key(|&(_, count)| count)
            .map(|(comm, _)| comm);

        // 4. Measure Whole-File tokens as baseline denominator
        let mut whole_file_tokens = 0usize;
        let mut seed_file_paths: HashSet<&std::path::Path> = HashSet::new();
        for &s_id in &resolved_seed_ids {
            if let Some(sym) = graph.symbol(s_id) {
                seed_file_paths.insert(&sym.file_path);
            }
        }
        for file_path in &seed_file_paths {
            if let Some(content) = repo.file_sources.get(*file_path) {
                whole_file_tokens += estimate_tokens(content);
            }
        }

        // 5. Run each strategy
        let mut metrics_list = Vec::new();

        for &strategy in strategies {
            let (selected_ids, tokens_used, elapsed_us, input_tokens, tool_call_count) =
                match strategy {
                    ContextStrategy::WholeFile => {
                        let start = Instant::now();
                        let mut sel = HashSet::new();
                        for file_path in &seed_file_paths {
                            for sym in graph.symbols() {
                                if sym.file_path == **file_path {
                                    sel.insert(sym.id.0);
                                }
                            }
                        }
                        let elapsed = start.elapsed().as_micros();
                        (sel, whole_file_tokens, elapsed, whole_file_tokens, 1)
                    }

                    ContextStrategy::Lexical => {
                        let start = Instant::now();
                        let query_str = scenario.query.as_deref().unwrap_or(&scenario.name);
                        let ret_config = RetrievalConfig::lexical_only(graph.num_symbols());
                        let retriever = HybridRetriever::with_config(ret_config);
                        let results = retriever.search(graph.symbols(), query_str);

                        let mut matched_symbols = Vec::new();
                        let mut score_map = HashMap::new();
                        for r in results {
                            if let Some(sym) = graph.symbol(r.symbol_id) {
                                matched_symbols.push(sym.clone());
                                score_map.insert(r.symbol_id, r.score);
                            }
                        }

                        let mut lod_map = HashMap::new();
                        for s in &matched_symbols {
                            lod_map.insert(s.id, LodLevel::SignatureOnly);
                        }
                        let (surviving, md, _) = ContextFormatter::format_markdown_budgeted(
                            &matched_symbols,
                            &lod_map,
                            &repo.file_sources,
                            scenario.budget,
                            TokenizerModel::default(),
                            &score_map,
                            &[],
                        );
                        let tokens = count_tokens(&md, TokenizerModel::default());
                        let sel: HashSet<u32> = surviving.iter().map(|s| s.id.0).collect();
                        let in_tok = estimate_tokens(query_str)
                            + matched_symbols.iter().map(|s| s.token_cost).sum::<usize>();
                        let elapsed = start.elapsed().as_micros();
                        (sel, tokens, elapsed, in_tok, 1)
                    }

                    ContextStrategy::NaiveGrep => {
                        let start = Instant::now();
                        let mut keywords: Vec<String> = Vec::new();
                        for s_name in &scenario.seeds {
                            let sub = &s_name[..s_name.len().min(5)];
                            keywords.push(sub.to_lowercase());
                        }
                        if let Some(ref q) = scenario.query {
                            for word in q.split_whitespace() {
                                if word.len() >= 4 {
                                    keywords.push(word.to_lowercase());
                                }
                            }
                        }

                        let mut matched: Vec<_> = graph
                            .symbols()
                            .iter()
                            .filter(|s| {
                                let s_low = s.name.to_lowercase();
                                keywords.iter().any(|k| s_low.contains(k))
                            })
                            .cloned()
                            .collect();
                        matched.sort_by_key(|s| s.token_cost);

                        let mut lod_map = HashMap::new();
                        for s in &matched {
                            lod_map.insert(s.id, LodLevel::SignatureOnly);
                        }
                        let (surviving, md, _) = ContextFormatter::format_markdown_budgeted(
                            &matched,
                            &lod_map,
                            &repo.file_sources,
                            scenario.budget,
                            TokenizerModel::default(),
                            &HashMap::new(),
                            &[],
                        );
                        let tokens = count_tokens(&md, TokenizerModel::default());
                        let sel: HashSet<u32> = surviving.iter().map(|s| s.id.0).collect();
                        let in_tok = matched.iter().map(|s| s.token_cost).sum::<usize>();
                        let elapsed = start.elapsed().as_micros();
                        (sel, tokens, elapsed, in_tok, 1)
                    }

                    ContextStrategy::GraphOnly => {
                        let start = Instant::now();
                        let mut symbol_degrees: Vec<(SymbolId, usize)> = graph
                            .symbols()
                            .iter()
                            .map(|s| (s.id, graph.neighbors(s.id).len()))
                            .collect();
                        symbol_degrees.sort_by_key(|b| std::cmp::Reverse(b.1));

                        let mut graph_symbols = Vec::new();
                        let mut deg_map = HashMap::new();
                        for (sym_id, deg) in symbol_degrees {
                            if let Some(sym) = graph.symbol(sym_id) {
                                graph_symbols.push(sym.clone());
                                deg_map.insert(sym_id, deg as f32);
                            }
                        }

                        let mut lod_map = HashMap::new();
                        for s in &graph_symbols {
                            lod_map.insert(s.id, LodLevel::SignatureOnly);
                        }
                        let (surviving, md, _) = ContextFormatter::format_markdown_budgeted(
                            &graph_symbols,
                            &lod_map,
                            &repo.file_sources,
                            scenario.budget,
                            TokenizerModel::default(),
                            &deg_map,
                            &[],
                        );
                        let tokens = count_tokens(&md, TokenizerModel::default());
                        let sel: HashSet<u32> = surviving.iter().map(|s| s.id.0).collect();
                        let in_tok = graph_symbols
                            .iter()
                            .take(20)
                            .map(|s| s.token_cost)
                            .sum::<usize>();
                        let elapsed = start.elapsed().as_micros();
                        (sel, tokens, elapsed, in_tok, 1)
                    }

                    ContextStrategy::AiderRepoMap => {
                        // Aider Repo Map simulation:
                        // 1. Uniform Global PageRank over all workspace symbols
                        // 2. Greedy definition packing ordered by Global PR score
                        let start = Instant::now();
                        let uniform_seeds: Vec<(SymbolId, f32)> = (0..num_symbols as u32)
                            .map(|id| (SymbolId(id), 1.0 / num_symbols.max(1) as f32))
                            .collect();
                        let ppr = PprSolver::default();
                        let pr_scores = ppr.compute(&graph, &uniform_seeds);

                        let mut sorted_nodes: Vec<(SymbolId, f32)> =
                            pr_scores.into_iter().collect();
                        sorted_nodes.sort_by(|a, b| {
                            b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal)
                        });

                        let mut aider_symbols: Vec<SymbolNode> = Vec::new();
                        let mut score_map = HashMap::new();
                        for (sym_id, score) in sorted_nodes {
                            if let Some(sym) = graph.symbol(sym_id) {
                                aider_symbols.push(sym.clone());
                                score_map.insert(sym_id, score);
                            }
                        }

                        let mut lod_map = HashMap::new();
                        for s in &aider_symbols {
                            lod_map.insert(s.id, LodLevel::SignatureOnly);
                        }
                        let (surviving, md, _) = ContextFormatter::format_markdown_budgeted(
                            &aider_symbols,
                            &lod_map,
                            &repo.file_sources,
                            scenario.budget,
                            TokenizerModel::default(),
                            &score_map,
                            &[],
                        );
                        let tokens = count_tokens(&md, TokenizerModel::default());
                        let sel: HashSet<u32> = surviving.iter().map(|s| s.id.0).collect();
                        let in_tok = aider_symbols
                            .iter()
                            .take(50)
                            .map(|s| s.token_cost)
                            .sum::<usize>();
                        let elapsed = start.elapsed().as_micros();
                        (sel, tokens, elapsed, in_tok, 1)
                    }

                    ContextStrategy::PprOnly | ContextStrategy::RepoTrimVanilla => {
                        // RepoTrim v0.1: Standard PPR from seeds, greedy knapsack
                        let start = Instant::now();
                        let seed_pairs: Vec<(SymbolId, f32)> =
                            resolved_seed_ids.iter().map(|&id| (id, 1.0)).collect();
                        let ppr = PprSolver::default();
                        let scores = ppr.compute(&graph, &seed_pairs);

                        let mut candidates: Vec<(SymbolId, f32)> =
                            scores.clone().into_iter().collect();
                        candidates.sort_by(|a, b| {
                            let cost_a =
                                graph.symbol(a.0).map(|s| s.token_cost).unwrap_or(1).max(1);
                            let cost_b =
                                graph.symbol(b.0).map(|s| s.token_cost).unwrap_or(1).max(1);
                            let dens_a = a.1 / cost_a as f32;
                            let dens_b = b.1 / cost_b as f32;
                            dens_b
                                .partial_cmp(&dens_a)
                                .unwrap_or(std::cmp::Ordering::Equal)
                        });

                        let mut ppr_symbols: Vec<SymbolNode> = Vec::new();
                        for (sym_id, _) in candidates {
                            if let Some(sym) = graph.symbol(sym_id) {
                                ppr_symbols.push(sym.clone());
                            }
                        }

                        let mut lod_map = HashMap::new();
                        for s in &ppr_symbols {
                            lod_map.insert(s.id, LodLevel::SignatureOnly);
                        }
                        let (surviving, md, _) = ContextFormatter::format_markdown_budgeted(
                            &ppr_symbols,
                            &lod_map,
                            &repo.file_sources,
                            scenario.budget,
                            TokenizerModel::default(),
                            &scores,
                            &resolved_seed_ids,
                        );
                        let tokens = count_tokens(&md, TokenizerModel::default());
                        let sel: HashSet<u32> = surviving.iter().map(|s| s.id.0).collect();
                        let in_tok = ppr_symbols
                            .iter()
                            .take(30)
                            .map(|s| s.token_cost)
                            .sum::<usize>();
                        let elapsed = start.elapsed().as_micros();
                        (sel, tokens, elapsed, in_tok, 1)
                    }

                    ContextStrategy::StaticSubmodular | ContextStrategy::RepoTrimFull => {
                        // Full modern RepoTrim: Multiplex CPG, Forward-Push PPR, CELF Knapsack with Best-Singleton,
                        // Community Cohesion Boost (0.35)
                        let start = Instant::now();
                        let selector = ContextSelector::default();
                        let seed_pairs: Vec<(SymbolId, f32)> =
                            resolved_seed_ids.iter().map(|&id| (id, 1.0)).collect();
                        let (selected, md) = selector.select_and_format_context_weighted(
                            &graph,
                            &seed_pairs,
                            scenario.budget,
                            &repo.file_sources,
                        );
                        let tokens = count_tokens(&md, TokenizerModel::default());
                        let sel: HashSet<u32> = selected.iter().map(|s| s.id.0).collect();
                        let in_tok = seed_pairs.len() * 50
                            + selected.iter().map(|s| s.token_cost).sum::<usize>();
                        let elapsed = start.elapsed().as_micros();
                        (sel, tokens, elapsed, in_tok, 1)
                    }

                    ContextStrategy::PathAware => {
                        // Tier 6: Submodular knapsack with Boltzmann path energy scoring & StructuredContext
                        let start = Instant::now();
                        let seed_pairs: Vec<(SymbolId, f32)> =
                            resolved_seed_ids.iter().map(|&id| (id, 1.0)).collect();
                        let intel = CodebaseIntelligence::new(&graph, &repo.file_sources);
                        let query_str = scenario.query.as_deref().unwrap_or(&scenario.name);
                        let task_ctx = TaskContext::with_seeds(query_str, scenario.seeds.clone());
                        let structured =
                            intel.context_with_seeds(&seed_pairs, Some(&task_ctx), scenario.budget);
                        let sel: HashSet<u32> = structured.symbols.iter().map(|s| s.id.0).collect();
                        let tokens = structured.tokens_used;
                        let in_tok = estimate_tokens(query_str)
                            + structured
                                .symbols
                                .iter()
                                .map(|s| s.token_cost)
                                .sum::<usize>()
                            + structured.paths.len() * 40;
                        let elapsed = start.elapsed().as_micros();
                        (sel, tokens, elapsed, in_tok, 1)
                    }

                    ContextStrategy::AdaptiveNavigation => {
                        // Tier 7: Sequential adaptive submodular greedy exploration
                        let start = Instant::now();
                        let intel = CodebaseIntelligence::new(&graph, &repo.file_sources);
                        let query_str = scenario.query.as_deref().unwrap_or(&scenario.name);
                        let task_ctx = TaskContext::with_seeds(query_str, scenario.seeds.clone());
                        let config = NavigatorConfig {
                            max_steps: 10,
                            min_efficiency_threshold: 0.0001,
                            ..Default::default()
                        };
                        let navigator = AdaptiveNavigator::new(config);
                        let budget_u32 = scenario.budget.min(u32::MAX as usize) as u32;
                        let (sel, tokens, in_tok, steps) =
                            match navigator.navigate(task_ctx, budget_u32, &intel) {
                                Ok(trajectory) => {
                                    let ids: HashSet<u32> = trajectory
                                        .structured_context
                                        .symbols
                                        .iter()
                                        .map(|s| s.id.0)
                                        .collect();
                                    let t = trajectory.tokens_used() as usize;
                                    let in_t: usize = trajectory
                                        .steps
                                        .iter()
                                        .map(|s| s.cost.input_tokens as usize)
                                        .sum();
                                    let s = trajectory.step_count().max(1);
                                    (ids, t, in_t, s)
                                }
                                Err(_) => (HashSet::new(), 0, 0, 1),
                            };
                        let elapsed = start.elapsed().as_micros();
                        (sel, tokens, elapsed, in_tok, steps)
                    }
                };

            // Calculate Metrics
            let budget_adherence = tokens_used <= scenario.budget;
            let token_reduction_pct = if whole_file_tokens > 0 {
                (1.0 - (tokens_used as f32 / whole_file_tokens as f32)).max(0.0) * 100.0
            } else {
                0.0
            };

            let direct_dep_recall_pct = if !n1_set.is_empty() {
                let hits = n1_set.intersection(&selected_ids).count();
                (hits as f32 / n1_set.len() as f32) * 100.0
            } else {
                100.0
            };

            let transitive_dep_recall_pct = if !n2_set.is_empty() {
                let hits = n2_set.intersection(&selected_ids).count();
                (hits as f32 / n2_set.len() as f32) * 100.0
            } else {
                100.0
            };

            let relevant_universe: HashSet<u32> = seed_id_set
                .union(&n1_set)
                .copied()
                .collect::<HashSet<u32>>()
                .union(&n2_set)
                .copied()
                .collect();

            let context_precision_pct = if !selected_ids.is_empty() {
                let relevant_count = selected_ids.intersection(&relevant_universe).count();
                (relevant_count as f32 / selected_ids.len() as f32) * 100.0
            } else {
                100.0
            };

            let community_cohesion_pct = if let Some(target_comm) = dominant_seed_community {
                if !selected_ids.is_empty() {
                    let matching_comm = selected_ids
                        .iter()
                        .filter(|&&id| {
                            community_res.membership.get(&SymbolId(id)).copied()
                                == Some(target_comm)
                        })
                        .count();
                    (matching_comm as f32 / selected_ids.len() as f32) * 100.0
                } else {
                    100.0
                }
            } else {
                100.0
            };

            // Orphan rate: fraction of selected symbols with degree 0 in induced subgraph G[S]
            let orphan_rate_pct = if selected_ids.len() > 1 {
                let mut orphan_count = 0usize;
                for &u in &selected_ids {
                    let mut has_edge = false;
                    // Check outgoing edges
                    for &v in graph.neighbors(SymbolId(u)) {
                        if v != u && selected_ids.contains(&v) {
                            has_edge = true;
                            break;
                        }
                    }
                    // Check incoming edges if no outgoing edge found
                    if !has_edge {
                        for &v in &selected_ids {
                            if v != u && graph.neighbors(SymbolId(v)).contains(&u) {
                                has_edge = true;
                                break;
                            }
                        }
                    }
                    if !has_edge {
                        orphan_count += 1;
                    }
                }
                (orphan_count as f32 / selected_ids.len() as f32) * 100.0
            } else {
                0.0
            };

            let output_tokens = tokens_used;
            let inspected_files = selected_ids
                .iter()
                .filter_map(|&id| graph.symbol(SymbolId(id)).map(|s| &s.file_path))
                .collect::<HashSet<_>>()
                .len();
            let ecr_pct = if whole_file_tokens > 0 {
                (1.0 - (input_tokens as f32 / whole_file_tokens as f32)).max(0.0) * 100.0
            } else {
                0.0
            };
            let spt_ratio = if output_tokens > 0 {
                (selected_ids.len() as f32 / output_tokens as f32) * 1000.0
            } else {
                0.0
            };

            metrics_list.push(BenchmarkMetrics {
                scenario: scenario.name.clone(),
                strategy,
                strategy_name: strategy.display_name().to_string(),
                budget: scenario.budget,
                tokens_used,
                budget_adherence,
                token_reduction_pct,
                direct_dep_recall_pct,
                transitive_dep_recall_pct,
                context_precision_pct,
                community_cohesion_pct,
                orphan_rate_pct,
                execution_latency_us: elapsed_us,
                symbol_count: selected_ids.len(),
                input_tokens,
                output_tokens,
                tool_call_count,
                inspected_files,
                ecr_pct,
                spt_ratio,
            });
        }

        metrics_list
    }

    /// Evaluates all scenarios in the benchmark suite.
    pub fn run_all(&self, repo: &LoadedRepository) -> Vec<BenchmarkMetrics> {
        let strategies = ContextStrategy::all();
        let mut all_metrics = Vec::new();
        for scenario in &self.scenarios {
            let metrics = self.evaluate_scenario(repo, scenario, strategies);
            all_metrics.extend(metrics);
        }
        all_metrics
    }
}

/// Aggregate summary across benchmark metrics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkSummary {
    /// Strategy being summarized.
    pub strategy: ContextStrategy,
    pub strategy_name: String,
    pub mean_tokens: usize,
    pub mean_token_reduction_pct: f32,
    pub mean_direct_recall_pct: f32,
    pub mean_transitive_recall_pct: f32,
    pub mean_precision_pct: f32,
    pub mean_cohesion_pct: f32,
    pub mean_orphan_rate_pct: f32,
    pub mean_latency_us: u128,
    /// Mean input tokens inspected during exploration.
    #[serde(default)]
    pub mean_input_tokens: usize,
    /// Mean output context tokens returned.
    #[serde(default)]
    pub mean_output_tokens: usize,
    /// Mean number of exploration action steps or tool calls.
    #[serde(default)]
    pub mean_tool_calls: f32,
    /// Mean number of distinct files inspected.
    #[serde(default)]
    pub mean_inspected_files: f32,
    /// Mean Exploration Cost Reduction percentage relative to Whole-File baseline.
    #[serde(default)]
    pub mean_ecr_pct: f32,
    /// Mean Information Density: Symbols Per Thousand Tokens (SPT).
    #[serde(default)]
    pub mean_spt_ratio: f32,
}

impl BenchmarkSummary {
    /// Computes aggregate summaries grouped by strategy.
    pub fn summarize(metrics: &[BenchmarkMetrics]) -> Vec<Self> {
        let mut grouped: HashMap<ContextStrategy, Vec<&BenchmarkMetrics>> = HashMap::new();
        for m in metrics {
            grouped.entry(m.strategy).or_default().push(m);
        }

        let mut summaries = Vec::new();
        for &strat in ContextStrategy::all_known() {
            if let Some(list) = grouped.get(&strat) {
                if list.is_empty() {
                    continue;
                }
                let count = list.len() as f32;
                let mean_tokens =
                    (list.iter().map(|m| m.tokens_used).sum::<usize>() as f32 / count) as usize;
                let mean_token_reduction_pct =
                    list.iter().map(|m| m.token_reduction_pct).sum::<f32>() / count;
                let mean_direct_recall_pct =
                    list.iter().map(|m| m.direct_dep_recall_pct).sum::<f32>() / count;
                let mean_transitive_recall_pct = list
                    .iter()
                    .map(|m| m.transitive_dep_recall_pct)
                    .sum::<f32>()
                    / count;
                let mean_precision_pct =
                    list.iter().map(|m| m.context_precision_pct).sum::<f32>() / count;
                let mean_cohesion_pct =
                    list.iter().map(|m| m.community_cohesion_pct).sum::<f32>() / count;
                let mean_orphan_rate_pct =
                    list.iter().map(|m| m.orphan_rate_pct).sum::<f32>() / count;
                let mean_latency_us = (list.iter().map(|m| m.execution_latency_us).sum::<u128>()
                    as f64
                    / count as f64) as u128;

                let mean_input_tokens =
                    (list.iter().map(|m| m.input_tokens).sum::<usize>() as f32 / count) as usize;
                let mean_output_tokens =
                    (list.iter().map(|m| m.output_tokens).sum::<usize>() as f32 / count) as usize;
                let mean_tool_calls =
                    list.iter().map(|m| m.tool_call_count as f32).sum::<f32>() / count;
                let mean_inspected_files =
                    list.iter().map(|m| m.inspected_files as f32).sum::<f32>() / count;
                let mean_ecr_pct = list.iter().map(|m| m.ecr_pct).sum::<f32>() / count;
                let mean_spt_ratio = list.iter().map(|m| m.spt_ratio).sum::<f32>() / count;

                summaries.push(Self {
                    strategy: strat,
                    strategy_name: strat.display_name().to_string(),
                    mean_tokens,
                    mean_token_reduction_pct,
                    mean_direct_recall_pct,
                    mean_transitive_recall_pct,
                    mean_precision_pct,
                    mean_cohesion_pct,
                    mean_orphan_rate_pct,
                    mean_latency_us,
                    mean_input_tokens,
                    mean_output_tokens,
                    mean_tool_calls,
                    mean_inspected_files,
                    mean_ecr_pct,
                    mean_spt_ratio,
                });
            }
        }

        summaries
    }
}
