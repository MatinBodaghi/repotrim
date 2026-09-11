use clap::Args;
use colored::Colorize;
use repotrim_engine::{
    BenchmarkMetrics, BenchmarkRunner, BenchmarkScenario, BenchmarkSummary, ContextStrategy,
};
use serde::Serialize;
use std::path::PathBuf;
use std::time::Instant;

use crate::loader::LoadedRepository;

#[derive(Args, Debug)]
pub struct BenchmarkArgs {
    /// Target codebase directory to scan
    #[arg(short = 'p', long = "path", default_value = ".")]
    pub path: PathBuf,

    /// Override token budget for scenarios
    #[arg(short = 'b', long = "budget")]
    pub budget: Option<usize>,

    /// Specific scenario ID or name filter (e.g. 'context_selector', 'ppr_solver', 'multi_seed')
    #[arg(short = 's', long = "scenario")]
    pub scenario: Option<String>,

    /// Comma-separated strategies to benchmark: 'whole_file', 'grep', 'aider', 'vanilla', 'full', 'all'
    #[arg(long = "strategies", default_value = "all")]
    pub strategies: String,

    /// Output format: 'table' or 'markdown'
    #[arg(long = "format", default_value = "table")]
    pub format: String,

    /// Disable incremental AST caching and force full re-parsing
    #[arg(long = "no-cache")]
    pub no_cache: bool,

    /// Output results in machine-readable JSON format
    #[arg(long = "json")]
    pub json: bool,
}

#[derive(Serialize)]
struct BenchmarkCliReport {
    repository: String,
    scenarios_evaluated: usize,
    strategies_evaluated: Vec<ContextStrategy>,
    metrics: Vec<BenchmarkMetrics>,
    summary: Vec<BenchmarkSummary>,
}

pub fn execute(args: BenchmarkArgs) -> Result<(), Box<dyn std::error::Error>> {
    let start_time = Instant::now();

    if !args.json {
        eprintln!(
            "{} Ingesting repository at '{}'...",
            "⚙".cyan().bold(),
            args.path.display().to_string().bold()
        );
    }

    let mut repo = LoadedRepository::load_with_options(&args.path, !args.no_cache)?;
    repo.load_all_sources()?;

    let mut runner = BenchmarkRunner::new();

    // Filter scenarios if specified
    if let Some(ref filter) = args.scenario {
        let filter_lower = filter.to_lowercase();
        runner.scenarios.retain(|s| {
            s.id.to_lowercase().contains(&filter_lower)
                || s.name.to_lowercase().contains(&filter_lower)
        });
        if runner.scenarios.is_empty() {
            return Err(format!("No benchmark scenarios matched filter: '{filter}'").into());
        }
    }

    // Apply budget override if requested
    if let Some(budget) = args.budget {
        for s in &mut runner.scenarios {
            s.budget = budget;
        }
    }

    // Parse strategies
    let strategies = parse_strategies(&args.strategies)?;

    if !args.json {
        eprintln!(
            "{} Executing empirical evaluation across {} scenario(s) and {} strategy(ies)...",
            "🚀".yellow().bold(),
            runner.scenarios.len(),
            strategies.len()
        );
    }

    let mut all_metrics = Vec::new();
    for scenario in &runner.scenarios {
        let metrics = runner.evaluate_scenario(&repo, scenario, &strategies);
        all_metrics.extend(metrics);
    }

    let summary = BenchmarkSummary::summarize(&all_metrics);

    if args.json || args.format.eq_ignore_ascii_case("json") {
        let report = BenchmarkCliReport {
            repository: args.path.display().to_string(),
            scenarios_evaluated: runner.scenarios.len(),
            strategies_evaluated: strategies,
            metrics: all_metrics,
            summary,
        };
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }

    eprintln!(
        "{} Benchmark suite completed in {:.2?}\n",
        "✓".green().bold(),
        start_time.elapsed()
    );

    let is_markdown =
        args.format.eq_ignore_ascii_case("markdown") || args.format.eq_ignore_ascii_case("md");

    // Group metrics by scenario for per-scenario reporting
    for scenario in &runner.scenarios {
        let scenario_metrics: Vec<&BenchmarkMetrics> = all_metrics
            .iter()
            .filter(|m| m.scenario == scenario.name)
            .collect();

        if is_markdown {
            print_scenario_markdown(scenario, &scenario_metrics);
        } else {
            print_scenario_table(scenario, &scenario_metrics);
        }
        println!();
    }

    // Print overall summary table
    if is_markdown {
        print_summary_markdown(&summary);
    } else {
        print_summary_table(&summary);
    }

    Ok(())
}

fn parse_strategies(input: &str) -> Result<Vec<ContextStrategy>, Box<dyn std::error::Error>> {
    let input_trim = input.trim().to_lowercase();
    if input_trim == "all" {
        return Ok(ContextStrategy::all().to_vec());
    }

    let mut result = Vec::new();
    for part in input_trim.split(',') {
        let p = part.trim();
        match p {
            "whole_file" | "wholefile" | "dump" => result.push(ContextStrategy::WholeFile),
            "grep" | "naive_grep" | "keyword" => result.push(ContextStrategy::NaiveGrep),
            "aider" | "aider_repo_map" | "repomap" => result.push(ContextStrategy::AiderRepoMap),
            "vanilla" | "repotrim_vanilla" => result.push(ContextStrategy::RepoTrimVanilla),
            "full" | "repotrim_full" | "repotrim" => result.push(ContextStrategy::RepoTrimFull),
            other => return Err(format!("Unknown context strategy '{other}'. Valid options: whole_file, grep, aider, vanilla, full, all").into()),
        }
    }

    if result.is_empty() {
        return Ok(ContextStrategy::all().to_vec());
    }
    result.dedup();
    Ok(result)
}

fn print_scenario_table(scenario: &BenchmarkScenario, metrics: &[&BenchmarkMetrics]) {
    println!(
        "{} {} (Budget: {} tokens)",
        "Scenario:".bold().cyan(),
        scenario.name.bold(),
        scenario.budget.to_string().yellow()
    );
    if !scenario.seeds.is_empty() {
        println!("  Seeds: {}", scenario.seeds.join(", ").dimmed());
    }
    if let Some(ref q) = scenario.query {
        println!("  Query: \"{}\"", q.dimmed());
    }
    println!("  {}", scenario.description.italic());
    println!();

    println!(
        "{:<28} | {:>6} | {:>7} | {:>8} | {:>8} | {:>7} | {:>8} | {:>7} | {:>5} | {:>8}",
        "Strategy",
        "Tokens",
        "Reduct%",
        "DirRec%",
        "TrnRec%",
        "Precis%",
        "Cohesion",
        "Orphans",
        "Syms",
        "Latency"
    );
    println!("{:-<122}", "");

    for m in metrics {
        let budget_color = if m.budget_adherence {
            m.tokens_used.to_string().green()
        } else {
            m.tokens_used.to_string().red()
        };

        let strategy_display = if m.strategy == ContextStrategy::RepoTrimFull {
            m.strategy_name.bold().green()
        } else if m.strategy == ContextStrategy::AiderRepoMap {
            m.strategy_name.yellow()
        } else {
            m.strategy_name.normal()
        };

        let latency_display = if m.execution_latency_us >= 1000 {
            format!("{:.2}ms", m.execution_latency_us as f64 / 1000.0)
        } else {
            format!("{}µs", m.execution_latency_us)
        };

        println!(
            "{:<28} | {:>15} | {:>6.1}% | {:>7.1}% | {:>7.1}% | {:>6.1}% | {:>7.1}% | {:>6.1}% | {:>5} | {:>8}",
            strategy_display,
            budget_color,
            m.token_reduction_pct,
            m.direct_dep_recall_pct,
            m.transitive_dep_recall_pct,
            m.context_precision_pct,
            m.community_cohesion_pct,
            m.orphan_rate_pct,
            m.symbol_count,
            latency_display
        );
    }
}

fn print_scenario_markdown(scenario: &BenchmarkScenario, metrics: &[&BenchmarkMetrics]) {
    println!("### Scenario: {}", scenario.name);
    println!("- **Budget:** {} tokens", scenario.budget);
    if !scenario.seeds.is_empty() {
        println!("- **Seeds:** `{}`", scenario.seeds.join("`, `"));
    }
    if let Some(ref q) = scenario.query {
        println!("- **Query:** \"{}\"", q);
    }
    println!("- **Description:** {}", scenario.description);
    println!();
    println!("| Strategy | Tokens | Reduction | Direct Recall | Transitive Recall | Precision | Cohesion | Orphans | Symbols | Latency |");
    println!("| :--- | :---: | :---: | :---: | :---: | :---: | :---: | :---: | :---: | :---: |");

    for m in metrics {
        let latency_display = if m.execution_latency_us >= 1000 {
            format!("{:.2}ms", m.execution_latency_us as f64 / 1000.0)
        } else {
            format!("{}µs", m.execution_latency_us)
        };

        let strategy_str = if m.strategy == ContextStrategy::RepoTrimFull {
            format!("**{}**", m.strategy_name)
        } else {
            m.strategy_name.clone()
        };

        println!(
            "| {} | {} | {:.1}% | {:.1}% | {:.1}% | {:.1}% | {:.1}% | {:.1}% | {} | {} |",
            strategy_str,
            m.tokens_used,
            m.token_reduction_pct,
            m.direct_dep_recall_pct,
            m.transitive_dep_recall_pct,
            m.context_precision_pct,
            m.community_cohesion_pct,
            m.orphan_rate_pct,
            m.symbol_count,
            latency_display
        );
    }
}

fn print_summary_table(summaries: &[BenchmarkSummary]) {
    println!(
        "{}",
        "== Aggregate Benchmark Summary (Mean across Scenarios) =="
            .bold()
            .cyan()
    );
    println!(
        "{:<28} | {:>6} | {:>7} | {:>8} | {:>8} | {:>7} | {:>8} | {:>7} | {:>8}",
        "Strategy",
        "Tokens",
        "Reduct%",
        "DirRec%",
        "TrnRec%",
        "Precis%",
        "Cohesion",
        "Orphans",
        "Latency"
    );
    println!("{:-<114}", "");

    for s in summaries {
        let strategy_display = if s.strategy == ContextStrategy::RepoTrimFull {
            s.strategy_name.bold().green()
        } else if s.strategy == ContextStrategy::AiderRepoMap {
            s.strategy_name.yellow()
        } else {
            s.strategy_name.normal()
        };

        let latency_display = if s.mean_latency_us >= 1000 {
            format!("{:.2}ms", s.mean_latency_us as f64 / 1000.0)
        } else {
            format!("{}µs", s.mean_latency_us)
        };

        println!(
            "{:<28} | {:>6} | {:>6.1}% | {:>7.1}% | {:>7.1}% | {:>6.1}% | {:>7.1}% | {:>6.1}% | {:>8}",
            strategy_display,
            s.mean_tokens,
            s.mean_token_reduction_pct,
            s.mean_direct_recall_pct,
            s.mean_transitive_recall_pct,
            s.mean_precision_pct,
            s.mean_cohesion_pct,
            s.mean_orphan_rate_pct,
            latency_display
        );
    }
}

fn print_summary_markdown(summaries: &[BenchmarkSummary]) {
    println!("## Aggregate Benchmark Summary");
    println!();
    println!("| Strategy | Mean Tokens | Token Reduction | Direct Recall | Transitive Recall | Precision | Cohesion | Orphans | Mean Latency |");
    println!("| :--- | :---: | :---: | :---: | :---: | :---: | :---: | :---: | :---: |");

    for s in summaries {
        let latency_display = if s.mean_latency_us >= 1000 {
            format!("{:.2}ms", s.mean_latency_us as f64 / 1000.0)
        } else {
            format!("{}µs", s.mean_latency_us)
        };

        let strategy_str = if s.strategy == ContextStrategy::RepoTrimFull {
            format!("**{}**", s.strategy_name)
        } else {
            s.strategy_name.clone()
        };

        println!(
            "| {} | {} | {:.1}% | {:.1}% | {:.1}% | {:.1}% | {:.1}% | {:.1}% | {} |",
            strategy_str,
            s.mean_tokens,
            s.mean_token_reduction_pct,
            s.mean_direct_recall_pct,
            s.mean_transitive_recall_pct,
            s.mean_precision_pct,
            s.mean_cohesion_pct,
            s.mean_orphan_rate_pct,
            latency_display
        );
    }
}
