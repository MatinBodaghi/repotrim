use clap::Args;
use colored::Colorize;
use repotrim_mcp::McpServer;
use std::path::PathBuf;

#[derive(Args, Debug)]
pub struct McpArgs {
    /// Default codebase directory to serve over Model Context Protocol
    #[arg(short = 'p', long = "path", default_value = ".")]
    pub path: PathBuf,
}

pub fn execute(args: McpArgs) -> Result<(), Box<dyn std::error::Error>> {
    eprintln!(
        "{} Starting RepoTrim Model Context Protocol (MCP) server at '{}'...",
        "⚙".cyan().bold(),
        args.path.display().to_string().bold()
    );

    let mut server = McpServer::with_root(args.path);
    server.run_stdio()
}
