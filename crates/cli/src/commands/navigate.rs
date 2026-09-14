use clap::{Args, ValueEnum};
use colored::Colorize;
use repotrim_engine::{AdaptiveNavigator, NavigationTrajectory, NavigatorConfig, TaskContext};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::Instant;

use crate::loader::LoadedRepository;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NavigateFormat {
    Text,
    Json,
}

#[derive(Args, Debug)]
pub struct NavigateArgs {
    /// Natural language task prompt, issue description, or query
    pub query: String,

    /// Initial token budget ceiling for sequential exploration
    #[arg(short = 'b', long = "budget", default_value = "2000")]
    pub budget: u32,

    /// Maximum number of exploration steps
    #[arg(short = 's', long = "max-steps", default_value = "15")]
    pub max_steps: usize,

    /// Target codebase directory to scan
    #[arg(short = 'p', long = "path", default_value = ".")]
    pub path: PathBuf,

    /// Output format (text or json)
    #[arg(short = 'f', long = "format", value_enum, default_value_t = NavigateFormat::Text)]
    pub format: NavigateFormat,

    /// Disable incremental AST caching and force full re-parsing
    #[arg(long = "no-cache")]
    pub no_cache: bool,
}

pub fn execute(args: NavigateArgs) -> Result<(), Box<dyn std::error::Error>> {
    let start_time = Instant::now();

    if args.format == NavigateFormat::Text {
        eprintln!(
            "{} Navigating codebase at '{}' for: '{}' (budget: {} tokens, max steps: {})...",
            "⚙".cyan().bold(),
            args.path.display().to_string().bold(),
            args.query.bold(),
            args.budget,
            args.max_steps
        );
    }

    let mut repo = LoadedRepository::load_with_options(&args.path, !args.no_cache)?;
    repo.load_all_sources()?;

    let graph = repo.build_graph();
    let intel = repo.intelligence(&graph);

    let task = TaskContext::from_query(&args.query);

    let config = NavigatorConfig {
        max_steps: args.max_steps,
        ..Default::default()
    };
    let navigator = AdaptiveNavigator::new(config);

    let trajectory: NavigationTrajectory = navigator.navigate(task, args.budget, &intel)?;
    let elapsed = start_time.elapsed();

    if args.format == NavigateFormat::Json {
        println!("{}", serde_json::to_string_pretty(&trajectory)?);
        return Ok(());
    }

    // Pretty text output
    println!(
        "\n{}",
        "=== RepoTrim Autonomous Sequential Navigation ==="
            .cyan()
            .bold()
    );
    println!(
        "{}: {}",
        "Task Query".bold(),
        trajectory.task.query.yellow()
    );
    println!(
        "{}: {} steps | {}/{} tokens used ({:.1}%) | Time: {:.2?}",
        "Trajectory Stats".bold(),
        trajectory.step_count(),
        trajectory.tokens_used(),
        trajectory.initial_budget,
        (trajectory.tokens_used() as f64 / trajectory.initial_budget.max(1) as f64) * 100.0,
        elapsed
    );
    println!(
        "{}: {}",
        "Termination Reason".bold(),
        trajectory.termination_reason.green()
    );

    println!(
        "\n{}",
        "--- Sequential Interaction Trajectory (H_t) ---".bold()
    );
    println!(
        "{:<4} | {:<10} | {:<6} | {:<8} | {}",
        "Step".bold(),
        "Action".bold(),
        "Cost".bold(),
        "Left".bold(),
        "Observation Summary".bold()
    );
    println!(
        "{:-<4}-+-{:-<10}-+-{:-<6}-+-{:-<8}-+-----------------------------------",
        "", "", "", ""
    );

    for step in &trajectory.steps {
        println!(
            "{:<4} | {:<10} | {:<6} | {:<8} | {}",
            format!("#{}", step.step_index).dimmed(),
            step.action.action_type().bright_magenta(),
            format!("{}t", step.cost.total_tokens).yellow(),
            format!("{}t", step.remaining_budget_after).dimmed(),
            step.observation.summary()
        );
    }

    println!(
        "\n{}",
        "--- Synthesized Structured Context Evidence ---".bold()
    );
    let ctx = &trajectory.structured_context;
    println!(
        "Symbols Captured: {} | Edges Resolved: {} | Paths Discovered: {} | Confidence: {:.1}%",
        ctx.symbols.len().to_string().green(),
        ctx.edges.len().to_string().green(),
        ctx.paths.len().to_string().green(),
        ctx.confidence_score * 100.0
    );

    if !ctx.symbols.is_empty() {
        println!("\n{}", "Key Discovered Symbols:".bold());
        for sym in ctx.symbols.iter().take(8) {
            println!(
                "  • {:<12} {:<30} (cost: {}t, lod: {:?})",
                format!("{:?}", sym.node_type).cyan(),
                sym.name.bold(),
                sym.token_cost,
                sym.lod
            );
        }
        if ctx.symbols.len() > 8 {
            println!("    ... and {} more symbols", ctx.symbols.len() - 8);
        }
    }

    if !ctx.paths.is_empty() {
        println!("\n{}", "Resolved Causal Paths:".bold());
        for (i, p) in ctx.paths.iter().enumerate().take(3) {
            println!(
                "  Path #{}: {} ({} hops, prob: {:.3})",
                i + 1,
                p.trace.yellow(),
                p.relations.len(),
                p.probability
            );
        }
    }

    Ok(())
}
