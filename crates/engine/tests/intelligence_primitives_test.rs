//! Comprehensive integration tests for Codebase Intelligence Primitives (Phase 38).
//!
//! Validates the six foundational primitives:
//! 1. `locate`
//! 2. `neighbors`
//! 3. `trace`
//! 4. `expand`
//! 5. `impact`
//! 6. `context`

use std::collections::HashMap;
use std::path::PathBuf;

use repotrim_engine::{
    CodebaseIntelligence, EdgeDirection, EdgeKind, EngineError, LayerWeights, MultiplexGraph,
    ReferenceEdge, RiskLevel, SymbolId, SymbolKind, SymbolNode, TaskContext, TextSpan,
};

fn make_symbol(
    id: u32,
    name: &str,
    file: &str,
    kind: SymbolKind,
    cost: usize,
    code: &str,
) -> SymbolNode {
    SymbolNode {
        id: SymbolId(id),
        name: name.to_string(),
        kind,
        file_path: PathBuf::from(file),
        span: TextSpan::new(0, code.len(), 1, 10),
        signature: format!("pub fn {}()", name),
        docstring: None,
        token_cost: cost,
        ast_hash: [id as u8; 32],
        container_name: None,
        trait_name: None,
    }
}

fn build_test_harness() -> (MultiplexGraph, HashMap<PathBuf, String>) {
    let syms = vec![
        make_symbol(
            0,
            "handle_request",
            "src/api.rs",
            SymbolKind::Function,
            40,
            "pub fn handle_request() { auth_user(); }",
        ),
        make_symbol(
            1,
            "auth_user",
            "src/auth.rs",
            SymbolKind::Function,
            30,
            "pub fn auth_user() { find_user(); }",
        ),
        make_symbol(
            2,
            "find_user",
            "src/db.rs",
            SymbolKind::Function,
            50,
            "pub fn find_user() { execute_query(); }",
        ),
        make_symbol(
            3,
            "execute_query",
            "src/db.rs",
            SymbolKind::Function,
            60,
            "pub fn execute_query() {}",
        ),
        make_symbol(
            4,
            "test_auth_flow",
            "tests/auth_test.rs",
            SymbolKind::Function,
            35,
            "#[test] fn test_auth_flow() { auth_user(); }",
        ),
        make_symbol(
            5,
            "UserConfig",
            "src/config.rs",
            SymbolKind::Struct,
            20,
            "pub struct UserConfig;",
        ),
    ];

    let edges = vec![
        ReferenceEdge {
            source: SymbolId(0),
            target_ident: "auth_user".to_string(),
            kind: EdgeKind::Call,
        },
        ReferenceEdge {
            source: SymbolId(1),
            target_ident: "find_user".to_string(),
            kind: EdgeKind::Call,
        },
        ReferenceEdge {
            source: SymbolId(2),
            target_ident: "execute_query".to_string(),
            kind: EdgeKind::Call,
        },
        ReferenceEdge {
            source: SymbolId(4),
            target_ident: "auth_user".to_string(),
            kind: EdgeKind::Call,
        },
        ReferenceEdge {
            source: SymbolId(1),
            target_ident: "UserConfig".to_string(),
            kind: EdgeKind::TypeRef,
        },
    ];

    let graph = MultiplexGraph::build(syms, &edges, LayerWeights::default());

    let mut sources = HashMap::new();
    sources.insert(
        PathBuf::from("src/api.rs"),
        "pub fn handle_request() { auth_user(); }\n".to_string(),
    );
    sources.insert(
        PathBuf::from("src/auth.rs"),
        "pub fn auth_user() { find_user(); }\n".to_string(),
    );
    sources.insert(
        PathBuf::from("src/db.rs"),
        "pub fn find_user() { execute_query(); }\npub fn execute_query() {}\n".to_string(),
    );
    sources.insert(
        PathBuf::from("tests/auth_test.rs"),
        "#[test] fn test_auth_flow() { auth_user(); }\n".to_string(),
    );
    sources.insert(
        PathBuf::from("src/config.rs"),
        "pub struct UserConfig;\n".to_string(),
    );

    (graph, sources)
}

#[test]
fn test_locate_primitive() {
    let (graph, sources) = build_test_harness();
    let intel = CodebaseIntelligence::new(&graph, &sources);

    // 1. Locate by query
    let ranked = intel.locate_query("authenticate user login", 3);
    assert!(!ranked.is_empty(), "Should return ranked entrypoints");
    assert_eq!(ranked[0].symbol.name, "auth_user");
    assert!(ranked[0].score > 0.0);
    assert!(!ranked[0].reason.is_empty());

    // 2. Locate by task with target files
    let mut task = TaskContext::from_query("database query execution");
    task.metadata.target_files = vec![PathBuf::from("src/db.rs")];
    let ranked_db = intel.locate(&task, 5);
    assert!(!ranked_db.is_empty());
    assert!(ranked_db[0].symbol.name == "find_user" || ranked_db[0].symbol.name == "execute_query");

    // 3. Locate with empty query falls back to top architectural hubs
    let fallback_results = intel.locate_query("", 3);
    assert!(!fallback_results.is_empty());
    assert_eq!(fallback_results[0].score, 0.30);
    assert!(fallback_results[0].reason.contains("fallback"));
}

#[test]
fn test_neighbors_primitive() {
    let (graph, sources) = build_test_harness();
    let intel = CodebaseIntelligence::new(&graph, &sources);

    // 1. Check 1-hop neighbors of auth_user (SymbolId(1))
    let hood = intel.neighbors(SymbolId(1)).expect("auth_user must exist");
    assert_eq!(hood.focal_symbol.name, "auth_user");
    assert_eq!(hood.focal_symbol.id, SymbolId(1));

    // Outgoing: find_user (Calls), UserConfig (References)
    let outgoing_targets: Vec<SymbolId> = hood
        .outgoing
        .iter()
        .filter(|e| e.direction == EdgeDirection::Outgoing)
        .map(|e| e.target.id)
        .collect();
    assert!(outgoing_targets.contains(&SymbolId(2))); // find_user
    assert!(outgoing_targets.contains(&SymbolId(5))); // UserConfig

    // Incoming: handle_request (Calls), test_auth_flow (Calls)
    let incoming_targets: Vec<SymbolId> = hood
        .incoming
        .iter()
        .filter(|e| e.direction == EdgeDirection::Incoming)
        .map(|e| e.target.id)
        .collect();
    assert!(incoming_targets.contains(&SymbolId(0))); // handle_request
    assert!(incoming_targets.contains(&SymbolId(4))); // test_auth_flow

    // 2. Check 2-hop neighborhood of handle_request (SymbolId(0))
    let hood_2hop = intel
        .neighbors_with_hops(SymbolId(0), 2)
        .expect("handle_request must exist");
    let neighbor_ids: Vec<SymbolId> = hood_2hop.outgoing.iter().map(|e| e.target.id).collect();
    assert!(neighbor_ids.contains(&SymbolId(1))); // 1-hop
    assert!(neighbor_ids.contains(&SymbolId(2))); // 2-hop (find_user)

    // 3. Non-existent symbol
    let err = intel.neighbors(SymbolId(999));
    match err {
        Err(EngineError::SymbolNotFound(999)) => {}
        _ => panic!("Expected SymbolNotFound error"),
    }
}

#[test]
fn test_trace_primitive() {
    let (graph, sources) = build_test_harness();
    let intel = CodebaseIntelligence::new(&graph, &sources);

    // 1. Trace causal path from handle_request (0) to execute_query (3)
    let task = TaskContext::from_query("trace database execution pipeline");
    let trace_result = intel
        .trace(SymbolId(0), SymbolId(3), Some(&task))
        .expect("Trace should succeed");

    assert_eq!(trace_result.source.name, "handle_request");
    assert_eq!(trace_result.target.name, "execute_query");
    assert!(!trace_result.paths.is_empty(), "Expected causal path");

    let best = trace_result
        .best_path
        .expect("Best path should be resolved");
    assert_eq!(
        best.nodes,
        vec![SymbolId(0), SymbolId(1), SymbolId(2), SymbolId(3)]
    );
    assert!(best.probability > 0.0);

    // Verify Mermaid diagram format
    assert!(trace_result.mermaid_diagram.starts_with("```mermaid\n"));
    assert!(trace_result
        .mermaid_diagram
        .contains("participant s0 as handle_request"));
    assert!(trace_result
        .mermaid_diagram
        .contains("participant s3 as execute_query"));
    assert!(trace_result.mermaid_diagram.contains("Calls"));

    // 2. Source equals target (trivial path)
    let trivial = intel
        .trace(SymbolId(1), SymbolId(1), None)
        .expect("Trivial trace should succeed");
    assert_eq!(trivial.paths.len(), 1);
    assert_eq!(trivial.paths[0].nodes, vec![SymbolId(1)]);

    // 3. Non-existent symbol
    let err = intel.trace(SymbolId(0), SymbolId(888), None);
    match err {
        Err(EngineError::SymbolNotFound(888)) => {}
        _ => panic!("Expected SymbolNotFound error"),
    }
}

#[test]
fn test_expand_primitive() {
    let (graph, sources) = build_test_harness();
    let intel = CodebaseIntelligence::new(&graph, &sources);

    // 1. Expand around auth_user (SymbolId(1)) with budget 100
    let exp = intel
        .expand(SymbolId(1), 100, None)
        .expect("Expand should succeed");
    assert_eq!(exp.focal_symbol.id, SymbolId(1));
    assert_eq!(exp.budget, 100);
    assert!(exp.tokens_used <= 100);
    assert!(!exp.symbols.is_empty());
    assert!(!exp.formatted_code.is_empty());

    // 2. Zero budget
    let zero_exp = intel
        .expand(SymbolId(1), 0, None)
        .expect("Zero budget should return empty");
    assert!(zero_exp.symbols.is_empty());
    assert_eq!(zero_exp.tokens_used, 0);

    // 3. Non-existent symbol
    let err = intel.expand(SymbolId(777), 100, None);
    match err {
        Err(EngineError::SymbolNotFound(777)) => {}
        _ => panic!("Expected SymbolNotFound error"),
    }
}

#[test]
fn test_impact_primitive() {
    let (graph, sources) = build_test_harness();
    let intel = CodebaseIntelligence::new(&graph, &sources);

    // 1. Impact analysis for find_user (SymbolId(2))
    let report = intel
        .impact(SymbolId(2), 200)
        .expect("Impact analysis should succeed");
    assert_eq!(report.mutated_symbols.len(), 1);
    assert_eq!(report.mutated_symbols[0].name, "find_user");

    // Downstream caller is auth_user (1), which is called by handle_request (0) and test_auth_flow (4)
    let direct_names: Vec<&str> = report
        .direct_impact
        .iter()
        .map(|s| s.name.as_str())
        .collect();
    assert!(direct_names.contains(&"auth_user"));

    let transitive_names: Vec<&str> = report
        .transitive_impact
        .iter()
        .map(|s| s.name.as_str())
        .collect();
    assert!(transitive_names.contains(&"handle_request"));

    // Affected tests should detect test_auth_flow
    let test_names: Vec<&str> = report
        .affected_tests
        .iter()
        .map(|s| s.name.as_str())
        .collect();
    assert!(test_names.contains(&"test_auth_flow"));

    assert!(report.summary.risk_level >= RiskLevel::Low);
    assert!(!report.context_markdown.is_empty());

    // 2. Invalid symbol
    let err = intel.impact(SymbolId(999), 100);
    match err {
        Err(EngineError::SymbolNotFound(999)) => {}
        _ => panic!("Expected SymbolNotFound error"),
    }
}

#[test]
fn test_context_primitive() {
    let (graph, sources) = build_test_harness();
    let intel = CodebaseIntelligence::new(&graph, &sources);

    // 1. End-to-end task-conditioned context generation
    let mut task = TaskContext::from_query("authenticate user credentials");
    task.metadata.target_files = vec![PathBuf::from("src/auth.rs")];

    let ctx = intel.context(&task, 150);
    assert_eq!(ctx.budget_limit, 150);
    assert!(ctx.tokens_used <= 150);
    assert!(!ctx.symbols.is_empty());
    assert!(ctx.symbols.iter().any(|s| s.name == "auth_user"));

    // 2. Explicit seed context generation
    let ctx_seeds = intel.context_with_seeds(&[(SymbolId(2), 1.0)], Some(&task), 100);
    assert_eq!(ctx_seeds.budget_limit, 100);
    assert!(ctx_seeds.tokens_used <= 100);
    assert!(ctx_seeds.symbols.iter().any(|s| s.id == SymbolId(2)));
}

#[test]
fn test_loader_intelligence_integration() {
    let temp_dir = std::env::temp_dir().join(format!("repotrim_intel_test_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&temp_dir);
    std::fs::create_dir_all(temp_dir.join("src")).unwrap();

    let file = temp_dir.join("src/lib.rs");
    std::fs::write(&file, "pub fn add(a: i32, b: i32) -> i32 { a + b }\n").unwrap();

    let mut repo = repotrim_engine::LoadedRepository::load(&temp_dir).expect("Failed to load repo");
    repo.load_all_sources().expect("Failed to load sources");

    let graph = repo.build_graph();
    let intel = repo.intelligence(&graph);

    let ranked = intel.locate_query("add numbers", 1);
    assert!(!ranked.is_empty());
    assert_eq!(ranked[0].symbol.name, "add");

    let _ = std::fs::remove_dir_all(&temp_dir);
}
