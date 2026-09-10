use clap::Args;
use colored::Colorize;
use repotrim_engine::{DiffResolver, ImpactAnalyzer, ImpactReport, RiskLevel, TokenizerModel};
use std::path::PathBuf;
use std::time::Instant;

use crate::loader::LoadedRepository;

#[derive(Args, Debug)]
pub struct ImpactArgs {
    /// Target symbol name to evaluate blast radius for
    #[arg(short = 's', long = "symbol")]
    pub symbol: Option<String>,

    /// Infer modified symbols from uncommitted git changes (default if no symbol specified)
    #[arg(long = "diff")]
    pub diff: bool,

    /// Optional git revision or branch to diff against (e.g. 'origin/main', 'HEAD~1')
    #[arg(long = "diff-against")]
    pub diff_against: Option<String>,

    /// Maximum token budget for blast radius context outline
    #[arg(short = 'b', long = "budget", default_value = "1000")]
    pub budget: String,

    /// Tokenizer model for blast radius token budgeting ('fast', 'calibrated', 'exact' / 'cl100k', 'o200k')
    #[arg(long = "tokenizer", default_value = "fast")]
    pub tokenizer: String,

    /// Output serialization format (markdown or json)
    #[arg(long = "format", default_value = "markdown")]
    pub format: String,

    /// Target codebase directory to scan
    #[arg(short = 'p', long = "path", default_value = ".")]
    pub path: PathBuf,

    /// Disable incremental AST caching and force full re-parsing
    #[arg(long = "no-cache")]
    pub no_cache: bool,
}

pub fn execute(args: ImpactArgs) -> Result<(), Box<dyn std::error::Error>> {
    let start_time = Instant::now();

    let mut repo = LoadedRepository::load_with_options(&args.path, !args.no_cache)?;
    repo.load_all_sources()?;
    let graph = repo.build_graph();

    let budget: usize = if args.budget == "auto" {
        1500
    } else {
        args.budget.parse().unwrap_or(1000)
    };

    let tokenizer_model = TokenizerModel::from_str_name(&args.tokenizer).unwrap_or_else(|| {
        eprintln!(
            "{} Unknown tokenizer '{}', falling back to 'fast'",
            "!".yellow().bold(),
            args.tokenizer
        );
        TokenizerModel::FastHeuristic
    });

    let report: ImpactReport = if let Some(sym_name) = &args.symbol {
        eprintln!(
            "{} Tracing change blast radius for symbol '{}' (tokenizer: {})...",
            "⚙".cyan().bold(),
            sym_name.bold(),
            tokenizer_model.name().cyan()
        );
        ImpactAnalyzer::analyze_symbol_with_model(
            &graph,
            sym_name,
            budget,
            &repo.file_sources,
            tokenizer_model,
        )
    } else if let Some(rev) = &args.diff_against {
        eprintln!(
            "{} Extracting git diff against '{}' (tokenizer: {})...",
            "⚙".cyan().bold(),
            rev.bold(),
            tokenizer_model.name().cyan()
        );
        let diff_text = DiffResolver::get_git_diff_against(&args.path, rev)?;
        ImpactAnalyzer::analyze_diff_with_model(
            &graph,
            &diff_text,
            budget,
            &repo.file_sources,
            tokenizer_model,
        )
    } else {
        eprintln!(
            "{} Extracting uncommitted git changes (tokenizer: {})...",
            "⚙".cyan().bold(),
            tokenizer_model.name().cyan()
        );
        let diff_text = DiffResolver::get_git_diff(&args.path)?;
        if diff_text.trim().is_empty() {
            eprintln!(
                "{} Working tree has no uncommitted changes to analyze. Use '--symbol <name>' or '--diff-against <rev>' to inspect specific targets.",
                "!".yellow().bold()
            );
        }
        ImpactAnalyzer::analyze_diff_with_model(
            &graph,
            &diff_text,
            budget,
            &repo.file_sources,
            tokenizer_model,
        )
    };

    let elapsed = start_time.elapsed();

    if args.format == "json" {
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }

    // Terminal Markdown Output
    let badge = match report.summary.risk_level {
        RiskLevel::Critical => "CRITICAL".on_red().white().bold(),
        RiskLevel::High => "HIGH".on_yellow().black().bold(),
        RiskLevel::Medium => "MEDIUM".on_cyan().black().bold(),
        RiskLevel::Low => "LOW".on_green().black().bold(),
    };

    eprintln!(
        "\n{} Change Impact Analysis completed in {:?}",
        "✓".green().bold(),
        elapsed
    );
    eprintln!(
        "   Architectural Risk: {} (Score: {:.2})",
        badge, report.summary.risk_score
    );
    eprintln!(
        "   Directly Mutated:   {} symbol(s)",
        report.summary.mutated_count.to_string().bold()
    );
    eprintln!(
        "   1st-Order Callers:  {} symbol(s)",
        report.summary.direct_impact_count.to_string().bold()
    );
    eprintln!(
        "   Transitive Ripple:  {} symbol(s)",
        report.summary.transitive_impact_count.to_string().bold()
    );
    eprintln!(
        "   Affected Tests:     {} test(s)",
        report.summary.affected_tests_count.to_string().bold()
    );
    eprintln!(
        "   Files Impacted:     {} file(s)\n",
        report.summary.affected_files_count.to_string().bold()
    );

    if !report.affected_tests.is_empty() {
        eprintln!(
            "{}",
            "Recommended Test Suite Targets to Run:".yellow().bold()
        );
        for t in &report.affected_tests {
            eprintln!(
                "   • {} in {}:{}",
                t.name.bold(),
                t.file_path.display(),
                t.span.start_row + 1
            );
        }
        eprintln!();
    }

    print!("{}", report.context_markdown);

    Ok(())
}
