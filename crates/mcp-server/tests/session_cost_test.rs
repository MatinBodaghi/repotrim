use repotrim_engine::{count_tokens, TokenizerModel};
use repotrim_mcp::McpServer;
use std::io::Cursor;
use std::path::PathBuf;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[derive(Debug, Default, Clone)]
pub struct SessionTokenProfile {
    pub handshake_tokens: usize,
    pub tools_list_tokens: usize,
    pub tool_calls_tokens: usize,
    pub total_session_tokens: usize,
}

impl SessionTokenProfile {
    pub fn calculate_total(&mut self) {
        self.total_session_tokens =
            self.handshake_tokens + self.tools_list_tokens + self.tool_calls_tokens;
    }
}

/// Helper driving an McpServer session over in-memory streams and profiling token consumption.
fn profile_mcp_session(requests: &[&str]) -> (Vec<serde_json::Value>, SessionTokenProfile) {
    let mut profile = SessionTokenProfile::default();
    let root = repo_root();
    let mut server = McpServer::with_root(&root);

    let input_payload = requests.join("\n") + "\n";
    let input_cursor = Cursor::new(input_payload.as_bytes().to_vec());
    let mut output_cursor = Cursor::new(Vec::new());

    server
        .run_loop(input_cursor, &mut output_cursor)
        .expect("Server run_loop must succeed");

    let output_str =
        String::from_utf8(output_cursor.into_inner()).expect("Output must be valid UTF-8");
    let mut responses = Vec::new();

    for line in output_str.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let val: serde_json::Value =
            serde_json::from_str(trimmed).expect("Each line must be a valid JSON-RPC response");
        responses.push(val);
    }

    // Profile each request/response pair
    for (idx, req_str) in requests.iter().enumerate() {
        let req_tokens = count_tokens(req_str, TokenizerModel::FastHeuristic);
        let resp_tokens = if idx < responses.len() {
            let resp_str = serde_json::to_string(&responses[idx]).unwrap_or_default();
            count_tokens(&resp_str, TokenizerModel::FastHeuristic)
        } else {
            0
        };

        if req_str.contains("\"method\":\"initialize\"")
            || req_str.contains("\"method\":\"notifications/initialized\"")
        {
            profile.handshake_tokens += req_tokens + resp_tokens;
        } else if req_str.contains("\"method\":\"tools/list\"") {
            profile.tools_list_tokens += req_tokens + resp_tokens;
        } else if req_str.contains("\"method\":\"tools/call\"") {
            profile.tool_calls_tokens += req_tokens + resp_tokens;
        }
    }

    profile.calculate_total();
    (responses, profile)
}

#[test]
fn test_mcp_whole_session_token_cost_harness() {
    let root = repo_root();
    let root_str = root.display().to_string().replace('\\', "/");

    let session_requests = [
        // 1. initialize
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05"}}"#,
        // 2. notifications/initialized
        r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
        // 3. tools/list
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#,
        // 4. tools/call: trim_context
        &format!(
            r#"{{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{{"name":"trim_context","arguments":{{"seeds":["ContextSelector"],"budget":400,"path":"{}"}}}}}}"#,
            root_str
        ),
    ];

    let (responses, profile) = profile_mcp_session(&session_requests);

    // Assert response structure
    assert!(
        responses.len() >= 3,
        "Expected at least 3 responses (initialize, tools/list, tools/call)"
    );
    assert_eq!(responses[0]["id"], 1);
    assert_eq!(responses[1]["id"], 2);
    assert_eq!(responses[2]["id"], 3);

    // Inspect tools/list payload
    let tools_list_resp = &responses[1];
    let tools = tools_list_resp["result"]["tools"]
        .as_array()
        .expect("tools list must be an array");
    assert!(
        !tools.is_empty(),
        "Tools list must contain exposed tool definitions"
    );

    let tools_list_str = serde_json::to_string(tools_list_resp).expect("Must serialize");
    let tools_list_tokens = count_tokens(&tools_list_str, TokenizerModel::FastHeuristic);

    eprintln!(
        "=== MCP Whole-Session Token Cost Profile ===\n\
         - Handshake Tokens: {}\n\
         - tools/list Tokens: {} (chars: {})\n\
         - Tool Call Tokens: {}\n\
         - Total Session Tokens: {}\n\
         - Declared Tools Count: {}\n\
         ============================================",
        profile.handshake_tokens,
        profile.tools_list_tokens,
        tools_list_str.len(),
        profile.tool_calls_tokens,
        profile.total_session_tokens,
        tools.len()
    );

    // In Phase 3 baseline, tools/list is profiled. We assert it is within bounded sanity limits (< 5000 tokens).
    // In T3.2 (after consolidation & schema compression), this will be asserted <= 1,200 tokens.
    assert!(
        tools_list_tokens > 0,
        "tools/list token count must be non-zero"
    );
    assert!(
        tools_list_tokens < 5000,
        "tools/list must remain bounded, got: {}",
        tools_list_tokens
    );

    // Profile individual tool call context budget adherence
    let tool_call_resp = &responses[2];
    assert!(
        tool_call_resp.get("result").is_some(),
        "trim_context must succeed"
    );
    let content_text = tool_call_resp["result"]["content"][0]["text"]
        .as_str()
        .expect("Result must have text");
    let rendered_tokens = count_tokens(content_text, TokenizerModel::FastHeuristic);
    assert!(
        rendered_tokens <= 450,
        "Rendered context tokens ({}) must adhere to budget (400 + margin)",
        rendered_tokens
    );
}

#[test]
fn test_tools_list_token_budget_invariant() {
    let mut server = McpServer::new();
    let input = "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"tools/list\"}\n";
    let mut output = Cursor::new(Vec::new());

    server
        .run_loop(Cursor::new(input.as_bytes().to_vec()), &mut output)
        .expect("Must succeed");

    let output_str = String::from_utf8(output.into_inner()).expect("UTF-8");
    let resp: serde_json::Value = serde_json::from_str(output_str.trim()).expect("Valid JSON-RPC");
    let resp_str = serde_json::to_string(&resp).expect("Serialize");
    let token_count = count_tokens(&resp_str, TokenizerModel::FastHeuristic);

    eprintln!(
        "Standalone tools/list response: {} characters, {} tokens",
        resp_str.len(),
        token_count
    );

    assert!(token_count > 0);
}
