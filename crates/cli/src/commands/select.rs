use clap::{Args, ValueEnum};
use colored::Colorize;
use repotrim_engine::{
    count_tokens, CoeditCache, CoeditConfig, ContextSelector, DiffResolver, EdgeWeightLearner,
    GitCommitMiner, LayerWeights, LodLevel, ModelProfile, SymbolId, TokenizerModel,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::time::Instant;

use crate::loader::LoadedRepository;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OutputFormat {
    Markdown,
    Json,
}

#[derive(Args, Debug)]
pub struct SelectArgs {
    /// One or more seed symbol identifiers (functions, methods, structs, traits)
    #[arg(short = 's', long = "seed")]
    pub seed: Vec<String>,

    /// Natural language intent, query terms, or error message to automatically infer seeds
    #[arg(short = 'q', long = "query")]
    pub query: Option<String>,

    /// Automatically infer seeds from current git diff (staged and unstaged changes)
    #[arg(long = "from-diff")]
    pub from_diff: bool,

    /// Maximum token budget for selected context, or 'auto' for Knee-Curve tuning
    #[arg(short = 'b', long = "budget", default_value = "1000")]
    pub budget: String,

    /// Target LLM architecture for auto-budgeting presets (e.g. 'claude', 'gpt-4o', 'deepseek', 'ollama')
    #[arg(short = 'm', long = "model")]
    pub model: Option<String>,

    /// Tokenizer model for token budgeting ('fast', 'calibrated', 'exact' / 'cl100k', 'o200k')
    #[arg(long = "tokenizer", default_value = "fast")]
    pub tokenizer: String,

    /// Target codebase directory to scan
    #[arg(short = 'p', long = "path", default_value = ".")]
    pub path: PathBuf,

    /// Output serialization format (markdown or json)
    #[arg(short = 'f', long = "format", value_enum, default_value_t = OutputFormat::Markdown)]
    pub format: OutputFormat,

    /// Disable incremental AST caching and force full re-parsing
    #[arg(long = "no-cache")]
    pub no_cache: bool,

    /// Optional file destination to write output (defaults to stdout)
    #[arg(short = 'o', long = "output")]
    pub output: Option<PathBuf>,

    /// Display numerical stability diagnostics and knapsack sensitivity analysis
    #[arg(long = "diagnostics", alias = "sensitivity")]
    pub diagnostics: bool,

    /// Enable Multiple-Choice Knapsack (MCKP) joint symbol selection and Level-of-Detail (LOD) optimization
    #[arg(long = "joint-lod", alias = "mckp")]
    pub joint_lod: bool,

    /// Include mined Git commit co-edit logical coupling edges in the multiplex graph
    #[arg(long = "coedit", alias = "use-coedit")]
    pub coedit: bool,

    /// Dynamically calibrate multiplex layer weights using empirical Git commit history
    #[arg(long = "learn-weights", alias = "learned-weights")]
    pub learn_weights: bool,

    /// Apply intra-community cohesion boost to focus seeds to reduce external hub drift
    #[arg(long = "community-boost", alias = "community", default_value = "0.0")]
    pub community_boost: f32,

    /// Retrieval modality for query resolution: hybrid (default), lexical, or dense
    #[arg(long = "retrieval-mode", value_enum, default_value_t = crate::commands::query::QueryModeCli::Hybrid)]
    pub retrieval_mode: crate::commands::query::QueryModeCli,

    /// Enable Rocchio pseudo-relevance feedback query expansion
    #[arg(long = "query-expand", alias = "expand")]
    pub query_expand: bool,
}

pub fn execute(args: SelectArgs) -> Result<(), Box<dyn std::error::Error>> {
    if args.seed.is_empty() && args.query.is_none() && !args.from_diff {
        return Err(
            "At least one seed source must be provided: use --seed <name>, --query \"<intent>\", or --from-diff"
                .into(),
        );
    }

    let start_time = Instant::now();

    eprintln!(
        "{} Scanning repository at '{}'...",
        "⚙".cyan().bold(),
        args.path.display().to_string().bold()
    );

    let mut repo = LoadedRepository::load_with_options(&args.path, !args.no_cache)?;
    repo.load_all_sources()?;
    let load_time = start_time.elapsed();

    let cache_info = if args.no_cache {
        " (cache disabled)".dimmed().to_string()
    } else {
        format!(
            " (cache: {}/{} warm hits, {:.1}%)",
            repo.cache_report.cached_files,
            repo.cache_report.total_files,
            repo.cache_report.hit_ratio * 100.0
        )
        .dimmed()
        .to_string()
    };

    eprintln!(
        "{} Ingested {} symbols across {} files in {:?}{}",
        "✓".green().bold(),
        repo.symbols.len().to_string().bold(),
        repo.cache_report.total_files.to_string().bold(),
        load_time,
        cache_info
    );

    let mut weighted_seeds: HashMap<SymbolId, f32> = HashMap::new();

    // 1. Resolve explicit seed strings
    let mut missing_seeds: Vec<&str> = Vec::new();
    for seed_name in &args.seed {
        let mut matches: Vec<SymbolId> = repo
            .symbols
            .iter()
            .filter(|s| s.name == *seed_name)
            .map(|s| s.id)
            .collect();

        if matches.is_empty() {
            // Case-insensitive fallback
            matches = repo
                .symbols
                .iter()
                .filter(|s| s.name.eq_ignore_ascii_case(seed_name))
                .map(|s| s.id)
                .collect();
        }

        if !matches.is_empty() {
            for id in matches {
                *weighted_seeds.entry(id).or_default() += 2.0;
            }
        } else {
            missing_seeds.push(seed_name);
        }
    }

    if !missing_seeds.is_empty() {
        for missing in &missing_seeds {
            eprintln!(
                "{} Seed '{}' was not found in parsed symbols.",
                "!".yellow().bold(),
                missing
            );

            // Find close matches
            let suggestions: Vec<&str> = repo
                .symbols
                .iter()
                .filter(|s| s.name.to_lowercase().contains(&missing.to_lowercase()))
                .take(5)
                .map(|s| s.name.as_str())
                .collect();

            if !suggestions.is_empty() {
                eprintln!(
                    "   {} {}",
                    "Did you mean:".dimmed(),
                    suggestions.join(", ").cyan()
                );
            }
        }
    }

    // 2. Resolve natural language query seeds
    if let Some(query_str) = &args.query {
        let retrieval_cfg = repotrim_engine::RetrievalConfig {
            mode: args.retrieval_mode.into(),
            top_k: 5,
            query_expansion: args.query_expand,
            ..Default::default()
        };
        let query_seeds = repotrim_engine::HybridRetriever::with_config(retrieval_cfg)
            .search(&repo.symbols, query_str);
        if !query_seeds.is_empty() {
            let names: Vec<String> = query_seeds
                .iter()
                .map(|r| format!("{} ({:.0}%)", r.symbol_name.bold(), r.score * 100.0))
                .collect();

            eprintln!(
                "{} Inferred seeds from query '{}' (mode: {:?}): {}",
                "⚙".cyan().bold(),
                query_str.bold(),
                args.retrieval_mode,
                names.join(", ")
            );

            for r in query_seeds {
                *weighted_seeds.entry(r.symbol_id).or_default() += r.score;
            }
        } else {
            eprintln!(
                "{} No matching symbols found for query '{}'",
                "!".yellow().bold(),
                query_str
            );
        }
    }

    // 3. Resolve git diff seeds
    if args.from_diff {
        match DiffResolver::get_git_diff(&args.path) {
            Ok(diff_text) => {
                let modified_lines = DiffResolver::parse_unified_diff(&diff_text);
                let diff_seeds =
                    DiffResolver::resolve_modified_symbols(&repo.symbols, &modified_lines);

                if !diff_seeds.is_empty() {
                    let names: Vec<String> = diff_seeds
                        .iter()
                        .take(5)
                        .filter_map(|(id, _)| {
                            repo.symbols
                                .iter()
                                .find(|s| s.id == *id)
                                .map(|s| s.name.bold().to_string())
                        })
                        .collect();

                    eprintln!(
                        "{} Inferred {} seeds from git diff: {}",
                        "⚙".cyan().bold(),
                        diff_seeds.len().to_string().bold(),
                        names.join(", ")
                    );

                    for (id, weight) in diff_seeds {
                        *weighted_seeds.entry(id).or_default() += weight;
                    }
                } else {
                    eprintln!(
                        "{} Git diff did not intersect with any known symbol declarations",
                        "!".yellow().bold()
                    );
                }
            }
            Err(e) => {
                eprintln!("{} Failed to read git diff: {}", "!".yellow().bold(), e);
            }
        }
    }

    if weighted_seeds.is_empty() {
        return Err(format!(
            "No valid seed symbols could be identified from the provided parameters in {}",
            args.path.display()
        )
        .into());
    }

    let seed_pairs: Vec<(SymbolId, f32)> = weighted_seeds.into_iter().collect();

    let is_auto = args.budget.eq_ignore_ascii_case("auto");
    let model_profile = if is_auto {
        Some(ModelProfile::parse(
            args.model.as_deref().unwrap_or("claude"),
        ))
    } else {
        None
    };

    let explicit_budget: Option<usize> = if is_auto {
        None
    } else {
        match args.budget.parse::<usize>() {
            Ok(b) => Some(b),
            Err(_) => {
                return Err(format!(
                    "Invalid budget '{}': must be a positive integer or 'auto'",
                    args.budget
                )
                .into());
            }
        }
    };

    eprintln!(
        "{} Building multiplex graph & running CELF submodular knapsack...",
        "⚙".cyan().bold()
    );

    let tokenizer_model = TokenizerModel::from_str_name(&args.tokenizer).unwrap_or_else(|| {
        eprintln!(
            "{} Unknown tokenizer '{}', falling back to 'fast'",
            "!".yellow().bold(),
            args.tokenizer
        );
        TokenizerModel::FastHeuristic
    });

    let mut coedit_edges: Vec<(SymbolId, SymbolId, f32)> = Vec::new();
    let mut layer_weights = LayerWeights::default();

    if args.coedit || args.learn_weights {
        if let Ok(head_hash) = GitCommitMiner::get_head_hash(&repo.root_path) {
            let cache_file = repo.root_path.join(".repotrim").join("coedit.bin");
            let coedit_graph = if !args.no_cache {
                if let Some(cached) = CoeditCache::load_from_file(&cache_file, &head_hash) {
                    cached
                } else {
                    let config = CoeditConfig::default();
                    let mined =
                        GitCommitMiner::mine_repository(&repo.root_path, &repo.symbols, &config)
                            .unwrap_or_default();
                    let _ = CoeditCache::save_to_file(&cache_file, &mined);
                    mined
                }
            } else {
                let config = CoeditConfig::default();
                let mined =
                    GitCommitMiner::mine_repository(&repo.root_path, &repo.symbols, &config)
                        .unwrap_or_default();
                let _ = CoeditCache::save_to_file(&cache_file, &mined);
                mined
            };

            if args.coedit {
                coedit_edges = coedit_graph.to_directed_edges(0.15);
                eprintln!(
                    "{} Injected {} mined Git co-edit edges into multiplex graph",
                    "⚙".cyan().bold(),
                    coedit_edges.len().to_string().bold()
                );
            }

            if args.learn_weights {
                let (learned, _) = EdgeWeightLearner::learn_weights(
                    &repo.symbols,
                    &repo.edges,
                    &repo.imports,
                    &coedit_graph,
                    LayerWeights::default(),
                );
                layer_weights = learned;
                eprintln!(
                    "{} Calibrated layer weights from commit history (AST: {:.2}, Call: {:.2}, Type: {:.2}, Import: {:.2}, CoEdit: {:.2})",
                    "⚙".cyan().bold(),
                    layer_weights.ast_parent,
                    layer_weights.call,
                    layer_weights.type_ref,
                    layer_weights.import,
                    layer_weights.co_edit
                );
            }
        }
    }

    let graph = repo.build_graph_with_coedits(&coedit_edges, layer_weights);
    let selector = ContextSelector::default().with_tokenizer(tokenizer_model);

    let select_start = Instant::now();
    let (selected_symbols, markdown, auto_report_opt, sensitivity_opt, mckp_opt) = if args.joint_lod
    {
        if let Some(ref model) = model_profile {
            let (selected, md, report, mckp) = selector
                .select_and_format_context_auto_weighted_joint_lod(
                    &graph,
                    &seed_pairs,
                    *model,
                    &repo.file_sources,
                );
            (selected, md, Some(report), None, Some(mckp))
        } else {
            let budget = explicit_budget.unwrap();
            let (selected, md, mckp) = selector.select_and_format_context_weighted_joint_lod(
                &graph,
                &seed_pairs,
                budget,
                &repo.file_sources,
            );
            (selected, md, None, None, Some(mckp))
        }
    } else if let Some(ref model) = model_profile {
        let (selected, md, report) = selector.select_and_format_context_auto_weighted(
            &graph,
            &seed_pairs,
            *model,
            &repo.file_sources,
        );
        let sens = if args.diagnostics {
            let (_, _, s) = selector.select_and_format_context_weighted_with_sensitivity(
                &graph,
                &seed_pairs,
                report.knee_tokens,
                &repo.file_sources,
            );
            Some(s)
        } else {
            None
        };
        (selected, md, Some(report), sens, None)
    } else {
        let budget = explicit_budget.unwrap();
        if args.diagnostics {
            let (selected, md, sens) = selector
                .select_and_format_context_weighted_with_sensitivity(
                    &graph,
                    &seed_pairs,
                    budget,
                    &repo.file_sources,
                );
            (selected, md, None, Some(sens), None)
        } else {
            let (selected, md) = if args.community_boost > 0.0 {
                selector.select_and_format_context_with_community(
                    &graph,
                    &seed_pairs,
                    budget,
                    args.community_boost,
                    &repo.file_sources,
                )
            } else {
                selector.select_and_format_context_weighted(
                    &graph,
                    &seed_pairs,
                    budget,
                    &repo.file_sources,
                )
            };
            (selected, md, None, None, None)
        }
    };
    let select_duration = select_start.elapsed();

    let total_tokens_used: usize = count_tokens(&markdown, tokenizer_model);

    if let Some(ref report) = auto_report_opt {
        eprintln!(
            "{} Auto-budget tuned to {} tokens via Knee-Curve (knee utility: {:.1}%, ceiling: {})",
            "⚡".yellow().bold(),
            report.knee_tokens.to_string().bold(),
            report.knee_utility_ratio * 100.0,
            report.optimal_budget
        );
        eprintln!(
            "{} Selected {} symbols (~{} tokens [{}] / {} auto-budget [{}]) in {:?}",
            "✓".green().bold(),
            selected_symbols.len().to_string().bold(),
            total_tokens_used.to_string().bold(),
            tokenizer_model.name().cyan(),
            report.knee_tokens,
            report.model_name,
            select_duration
        );
    } else {
        eprintln!(
            "{} Selected {} symbols (~{} tokens [{}] / {} budget) in {:?}",
            "✓".green().bold(),
            selected_symbols.len().to_string().bold(),
            total_tokens_used.to_string().bold(),
            tokenizer_model.name().cyan(),
            explicit_budget.unwrap(),
            select_duration
        );
    }

    if let Some(ref mckp) = mckp_opt {
        let mut sig_c = 0;
        let mut doc_c = 0;
        let mut slice_c = 0;
        let mut full_c = 0;
        for &lod in mckp.selected_lods.values() {
            match lod {
                LodLevel::SignatureOnly => sig_c += 1,
                LodLevel::SignatureAndDoc => doc_c += 1,
                LodLevel::SlicedBody => slice_c += 1,
                LodLevel::FullBody => full_c += 1,
            }
        }
        eprintln!(
            "  {} Joint LOD (MCKP): {} signature, {} sig+doc, {} sliced, {} full (utility: {:.3})",
            "→".cyan().bold(),
            sig_c.to_string().bold(),
            doc_c.to_string().bold(),
            slice_c.to_string().bold(),
            full_c.to_string().bold(),
            mckp.cumulative_utility
        );
    }

    if args.diagnostics {
        if let Some(ref sens) = sensitivity_opt {
            eprintln!();
            eprintln!(
                "{}",
                "================================================================================"
                    .cyan()
            );
            eprintln!(
                "  {}",
                "RepoTrim Knapsack Sensitivity & Numerical Stability Analysis".bold()
            );
            eprintln!(
                "{}",
                "================================================================================"
                    .cyan()
            );
            eprintln!(
                "  {:<24} : {:.1}% ({} of {} selected symbols unconditionally stable)",
                "Stability Index",
                sens.stability_index * 100.0,
                sens.stable_count,
                sens.selected_count
            );
            eprintln!("  {:<24} : {:e}", "ACL Epsilon (ε)", sens.epsilon);
            eprintln!("  {:<24} : {:.2}", "ACL Damping Factor (α)", sens.alpha);
            eprintln!(
                "  {:<24} : {:.5}",
                "Max Theoretical Error (δ)", sens.max_error_bound
            );
            eprintln!(
                "  {:<24} : {:.5}",
                "Mean Theoretical Error", sens.mean_error_bound
            );
            eprintln!(
                "  {:<24} : {}",
                "Candidate Pool Evaluated", sens.total_candidates
            );

            if !sens.borderline_pairs.is_empty() {
                eprintln!();
                eprintln!(
                    "  {}",
                    "Borderline Candidate Pairs (Near Knapsack Decision Boundary):"
                        .yellow()
                        .bold()
                );
                eprintln!("  {}", "------------------------------------------------------------------------------".dimmed());
                for (idx, pair) in sens.borderline_pairs.iter().take(5).enumerate() {
                    eprintln!(
                        "  {}. Selected   : {} [min density: {:.4}]",
                        idx + 1,
                        pair.selected_name.bold(),
                        pair.selected_density_min
                    );
                    eprintln!(
                        "     Unselected : {} [max density: {:.4}]",
                        pair.unselected_name.dimmed(),
                        pair.unselected_density_max
                    );
                    eprintln!("     Overlap    : +{:.4}", pair.overlap);
                }
            }
            eprintln!(
                "{}",
                "================================================================================"
                    .cyan()
            );
            eprintln!();
        }
    }

    let output_content = match args.format {
        OutputFormat::Markdown => markdown,
        OutputFormat::Json => {
            let budget_val = if let Some(ref report) = auto_report_opt {
                serde_json::json!(report.knee_tokens)
            } else {
                serde_json::json!(explicit_budget.unwrap())
            };
            let mut json_obj = serde_json::json!({
                "budget": budget_val,
                "symbols_count": selected_symbols.len(),
                "tokens_used": total_tokens_used,
                "tokenizer": tokenizer_model.name(),
                "symbols": selected_symbols.iter().map(|s| {
                    serde_json::json!({
                        "id": s.id.0,
                        "name": s.name,
                        "kind": format!("{:?}", s.kind),
                        "file": s.file_path.display().to_string(),
                        "lines": [s.span.start_row + 1, s.span.end_row + 1],
                        "token_cost": s.token_cost,
                        "signature": s.signature,
                        "docstring": s.docstring,
                    })
                }).collect::<Vec<_>>(),
                "markdown": markdown,
            });
            if let Some(ref report) = auto_report_opt {
                json_obj["auto_budget"] = serde_json::json!({
                    "model": report.model_name,
                    "optimal_budget": report.optimal_budget,
                    "knee_tokens": report.knee_tokens,
                    "knee_utility_ratio": report.knee_utility_ratio,
                    "candidate_count": report.candidate_count,
                    "selected_count": report.selected_count,
                });
            }
            if let Some(ref sens) = sensitivity_opt {
                json_obj["sensitivity"] = serde_json::to_value(sens)?;
            }
            if let Some(ref mckp) = mckp_opt {
                json_obj["joint_lod"] = serde_json::json!({
                    "enabled": true,
                    "total_tokens": mckp.total_tokens,
                    "cumulative_utility": mckp.cumulative_utility,
                    "selected_lods": mckp.selected_lods.iter().map(|(id, lod)| {
                        (id.0.to_string(), format!("{:?}", lod))
                    }).collect::<HashMap<_, _>>(),
                });
            }
            serde_json::to_string_pretty(&json_obj)?
        }
    };

    if let Some(out_path) = args.output {
        fs::write(&out_path, &output_content)?;
        eprintln!(
            "{} Context successfully written to '{}'",
            "✓".green().bold(),
            out_path.display().to_string().bold()
        );
    } else {
        println!("{}", output_content);
    }

    Ok(())
}
