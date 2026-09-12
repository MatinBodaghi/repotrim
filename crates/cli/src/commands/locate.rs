use clap::Args;
use colored::Colorize;
use repotrim_engine::{RankedEntrypoint, TaskContext};
use serde::Serialize;
use std::path::PathBuf;
use std::time::Instant;

use crate::loader::LoadedRepository;

#[derive(Args, Debug)]
pub struct LocateArgs {
    /// Natural language task prompt, issue description, or symbol hint
    pub query: String,

    /// Maximum number of ranked candidate entrypoints to return
    #[arg(short = 'n', long = "limit", default_value = "10")]
    pub limit: usize,

    /// Target file paths or globs of interest to prioritize
    #[arg(short = 't', long = "target-file")]
    pub target_files: Vec<PathBuf>,

    /// Target codebase directory to scan
    #[arg(short = 'p', long = "path", default_value = ".")]
    pub path: PathBuf,

    /// Disable incremental AST caching and force full re-parsing
    #[arg(long = "no-cache")]
    pub no_cache: bool,

    /// Output results in machine-readable JSON format
    #[arg(long = "json")]
    pub json: bool,
}

#[derive(Serialize)]
struct LocateCliReport {
    query: String,
    target_files: Vec<PathBuf>,
    total_candidates: usize,
    entrypoints: Vec<RankedEntrypoint>,
}

pub fn execute(args: LocateArgs) -> Result<(), Box<dyn std::error::Error>> {
    let start_time = Instant::now();

    if !args.json {
        eprintln!(
            "{} Discovering entrypoints at '{}' for: '{}' (limit: {})...",
            "⚙".cyan().bold(),
            args.path.display().to_string().bold(),
            args.query.bold(),
            args.limit
        );
    }

    let mut repo = LoadedRepository::load_with_options(&args.path, !args.no_cache)?;
    repo.load_all_sources()?;

    let graph = repo.build_graph();
    let intel = repo.intelligence(&graph);

    let mut task = TaskContext::from_query(&args.query);
    if !args.target_files.is_empty() {
        task.metadata.target_files = args.target_files.clone();
    }

    let entrypoints = intel.locate(&task, args.limit);
    let elapsed = start_time.elapsed();

    if args.json {
        let report = LocateCliReport {
            query: args.query,
            target_files: args.target_files,
            total_candidates: entrypoints.len(),
            entrypoints,
        };
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }

    println!();
    println!(
        "{}",
        "=== REPOTRIM TASK-CONDITIONED ENTRYPOINT DISCOVERY ==="
            .cyan()
            .bold()
    );
    println!(
        "Query: {} | Discovered: {} entrypoints in {:.2}ms",
        args.query.bold(),
        entrypoints.len().to_string().green().bold(),
        elapsed.as_secs_f64() * 1000.0
    );
    println!();

    if entrypoints.is_empty() {
        println!("{}", "No matching entrypoints found.".yellow());
        return Ok(());
    }

    for (idx, entry) in entrypoints.iter().enumerate() {
        let rank_str = format!("#{}", idx + 1);
        let score_pct = (entry.score * 100.0).round() as usize;
        let score_colored = if entry.score >= 0.8 {
            format!("{:.2} ({}%)", entry.score, score_pct)
                .green()
                .bold()
        } else if entry.score >= 0.5 {
            format!("{:.2} ({}%)", entry.score, score_pct)
                .yellow()
                .bold()
        } else {
            format!("{:.2} ({}%)", entry.score, score_pct).red()
        };

        println!(
            "{} {} ({:?}) - Score: {}",
            rank_str.bold(),
            entry.symbol.name.cyan().bold(),
            entry.symbol.kind,
            score_colored
        );
        println!(
            "   Location: {}:{}-{}",
            entry.symbol.file_path.display().to_string().dimmed(),
            entry.symbol.span.start_row + 1,
            entry.symbol.span.end_row + 1
        );
        println!("   Reason:   {}", entry.reason.dimmed());
        if !entry.symbol.signature.trim().is_empty() {
            println!("   Signature: {}", entry.symbol.signature.dimmed());
        }
        println!();
    }

    Ok(())
}
