use repotrim_mcp::McpServer;
use std::path::PathBuf;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mut root_path = PathBuf::from(".");

    let mut i = 1;
    while i < args.len() {
        if (args[i] == "--path" || args[i] == "-p") && i + 1 < args.len() {
            root_path = PathBuf::from(&args[i + 1]);
            i += 2;
        } else if args[i] == "--help" || args[i] == "-h" {
            eprintln!("repotrim-mcp: Model Context Protocol (MCP) server for RepoTrim");
            eprintln!("\nUsage: repotrim-mcp [--path <DIR>]");
            eprintln!("\nCommunicates via standard JSON-RPC 2.0 over stdio.");
            std::process::exit(0);
        } else {
            i += 1;
        }
    }

    eprintln!(
        "repotrim-mcp: Starting MCP server on stdio (root: '{}')",
        root_path.display()
    );
    let mut server = McpServer::with_root(root_path);
    if let Err(e) = server.run_stdio() {
        eprintln!("repotrim-mcp error: {}", e);
        std::process::exit(1);
    }
}
