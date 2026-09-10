use std::collections::HashMap;
use std::path::PathBuf;

use repotrim_engine::{
    EdgeKind, ImpactAnalyzer, LayerWeights, MultiplexGraph, ReferenceEdge, RiskLevel, SymbolId,
    SymbolKind, SymbolNode, TextSpan,
};

fn make_node(id: u32, name: &str, file: &str, kind: SymbolKind) -> SymbolNode {
    SymbolNode {
        id: SymbolId(id),
        name: name.to_string(),
        kind,
        file_path: PathBuf::from(file),
        span: TextSpan::new(0, 100, 1, 10),
        signature: format!("fn {name}()"),
        docstring: Some(format!("Documentation for {name}")),
        token_cost: 20,
        ast_hash: [id as u8; 32],
        container_name: None,
        trait_name: None,
    }
}

#[test]
fn test_impact_analysis_e2e_ripple_and_tests() {
    // Topology:
    // Core utility: fn compute_hash() [node 0, src/hash.rs]
    // 1st-order production caller: fn process_payload() [node 1, src/service.rs] -> calls compute_hash
    // 2nd-order production caller: fn handle_http_request() [node 2, src/api.rs] -> calls process_payload
    // 3rd-order production caller: fn main() [node 3, src/main.rs] -> calls handle_http_request
    //
    // Test callers:
    // Rust test: fn test_hash_calculation() [node 4, tests/hash_test.rs] -> calls compute_hash
    // Python test: fn test_service_integration() [node 5, tests/test_service.py] -> calls process_payload
    // Go test: fn TestApiEndpoint() [node 6, api/api_test.go] -> calls handle_http_request
    // TypeScript test: fn test_e2e() [node 7, frontend/src/e2e.spec.ts] -> calls handle_http_request
    //
    // Independent node: fn independent_task() [node 8, src/task.rs]

    let symbols = vec![
        make_node(0, "compute_hash", "src/hash.rs", SymbolKind::Function),
        make_node(1, "process_payload", "src/service.rs", SymbolKind::Function),
        make_node(2, "handle_http_request", "src/api.rs", SymbolKind::Function),
        make_node(3, "main", "src/main.rs", SymbolKind::Function),
        make_node(
            4,
            "test_hash_calculation",
            "tests/hash_test.rs",
            SymbolKind::Function,
        ),
        make_node(
            5,
            "test_service_integration",
            "tests/test_service.py",
            SymbolKind::Function,
        ),
        make_node(
            6,
            "TestApiEndpoint",
            "api/api_test.go",
            SymbolKind::Function,
        ),
        make_node(
            7,
            "test_e2e",
            "frontend/src/e2e.spec.ts",
            SymbolKind::Function,
        ),
        make_node(8, "independent_task", "src/task.rs", SymbolKind::Function),
    ];

    let edges = vec![
        // 1 calls 0
        ReferenceEdge {
            source: SymbolId(1),
            target_ident: "compute_hash".to_string(),
            kind: EdgeKind::Call,
        },
        // 2 calls 1
        ReferenceEdge {
            source: SymbolId(2),
            target_ident: "process_payload".to_string(),
            kind: EdgeKind::Call,
        },
        // 3 calls 2
        ReferenceEdge {
            source: SymbolId(3),
            target_ident: "handle_http_request".to_string(),
            kind: EdgeKind::Call,
        },
        // Test 4 calls 0
        ReferenceEdge {
            source: SymbolId(4),
            target_ident: "compute_hash".to_string(),
            kind: EdgeKind::Call,
        },
        // Test 5 calls 1
        ReferenceEdge {
            source: SymbolId(5),
            target_ident: "process_payload".to_string(),
            kind: EdgeKind::Call,
        },
        // Test 6 calls 2
        ReferenceEdge {
            source: SymbolId(6),
            target_ident: "handle_http_request".to_string(),
            kind: EdgeKind::Call,
        },
        // Test 7 calls 2
        ReferenceEdge {
            source: SymbolId(7),
            target_ident: "handle_http_request".to_string(),
            kind: EdgeKind::Call,
        },
    ];

    let multiplex = MultiplexGraph::build(symbols, &edges, LayerWeights::default());
    let file_sources = HashMap::new();

    // 1. Analyze impact of mutating `compute_hash` (node 0)
    let report = ImpactAnalyzer::analyze_symbols(&multiplex, &[SymbolId(0)], 4000, &file_sources);

    // Direct mutations
    assert_eq!(report.mutated_symbols.len(), 1);
    assert_eq!(report.mutated_symbols[0].name, "compute_hash");

    // 1st-order production caller: process_payload (node 1)
    let direct_caller_names: Vec<&str> = report
        .direct_impact
        .iter()
        .map(|s| s.name.as_str())
        .collect();
    assert_eq!(direct_caller_names, vec!["process_payload"]);

    // Transitive production callers: handle_http_request and main
    let ripple_names: Vec<&str> = report
        .transitive_impact
        .iter()
        .map(|s| s.name.as_str())
        .collect();
    assert!(ripple_names.contains(&"handle_http_request"));
    assert!(ripple_names.contains(&"main"));
    assert_eq!(ripple_names.len(), 2);

    // Independent task (node 8) should NOT be affected
    assert!(!ripple_names.contains(&"independent_task"));
    assert!(!direct_caller_names.contains(&"independent_task"));

    // Affected test targets: all 4 polyglot tests should be detected
    let test_names: Vec<&str> = report
        .affected_tests
        .iter()
        .map(|s| s.name.as_str())
        .collect();
    assert_eq!(test_names.len(), 4);
    assert!(test_names.contains(&"test_hash_calculation")); // Rust test in tests/
    assert!(test_names.contains(&"test_service_integration")); // Python test in test_*.py
    assert!(test_names.contains(&"TestApiEndpoint")); // Go test in *_test.go
    assert!(test_names.contains(&"test_e2e")); // TypeScript test in *.spec.ts

    // Check summary statistics and risk rating
    assert_eq!(report.summary.mutated_count, 1);
    assert_eq!(report.summary.direct_impact_count, 1);
    assert_eq!(report.summary.transitive_impact_count, 2);
    assert_eq!(report.summary.affected_tests_count, 4);
    assert!(report.summary.risk_score > 0.3);
    assert!(
        matches!(
            report.summary.risk_level,
            RiskLevel::High | RiskLevel::Critical
        ),
        "Expected High or Critical risk, got {:?}",
        report.summary.risk_level
    );

    // Verify markdown generation
    let md = &report.context_markdown;
    assert!(md.contains("# Semantic Change Impact Analysis Report"));
    assert!(md.contains("compute_hash"));
    assert!(md.contains("process_payload"));
    assert!(md.contains("`test_hash_calculation`"));
    assert!(md.contains("## Recommended Test Suite Candidates"));
}

#[test]
fn test_impact_analysis_by_name() {
    let symbols = vec![
        make_node(0, "compute_hash", "src/hash.rs", SymbolKind::Function),
        make_node(1, "process_payload", "src/service.rs", SymbolKind::Function),
    ];

    let edges = vec![ReferenceEdge {
        source: SymbolId(1),
        target_ident: "compute_hash".to_string(),
        kind: EdgeKind::Call,
    }];

    let multiplex = MultiplexGraph::build(symbols, &edges, LayerWeights::default());
    let file_sources = HashMap::new();

    let report = ImpactAnalyzer::analyze_symbol(&multiplex, "compute_hash", 2000, &file_sources);
    assert_eq!(report.mutated_symbols.len(), 1);
    assert_eq!(report.direct_impact.len(), 1);
    assert_eq!(report.direct_impact[0].name, "process_payload");
}

#[test]
fn test_impact_analysis_cycle_resilience() {
    // Cyclic graph: A (0) <-> B (1)
    let symbols = vec![
        make_node(0, "cycle_a", "src/a.rs", SymbolKind::Function),
        make_node(1, "cycle_b", "src/b.rs", SymbolKind::Function),
    ];

    let edges = vec![
        ReferenceEdge {
            source: SymbolId(1),
            target_ident: "cycle_a".to_string(),
            kind: EdgeKind::Call,
        },
        ReferenceEdge {
            source: SymbolId(0),
            target_ident: "cycle_b".to_string(),
            kind: EdgeKind::Call,
        },
    ];

    let multiplex = MultiplexGraph::build(symbols, &edges, LayerWeights::default());
    let file_sources = HashMap::new();

    // Must terminate without infinite loop
    let report = ImpactAnalyzer::analyze_symbols(&multiplex, &[SymbolId(0)], 2000, &file_sources);
    assert_eq!(report.mutated_symbols.len(), 1);
    assert_eq!(report.direct_impact.len(), 1);
    assert_eq!(report.direct_impact[0].name, "cycle_b");
    assert_eq!(report.transitive_impact.len(), 0);
}

#[test]
fn test_impact_risk_level_zero_callers() {
    let symbols = vec![
        make_node(0, "isolated_fn", "src/util.rs", SymbolKind::Function),
        make_node(1, "other_fn", "src/other.rs", SymbolKind::Function),
    ];

    let multiplex = MultiplexGraph::build(symbols, &[], LayerWeights::default());
    let file_sources = HashMap::new();

    let report = ImpactAnalyzer::analyze_symbols(&multiplex, &[SymbolId(0)], 2000, &file_sources);
    assert_eq!(report.summary.direct_impact_count, 0);
    assert_eq!(report.summary.transitive_impact_count, 0);
    assert_eq!(report.summary.affected_tests_count, 0);
    assert_eq!(report.summary.risk_level, RiskLevel::Low);
    assert!(report.summary.risk_score < 0.1);
}

#[test]
fn test_impact_is_test_symbol_polyglot() {
    // Rust tests
    let r1 = make_node(0, "test_foo", "tests/foo.rs", SymbolKind::Function);
    let r2 = make_node(1, "test_bar", "src/foo_test.rs", SymbolKind::Function);
    let r3 = make_node(2, "test_baz", "src/foo.rs", SymbolKind::Function);
    assert!(ImpactAnalyzer::is_test_symbol(&r1));
    assert!(ImpactAnalyzer::is_test_symbol(&r2));
    assert!(ImpactAnalyzer::is_test_symbol(&r3));

    // Python tests
    let p1 = make_node(
        3,
        "test_endpoint",
        "tests/test_api.py",
        SymbolKind::Function,
    );
    let p2 = make_node(4, "test_client", "src/client_test.py", SymbolKind::Function);
    assert!(ImpactAnalyzer::is_test_symbol(&p1));
    assert!(ImpactAnalyzer::is_test_symbol(&p2));

    // TypeScript / JavaScript tests
    let t1 = make_node(5, "shouldRender", "src/App.test.tsx", SymbolKind::Function);
    let t2 = make_node(
        6,
        "shouldCompute",
        "test/math.spec.ts",
        SymbolKind::Function,
    );
    assert!(ImpactAnalyzer::is_test_symbol(&t1));
    assert!(ImpactAnalyzer::is_test_symbol(&t2));

    // Go tests
    let g1 = make_node(7, "TestServer", "pkg/server_test.go", SymbolKind::Function);
    assert!(ImpactAnalyzer::is_test_symbol(&g1));

    // Non-test symbols
    let non_test = make_node(8, "compute_salary", "src/finance.rs", SymbolKind::Function);
    assert!(!ImpactAnalyzer::is_test_symbol(&non_test));
}
