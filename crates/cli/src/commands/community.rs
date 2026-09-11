use clap::Args;
use colored::Colorize;
use repotrim_engine::{
    ArchitecturalDrift, Community, CommunityConfig, CommunityDetector, CommunityHierarchy,
};
use serde::Serialize;
use std::path::PathBuf;
use std::time::Instant;

use crate::loader::LoadedRepository;

#[derive(Args, Debug)]
pub struct CommunityArgs {
    /// Target codebase directory to scan
    #[arg(short = 'p', long = "path", default_value = ".")]
    pub path: PathBuf,

    /// Modularity resolution parameter gamma (Reichardt & Bornholdt, 2004)
    #[arg(short = 'r', long = "resolution", default_value = "1.0")]
    pub resolution: f64,

    /// Display multi-resolution hierarchy (Macro gamma=0.5, Meso gamma=1.0, Micro gamma=2.5)
    #[arg(long = "hierarchy")]
    pub hierarchy: bool,

    /// Identify architectural drift and misplaced / leaky symbols
    #[arg(long = "drift")]
    pub drift: bool,

    /// Disable incremental AST caching and force full re-parsing
    #[arg(long = "no-cache")]
    pub no_cache: bool,

    /// Output results in machine-readable JSON format
    #[arg(long = "json")]
    pub json: bool,
}

#[derive(Serialize)]
struct CommunityCliReport {
    resolution: f64,
    modularity: f32,
    total_symbols: usize,
    community_count: usize,
    communities: Vec<Community>,
    #[serde(skip_serializing_if = "Option::is_none")]
    drift: Option<Vec<ArchitecturalDrift>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    hierarchy: Option<CommunityHierarchy>,
}

pub fn execute(args: CommunityArgs) -> Result<(), Box<dyn std::error::Error>> {
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
    let graph = repo.build_graph();

    if args.hierarchy {
        let hierarchy = CommunityDetector::detect_hierarchy(&graph);

        if args.json {
            let report = CommunityCliReport {
                resolution: args.resolution,
                modularity: hierarchy.meso_modularity,
                total_symbols: graph.num_symbols(),
                community_count: hierarchy.meso_communities.len(),
                communities: hierarchy.meso_communities.clone(),
                drift: None,
                hierarchy: Some(hierarchy),
            };
            println!("{}", serde_json::to_string_pretty(&report)?);
            return Ok(());
        }

        eprintln!(
            "{} Discovered multi-resolution hierarchy in {:?}\n",
            "✓".green().bold(),
            start_time.elapsed()
        );

        println!("{}", "== Macro Subsystems (γ = 0.5) ==".bold().cyan());
        println!(
            "Modularity Q(0.5): {:.4} | Communities: {}",
            hierarchy.macro_modularity,
            hierarchy.macro_communities.len()
        );
        print_community_table(&hierarchy.macro_communities);
        println!();

        println!("{}", "== Meso Modules (γ = 1.0) ==".bold().green());
        println!(
            "Modularity Q(1.0): {:.4} | Communities: {}",
            hierarchy.meso_modularity,
            hierarchy.meso_communities.len()
        );
        print_community_table(&hierarchy.meso_communities);
        println!();

        println!("{}", "== Micro Components (γ = 2.5) ==".bold().yellow());
        println!(
            "Modularity Q(2.5): {:.4} | Communities: {}",
            hierarchy.micro_modularity,
            hierarchy.micro_communities.len()
        );
        print_community_table(
            &hierarchy
                .micro_communities
                .iter()
                .take(15)
                .cloned()
                .collect::<Vec<_>>(),
        );
        if hierarchy.micro_communities.len() > 15 {
            println!(
                "  ... and {} more fine-grained micro-communities",
                hierarchy.micro_communities.len() - 15
            );
        }

        return Ok(());
    }

    let config = CommunityConfig::with_resolution(args.resolution);
    let result = CommunityDetector::detect(&graph, &config);

    let drift = if args.drift {
        Some(CommunityDetector::analyze_drift(
            &graph,
            &result.communities,
            &repo.root_path,
        ))
    } else {
        None
    };

    if args.json {
        let report = CommunityCliReport {
            resolution: result.resolution,
            modularity: result.modularity,
            total_symbols: graph.num_symbols(),
            community_count: result.communities.len(),
            communities: result.communities,
            drift,
            hierarchy: None,
        };
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }

    eprintln!(
        "{} Detected {} topological communities in {:?} (γ = {:.2}, Q = {:.4})\n",
        "✓".green().bold(),
        result.communities.len().to_string().bold(),
        start_time.elapsed(),
        result.resolution,
        result.modularity
    );

    print_community_table(&result.communities);

    if let Some(drifts) = drift {
        println!();
        println!("{}", "== Architectural Drift Analysis ==".bold().magenta());
        if drifts.is_empty() {
            println!("✓ No significant architectural drift detected. Directory boundaries align with graph modularity.");
        } else {
            println!(
                "Found {} symbols exhibiting functional drift into foreign communities:\n",
                drifts.len().to_string().bold()
            );
            println!(
                "{:<24} {:<12} {:<24} {:<24} {:>10}",
                "Symbol".bold(),
                "Kind".bold(),
                "Declared Dir".bold(),
                "Coupled Community".bold(),
                "Drift".bold()
            );
            println!("{:-<98}", "");

            for d in drifts.iter().take(20) {
                let sym_str = if d.symbol_name.len() > 22 {
                    format!("{}...", &d.symbol_name[..19])
                } else {
                    d.symbol_name.clone()
                };

                let declared_str = d
                    .declared_directory
                    .display()
                    .to_string()
                    .replace('\\', "/");
                let dec_disp = if declared_str.len() > 22 {
                    format!("{}...", &declared_str[..19])
                } else {
                    declared_str
                };

                let comm_disp = if d.community_name.len() > 22 {
                    format!("{}...", &d.community_name[..19])
                } else {
                    d.community_name.clone()
                };

                let drift_pct = format!("{:.1}%", d.drift_score * 100.0);
                let colored_pct = if d.drift_score >= 0.75 {
                    drift_pct.red().bold()
                } else if d.drift_score >= 0.55 {
                    drift_pct.yellow()
                } else {
                    drift_pct.normal()
                };

                println!(
                    "{:<24} {:<12} {:<24} {:<24} {:>10}",
                    sym_str.cyan(),
                    format!("{:?}", d.symbol_kind),
                    dec_disp,
                    comm_disp.green(),
                    colored_pct
                );
            }

            if drifts.len() > 20 {
                println!("  ... and {} more drifting symbols", drifts.len() - 20);
            }
        }
    }

    Ok(())
}

fn print_community_table(communities: &[Community]) {
    println!(
        "{:<4} {:<24} {:>8} {:>10} {:>8} {:<22} {:>8}",
        "ID".bold(),
        "Community Name".bold(),
        "Symbols".bold(),
        "Tokens".bold(),
        "Density".bold(),
        "Dominant Dir".bold(),
        "Purity".bold()
    );
    println!("{:-<92}", "");

    for c in communities {
        let name_str = if c.name.len() > 22 {
            format!("{}...", &c.name[..19])
        } else {
            c.name.clone()
        };

        let dir_str = c
            .dominant_directory
            .display()
            .to_string()
            .replace('\\', "/");
        let dir_disp = if dir_str.len() > 20 {
            format!("{}...", &dir_str[..17])
        } else {
            dir_str
        };

        let purity_str = format!("{:.0}%", c.directory_purity * 100.0);
        let density_str = format!("{:.2}", c.density);

        println!(
            "{:<4} {:<24} {:>8} {:>10} {:>8} {:<22} {:>8}",
            c.id,
            name_str.cyan(),
            c.symbol_count,
            c.total_tokens,
            density_str,
            dir_disp,
            purity_str.green()
        );
    }
}
