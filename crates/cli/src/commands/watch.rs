use clap::Args;
use colored::Colorize;
use repotrim_engine::{LoadedRepository, RepositoryWatcher, WatcherEvent};
use std::path::PathBuf;
use std::sync::mpsc;
use std::sync::{Arc, RwLock};
use std::time::Duration;

#[derive(Args, Debug)]
pub struct WatchArgs {
    /// Target codebase directory to watch
    #[arg(short = 'p', long = "path", default_value = ".")]
    pub path: PathBuf,

    /// Debounce delay in milliseconds before rebuilding the in-memory graph
    #[arg(short = 'd', long = "debounce", default_value = "75")]
    pub debounce: u64,
}

pub fn execute(args: WatchArgs) -> Result<(), Box<dyn std::error::Error>> {
    let root_path = if args.path.is_relative() {
        std::env::current_dir()?.join(&args.path)
    } else {
        args.path.clone()
    };

    eprintln!(
        "{} Ingesting repository at '{}'...",
        "•".cyan().bold(),
        root_path.display().to_string().bold()
    );

    let start_time = std::time::Instant::now();
    let loaded = LoadedRepository::load(&root_path)?;
    let initial_symbols = loaded.symbols.len();
    let initial_edges = loaded.edges.len();
    let initial_files = loaded.cache.entries.len();
    let elapsed = start_time.elapsed();

    eprintln!(
        "{} Loaded {} files ({} symbols, {} edges) in {:.2?}",
        "✓".green().bold(),
        initial_files.to_string().bold(),
        initial_symbols.to_string().bold(),
        initial_edges.to_string().bold(),
        elapsed
    );

    let graph = loaded.build_graph();
    let repo_arc = Arc::new(RwLock::new(loaded));
    let graph_arc = Arc::new(RwLock::new(graph));

    let (event_tx, event_rx) = mpsc::channel();
    let debounce = Duration::from_millis(args.debounce);

    let _watcher = RepositoryWatcher::start_with_debounce(
        Arc::clone(&repo_arc),
        Some(Arc::clone(&graph_arc)),
        Some(event_tx),
        debounce,
    )?;

    eprintln!(
        "{} Watching for changes (debounce: {}ms). Press Ctrl+C to stop.",
        "▶".blue().bold(),
        args.debounce
    );

    while let Ok(event) = event_rx.recv() {
        match event {
            WatcherEvent::FilePatched { path } => {
                eprintln!(
                    "{} Patched '{}'",
                    "⚡".yellow().bold(),
                    path.display().to_string().bold()
                );
            }
            WatcherEvent::FileRemoved { path } => {
                eprintln!(
                    "{} Removed '{}'",
                    "🗑".red().bold(),
                    path.display().to_string().bold()
                );
            }
            WatcherEvent::GraphUpdated {
                num_symbols,
                num_edges,
            } => {
                eprintln!(
                    "{} Graph synchronized: {} symbols, {} edges",
                    "✓".green().bold(),
                    num_symbols.to_string().bold(),
                    num_edges.to_string().bold()
                );
            }
        }
    }

    Ok(())
}
