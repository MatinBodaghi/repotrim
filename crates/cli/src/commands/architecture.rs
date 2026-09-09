use clap::Args;
use colored::Colorize;
use repotrim_engine::ArchitectureReport;
use std::fs;
use std::path::PathBuf;
use std::time::Instant;

use crate::loader::LoadedRepository;

#[derive(Args, Debug)]
pub struct ArchitectureArgs {
    /// Target codebase directory to scan
    #[arg(short = 'p', long = "path", default_value = ".")]
    pub path: PathBuf,

    /// Destination file to write architectural documentation (defaults to 'docs/ARCHITECTURE.md', or '-' for stdout)
    #[arg(short = 'o', long = "output", default_value = "docs/ARCHITECTURE.md")]
    pub output: String,

    /// Disable incremental AST caching and force full re-parsing
    #[arg(long = "no-cache")]
    pub no_cache: bool,
}

pub fn execute(args: ArchitectureArgs) -> Result<(), Box<dyn std::error::Error>> {
    let start_time = Instant::now();

    eprintln!(
        "{} Analyzing codebase architecture at '{}'...",
        "⚙".cyan().bold(),
        args.path.display().to_string().bold()
    );

    let mut repo = LoadedRepository::load_with_options(&args.path, !args.no_cache)?;
    repo.load_all_sources()?;

    let graph = repo.build_graph();
    let report = ArchitectureReport::analyze(&graph, &repo.root_path);
    let markdown_content = report.to_markdown();

    if args.output == "-" {
        println!("{}", markdown_content);
    } else {
        let out_path = PathBuf::from(&args.output);
        if let Some(parent) = out_path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent)?;
            }
        }
        fs::write(&out_path, &markdown_content)?;

        eprintln!(
            "{} Successfully generated architecture specification in '{}' ({:?})",
            "✓".green().bold(),
            out_path.display().to_string().bold(),
            start_time.elapsed()
        );
        eprintln!(
            "  • Subsystems: {} | Symbols: {} | Edges: {} | Modularity (Q): {:.3}",
            report.subsystems.len().to_string().bold(),
            report.total_symbols.to_string().bold(),
            report.total_edges.to_string().bold(),
            report.modularity
        );
    }

    Ok(())
}
