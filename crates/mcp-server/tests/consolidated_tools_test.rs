use repotrim_mcp::McpServer;
use std::io::Cursor;
use std::path::PathBuf;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[test]
fn test_consolidated_find_symbols() {
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
            // 3. find_symbols in search mode (query)
            r#"{{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{{"name":"find_symbols","arguments":{{"query":"ContextSelector","path":"{}"}}}}}}"#,
            "\n",
            // 4. find_symbols in inspect mode (symbol)
            r#"{{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{{"name":"find_symbols","arguments":{{"symbol":"ContextSelector","path":"{}"}}}}}}"#,
            "\n",
            // 5. find_symbols in entrypoint location mode (query + locate=true)
            r#"{{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{{"name":"find_symbols","arguments":{{"query":"context selector","locate":true,"limit":3,"path":"{}"}}}}}}"#,
            "\n"
        ),
        root_str, root_str, root_str
    );

    let reader = Cursor::new(input.as_bytes());
    let mut writer = Vec::new();
    let mut server = McpServer::with_root(&root);
    server
        .run_loop(reader, &mut writer)
        .expect("Server loop failed");

    let output_str = String::from_utf8(writer).expect("Invalid UTF-8 from server");
    let lines: Vec<&str> = output_str.trim().lines().collect();
    assert_eq!(lines.len(), 4);

    // Response 2: search
    let resp2: serde_json::Value = serde_json::from_str(lines[1]).unwrap();
    assert_eq!(resp2["id"], 2);
    let content2 = resp2["result"]["content"][0]["text"].as_str().unwrap();
    assert!(content2.contains("ContextSelector"));

    // Response 3: inspect
    let resp3: serde_json::Value = serde_json::from_str(lines[2]).unwrap();
    assert_eq!(resp3["id"], 3);
    let content3 = resp3["result"]["content"][0]["text"].as_str().unwrap();
    assert!(content3.contains("ContextSelector"));

    // Response 4: locate entrypoints
    let resp4: serde_json::Value = serde_json::from_str(lines[3]).unwrap();
    assert_eq!(resp4["id"], 4);
    let content4 = resp4["result"]["content"][0]["text"].as_str().unwrap();
    assert!(!content4.is_empty());
}

#[test]
fn test_consolidated_analyze_graph() {
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
            // 3. analyze_graph: mode="stats" (default)
            r#"{{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{{"name":"analyze_graph","arguments":{{"mode":"stats","path":"{}"}}}}}}"#,
            "\n",
            // 4. analyze_graph: mode="communities"
            r#"{{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{{"name":"analyze_graph","arguments":{{"mode":"communities","path":"{}"}}}}}}"#,
            "\n",
            // 5. analyze_graph: mode="coedits"
            r#"{{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{{"name":"analyze_graph","arguments":{{"mode":"coedits","max_commits":10,"path":"{}"}}}}}}"#,
            "\n"
        ),
        root_str, root_str, root_str
    );

    let reader = Cursor::new(input.as_bytes());
    let mut writer = Vec::new();
    let mut server = McpServer::with_root(&root);
    server
        .run_loop(reader, &mut writer)
        .expect("Server loop failed");

    let output_str = String::from_utf8(writer).expect("Invalid UTF-8 from server");
    let lines: Vec<&str> = output_str.trim().lines().collect();
    assert_eq!(lines.len(), 4);

    // Response 2: stats
    let resp2: serde_json::Value = serde_json::from_str(lines[1]).unwrap();
    assert_eq!(resp2["id"], 2);
    let content2 = resp2["result"]["content"][0]["text"].as_str().unwrap();
    assert!(content2.contains("RepoTrim Codebase Graph Statistics"));
    assert!(content2.contains("Symbols (Nodes V)"));

    // Response 3: communities
    let resp3: serde_json::Value = serde_json::from_str(lines[2]).unwrap();
    assert_eq!(resp3["id"], 3);
    let content3 = resp3["result"]["content"][0]["text"].as_str().unwrap();
    assert!(
        content3.contains("Community")
            || content3.contains("modularity")
            || content3.contains("communities")
    );

    // Response 4: coedits
    let resp4: serde_json::Value = serde_json::from_str(lines[3]).unwrap();
    assert_eq!(resp4["id"], 4);
    let content4 = resp4["result"]["content"][0]["text"].as_str().unwrap();
    assert!(!content4.is_empty());
}

#[test]
fn test_legacy_mcp_aliases_backward_compatibility() {
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
            // 3. legacy inspect_symbol
            r#"{{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{{"name":"inspect_symbol","arguments":{{"symbol":"ContextSelector","path":"{}"}}}}}}"#,
            "\n",
            // 4. legacy query_graph_stats
            r#"{{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{{"name":"query_graph_stats","arguments":{{"path":"{}"}}}}}}"#,
            "\n",
            // 5. legacy search_symbols
            r#"{{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{{"name":"search_symbols","arguments":{{"query":"ContextSelector","path":"{}"}}}}}}"#,
            "\n",
            // 6. legacy detect_communities
            r#"{{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{{"name":"detect_communities","arguments":{{"path":"{}"}}}}}}"#,
            "\n",
            // 7. legacy mine_coedits
            r#"{{"jsonrpc":"2.0","id":6,"method":"tools/call","params":{{"name":"mine_coedits","arguments":{{"max_commits":10,"path":"{}"}}}}}}"#,
            "\n",
            // 8. legacy locate_entrypoints
            r#"{{"jsonrpc":"2.0","id":7,"method":"tools/call","params":{{"name":"locate_entrypoints","arguments":{{"query":"context selector","limit":2,"path":"{}"}}}}}}"#,
            "\n",
            // 9. legacy clean_cache
            r#"{{"jsonrpc":"2.0","id":8,"method":"tools/call","params":{{"name":"clean_cache","arguments":{{"path":"{}"}}}}}}"#,
            "\n"
        ),
        root_str, root_str, root_str, root_str, root_str, root_str, root_str
    );

    let reader = Cursor::new(input.as_bytes());
    let mut writer = Vec::new();
    let mut server = McpServer::with_root(&root);
    server
        .run_loop(reader, &mut writer)
        .expect("Server loop failed");

    let output_str = String::from_utf8(writer).expect("Invalid UTF-8 from server");
    let lines: Vec<&str> = output_str.trim().lines().collect();
    assert_eq!(lines.len(), 8);

    // Each call should return success (no error object in response)
    for (idx, line) in lines.iter().enumerate().skip(1) {
        let resp: serde_json::Value = serde_json::from_str(line).unwrap();
        assert_eq!(
            resp["id"],
            idx as u64 + 1,
            "Failed response id matching line: {}",
            line
        );
        assert!(
            resp.get("error").is_none(),
            "Tool call {} returned error: {:?}",
            idx,
            resp.get("error")
        );
        let content = resp["result"]["content"][0]["text"].as_str().unwrap();
        assert!(!content.is_empty());
    }
}

#[test]
fn test_max_tokens_budgeting_on_unbounded_tools() {
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
            // 3. analyze_graph with tight max_tokens
            r#"{{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{{"name":"analyze_graph","arguments":{{"max_tokens":50,"path":"{}"}}}}}}"#,
            "\n",
            // 4. generate_architecture_docs with tight max_tokens
            r#"{{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{{"name":"generate_architecture_docs","arguments":{{"max_tokens":80,"path":"{}"}}}}}}"#,
            "\n"
        ),
        root_str, root_str
    );

    let reader = Cursor::new(input.as_bytes());
    let mut writer = Vec::new();
    let mut server = McpServer::with_root(&root);
    server
        .run_loop(reader, &mut writer)
        .expect("Server loop failed");

    let output_str = String::from_utf8(writer).expect("Invalid UTF-8 from server");
    let lines: Vec<&str> = output_str.trim().lines().collect();
    assert_eq!(lines.len(), 3);

    // Response 2: analyze_graph budgeted
    let resp2: serde_json::Value = serde_json::from_str(lines[1]).unwrap();
    assert_eq!(resp2["id"], 2);
    let text2 = resp2["result"]["content"][0]["text"].as_str().unwrap();
    assert!(text2.contains("Output truncated to stay within max_tokens budget"));

    // Response 3: generate_architecture_docs budgeted
    let resp3: serde_json::Value = serde_json::from_str(lines[2]).unwrap();
    assert_eq!(resp3["id"], 3);
    let text3 = resp3["result"]["content"][0]["text"].as_str().unwrap();
    assert!(text3.contains("Output truncated to stay within max_tokens budget"));
}

#[test]
fn test_mcp_protocol_version_negotiation() {
    let root = repo_root();

    // 1. Negotiation with earlier supported version "2024-10-07"
    let input1 = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-10-07"}}"#;
    let mut writer1 = Vec::new();
    let mut server1 = McpServer::with_root(&root);
    server1
        .run_loop(Cursor::new(input1.as_bytes()), &mut writer1)
        .expect("Server loop failed");
    let resp1: serde_json::Value =
        serde_json::from_str(String::from_utf8(writer1).unwrap().trim()).unwrap();
    assert_eq!(resp1["result"]["protocolVersion"], "2024-10-07");

    // 2. Negotiation with standard version "2024-11-05"
    let input2 = r#"{"jsonrpc":"2.0","id":2,"method":"initialize","params":{"protocolVersion":"2024-11-05"}}"#;
    let mut writer2 = Vec::new();
    let mut server2 = McpServer::with_root(&root);
    server2
        .run_loop(Cursor::new(input2.as_bytes()), &mut writer2)
        .expect("Server loop failed");
    let resp2: serde_json::Value =
        serde_json::from_str(String::from_utf8(writer2).unwrap().trim()).unwrap();
    assert_eq!(resp2["result"]["protocolVersion"], "2024-11-05");

    // 3. Fallback on unknown version to default standard "2024-11-05"
    let input3 = r#"{"jsonrpc":"2.0","id":3,"method":"initialize","params":{"protocolVersion":"unknown-future-version"}}"#;
    let mut writer3 = Vec::new();
    let mut server3 = McpServer::with_root(&root);
    server3
        .run_loop(Cursor::new(input3.as_bytes()), &mut writer3)
        .expect("Server loop failed");
    let resp3: serde_json::Value =
        serde_json::from_str(String::from_utf8(writer3).unwrap().trim()).unwrap();
    assert_eq!(resp3["result"]["protocolVersion"], "2024-11-05");
}
