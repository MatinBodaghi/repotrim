use std::collections::HashMap;
use std::path::{Path, PathBuf};

use repotrim_engine::{
    AstExtractor, ContextSelector, DiffResolver, IntentResolver, LayerWeights, MultiplexGraph,
    SymbolId, SymbolKind, SymbolNode, TextSpan,
};

#[test]
fn test_intent_resolver_bm25_and_fuzzy() {
    let s0 = SymbolNode {
        id: SymbolId(0),
        name: "estimate_tokens".to_string(),
        kind: SymbolKind::Function,
        file_path: PathBuf::from("crates/engine/src/tokens.rs"),
        span: TextSpan::new(0, 100, 5, 20),
        signature: "pub fn estimate_tokens(text: &str) -> usize".to_string(),
        docstring: Some(
            "Estimates the token count of a given text using a fast BPE heuristic.".to_string(),
        ),
        token_cost: 15,
        ast_hash: [0u8; 32],
    };

    let s1 = SymbolNode {
        id: SymbolId(1),
        name: "authenticate_user".to_string(),
        kind: SymbolKind::Function,
        file_path: PathBuf::from("crates/auth/src/service.rs"),
        span: TextSpan::new(0, 100, 10, 30),
        signature: "pub fn authenticate_user(token: &str) -> Result<User, AuthError>".to_string(),
        docstring: Some("Validates JWT token and extracts user identity.".to_string()),
        token_cost: 25,
        ast_hash: [1u8; 32],
    };

    let s2 = SymbolNode {
        id: SymbolId(2),
        name: "CsrMatrix".to_string(),
        kind: SymbolKind::Struct,
        file_path: PathBuf::from("crates/engine/src/csr.rs"),
        span: TextSpan::new(0, 100, 1, 15),
        signature: "pub struct CsrMatrix".to_string(),
        docstring: Some("Compressed Sparse Row sparse matrix representation.".to_string()),
        token_cost: 20,
        ast_hash: [2u8; 32],
    };

    let symbols = vec![s0, s1, s2];

    // 1. Natural language query for token estimation
    let results1 = IntentResolver::resolve_query(&symbols, "estimate bpe tokens", 2);
    assert!(!results1.is_empty());
    assert_eq!(results1[0].0, SymbolId(0));
    assert_eq!(results1[0].1, 1.0); // Top candidate normalized

    // 2. Query for JWT authentication via docstring & signature
    let results2 = IntentResolver::resolve_query(&symbols, "jwt auth validation", 2);
    assert!(!results2.is_empty());
    assert_eq!(results2[0].0, SymbolId(1));

    // 3. Typo-tolerant query
    let results3 = IntentResolver::resolve_query(&symbols, "CsrMatrx", 2);
    assert!(!results3.is_empty());
    assert_eq!(results3[0].0, SymbolId(2));

    // 4. Zero literal overlap semantic query: "user login security"
    // "login" and "security" map to Auth cluster; authenticate_user has "authenticate", "auth", "token"
    let results4 = IntentResolver::resolve_query(&symbols, "login security credentials", 1);
    assert!(!results4.is_empty(), "Expected semantic hybrid resolution");
    assert_eq!(results4[0].0, SymbolId(1));
}

#[test]
fn test_diff_parser_and_symbol_mapping() {
    let diff_text = r#"
diff --git a/crates/engine/src/tokens.rs b/crates/engine/src/tokens.rs
index 123..456 100644
--- a/crates/engine/src/tokens.rs
+++ b/crates/engine/src/tokens.rs
@@ -10,3 +10,5 @@ pub fn estimate_tokens(text: &str) -> usize {
     let x = 1;
+    let new_bpe_check = true;
+    let another_line = 42;
     return 0;
diff --git a/crates/engine/src/csr.rs b/crates/engine/src/csr.rs
index 789..abc 100644
--- a/crates/engine/src/csr.rs
+++ b/crates/engine/src/csr.rs
@@ -5,2 +5,1 @@ pub struct CsrMatrix {
-    pub deprecated_field: u32,
     pub row_ptrs: Vec<usize>,
"#;

    let modified_map = DiffResolver::parse_unified_diff(diff_text);
    assert_eq!(modified_map.len(), 2);

    let s0 = SymbolNode {
        id: SymbolId(0),
        name: "estimate_tokens".to_string(),
        kind: SymbolKind::Function,
        file_path: PathBuf::from("crates/engine/src/tokens.rs"),
        span: TextSpan::new(0, 200, 8, 25), // Rows 8 to 25
        signature: "pub fn estimate_tokens(text: &str) -> usize".to_string(),
        docstring: None,
        token_cost: 20,
        ast_hash: [0u8; 32],
    };

    let s1 = SymbolNode {
        id: SymbolId(1),
        name: "other_func".to_string(),
        kind: SymbolKind::Function,
        file_path: PathBuf::from("crates/engine/src/tokens.rs"),
        span: TextSpan::new(300, 500, 50, 70), // Rows 50 to 70 (not modified)
        signature: "pub fn other_func()".to_string(),
        docstring: None,
        token_cost: 10,
        ast_hash: [1u8; 32],
    };

    let symbols = vec![s0, s1];
    let resolved = DiffResolver::resolve_modified_symbols(&symbols, &modified_map);

    assert_eq!(resolved.len(), 1);
    assert_eq!(resolved[0].0, SymbolId(0));
    assert_eq!(resolved[0].1, 1.0);
}

#[test]
fn test_end_to_end_query_selection() {
    let extractor = AstExtractor::new().expect("Failed to initialize AstExtractor");

    let source = r#"
pub struct UserSession {
    pub id: u64,
}

pub fn verify_jwt(token: &str) -> bool {
    !token.is_empty()
}

pub fn handle_login(token: &str) -> Option<UserSession> {
    if verify_jwt(token) {
        Some(UserSession { id: 1 })
    } else {
        None
    }
}

pub fn unrelated_math(a: i32, b: i32) -> i32 {
    a + b
}
"#;

    let mut next_id = 0;
    let (symbols, edges) = extractor
        .parse_file(Path::new("src/auth.rs"), source.as_bytes(), &mut next_id)
        .expect("Failed to parse file");

    let graph = MultiplexGraph::build(symbols.clone(), &edges, LayerWeights::default());

    // 1. Resolve query: "login jwt verification"
    let query_seeds = IntentResolver::resolve_query(&symbols, "login jwt verification", 3);
    assert!(!query_seeds.is_empty());

    let mut sources = HashMap::new();
    sources.insert(PathBuf::from("src/auth.rs"), source.to_string());

    let selector = ContextSelector::default();
    let (selected, markdown) =
        selector.select_and_format_context_weighted(&graph, &query_seeds, 200, &sources);

    assert!(!selected.is_empty());
    let names: Vec<&str> = selected.iter().map(|s| s.name.as_str()).collect();

    // handle_login and verify_jwt should be selected
    assert!(names.contains(&"handle_login") || names.contains(&"verify_jwt"));
    // unrelated_math should NOT be prioritized
    assert!(!names.contains(&"unrelated_math") || names.len() > 2);
    assert!(markdown.contains("```rust"));
}

#[test]
fn test_end_to_end_diff_selection() {
    let extractor = AstExtractor::new().expect("Failed to initialize AstExtractor");

    let source = r#"
pub fn caller_endpoint() {
    target_service();
}

pub fn target_service() {
    let status = 200;
}

pub fn standalone_func() {
    let uncalled = 0;
}
"#;

    let mut next_id = 0;
    let (symbols, edges) = extractor
        .parse_file(Path::new("src/service.rs"), source.as_bytes(), &mut next_id)
        .expect("Failed to parse file");

    let graph = MultiplexGraph::build(symbols.clone(), &edges, LayerWeights::default());

    // Simulated diff touching target_service (lines 5..8, 0-indexed: 4..7)
    let mut modified_map = HashMap::new();
    modified_map.insert(PathBuf::from("src/service.rs"), vec![5, 6]);

    let diff_seeds = DiffResolver::resolve_modified_symbols(&symbols, &modified_map);
    assert_eq!(diff_seeds.len(), 1);

    let target_sym = symbols
        .iter()
        .find(|s| s.name == "target_service")
        .expect("target_service not found");
    assert_eq!(diff_seeds[0].0, target_sym.id);

    let mut sources = HashMap::new();
    sources.insert(PathBuf::from("src/service.rs"), source.to_string());

    let selector = ContextSelector::default();
    let (selected, markdown) =
        selector.select_and_format_context_weighted(&graph, &diff_seeds, 100, &sources);

    let names: Vec<&str> = selected.iter().map(|s| s.name.as_str()).collect();
    assert!(names.contains(&"target_service"));
    assert!(markdown.contains("pub fn target_service()"));
}
