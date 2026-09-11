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
    /// Generate a structured feature blueprint with inferred seed anchors for agent harnesses
    Blueprint(commands::blueprint::BlueprintArgs),
    /// Generate evergreen repository architectural specification and Mermaid diagrams
    Architecture(commands::architecture::ArchitectureArgs),
    /// Display repository graph statistics and central architectural hubs
    Stats(commands::stats::StatsArgs),
    /// Deeply inspect a symbol's graph dependencies and callers
    Inspect(commands::inspect::InspectArgs),
    /// Clear incremental AST Merkle cache (.repotrim directory)
    Clean(commands::clean::CleanArgs),
    /// Trace semantic blast radius, ripple effects, and affected test targets of code changes
    Impact(commands::impact::ImpactArgs),
    /// Mine historical Git co-edits and learn empirical multiplex layer edge weights
    Coedit(commands::coedit::CoeditArgs),
    /// Detect multi-resolution topological communities, hierarchy, and architectural drift
    Community(commands::community::CommunityArgs),
    /// Search codebase symbols via hybrid BM25+ and dense subword semantic retrieval
    Query(commands::query::QueryArgs),
    /// Run Model Context Protocol (MCP) server over stdio for AI agent harnesses
    Mcp(commands::mcp::McpArgs),
    /// Watch codebase for file changes and incrementally maintain the in-memory graph
    Watch(commands::watch::WatchArgs),
}

fn main() {
    let cli = Cli::parse();

    let result = match cli.command {
        Commands::Select(args) => commands::select::execute(args),
        Commands::Blueprint(args) => commands::blueprint::execute(args),
        Commands::Architecture(args) => commands::architecture::execute(args),
        Commands::Stats(args) => commands::stats::execute(args),
        Commands::Inspect(args) => commands::inspect::execute(args),
        Commands::Clean(args) => commands::clean::execute(args),
        Commands::Impact(args) => commands::impact::execute(args),
        Commands::Coedit(args) => commands::coedit::execute(args),
        Commands::Community(args) => commands::community::execute(args),
        Commands::Query(args) => commands::query::execute(args),
        Commands::Mcp(args) => commands::mcp::execute(args),
        Commands::Watch(args) => commands::watch::execute(args),
    };

    if let Err(e) = result {
        eprintln!("{} {}", "Error:".red().bold(), e);
        std::process::exit(1);
    }
}
