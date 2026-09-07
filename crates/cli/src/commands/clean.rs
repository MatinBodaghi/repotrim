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
    let cache_dir = args.path.join(".repotrim");
    if cache_dir.exists() {
        fs::remove_dir_all(&cache_dir)?;
        eprintln!(
            "{} Removed cache directory at '{}'",
            "✓".green().bold(),
            cache_dir.display().to_string().bold()
        );
    } else {
        eprintln!(
            "{} No cache directory found at '{}'",
            "•".dimmed(),
            cache_dir.display().to_string().dimmed()
        );
    }
    Ok(())
}
