use repotrim_mcp::McpServer;
use std::io::Cursor;
use std::path::PathBuf;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[test]
fn test_mcp_malformed_json_returns_parse_error() {
    let root = repo_root();
    let input = "{this is definitely not valid json}\n";
    let mut writer = Vec::new();
    let mut server = McpServer::with_root(&root);
    server
        .run_loop(Cursor::new(input.as_bytes()), &mut writer)
        .expect("Server loop failed");

    let output_str = String::from_utf8(writer).expect("UTF-8");
    let resp: serde_json::Value = serde_json::from_str(output_str.trim()).expect("Valid JSON-RPC");
    assert_eq!(resp["jsonrpc"], "2.0");
    assert!(resp["id"].is_null());
    assert_eq!(resp["error"]["code"], -32700); // PARSE_ERROR
    assert!(resp["error"]["message"]
        .as_str()
        .unwrap()
        .contains("Parse error"));
}

#[test]
fn test_mcp_invalid_jsonrpc_version_returns_invalid_request() {
    let root = repo_root();
    let input = r#"{"jsonrpc":"1.0","id":100,"method":"ping"}"#;
    let mut writer = Vec::new();
    let mut server = McpServer::with_root(&root);
    server
        .run_loop(Cursor::new(input.as_bytes()), &mut writer)
        .expect("Server loop failed");

    let output_str = String::from_utf8(writer).expect("UTF-8");
    let resp: serde_json::Value = serde_json::from_str(output_str.trim()).expect("Valid JSON-RPC");
    assert_eq!(resp["id"], 100);
    assert_eq!(resp["error"]["code"], -32600); // INVALID_REQUEST
    assert!(resp["error"]["message"]
        .as_str()
        .unwrap()
        .contains("Invalid JSON-RPC"));
}

#[test]
fn test_mcp_unknown_method_returns_method_not_found() {
    let root = repo_root();
    let input = r#"{"jsonrpc":"2.0","id":200,"method":"nonexistent_method_xyz"}"#;
    let mut writer = Vec::new();
    let mut server = McpServer::with_root(&root);
    server
        .run_loop(Cursor::new(input.as_bytes()), &mut writer)
        .expect("Server loop failed");

    let output_str = String::from_utf8(writer).expect("UTF-8");
    let resp: serde_json::Value = serde_json::from_str(output_str.trim()).expect("Valid JSON-RPC");
    assert_eq!(resp["id"], 200);
    assert_eq!(resp["error"]["code"], -32601); // METHOD_NOT_FOUND
    assert!(resp["error"]["message"]
        .as_str()
        .unwrap()
        .contains("Unknown method"));
}

#[test]
fn test_mcp_tools_call_missing_and_invalid_params() {
    let root = repo_root();

    // 1. Missing params object
    let input1 = r#"{"jsonrpc":"2.0","id":301,"method":"tools/call"}"#;
    let mut writer1 = Vec::new();
    let mut server1 = McpServer::with_root(&root);
    server1
        .run_loop(Cursor::new(input1.as_bytes()), &mut writer1)
        .expect("Server loop failed");
    let resp1: serde_json::Value =
        serde_json::from_str(String::from_utf8(writer1).unwrap().trim()).unwrap();
    assert_eq!(resp1["id"], 301);
    assert_eq!(resp1["error"]["code"], -32602); // INVALID_PARAMS

    // 2. Params is not an object (e.g. string)
    let input2 = r#"{"jsonrpc":"2.0","id":302,"method":"tools/call","params":"not_an_object"}"#;
    let mut writer2 = Vec::new();
    let mut server2 = McpServer::with_root(&root);
    server2
        .run_loop(Cursor::new(input2.as_bytes()), &mut writer2)
        .expect("Server loop failed");
    let resp2: serde_json::Value =
        serde_json::from_str(String::from_utf8(writer2).unwrap().trim()).unwrap();
    assert_eq!(resp2["id"], 302);
    assert_eq!(resp2["error"]["code"], -32602); // INVALID_PARAMS

    // 3. Params missing 'name' field
    let input3 = r#"{"jsonrpc":"2.0","id":303,"method":"tools/call","params":{"arguments":{}}}"#;
    let mut writer3 = Vec::new();
    let mut server3 = McpServer::with_root(&root);
    server3
        .run_loop(Cursor::new(input3.as_bytes()), &mut writer3)
        .expect("Server loop failed");
    let resp3: serde_json::Value =
        serde_json::from_str(String::from_utf8(writer3).unwrap().trim()).unwrap();
    assert_eq!(resp3["id"], 303);
    assert_eq!(resp3["error"]["code"], -32602); // INVALID_PARAMS
}

#[test]
fn test_mcp_unknown_tool_returns_error_result_without_panic() {
    let root = repo_root();
    let input = r#"{"jsonrpc":"2.0","id":400,"method":"tools/call","params":{"name":"unknown_tool_random","arguments":{}}}"#;
    let mut writer = Vec::new();
    let mut server = McpServer::with_root(&root);
    server
        .run_loop(Cursor::new(input.as_bytes()), &mut writer)
        .expect("Server loop failed");

    let output_str = String::from_utf8(writer).expect("UTF-8");
    let resp: serde_json::Value = serde_json::from_str(output_str.trim()).expect("Valid JSON-RPC");
    assert_eq!(resp["id"], 400);
    assert_eq!(resp["result"]["isError"], true);
    let text = resp["result"]["content"][0]["text"].as_str().unwrap();
    assert!(text.contains("Unsupported tool 'unknown_tool_random'"));
}

#[test]
fn test_mcp_notification_handling_does_not_return_response() {
    let root = repo_root();

    // Standard notification
    let input1 = r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#;
    let mut writer1 = Vec::new();
    let mut server1 = McpServer::with_root(&root);
    server1
        .run_loop(Cursor::new(input1.as_bytes()), &mut writer1)
        .expect("Server loop failed");
    assert!(
        writer1.is_empty(),
        "Notifications must never produce a response"
    );

    // Unknown notification
    let input2 =
        r#"{"jsonrpc":"2.0","method":"notifications/custom_event","params":{"key":"val"}}"#;
    let mut writer2 = Vec::new();
    let mut server2 = McpServer::with_root(&root);
    server2
        .run_loop(Cursor::new(input2.as_bytes()), &mut writer2)
        .expect("Server loop failed");
    assert!(
        writer2.is_empty(),
        "Unknown notifications must be silently dropped without error response"
    );
}

#[test]
fn test_mcp_whitespace_and_empty_lines_ignored() {
    let root = repo_root();
    let input = "\n\n   \t  \r\n\n";
    let mut writer = Vec::new();
    let mut server = McpServer::with_root(&root);
    server
        .run_loop(Cursor::new(input.as_bytes()), &mut writer)
        .expect("Server loop failed");
    assert!(
        writer.is_empty(),
        "Whitespace lines must be completely ignored"
    );
}

#[test]
fn test_mcp_continuous_stream_survivability() {
    let root = repo_root();

    // Feed a sequence containing malformed lines, notifications, and valid requests
    let input = concat!(
        "not json\n",
        r#"{"jsonrpc":"1.0","id":1,"method":"ping"}"#,
        "\n",
        "\n",
        r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
        "\n",
        r#"{"jsonrpc":"2.0","id":2,"method":"ping"}"#,
        "\n",
        r#"{"jsonrpc":"2.0","id":3,"method":"unknown_foo"}"#,
        "\n",
        r#"{"jsonrpc":"2.0","id":4,"method":"initialize","params":{"protocolVersion":"2024-11-05"}}"#,
        "\n",
        r#"{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"unknown_bar"}}"#,
        "\n"
    );

    let mut writer = Vec::new();
    let mut server = McpServer::with_root(&root);
    server
        .run_loop(Cursor::new(input.as_bytes()), &mut writer)
        .expect("Server loop must survive all inputs");

    let output_str = String::from_utf8(writer).expect("UTF-8");
    let lines: Vec<&str> = output_str.trim().lines().collect();

    // 1. parse error for "not json"
    // 2. invalid request for "1.0"
    // 3. (notification produces no line)
    // 4. ping response for id 2
    // 5. method not found for id 3
    // 6. initialize response for id 4
    // 7. tool result (error) for id 5
    assert_eq!(lines.len(), 6, "Expected 6 responses for stream");

    let r1: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
    assert_eq!(r1["error"]["code"], -32700);

    let r2: serde_json::Value = serde_json::from_str(lines[1]).unwrap();
    assert_eq!(r2["id"], 1);
    assert_eq!(r2["error"]["code"], -32600);

    let r3: serde_json::Value = serde_json::from_str(lines[2]).unwrap();
    assert_eq!(r3["id"], 2);
    assert_eq!(r3["result"], serde_json::json!({}));

    let r4: serde_json::Value = serde_json::from_str(lines[3]).unwrap();
    assert_eq!(r4["id"], 3);
    assert_eq!(r4["error"]["code"], -32601);

    let r5: serde_json::Value = serde_json::from_str(lines[4]).unwrap();
    assert_eq!(r5["id"], 4);
    assert_eq!(r5["result"]["protocolVersion"], "2024-11-05");

    let r6: serde_json::Value = serde_json::from_str(lines[5]).unwrap();
    assert_eq!(r6["id"], 5);
    assert_eq!(r6["result"]["isError"], true);
}
