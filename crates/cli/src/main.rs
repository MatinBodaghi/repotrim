use clap::{Parser, Subcommand};
use colored::Colorize;

pub mod commands;
pub mod loader;

#[derive(Parser, Debug)]
#[command(
    name = "repotrim",
    version,
    about = "Mathematically optimal codebase context trimmer for AI coding agents",
    long_about = "RepoTrim builds an in-memory multiplex code property graph and uses\nPersonalized PageRank and CELF submodular knapsack optimization to extract\noptimal prompt context within strict token budgets."
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Select mathematically optimal code context for AI prompts
    Select(commands::select::SelectArgs),
    /// Display repository graph statistics and central architectural hubs
    Stats(commands::stats::StatsArgs),
    /// Deeply inspect a symbol's graph dependencies and callers
    Inspect(commands::inspect::InspectArgs),
}

fn main() {
    let cli = Cli::parse();

    let result = match cli.command {
        Commands::Select(args) => commands::select::execute(args),
        Commands::Stats(args) => commands::stats::execute(args),
        Commands::Inspect(args) => commands::inspect::execute(args),
    };

    if let Err(e) = result {
        eprintln!("{} {}", "Error:".red().bold(), e);
        std::process::exit(1);
    }
}
