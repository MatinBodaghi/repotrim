use clap::Args;
use colored::Colorize;
use repotrim_engine::{CoeditCache, CoeditConfig, EdgeWeightLearner, GitCommitMiner, LayerWeights};
use std::path::PathBuf;
use std::time::Instant;

use crate::loader::LoadedRepository;

#[derive(Args, Debug)]
pub struct CoeditArgs {
    /// Target codebase directory to scan
    #[arg(short = 'p', long = "path", default_value = ".")]
    pub path: PathBuf,

    /// Maximum number of historical commits to inspect from HEAD
    #[arg(long = "max-commits", default_value = "200")]
    pub max_commits: usize,

    /// Half-life in days for exponential recency decay
    #[arg(long = "half-life-days", default_value = "90.0")]
    pub half_life_days: f64,

    /// Maximum files per commit before classifying as bulk megacommit noise
    #[arg(long = "max-files", default_value = "25")]
    pub max_files: usize,

    /// Maximum symbols per commit before classifying as megacommit noise
    #[arg(long = "max-symbols", default_value = "40")]
    pub max_symbols: usize,

    /// Minimum raw co-edit occurrences required to consider two symbols logically coupled
    #[arg(long = "min-support", default_value = "2")]
    pub min_support: usize,

    /// Minimum association confidence threshold
    #[arg(long = "min-confidence", default_value = "0.15")]
    pub min_confidence: f32,

    /// Minimum Jaccard similarity threshold
    #[arg(long = "min-jaccard", default_value = "0.05")]
    pub min_jaccard: f32,

    /// Disable cache and force re-mining Git history
    #[arg(long = "no-cache")]
    pub no_cache: bool,

    /// Output results in machine-readable JSON format
    #[arg(long = "json")]
    pub json: bool,
}

pub fn execute(args: CoeditArgs) -> Result<(), Box<dyn std::error::Error>> {
    let start_time = Instant::now();

    if !args.json {
        eprintln!(
            "{} Ingesting repository at '{}'...",
            "⚙".cyan().bold(),
            args.path.display().to_string().bold()
        );
    }

    let repo = LoadedRepository::load_with_options(&args.path, true)?;

    let head_hash = match GitCommitMiner::get_head_hash(&repo.root_path) {
        Ok(h) => h,
        Err(e) => {
            if args.json {
                let err_obj = serde_json::json!({
                    "error": format!("Git repository not found or git command failed: {}", e)
                });
                println!("{}", serde_json::to_string_pretty(&err_obj)?);
                return Ok(());
            } else {
                eprintln!(
                    "{} Git repository not detected or git command failed at '{}': {}",
                    "⚠".yellow().bold(),
                    repo.root_path.display(),
                    e
                );
                return Ok(());
            }
        }
    };

    let cache_file = repo.root_path.join(".repotrim").join("coedit.bin");
    let mut from_cache = false;

    let config = CoeditConfig {
        max_commits: args.max_commits,
        half_life_days: args.half_life_days,
        max_files_per_commit: args.max_files,
        max_symbols_per_commit: args.max_symbols,
        min_support: args.min_support,
        min_confidence: args.min_confidence,
        min_jaccard: args.min_jaccard,
    };

    let coedit_graph = if !args.no_cache {
        if let Some(cached) = CoeditCache::load_from_file(&cache_file, &head_hash) {
            from_cache = true;
            cached
        } else {
            let mined = GitCommitMiner::mine_repository(&repo.root_path, &repo.symbols, &config)?;
            let _ = CoeditCache::save_to_file(&cache_file, &mined);
            mined
        }
    } else {
        let mined = GitCommitMiner::mine_repository(&repo.root_path, &repo.symbols, &config)?;
        let _ = CoeditCache::save_to_file(&cache_file, &mined);
        mined
    };

    let (learned_weights, report) = EdgeWeightLearner::learn_weights(
        &repo.symbols,
        &repo.edges,
        &repo.imports,
        &coedit_graph,
        LayerWeights::default(),
    );

    let duration = start_time.elapsed();

    if args.json {
        let top_pairs: Vec<serde_json::Value> = coedit_graph
            .top_pairs(50)
            .into_iter()
            .map(|p| {
                let s_sym = repo.symbols.iter().find(|s| s.id == p.source);
                let t_sym = repo.symbols.iter().find(|s| s.id == p.target);
                serde_json::json!({
                    "source_id": p.source.0,
                    "source_name": s_sym.map(|s| s.name.as_str()).unwrap_or("?"),
                    "source_file": s_sym.map(|s| s.file_path.to_string_lossy().to_string()).unwrap_or_default(),
                    "target_id": p.target.0,
                    "target_name": t_sym.map(|s| s.name.as_str()).unwrap_or("?"),
                    "target_file": t_sym.map(|s| s.file_path.to_string_lossy().to_string()).unwrap_or_default(),
                    "raw_count": p.raw_count,
                    "support": p.support,
                    "confidence": p.confidence,
                    "jaccard": p.jaccard,
                })
            })
            .collect();

        let json_out = serde_json::json!({
            "head_hash": coedit_graph.head_hash,
            "cached": from_cache,
            "commits_analyzed": coedit_graph.total_commits_analyzed,
            "valid_commits": coedit_graph.valid_commits,
            "megacommits_filtered": coedit_graph.megacommits_filtered,
            "total_coedit_pairs": coedit_graph.pairs.len(),
            "elapsed_ms": duration.as_millis(),
            "top_couplings": top_pairs,
            "layer_stats": report.layer_stats,
            "default_weights": report.default_weights,
            "learned_weights": learned_weights,
            "baseline_mrr": report.baseline_mrr,
            "learned_mrr": report.learned_mrr,
            "mrr_improvement_pct": report.mrr_improvement_pct,
        });

        println!("{}", serde_json::to_string_pretty(&json_out)?);
        return Ok(());
    }

    // Rich terminal formatting
    println!(
        "\n{}",
        "================================================================================".blue()
    );
    println!(
        "  {}",
        "REPOTRIM GIT CO-EDIT MINING & WEIGHT LEARNING".bold()
    );
    println!(
        "{}",
        "================================================================================".blue()
    );

    println!("\n  {}", "Git Evolution & Mining Summary:".yellow().bold());
    println!(
        "    • Git HEAD:           {}",
        coedit_graph.head_hash.cyan().bold()
    );
    println!(
        "    • Cache Status:       {}",
        if from_cache {
            "HIT (.repotrim/coedit.bin)".green()
        } else {
            "MINED (persisted to cache)".yellow()
        }
    );
    println!(
        "    • Commits Analyzed:   {}",
        coedit_graph.total_commits_analyzed.to_string().bold()
    );
    println!(
        "    • Valid Changesets:   {}",
        coedit_graph.valid_commits.to_string().green().bold()
    );
    println!(
        "    • Megacommits Filter: {} (bulk churn filtered)",
        coedit_graph.megacommits_filtered.to_string().yellow()
    );
    println!(
        "    • Discovered Pairs:   {}",
        coedit_graph.pairs.len().to_string().cyan().bold()
    );
    println!(
        "    • Total Latency:      {:.2} ms",
        duration.as_secs_f64() * 1000.0
    );

    println!(
        "\n  {}",
        "Layer Empirical Correlation & Learned Weights:"
            .yellow()
            .bold()
    );
    println!(
        "  {:<14} {:>8} {:>12} {:>14} {:>10} {:>10} {:>8}",
        "Layer".bold(),
        "Edges".bold(),
        "Co-Changed".bold(),
        "Empirical Rate".bold(),
        "Default".bold(),
        "Learned".bold(),
        "Delta".bold()
    );
    println!("  {}", "─".repeat(80).dimmed());

    let mut layer_names: Vec<&String> = report.layer_stats.keys().collect();
    layer_names.sort();

    for name in layer_names {
        if let Some(stat) = report.layer_stats.get(name) {
            let delta = stat.learned_weight - stat.default_weight;
            let delta_str = if delta >= 0.0 {
                format!("+{:0.2}", delta).green()
            } else {
                format!("{:0.2}", delta).red()
            };
            let learned_str = format!("{:>10.2}", stat.learned_weight).cyan().bold();
            println!(
                "  {:<14} {:>8} {:>12} {:>14.4} {:>10.2} {} {:>8}",
                name,
                stat.edge_count,
                stat.cochanged_edges,
                stat.empirical_rate,
                stat.default_weight,
                learned_str,
                delta_str
            );
        }
    }

    println!(
        "\n  {}",
        "Empirical Ranking Validation (MRR):".yellow().bold()
    );
    println!(
        "    • Baseline MRR (Default Weights): {:.4}",
        report.baseline_mrr
    );
    println!(
        "    • Learned MRR (+ Co-Edit Fusion):  {}",
        format!("{:.4}", report.learned_mrr).green().bold()
    );
    println!(
        "    • Retrieval Accuracy Delta:       {}",
        format!("{:+.1}%", report.mrr_improvement_pct).cyan().bold()
    );

    let top_couplings = coedit_graph.top_pairs(10);
    if !top_couplings.is_empty() {
        println!("\n  {}", "Top Mined Logical Couplings:".yellow().bold());
        for (idx, p) in top_couplings.iter().enumerate() {
            let s_sym = repo.symbols.iter().find(|s| s.id == p.source);
            let t_sym = repo.symbols.iter().find(|s| s.id == p.target);
            let s_name = s_sym.map(|s| s.name.as_str()).unwrap_or("?");
            let t_name = t_sym.map(|s| s.name.as_str()).unwrap_or("?");
            let s_file = s_sym
                .map(|s| s.file_path.to_string_lossy())
                .unwrap_or_default();
            let t_file = t_sym
                .map(|s| s.file_path.to_string_lossy())
                .unwrap_or_default();

            println!(
                "    {}. {} ({}) ⇄ {} ({})",
                idx + 1,
                s_name.cyan().bold(),
                s_file.dimmed(),
                t_name.cyan().bold(),
                t_file.dimmed()
            );
            println!(
                "       raw_edits: {}, support: {:.2}, conf: {:.1}%, jaccard: {:.2}",
                p.raw_count,
                p.support,
                p.confidence * 100.0,
                p.jaccard
            );
        }
    }

    println!(
        "\n  {} To select context using Git co-edits and learned weights:",
        "💡".yellow()
    );
    println!(
        "     {}",
        "repotrim select --query <QUERY> --coedit --learn-weights".bold()
    );

    Ok(())
}
