use clap::Args;
use colored::Colorize;
use repotrim_engine::SymbolId;
use std::path::PathBuf;

use crate::loader::LoadedRepository;

#[derive(Args, Debug)]
pub struct InspectArgs {
    /// Name of the symbol to inspect
    #[arg(short = 's', long = "symbol", required = true)]
    pub symbol: String,

    /// Target codebase directory to scan
    #[arg(short = 'p', long = "path", default_value = ".")]
    pub path: PathBuf,
}

pub fn execute(args: InspectArgs) -> Result<(), Box<dyn std::error::Error>> {
    eprintln!(
        "{} Loading repository at '{}'...",
        "⚙".cyan().bold(),
        args.path.display().to_string().bold()
    );

    let repo = LoadedRepository::load(&args.path)?;
    let graph = repo.build_graph();

    let matching_symbols: Vec<_> = graph
        .symbols()
        .iter()
        .filter(|s| s.name.eq_ignore_ascii_case(&args.symbol))
        .collect();

    if matching_symbols.is_empty() {
        eprintln!(
            "{} No symbol matching '{}' was found.",
            "!".red().bold(),
            args.symbol
        );

        let suggestions: Vec<&str> = graph
            .symbols()
            .iter()
            .filter(|s| s.name.to_lowercase().contains(&args.symbol.to_lowercase()))
            .take(5)
            .map(|s| s.name.as_str())
            .collect();

        if !suggestions.is_empty() {
            eprintln!(
                "   {} {}",
                "Did you mean:".dimmed(),
                suggestions.join(", ").cyan()
            );
        }
        return Ok(());
    }

    // Find in-neighbors (who references target symbol)
    let num_symbols = graph.num_symbols();

    for sym in matching_symbols {
        let sym_id = sym.id;
        let neighbors = graph.neighbors(sym_id);
        let weights = graph.transition_probabilities(sym_id);

        let mut incoming_callers = Vec::new();
        for other_id in 0..num_symbols as u32 {
            if other_id == sym_id.0 {
                continue;
            }
            let other_neighbors = graph.neighbors(SymbolId(other_id));
            if other_neighbors.contains(&sym_id.0) {
                if let Some(other_sym) = graph.symbol(SymbolId(other_id)) {
                    incoming_callers.push(other_sym);
                }
            }
        }

        println!(
            "\n{}",
            "================================================================================"
                .blue()
        );
        println!(
            "  {} {} {}",
            format!("{:?}", sym.kind).cyan(),
            sym.name.bold(),
            format!("(SymbolId: {})", sym.id.0).dimmed()
        );
        println!(
            "{}",
            "================================================================================"
                .blue()
        );

        println!("\n  {}", "Declaration Details:".yellow().bold());
        println!(
            "    • File:              {}:L{}-L{}",
            sym.file_path.display(),
            sym.span.start_row + 1,
            sym.span.end_row + 1
        );
        println!("    • Token Cost:        {} tokens", sym.token_cost);
        let hash_hex: String = sym
            .ast_hash
            .iter()
            .take(8)
            .map(|b| format!("{:02x}", b))
            .collect();
        println!("    • BLAKE3 Hash:       {}", hash_hex.dimmed());
        println!("    • Signature:         {}", sym.signature.cyan());
        if let Some(doc) = &sym.docstring {
            println!("    • Docstring:         {}", doc.dimmed());
        }

        println!(
            "\n  {} ({} total):",
            "Outgoing Graph Dependencies".yellow().bold(),
            neighbors.len()
        );
        if neighbors.is_empty() {
            println!(
                "    {}",
                "(No outgoing dependencies - terminal leaf node)".dimmed()
            );
        } else {
            for (idx, &dst_id) in neighbors.iter().enumerate() {
                let prob = weights.get(idx).copied().unwrap_or(0.0);
                if let Some(dst_sym) = graph.symbol(SymbolId(dst_id)) {
                    println!(
                        "    → {:<24} {:<10} {:>5.1}% weight | {}:L{}",
                        dst_sym.name.bold(),
                        format!("({:?})", dst_sym.kind).dimmed(),
                        prob * 100.0,
                        dst_sym.file_path.display(),
                        dst_sym.span.start_row + 1
                    );
                }
            }
        }

        println!(
            "\n  {} ({} total):",
            "Incoming Callers & References".yellow().bold(),
            incoming_callers.len()
        );
        if incoming_callers.is_empty() {
            println!(
                "    {}",
                "(No known incoming references in workspace)".dimmed()
            );
        } else {
            for caller in incoming_callers {
                println!(
                    "    ← {:<24} {:<10} | {}:L{}",
                    caller.name.bold(),
                    format!("({:?})", caller.kind).dimmed(),
                    caller.file_path.display(),
                    caller.span.start_row + 1
                );
            }
        }
    }

    println!(
        "\n{}",
        "================================================================================".blue()
    );
    Ok(())
}
