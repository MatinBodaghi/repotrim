use clap::{Args, ValueEnum};
use colored::Colorize;
use repotrim_engine::{ContextSelector, DiffResolver, IntentResolver, ModelProfile, SymbolId};
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
        let query_seeds = IntentResolver::resolve_query(&repo.symbols, query_str, 5);
        if !query_seeds.is_empty() {
            let names: Vec<String> = query_seeds
                .iter()
                .filter_map(|(id, w)| {
                    repo.symbols
                        .iter()
                        .find(|s| s.id == *id)
                        .map(|s| format!("{} ({:.0}%)", s.name.bold(), w * 100.0))
                })
                .collect();

            eprintln!(
                "{} Inferred seeds from query '{}': {}",
                "⚙".cyan().bold(),
                query_str.bold(),
                names.join(", ")
            );

            for (id, weight) in query_seeds {
                *weighted_seeds.entry(id).or_default() += weight;
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

    let graph = repo.build_graph();
    let selector = ContextSelector::default();

    let select_start = Instant::now();
    let (selected_symbols, markdown, auto_report_opt) = if let Some(ref model) = model_profile {
        let (selected, md, report) = selector.select_and_format_context_auto_weighted(
            &graph,
            &seed_pairs,
            *model,
            &repo.file_sources,
        );
        (selected, md, Some(report))
    } else {
        let budget = explicit_budget.unwrap();
        let (selected, md) = selector.select_and_format_context_weighted(
            &graph,
            &seed_pairs,
            budget,
            &repo.file_sources,
        );
        (selected, md, None)
    };
    let select_duration = select_start.elapsed();

    let total_tokens_used: usize = selected_symbols.iter().map(|s| s.token_cost).sum();

    if let Some(ref report) = auto_report_opt {
        eprintln!(
            "{} Auto-budget tuned to {} tokens via Knee-Curve (knee utility: {:.1}%, ceiling: {})",
            "⚡".yellow().bold(),
            report.knee_tokens.to_string().bold(),
            report.knee_utility_ratio * 100.0,
            report.optimal_budget
        );
        eprintln!(
            "{} Selected {} symbols (~{} tokens / {} auto-budget [{}]) in {:?}",
            "✓".green().bold(),
            selected_symbols.len().to_string().bold(),
            total_tokens_used.to_string().bold(),
            report.knee_tokens,
            report.model_name,
            select_duration
        );
    } else {
        eprintln!(
            "{} Selected {} symbols (~{} tokens / {} budget) in {:?}",
            "✓".green().bold(),
            selected_symbols.len().to_string().bold(),
            total_tokens_used.to_string().bold(),
            explicit_budget.unwrap(),
            select_duration
        );
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
