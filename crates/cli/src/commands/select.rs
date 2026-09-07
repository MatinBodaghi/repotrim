use clap::{Args, ValueEnum};
use colored::Colorize;
use repotrim_engine::{ContextSelector, SymbolId};
use serde::{Deserialize, Serialize};
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
    #[arg(short = 's', long = "seed", required = true)]
    pub seed: Vec<String>,

    /// Maximum token budget for selected context
    #[arg(short = 'b', long = "budget", default_value_t = 1000)]
    pub budget: usize,

    /// Target codebase directory to scan
    #[arg(short = 'p', long = "path", default_value = ".")]
    pub path: PathBuf,

    /// Output serialization format (markdown or json)
    #[arg(short = 'f', long = "format", value_enum, default_value_t = OutputFormat::Markdown)]
    pub format: OutputFormat,

    /// Optional file destination to write output (defaults to stdout)
    #[arg(short = 'o', long = "output")]
    pub output: Option<PathBuf>,
}

pub fn execute(args: SelectArgs) -> Result<(), Box<dyn std::error::Error>> {
    let start_time = Instant::now();

    eprintln!(
        "{} Scanning repository at '{}'...",
        "⚙".cyan().bold(),
        args.path.display().to_string().bold()
    );

    let repo = LoadedRepository::load(&args.path)?;
    let load_time = start_time.elapsed();

    eprintln!(
        "{} Ingested {} symbols across {} files in {:?}",
        "✓".green().bold(),
        repo.symbols.len().to_string().bold(),
        repo.file_sources.len().to_string().bold(),
        load_time
    );

    // Resolve seed strings to SymbolIds
    let mut seed_ids: Vec<SymbolId> = Vec::new();
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
            seed_ids.extend(matches);
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

    if seed_ids.is_empty() {
        return Err(format!(
            "None of the provided seed identifiers ({}) could be resolved in {}",
            args.seed.join(", "),
            args.path.display()
        )
        .into());
    }

    eprintln!(
        "{} Building multiplex graph & running CELF submodular knapsack...",
        "⚙".cyan().bold()
    );

    let graph = repo.build_graph();
    let selector = ContextSelector::default();

    let select_start = Instant::now();
    let (selected_symbols, markdown) =
        selector.select_and_format_context(&graph, &seed_ids, args.budget, &repo.file_sources);
    let select_duration = select_start.elapsed();

    let total_tokens_used: usize = selected_symbols.iter().map(|s| s.token_cost).sum();
    eprintln!(
        "{} Selected {} symbols (~{} tokens / {} budget) in {:?}",
        "✓".green().bold(),
        selected_symbols.len().to_string().bold(),
        total_tokens_used.to_string().bold(),
        args.budget,
        select_duration
    );

    let output_content = match args.format {
        OutputFormat::Markdown => markdown,
        OutputFormat::Json => {
            let json_obj = serde_json::json!({
                "budget": args.budget,
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
