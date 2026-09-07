use clap::Args;
use colored::Colorize;
use repotrim_engine::{PprSolver, SymbolId, SymbolKind};
use std::path::PathBuf;
use std::time::Instant;

use crate::loader::LoadedRepository;

#[derive(Args, Debug)]
pub struct StatsArgs {
    /// Target codebase directory to scan
    #[arg(short = 'p', long = "path", default_value = ".")]
    pub path: PathBuf,
}

pub fn execute(args: StatsArgs) -> Result<(), Box<dyn std::error::Error>> {
    let start_time = Instant::now();

    eprintln!(
        "{} Analyzing codebase statistics at '{}'...",
        "⚙".cyan().bold(),
        args.path.display().to_string().bold()
    );

    let repo = LoadedRepository::load(&args.path)?;
    let load_time = start_time.elapsed();

    let graph_start = Instant::now();
    let graph = repo.build_graph();
    let graph_time = graph_start.elapsed();

    // Tally symbols by kind
    let mut num_fns = 0;
    let mut num_methods = 0;
    let mut num_structs = 0;
    let mut num_enums = 0;
    let mut num_traits = 0;
    let mut total_tokens = 0;

    for s in graph.symbols() {
        total_tokens += s.token_cost;
        match s.kind {
            SymbolKind::Function => num_fns += 1,
            SymbolKind::Method => num_methods += 1,
            SymbolKind::Struct => num_structs += 1,
            SymbolKind::Enum => num_enums += 1,
            SymbolKind::Trait => num_traits += 1,
            _ => {}
        }
    }

    // Degree distribution
    let num_symbols = graph.num_symbols();
    let num_edges = graph.num_edges();
    let avg_out_degree = if num_symbols > 0 {
        num_edges as f32 / num_symbols as f32
    } else {
        0.0
    };

    let max_out_degree = (0..num_symbols as u32)
        .map(|id| graph.out_degree(SymbolId(id)))
        .max()
        .unwrap_or(0);

    // Compute global PageRank hubs
    let ppr = PprSolver::default();
    let uniform_seeds: Vec<(SymbolId, f32)> = (0..num_symbols as u32)
        .map(|id| (SymbolId(id), 1.0 / num_symbols.max(1) as f32))
        .collect();

    let ppr_scores = ppr.compute(&graph, &uniform_seeds);
    let mut hub_scores: Vec<(SymbolId, f32)> = ppr_scores.into_iter().collect();
    hub_scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    println!(
        "\n{}",
        "================================================================================".blue()
    );
    println!("  {}", "REPOTRIM CODEBASE GRAPH STATISTICS".bold());
    println!(
        "{}",
        "================================================================================".blue()
    );

    println!("\n  {}", "Repository Ingestion:".yellow().bold());
    println!(
        "    • Directory:         {}",
        repo.root_path.display().to_string().cyan()
    );
    println!(
        "    • Source Files:      {}",
        repo.file_sources.len().to_string().bold()
    );
    println!(
        "    • Source Bytes:      {} bytes",
        repo.total_bytes.to_string().bold()
    );
    println!("    • Ingestion Time:    {:?}", load_time);
    println!(
        "    • Total Inventory:   {} tokens",
        total_tokens.to_string().bold()
    );

    println!(
        "\n  {}",
        "Syntax Entity Breakdown (Nodes V):".yellow().bold()
    );
    println!(
        "    • Total Symbols:     {}",
        num_symbols.to_string().bold()
    );
    println!("    • Functions:         {}", num_fns.to_string().cyan());
    println!(
        "    • Methods:           {}",
        num_methods.to_string().cyan()
    );
    println!(
        "    • Structs:           {}",
        num_structs.to_string().cyan()
    );
    println!("    • Enums:             {}", num_enums.to_string().cyan());
    println!("    • Traits:            {}", num_traits.to_string().cyan());

    println!(
        "\n  {}",
        "Multiplex Graph Connectivity (Edges E):".yellow().bold()
    );
    println!("    • Resolved Edges:    {}", num_edges.to_string().bold());
    println!("    • Graph Build Time:  {:?}", graph_time);
    println!("    • Avg Out-Degree:    {:.2}", avg_out_degree);
    println!("    • Max Out-Degree:    {}", max_out_degree);

    println!(
        "\n  {}",
        "Top Architectural Hubs (Global PageRank Centrality):"
            .yellow()
            .bold()
    );
    for (rank, (sym_id, score)) in hub_scores.iter().take(10).enumerate() {
        if let Some(sym) = graph.symbol(*sym_id) {
            let out_deg = graph.out_degree(*sym_id);
            println!(
                "    {:02}. {:<24} {:<10} {:>5.2}% PR | {:>2} edges | {}:L{}",
                rank + 1,
                sym.name.bold(),
                format!("({:?})", sym.kind).dimmed(),
                score * 100.0,
                out_deg,
                sym.file_path.display(),
                sym.span.start_row + 1
            );
        }
    }

    println!(
        "\n{}",
        "================================================================================".blue()
    );
    Ok(())
}
