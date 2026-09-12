use clap::Args;
use colored::Colorize;
use repotrim_engine::{CausalTraceResult, TaskContext};
use std::path::PathBuf;
use std::time::Instant;

use crate::loader::LoadedRepository;

#[derive(Args, Debug)]
pub struct TraceArgs {
    /// Source entrypoint symbol name
    pub source: String,

    /// Target destination symbol name
    pub target: String,

    /// Optional natural language task prompt for task-conditioned energy scoring
    #[arg(short = 't', long = "task")]
    pub task: Option<String>,

    /// Target codebase directory to scan
    #[arg(short = 'p', long = "path", default_value = ".")]
    pub path: PathBuf,

    /// Disable rendering of Mermaid sequence diagram
    #[arg(long = "no-mermaid")]
    pub no_mermaid: bool,

    /// Disable incremental AST caching and force full re-parsing
    #[arg(long = "no-cache")]
    pub no_cache: bool,

    /// Output results in machine-readable JSON format
    #[arg(long = "json")]
    pub json: bool,
}

pub fn execute(args: TraceArgs) -> Result<(), Box<dyn std::error::Error>> {
    let start_time = Instant::now();

    if !args.json {
        eprintln!(
            "{} Tracing causal paths from '{}' to '{}' in '{}'...",
            "⚙".cyan().bold(),
            args.source.bold(),
            args.target.bold(),
            args.path.display().to_string().bold()
        );
    }

    let mut repo = LoadedRepository::load_with_options(&args.path, !args.no_cache)?;
    repo.load_all_sources()?;

    let graph = repo.build_graph();
    let symbols = graph.symbols();

    let src_sym = symbols
        .iter()
        .find(|s| s.name.eq_ignore_ascii_case(&args.source))
        .ok_or_else(|| format!("Source symbol '{}' was not found in codebase", args.source))?;

    let tgt_sym = symbols
        .iter()
        .find(|s| s.name.eq_ignore_ascii_case(&args.target))
        .ok_or_else(|| format!("Target symbol '{}' was not found in codebase", args.target))?;

    let intel = repo.intelligence(&graph);
    let task_ctx = args.task.as_deref().map(TaskContext::from_query);

    let trace_result: CausalTraceResult = intel.trace(src_sym.id, tgt_sym.id, task_ctx.as_ref())?;
    let elapsed = start_time.elapsed();

    if args.json {
        println!("{}", serde_json::to_string_pretty(&trace_result)?);
        return Ok(());
    }

    println!();
    println!(
        "{}",
        "=== REPOTRIM CAUSAL PATH TRACE & SEQUENCE RECONSTRUCTION ==="
            .cyan()
            .bold()
    );
    println!(
        "Source: {} ({}:L{})",
        trace_result.source.name.green().bold(),
        trace_result.source.file_path.display(),
        trace_result.source.span.start_row + 1
    );
    println!(
        "Target: {} ({}:L{})",
        trace_result.target.name.red().bold(),
        trace_result.target.file_path.display(),
        trace_result.target.span.start_row + 1
    );
    println!(
        "Discovered: {} paths in {:.2}ms",
        trace_result.paths.len().to_string().bold(),
        elapsed.as_secs_f64() * 1000.0
    );
    println!();

    if trace_result.paths.is_empty() {
        println!("{}", "No causal path discovered between symbols.".yellow());
        return Ok(());
    }

    if let Some(ref best) = trace_result.best_path {
        let prob_pct = (best.probability * 100.0).round() as usize;
        println!(
            "{}",
            format!(
                "Rank #1 (Best Path - Probability: {}% | Energy Score: {:.2}):",
                prob_pct, best.score
            )
            .green()
            .bold()
        );

        let mut step_strs = Vec::new();
        for &node_id in &best.nodes {
            let name = symbols
                .get(node_id.0 as usize)
                .map(|s| s.name.as_str())
                .unwrap_or("Unknown");
            step_strs.push(name);
        }

        let mut path_display = String::new();
        for (i, name) in step_strs.iter().enumerate() {
            if i > 0 {
                let rel = best
                    .relations
                    .get(i - 1)
                    .copied()
                    .unwrap_or(repotrim_engine::RelationType::Calls);
                path_display.push_str(&format!(" --[{:?}]--> ", rel).dimmed().to_string());
            }
            path_display.push_str(&name.bold().to_string());
        }
        println!("  {}", path_display);
        println!();
    }

    if trace_result.paths.len() > 1 {
        println!("{}", "Alternative Discovered Paths:".bold());
        for (idx, path) in trace_result.paths.iter().skip(1).take(4).enumerate() {
            let prob_pct = (path.probability * 100.0).round() as usize;
            let mut step_strs = Vec::new();
            for &node_id in &path.nodes {
                let name = symbols
                    .get(node_id.0 as usize)
                    .map(|s| s.name.as_str())
                    .unwrap_or("Unknown");
                step_strs.push(name);
            }
            println!(
                "  Path #{}: {} (Prob: {}%, Score: {:.2})",
                idx + 2,
                step_strs.join(" -> ").dimmed(),
                prob_pct,
                path.score
            );
        }
        println!();
    }

    if !args.no_mermaid && !trace_result.mermaid_diagram.is_empty() {
        println!("{}", "Sequence Diagram:".cyan().bold());
        println!("{}", trace_result.mermaid_diagram);
    }

    Ok(())
}
