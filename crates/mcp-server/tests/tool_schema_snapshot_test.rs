use repotrim_mcp::McpServer;
use std::io::Cursor;
use std::path::PathBuf;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[test]
fn test_mcp_tool_schemas_snapshot() {
    let root = repo_root();

    let input = concat!(
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05"}}"#,
        "\n",
        r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
        "\n",
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#,
        "\n"
    );

    let reader = Cursor::new(input.as_bytes());
    let mut writer = Vec::new();
    let mut server = McpServer::with_root(&root);
    server
        .run_loop(reader, &mut writer)
        .expect("Server loop failed");

    let output_str = String::from_utf8(writer).expect("Invalid UTF-8 from server");
    let lines: Vec<&str> = output_str.trim().lines().collect();
    assert_eq!(lines.len(), 2);

    let resp: serde_json::Value = serde_json::from_str(lines[1]).expect("Valid JSON-RPC response");
    let tools = resp["result"]["tools"].as_array().expect("Tools array");

    // 1. Invariant: Exactly 8 consolidated primitives
    assert_eq!(tools.len(), 8, "Expected exactly 8 consolidated tools");

    // 2. Canonical tool names check
    let mut names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
    names.sort();
    let expected_names = vec![
        "analyze_graph",
        "analyze_impact",
        "find_symbols",
        "generate_architecture_docs",
        "generate_blueprint",
        "navigate_codebase",
        "trace_paths",
        "trim_context",
    ];
    assert_eq!(names, expected_names);

    // 3. Strict schema validation per tool
    for tool in tools {
        let name = tool["name"].as_str().unwrap();
        let desc = tool["description"].as_str().unwrap();
        assert!(
            !desc.trim().is_empty(),
            "Tool '{}' must have a non-empty description",
            name
        );
        assert!(
            desc.len() <= 100,
            "Tool '{}' description should be concise (<= 100 chars), got {}",
            name,
            desc.len()
        );

        let schema = &tool["inputSchema"];
        assert_eq!(
            schema["type"], "object",
            "Tool '{}' inputSchema must be object",
            name
        );
        assert!(
            schema["properties"].is_object(),
            "Tool '{}' inputSchema must have properties",
            name
        );

        match name {
            "trim_context" => {
                let props = &schema["properties"];
                assert!(props["seeds"].is_object());
                assert!(props["query"].is_object());
                assert!(props["budget"].is_object());
                assert!(props["path"].is_object());
            }
            "find_symbols" => {
                let props = &schema["properties"];
                assert!(props["query"].is_object());
                assert!(props["symbol"].is_object());
                assert!(props["targetFiles"].is_object());
                assert!(props["limit"].is_object());
                assert!(props["path"].is_object());
            }
            "analyze_graph" => {
                let props = &schema["properties"];
                assert!(props["aspect"].is_object());
                assert!(props["resolution"].is_object());
                assert!(props["path"].is_object());
            }
            "analyze_impact" => {
                let props = &schema["properties"];
                assert!(props["symbol"].is_object());
                assert!(props["fromDiff"].is_object());
                assert!(props["budget"].is_object());
                assert!(props["path"].is_object());
            }
            "trace_paths" => {
                let props = &schema["properties"];
                assert!(props["source"].is_object());
                assert!(props["target"].is_object());
                let required = schema["required"].as_array().unwrap();
                let req_fields: Vec<&str> = required.iter().map(|v| v.as_str().unwrap()).collect();
                assert!(req_fields.contains(&"source"));
                assert!(req_fields.contains(&"target"));
            }
            "navigate_codebase" => {
                let props = &schema["properties"];
                assert!(props["query"].is_object());
                assert!(props["budget"].is_object());
                let required = schema["required"].as_array().unwrap();
                let req_fields: Vec<&str> = required.iter().map(|v| v.as_str().unwrap()).collect();
                assert!(req_fields.contains(&"query"));
            }
            "generate_blueprint" => {
                let props = &schema["properties"];
                assert!(props["task"].is_object());
                let required = schema["required"].as_array().unwrap();
                let req_fields: Vec<&str> = required.iter().map(|v| v.as_str().unwrap()).collect();
                assert!(req_fields.contains(&"task"));
            }
            "generate_architecture_docs" => {
                let props = &schema["properties"];
                assert!(props["path"].is_object());
                assert!(props["output"].is_object());
            }
            _ => panic!("Unexpected tool: {}", name),
        }
    }

    // 4. Payload size freeze assertion
    let serialized = serde_json::to_string(&resp).unwrap();
    assert!(
        serialized.len() < 4000,
        "tools/list response payload exceeds 4,000 characters (actual: {})",
        serialized.len()
    );
}
