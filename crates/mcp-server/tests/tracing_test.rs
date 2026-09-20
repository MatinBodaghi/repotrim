use repotrim_mcp::McpServer;
use std::io::Cursor;
use std::path::PathBuf;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[test]
fn test_mcp_tracing_and_stdout_purity() {
    let root = repo_root();
    let mut server = McpServer::with_root(&root);

    let requests = [
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05"}}"#,
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#,
        r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"query_graph_stats","arguments":{}}}"#,
        r#"{"jsonrpc":"2.0","id":4,"method":"ping"}"#,
    ];

    let combined_input = requests.join("\n") + "\n";
    let mut writer = Vec::new();

    server
        .run_loop(Cursor::new(combined_input.as_bytes()), &mut writer)
        .expect("Server loop failed");

    let output_str = String::from_utf8(writer).expect("Valid UTF-8 output");
    let lines: Vec<&str> = output_str
        .lines()
        .filter(|l| !l.trim().is_empty())
        .collect();

    assert_eq!(
        lines.len(),
        4,
        "Expected exactly 4 responses for 4 requests, without stdout log pollution"
    );

    for (idx, line) in lines.iter().enumerate() {
        let val: serde_json::Value = serde_json::from_str(line)
            .unwrap_or_else(|e| panic!("Line {} is not valid JSON: {}", idx, e));
        assert_eq!(
            val["jsonrpc"], "2.0",
            "Line {} must be a valid JSON-RPC 2.0 message",
            idx
        );
        assert_eq!(
            val["id"],
            (idx + 1) as i64,
            "Response ID must match request ID"
        );
    }
}
