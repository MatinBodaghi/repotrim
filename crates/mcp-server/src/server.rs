use std::io::{BufRead, Write};

use crate::handler::McpHandler;
use crate::protocol::{JsonRpcRequest, JsonRpcResponse, PARSE_ERROR};

/// Stdio JSON-RPC 2.0 server managing the MCP lifecycle.
pub struct McpServer {
    handler: McpHandler,
}

impl Default for McpServer {
    fn default() -> Self {
        Self::new()
    }
}

impl McpServer {
    /// Creates a new MCP server instance with default handler configuration.
    pub fn new() -> Self {
        Self {
            handler: McpHandler::new(),
        }
    }

    /// Creates an MCP server initialized with a specific default root path.
    pub fn with_root<P: Into<std::path::PathBuf>>(root: P) -> Self {
        Self {
            handler: McpHandler::with_root(root),
        }
    }

    /// Runs the stdio event loop using process standard input and standard output.
    pub fn run_stdio(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let stdin = std::io::stdin();
        let stdout = std::io::stdout();
        self.run_loop(stdin.lock(), stdout.lock())
    }

    /// Runs the message loop over any reader and writer stream.
    pub fn run_loop<R: BufRead, W: Write>(
        &mut self,
        mut reader: R,
        mut writer: W,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut line_buffer = String::new();

        loop {
            line_buffer.clear();
            let bytes_read = reader.read_line(&mut line_buffer)?;
            if bytes_read == 0 {
                // EOF reached
                break;
            }

            let trimmed = line_buffer.trim().trim_start_matches('\u{feff}');
            if trimmed.is_empty() {
                continue;
            }

            let response_opt = match serde_json::from_str::<JsonRpcRequest>(trimmed) {
                Ok(request) => self.handler.handle_request(request),
                Err(err) => {
                    eprintln!("repotrim-mcp: JSON parse error: {}", err);
                    Some(JsonRpcResponse::error(
                        serde_json::Value::Null,
                        PARSE_ERROR,
                        format!("Parse error: {}", err),
                    ))
                }
            };

            if let Some(response) = response_opt {
                let serialized = serde_json::to_string(&response)?;
                writer.write_all(serialized.as_bytes())?;
                writer.write_all(b"\n")?;
                writer.flush()?;
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn test_server_handshake_and_ping_flow() {
        let input = concat!(
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05"}}"#,
            "\n",
            r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
            "\n",
            r#"{"jsonrpc":"2.0","id":2,"method":"ping"}"#,
            "\n",
            r#"{"jsonrpc":"2.0","id":3,"method":"tools/list"}"#,
            "\n"
        );

        let reader = Cursor::new(input.as_bytes());
        let mut writer = Vec::new();

        let mut server = McpServer::new();
        server
            .run_loop(reader, &mut writer)
            .expect("Server loop failed");

        let output_str = String::from_utf8(writer).expect("Valid UTF-8 output");
        let lines: Vec<&str> = output_str.trim().split('\n').collect();

        // Should have 3 responses: initialize, ping, tools/list (notifications/initialized produces no response)
        assert_eq!(lines.len(), 3);

        // 1. initialize response
        let resp1: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
        assert_eq!(resp1["id"], 1);
        assert_eq!(resp1["result"]["protocolVersion"], "2024-11-05");
        assert_eq!(resp1["result"]["serverInfo"]["name"], "repotrim-mcp");

        // 2. ping response
        let resp2: serde_json::Value = serde_json::from_str(lines[1]).unwrap();
        assert_eq!(resp2["id"], 2);
        assert_eq!(resp2["result"], serde_json::json!({}));

        // 3. tools/list response
        let resp3: serde_json::Value = serde_json::from_str(lines[2]).unwrap();
        assert_eq!(resp3["id"], 3);
        let tools = resp3["result"]["tools"].as_array().expect("Tools array");
        assert_eq!(tools.len(), 4);
        let tool_names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
        assert!(tool_names.contains(&"trim_context"));
        assert!(tool_names.contains(&"query_graph_stats"));
        assert!(tool_names.contains(&"inspect_symbol"));
        assert!(tool_names.contains(&"clean_cache"));
    }
}
