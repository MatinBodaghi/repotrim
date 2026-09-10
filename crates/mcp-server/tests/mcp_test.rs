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
            // 8. tools/call: generate_blueprint
            r#"{{"jsonrpc":"2.0","id":7,"method":"tools/call","params":{{"name":"generate_blueprint","arguments":{{"task":"token estimation","path":"{}"}}}}}}"#,
            "\n",
            // 9. tools/call: generate_architecture_docs
            r#"{{"jsonrpc":"2.0","id":8,"method":"tools/call","params":{{"name":"generate_architecture_docs","arguments":{{"path":"{}"}}}}}}"#,
            "\n",
            // 10. tools/call: analyze_impact
            r#"{{"jsonrpc":"2.0","id":9,"method":"tools/call","params":{{"name":"analyze_impact","arguments":{{"symbol":"ContextSelector","path":"{}"}}}}}}"#,
            "\n",
            // 11. tools/call: unknown tool error handling
            r#"{{"jsonrpc":"2.0","id":10,"method":"tools/call","params":{{"name":"nonexistent_tool","arguments":{{}}}}}}"#,
            "\n",
            // 12. unknown method error handling
            r#"{{"jsonrpc":"2.0","id":11,"method":"unknown_method"}}"#,
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

    assert_eq!(lines.len(), 11);

    // 1. initialize
    let resp1: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
    assert_eq!(resp1["id"], 1);
    assert_eq!(resp1["result"]["protocolVersion"], "2024-11-05");
    assert_eq!(resp1["result"]["serverInfo"]["name"], "repotrim-mcp");

    // 2. tools/list
    let resp2: serde_json::Value = serde_json::from_str(lines[1]).unwrap();
    assert_eq!(resp2["id"], 2);
    let tools = resp2["result"]["tools"].as_array().unwrap();
    assert_eq!(tools.len(), 7);

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

    // 7. generate_blueprint
    let resp7: serde_json::Value = serde_json::from_str(lines[6]).unwrap();
    assert_eq!(resp7["id"], 7);
    assert_eq!(resp7["result"]["isError"], false);
    let bp_text = resp7["result"]["content"][0]["text"].as_str().unwrap();
    assert!(bp_text.contains("Feature Blueprint: token estimation"));
    assert!(bp_text.contains("estimate_tokens"));

    // 8. generate_architecture_docs
    let resp8: serde_json::Value = serde_json::from_str(lines[7]).unwrap();
    assert_eq!(resp8["id"], 8);
    assert_eq!(resp8["result"]["isError"], false);
    let arch_text = resp8["result"]["content"][0]["text"].as_str().unwrap();
    assert!(arch_text.contains("# Repository Architecture & Subsystem Specification"));
    assert!(arch_text.contains("```mermaid\nflowchart TD"));
    assert!(arch_text.contains("## 3. Subsystem Community Catalog"));

    // 9. analyze_impact
    let resp9: serde_json::Value = serde_json::from_str(lines[8]).unwrap();
    assert_eq!(resp9["id"], 9);
    assert_eq!(resp9["result"]["isError"], false);
    let impact_text = resp9["result"]["content"][0]["text"].as_str().unwrap();
    assert!(impact_text.contains("# Semantic Change Impact Analysis Report"));
    assert!(impact_text.contains("ContextSelector"));

    // 10. nonexistent_tool error
    let resp10: serde_json::Value = serde_json::from_str(lines[9]).unwrap();
    assert_eq!(resp10["id"], 10);
    assert_eq!(resp10["result"]["isError"], true);
    let err_text = resp10["result"]["content"][0]["text"].as_str().unwrap();
    assert!(err_text.contains("Unsupported tool 'nonexistent_tool'"));

    // 11. unknown_method error
    let resp11: serde_json::Value = serde_json::from_str(lines[10]).unwrap();
    assert_eq!(resp11["id"], 11);
    assert_eq!(resp11["error"]["code"], -32601);
    assert!(resp11["error"]["message"]
        .as_str()
        .unwrap()
        .contains("Unknown method"));
}

#[test]
fn test_mcp_trim_context_auto_budget() {
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
            // 3. tools/call: trim_context auto markdown
            r#"{{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{{"name":"trim_context","arguments":{{"seeds":["ContextSelector"],"budget":"auto","model":"claude","path":"{}"}}}}}}"#,
            "\n",
            // 4. tools/call: trim_context auto json
            r#"{{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{{"name":"trim_context","arguments":{{"seeds":["ContextSelector"],"budget":"auto","model":"deepseek","format":"json","path":"{}"}}}}}}"#,
            "\n",
            // 5. tools/call: generate_blueprint auto
            r#"{{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{{"name":"generate_blueprint","arguments":{{"task":"submodular optimizer","budget":"auto","model":"ollama","path":"{}"}}}}}}"#,
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

    let output_str = String::from_utf8(writer).expect("Valid UTF-8 output");
    let lines: Vec<&str> = output_str.trim().split('\n').collect();

    assert_eq!(lines.len(), 4);

    // 1. initialize
    let resp1: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
    assert_eq!(resp1["id"], 1);

    // 2. trim_context auto markdown
    let resp2: serde_json::Value = serde_json::from_str(lines[1]).unwrap();
    assert_eq!(resp2["id"], 2);
    assert_eq!(resp2["result"]["isError"], false);
    let md_text = resp2["result"]["content"][0]["text"].as_str().unwrap();
    assert!(md_text.contains("Auto-budget tuned to"));
    assert!(md_text.contains("via Knee-Curve"));
    assert!(md_text.contains("ContextSelector"));

    // 3. trim_context auto json
    let resp3: serde_json::Value = serde_json::from_str(lines[2]).unwrap();
    assert_eq!(resp3["id"], 3);
    assert_eq!(resp3["result"]["isError"], false);
    let json_text = resp3["result"]["content"][0]["text"].as_str().unwrap();
    let parsed_json: serde_json::Value = serde_json::from_str(json_text).unwrap();
    assert!(parsed_json.get("auto_budget").is_some());
    let auto_budget = &parsed_json["auto_budget"];
    assert_eq!(auto_budget["model"], "deepseek-v3");
    assert!(auto_budget["knee_tokens"].as_u64().unwrap() > 0);

    // 4. generate_blueprint auto
    let resp4: serde_json::Value = serde_json::from_str(lines[3]).unwrap();
    assert_eq!(resp4["id"], 4);
    assert_eq!(resp4["result"]["isError"], false);
    let bp_text = resp4["result"]["content"][0]["text"].as_str().unwrap();
    assert!(bp_text.contains("\"budget\": \"auto\""));
    assert!(bp_text.contains("\"model\": \"ollama\""));
}
