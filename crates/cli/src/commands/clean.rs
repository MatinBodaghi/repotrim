use clap::Args;
use colored::Colorize;
use std::fs;
use std::path::PathBuf;

#[derive(Args, Debug)]
pub struct CleanArgs {
    /// Target codebase directory containing .repotrim cache
    #[arg(short = 'p', long = "path", default_value = ".")]
    pub path: PathBuf,
}

pub fn execute(args: CleanArgs) -> Result<(), Box<dyn std::error::Error>> {
    let cache_dir = repotrim_engine::get_cache_dir_for_root(&args.path);
    let legacy_dir = args.path.join(".repotrim");
    let mut removed = false;

    if cache_dir.exists() {
        fs::remove_dir_all(&cache_dir)?;
        eprintln!(
            "{} Removed cache directory at '{}'",
            "✓".green().bold(),
            cache_dir.display().to_string().bold()
        );
        removed = true;
    }

    if legacy_dir.exists() {
        let _ = fs::remove_dir_all(&legacy_dir);
        removed = true;
    }

    if !removed {
        eprintln!(
            "{} No cache directory found for '{}'",
            "•".dimmed(),
            args.path.display().to_string().dimmed()
        );
    }
    Ok(())
}
