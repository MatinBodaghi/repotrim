use clap::Args;
use colored::Colorize;
use repotrim_engine::{HarnessBenchmarkRunner, HarnessComparisonReport};
use std::fs;
use std::path::PathBuf;
use std::time::Instant;

use crate::loader::LoadedRepository;

#[derive(Args, Debug)]
pub struct HarnessArgs {
    /// Target codebase directory to scan
    #[arg(short = 'p', long = "path", default_value = ".")]
    pub path: PathBuf,

    /// Filter scenario by ID or name
    #[arg(short = 's', long = "scenario")]
    pub scenario: Option<String>,

    /// Output format: 'table', 'markdown', or 'json'
    #[arg(long = "format", default_value = "table")]
    pub format: String,

    /// Optional file path to write report output
    #[arg(short = 'o', long = "output")]
    pub output: Option<PathBuf>,

    /// Disable incremental AST caching and force full re-parsing
    #[arg(long = "no-cache")]
    pub no_cache: bool,

    /// Output results in machine-readable JSON format
    #[arg(long = "json")]
    pub json: bool,
}

pub fn execute(args: HarnessArgs) -> Result<(), Box<dyn std::error::Error>> {
    let start_time = Instant::now();

    if !args.json && !args.format.eq_ignore_ascii_case("json") {
        eprintln!(
            "{} Ingesting repository at '{}'...",
            "⚙".cyan().bold(),
            args.path.display().to_string().bold()
        );
    }

    let mut repo = LoadedRepository::load_with_options(&args.path, !args.no_cache)?;
    repo.load_all_sources()?;

    let mut runner = HarnessBenchmarkRunner::new();

    if let Some(ref filter) = args.scenario {
        let filter_lower = filter.to_lowercase();
        runner.scenarios.retain(|s| {
            s.id.to_lowercase().contains(&filter_lower)
                || s.name.to_lowercase().contains(&filter_lower)
        });
        if runner.scenarios.is_empty() {
            return Err(format!("No harness scenarios matched filter: '{filter}'").into());
        }
    }

    if !args.json && !args.format.eq_ignore_ascii_case("json") {
        eprintln!(
            "{} Evaluating agent exploration harness across {} scenario(s)...",
            "🚀".yellow().bold(),
            runner.scenarios.len()
        );
    }

    let mut comparisons = Vec::new();
    for scenario in &runner.scenarios {
        let (_, _, comp) = runner.evaluate_scenario(&repo, scenario);
        comparisons.push(comp);
    }

    let report = HarnessComparisonReport::summarize(comparisons);

    let is_json = args.json || args.format.eq_ignore_ascii_case("json");
    let is_markdown =
        args.format.eq_ignore_ascii_case("markdown") || args.format.eq_ignore_ascii_case("md");

    let output_text = if is_json {
        serde_json::to_string_pretty(&report)?
    } else if is_markdown {
        report.render_markdown()
    } else {
        report.render_table()
    };

    if let Some(ref out_path) = args.output {
        if let Some(parent) = out_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(out_path, &output_text)?;
        if !is_json {
            eprintln!(
                "{} Report written to '{}'",
                "✓".green().bold(),
                out_path.display()
            );
        }
    } else {
        println!("{output_text}");
    }

    if !is_json {
        eprintln!(
            "{} Harness evaluation completed in {:.2?}\n",
            "✓".green().bold(),
            start_time.elapsed()
        );
    }

    Ok(())
}
