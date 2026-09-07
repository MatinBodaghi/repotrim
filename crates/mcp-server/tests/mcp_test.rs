use repotrim_mcp::McpServer;
use std::io::Cursor;
use std::path::PathBuf;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[test]
fn test_mcp_full_lifecycle_and_tools() {
    let root = repo_root();
    let root_str = root.display().to_string().replace('\\', "/");

    let input = format!(
        concat!(
            // 1. initialize
            r#"{{"jsonrpc":"2.0","id":1,"method":"initialize","params":{{"protocolVersion":"2024-11-05"}}}}"#,
            "\n",
            // 2. notifications/initialized
            r#"{{"jsonrpc":"2.0","method":"notifications/initialized"}}"#,
            "\n",
            // 3. tools/list
            r#"{{"jsonrpc":"2.0","id":2,"method":"tools/list"}}"#,
            "\n",
            // 4. tools/call: query_graph_stats
            r#"{{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{{"name":"query_graph_stats","arguments":{{"path":"{}"}}}}}}"#,
            "\n",
            // 5. tools/call: inspect_symbol
            r#"{{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{{"name":"inspect_symbol","arguments":{{"symbol":"ContextSelector","path":"{}"}}}}}}"#,
            "\n",
            // 6. tools/call: trim_context (markdown)
            r#"{{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{{"name":"trim_context","arguments":{{"seeds":["ContextSelector"],"budget":500,"path":"{}"}}}}}}"#,
            "\n",
            // 7. tools/call: trim_context (json)
            r#"{{"jsonrpc":"2.0","id":6,"method":"tools/call","params":{{"name":"trim_context","arguments":{{"seeds":["ContextSelector"],"budget":300,"format":"json","path":"{}"}}}}}}"#,
            "\n",
            // 8. tools/call: unknown tool error handling
            r#"{{"jsonrpc":"2.0","id":7,"method":"tools/call","params":{{"name":"nonexistent_tool","arguments":{{}}}}}}"#,
            "\n",
            // 9. unknown method error handling
            r#"{{"jsonrpc":"2.0","id":8,"method":"unknown_method"}}"#,
            "\n"
        ),
        root_str, root_str, root_str, root_str
    );

    let reader = Cursor::new(input.as_bytes());
    let mut writer = Vec::new();

    let mut server = McpServer::with_root(&root);
    server
        .run_loop(reader, &mut writer)
        .expect("Server loop failed");

    let output_str = String::from_utf8(writer).expect("Valid UTF-8 output");
    let lines: Vec<&str> = output_str.trim().split('\n').collect();

    // Responses expected:
    // id:1 (initialize)
    // id:2 (tools/list)
    // id:3 (query_graph_stats)
    // id:4 (inspect_symbol)
    // id:5 (trim_context markdown)
    // id:6 (trim_context json)
    // id:7 (nonexistent_tool error)
    // id:8 (unknown_method error)
    assert_eq!(lines.len(), 8);

    // 1. initialize
    let resp1: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
    assert_eq!(resp1["id"], 1);
    assert_eq!(resp1["result"]["protocolVersion"], "2024-11-05");
    assert_eq!(resp1["result"]["serverInfo"]["name"], "repotrim-mcp");

    // 2. tools/list
    let resp2: serde_json::Value = serde_json::from_str(lines[1]).unwrap();
    assert_eq!(resp2["id"], 2);
    let tools = resp2["result"]["tools"].as_array().unwrap();
    assert_eq!(tools.len(), 4);

    // 3. query_graph_stats
    let resp3: serde_json::Value = serde_json::from_str(lines[2]).unwrap();
    assert_eq!(resp3["id"], 3);
    assert_eq!(resp3["result"]["isError"], false);
    let stats_text = resp3["result"]["content"][0]["text"].as_str().unwrap();
    assert!(stats_text.contains("RepoTrim Codebase Graph Statistics"));
    assert!(stats_text.contains("Symbols (Nodes V)"));
    assert!(stats_text.contains("Resolved Edges (E)"));

    // 4. inspect_symbol
    let resp4: serde_json::Value = serde_json::from_str(lines[3]).unwrap();
    assert_eq!(resp4["id"], 4);
    assert_eq!(resp4["result"]["isError"], false);
    let inspect_text = resp4["result"]["content"][0]["text"].as_str().unwrap();
    assert!(inspect_text.contains("ContextSelector"));
    assert!(inspect_text.contains("Outgoing Dependencies"));

    // 5. trim_context markdown
    let resp5: serde_json::Value = serde_json::from_str(lines[4]).unwrap();
    assert_eq!(resp5["id"], 5);
    assert_eq!(resp5["result"]["isError"], false);
    let md_text = resp5["result"]["content"][0]["text"].as_str().unwrap();
    assert!(md_text.contains("### File:"));
    assert!(md_text.contains("ContextSelector"));

    // 6. trim_context json
    let resp6: serde_json::Value = serde_json::from_str(lines[5]).unwrap();
    assert_eq!(resp6["id"], 6);
    assert_eq!(resp6["result"]["isError"], false);
    let json_text = resp6["result"]["content"][0]["text"].as_str().unwrap();
    let parsed_json: serde_json::Value = serde_json::from_str(json_text).unwrap();
    assert_eq!(parsed_json["budget"], 300);
    assert!(parsed_json["symbols"].is_array());
    assert!(parsed_json["markdown"].is_string());

    // 7. nonexistent_tool error
    let resp7: serde_json::Value = serde_json::from_str(lines[6]).unwrap();
    assert_eq!(resp7["id"], 7);
    assert_eq!(resp7["result"]["isError"], true);
    let err_text = resp7["result"]["content"][0]["text"].as_str().unwrap();
    assert!(err_text.contains("Unsupported tool 'nonexistent_tool'"));

    // 8. unknown_method error
    let resp8: serde_json::Value = serde_json::from_str(lines[7]).unwrap();
    assert_eq!(resp8["id"], 8);
    assert_eq!(resp8["error"]["code"], -32601);
    assert!(resp8["error"]["message"]
        .as_str()
        .unwrap()
        .contains("Unknown method"));
}
