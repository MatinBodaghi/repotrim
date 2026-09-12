use clap::{Args, ValueEnum};
use colored::Colorize;
use repotrim_engine::LocalExpansion;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::Instant;

use crate::loader::LoadedRepository;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ExpandFormat {
    Markdown,
    Json,
}

#[derive(Args, Debug)]
pub struct ExpandArgs {
    /// Focal symbol name to expand context around
    pub symbol: String,

    /// Maximum token budget ceiling for expansion cluster
    #[arg(short = 'b', long = "budget", default_value = "1000")]
    pub budget: usize,

    /// Target codebase directory to scan
    #[arg(short = 'p', long = "path", default_value = ".")]
    pub path: PathBuf,

    /// Output serialization format (markdown or json)
    #[arg(short = 'f', long = "format", value_enum, default_value_t = ExpandFormat::Markdown)]
    pub format: ExpandFormat,

    /// Disable incremental AST caching and force full re-parsing
    #[arg(long = "no-cache")]
    pub no_cache: bool,
}

pub fn execute(args: ExpandArgs) -> Result<(), Box<dyn std::error::Error>> {
    let start_time = Instant::now();

    if args.format != ExpandFormat::Json {
        eprintln!(
            "{} Expanding local context around '{}' (budget: {} tokens) in '{}'...",
            "⚙".cyan().bold(),
            args.symbol.bold(),
            args.budget,
            args.path.display().to_string().bold()
        );
    }

    let mut repo = LoadedRepository::load_with_options(&args.path, !args.no_cache)?;
    repo.load_all_sources()?;

    let graph = repo.build_graph();
    let symbols = graph.symbols();

    let focal_sym = symbols
        .iter()
        .find(|s| s.name.eq_ignore_ascii_case(&args.symbol))
        .ok_or_else(|| format!("Focal symbol '{}' was not found in codebase", args.symbol))?;

    let intel = repo.intelligence(&graph);
    let expansion: LocalExpansion = intel.expand(focal_sym.id, args.budget, None)?;
    let elapsed = start_time.elapsed();

    if args.format == ExpandFormat::Json {
        println!("{}", serde_json::to_string_pretty(&expansion)?);
        return Ok(());
    }

    println!();
    println!(
        "{}",
        "=== REPOTRIM LOCAL SUBMODULAR CONTEXT EXPANSION ==="
            .cyan()
            .bold()
    );
    println!(
        "Focal Symbol: {} ({}:L{})",
        expansion.focal_symbol.name.green().bold(),
        expansion.focal_symbol.file_path.display(),
        expansion.focal_symbol.span.start_row + 1
    );
    println!(
        "Budget: {} | Tokens Used: {} ({:.1}%) | Cluster: {} symbols | Time: {:.2}ms",
        expansion.budget,
        expansion.tokens_used.to_string().bold(),
        (expansion.tokens_used as f64 / expansion.budget.max(1) as f64) * 100.0,
        expansion.symbols.len().to_string().bold(),
        elapsed.as_secs_f64() * 1000.0
    );
    println!();

    if !expansion.paths.is_empty() {
        println!("{}", "Causal Paths to Cluster Symbols:".cyan().bold());
        for (i, path) in expansion.paths.iter().take(5).enumerate() {
            let mut step_strs = Vec::new();
            for &node_id in &path.nodes {
                let name = symbols
                    .get(node_id.0 as usize)
                    .map(|s| s.name.as_str())
                    .unwrap_or("Unknown");
                step_strs.push(name);
            }
            println!(
                "  #{}: {} (Prob: {:.1}%)",
                i + 1,
                step_strs.join(" -> ").dimmed(),
                path.probability * 100.0
            );
        }
        println!();
    }

    println!("{}", "Expanded Source Context:".bold());
    println!("{}", expansion.formatted_code);

    Ok(())
}
