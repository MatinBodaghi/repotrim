//! Integration test suite for Structured Context Graph Object & Diagnostic Explanation Metadata.

use proptest::prelude::*;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use repotrim_engine::{
    ContextSelector, CostBreakdown, EdgeKind, LayerWeights, LodLevel, MultiplexGraph, NodeType,
    OmissionReason, PathTrace, ReferenceEdge, RelationType, StructuredContext, StructuredEdge,
    StructuredSymbol, SymbolId, SymbolKind, SymbolNode, TaskContext, TextSpan,
};

fn make_test_symbol(
    id: u32,
    name: &str,
    file: &str,
    cost: usize,
    signature: &str,
    code: &str,
) -> SymbolNode {
    SymbolNode {
        id: SymbolId(id),
        name: name.to_string(),
        kind: SymbolKind::Function,
        file_path: PathBuf::from(file),
        span: TextSpan::new(0, code.len(), 1, 10),
        signature: signature.to_string(),
        docstring: None,
        token_cost: cost,
        ast_hash: [id as u8; 32],
        container_name: None,
        trait_name: None,
    }
}

#[test]
fn test_structured_context_schema_and_json_roundtrip() {
    let mut ctx = StructuredContext::new(4096);
    ctx.tokens_used = 512;
    ctx.confidence_score = 0.89;
    ctx.task = Some(TaskContext::from_query("implement OAuth2 authentication"));

    let s0 = StructuredSymbol {
        id: SymbolId(0),
        name: "authenticate_oauth".to_string(),
        file_path: PathBuf::from("src/oauth.rs"),
        span: TextSpan::new(0, 120, 1, 6),
        kind: SymbolKind::Function,
        node_type: NodeType::Function,
        lod: LodLevel::SignatureAndDoc,
        token_cost: 60,
        score: 0.94,
        container_name: None,
        trait_name: None,
        code: "pub fn authenticate_oauth(token: &str) -> Result<User>".to_string(),
    };
    let s1 = StructuredSymbol {
        id: SymbolId(1),
        name: "verify_signature".to_string(),
        file_path: PathBuf::from("src/crypto.rs"),
        span: TextSpan::new(0, 80, 1, 5),
        kind: SymbolKind::Function,
        node_type: NodeType::Function,
        lod: LodLevel::FullBody,
        token_cost: 45,
        score: 0.88,
        container_name: None,
        trait_name: None,
        code: "pub fn verify_signature(data: &[u8], sig: &[u8]) -> bool { true }".to_string(),
    };

    ctx.symbols.push(s0);
    ctx.symbols.push(s1);

    ctx.edges.push(StructuredEdge {
        source: SymbolId(0),
        target: SymbolId(1),
        relation: RelationType::Calls,
        weight: 0.95,
    });

    ctx.paths.push(PathTrace {
        nodes: vec![SymbolId(0), SymbolId(1)],
        relations: vec![RelationType::Calls],
        trace: "authenticate_oauth --[Calls]--> verify_signature".to_string(),
        probability: 0.91,
        rationale: "OAuth entrypoint invokes signature validation".to_string(),
    });

    ctx.record_omission(
        SymbolId(2),
        "legacy_sha1",
        PathBuf::from("src/legacy.rs"),
        80,
        0.12,
        0.005,
        OmissionReason::BudgetExhausted,
        "Candidate cost exceeds available budget capacity",
    );

    ctx.cost_breakdown = CostBreakdown::new(105, 30, 20, 40);

    let json_str = ctx.to_json_pretty().expect("JSON serialization failed");
    assert!(json_str.contains("authenticate_oauth"));
    assert!(json_str.contains("verify_signature"));
    assert!(json_str.contains("legacy_sha1"));
    assert!(json_str.contains("BudgetExhausted"));

    let deserialized =
        StructuredContext::from_json(&json_str).expect("JSON deserialization failed");
    assert_eq!(ctx, deserialized);
    assert_eq!(deserialized.num_symbols(), 2);
    assert_eq!(deserialized.num_edges(), 1);
    assert_eq!(deserialized.num_paths(), 1);
    assert_eq!(deserialized.num_omissions(), 1);
}

#[test]
fn test_omission_diagnostic_accounting_invariant() {
    let s0 = make_test_symbol(
        0,
        "entry",
        "src/entry.rs",
        30,
        "fn entry()",
        "fn entry() { a(); b(); c(); }",
    );
    let s1 = make_test_symbol(1, "func_a", "src/a.rs", 25, "fn func_a()", "fn func_a() {}");
    let s2 = make_test_symbol(2, "func_b", "src/b.rs", 40, "fn func_b()", "fn func_b() {}");
    let s3 = make_test_symbol(3, "func_c", "src/c.rs", 50, "fn func_c()", "fn func_c() {}");

    let edges = vec![
        ReferenceEdge {
            source: SymbolId(0),
            target_ident: "func_a".to_string(),
            kind: EdgeKind::Call,
        },
        ReferenceEdge {
            source: SymbolId(0),
            target_ident: "func_b".to_string(),
            kind: EdgeKind::Call,
        },
        ReferenceEdge {
            source: SymbolId(0),
            target_ident: "func_c".to_string(),
            kind: EdgeKind::Call,
        },
    ];

    let graph = MultiplexGraph::build(vec![s0, s1, s2, s3], &edges, LayerWeights::default());
    let selector = ContextSelector::default();

    let mut sources = HashMap::new();
    sources.insert(
        PathBuf::from("src/entry.rs"),
        "fn entry() { a(); b(); c(); }".to_string(),
    );
    sources.insert(PathBuf::from("src/a.rs"), "fn func_a() {}".to_string());
    sources.insert(PathBuf::from("src/b.rs"), "fn func_b() {}".to_string());
    sources.insert(PathBuf::from("src/c.rs"), "fn func_c() {}".to_string());

    // Allocate modest budget fitting only entry and func_a
    let budget = 80;
    let task = TaskContext::from_query("execute entry");
    let ctx =
        selector.select_structured_context(&graph, &[SymbolId(0)], budget, &sources, Some(task));

    // INVARIANT: Every positive candidate in the graph is either included in symbols OR listed in omissions
    let selected_ids: HashSet<SymbolId> = ctx.symbols.iter().map(|s| s.id).collect();
    let omitted_ids: HashSet<SymbolId> = ctx.omissions.iter().map(|o| o.symbol_id).collect();

    // No overlap between selected and omitted symbols
    let intersection: HashSet<_> = selected_ids.intersection(&omitted_ids).collect();
    assert!(
        intersection.is_empty(),
        "Symbol cannot be simultaneously selected and omitted"
    );

    // All graph candidates accounted for
    for sym in graph.symbols() {
        assert!(
            selected_ids.contains(&sym.id) || omitted_ids.contains(&sym.id),
            "Symbol {} was neither selected nor tracked in omissions",
            sym.name
        );
    }

    assert!(
        ctx.tokens_used <= budget,
        "Token expenditure must not exceed budget limit"
    );
    assert!(
        !ctx.omissions.is_empty(),
        "Omissions must be tracked under budget constraints"
    );
}

#[test]
fn test_end_to_end_structured_context_generation_with_paths() {
    let s0 = make_test_symbol(
        0,
        "handle_request",
        "src/server.rs",
        40,
        "fn handle_request()",
        "fn handle_request() { auth(); }",
    );
    let s1 = make_test_symbol(
        1,
        "auth",
        "src/auth.rs",
        35,
        "fn auth()",
        "fn auth() { query_user(); }",
    );
    let s2 = make_test_symbol(
        2,
        "query_user",
        "src/db.rs",
        45,
        "fn query_user()",
        "fn query_user() {}",
    );
    let s3 = make_test_symbol(
        3,
        "format_json",
        "src/json.rs",
        30,
        "fn format_json()",
        "fn format_json() {}",
    );

    let edges = vec![
        ReferenceEdge {
            source: SymbolId(0),
            target_ident: "auth".to_string(),
            kind: EdgeKind::Call,
        },
        ReferenceEdge {
            source: SymbolId(1),
            target_ident: "query_user".to_string(),
            kind: EdgeKind::Call,
        },
        ReferenceEdge {
            source: SymbolId(0),
            target_ident: "format_json".to_string(),
            kind: EdgeKind::Call,
        },
    ];

    let graph = MultiplexGraph::build(vec![s0, s1, s2, s3], &edges, LayerWeights::default());
    let selector = ContextSelector::default();

    let mut sources = HashMap::new();
    sources.insert(
        PathBuf::from("src/server.rs"),
        "fn handle_request() { auth(); }".to_string(),
    );
    sources.insert(
        PathBuf::from("src/auth.rs"),
        "fn auth() { query_user(); }".to_string(),
    );
    sources.insert(PathBuf::from("src/db.rs"), "fn query_user() {}".to_string());
    sources.insert(
        PathBuf::from("src/json.rs"),
        "fn format_json() {}".to_string(),
    );

    let task = TaskContext::from_query("debug auth query user workflow");
    let ctx = selector.select_structured_context(&graph, &[SymbolId(0)], 400, &sources, Some(task));

    assert!(!ctx.is_empty());
    assert!(ctx.num_symbols() >= 3);
    assert!(ctx.confidence_score > 0.50);

    // Verify Markdown serialization contains all primary sections
    let md = ctx.to_markdown();
    assert!(md.contains("# RepoTrim Structured Context Report"));
    assert!(md.contains("## Task Context"));
    assert!(md.contains("## Token Cost Breakdown"));
    assert!(md.contains("## Context Dependency Topology"));
    assert!(md.contains("## Extracted Code Symbols"));

    // Verify Mermaid flowchart
    let flowchart = ctx.generate_mermaid_flowchart();
    assert!(flowchart.contains("graph TD"));
    assert!(flowchart.contains("handle_request"));

    // Verify sequence diagram if paths are discovered
    if !ctx.paths.is_empty() {
        let seq = ctx.generate_mermaid_sequence();
        assert!(seq.contains("sequenceDiagram"));
        for p in &ctx.paths {
            assert!(p.probability >= 0.0 && p.probability <= 1.0);
        }
    }
}

#[test]
fn test_confidence_score_monotonicity() {
    let s0 = make_test_symbol(
        0,
        "entry",
        "src/main.rs",
        30,
        "fn entry()",
        "fn entry() { f1(); f2(); }",
    );
    let s1 = make_test_symbol(1, "f1", "src/f1.rs", 50, "fn f1()", "fn f1() {}");
    let s2 = make_test_symbol(2, "f2", "src/f2.rs", 60, "fn f2()", "fn f2() {}");
    let s3 = make_test_symbol(3, "f3", "src/f3.rs", 70, "fn f3()", "fn f3() {}");

    let edges = vec![
        ReferenceEdge {
            source: SymbolId(0),
            target_ident: "f1".to_string(),
            kind: EdgeKind::Call,
        },
        ReferenceEdge {
            source: SymbolId(0),
            target_ident: "f2".to_string(),
            kind: EdgeKind::Call,
        },
        ReferenceEdge {
            source: SymbolId(1),
            target_ident: "f3".to_string(),
            kind: EdgeKind::Call,
        },
    ];

    let graph = MultiplexGraph::build(vec![s0, s1, s2, s3], &edges, LayerWeights::default());
    let selector = ContextSelector::default();

    let mut sources = HashMap::new();
    sources.insert(
        PathBuf::from("src/main.rs"),
        "fn entry() { f1(); f2(); }".to_string(),
    );
    sources.insert(PathBuf::from("src/f1.rs"), "fn f1() {}".to_string());
    sources.insert(PathBuf::from("src/f2.rs"), "fn f2() {}".to_string());
    sources.insert(PathBuf::from("src/f3.rs"), "fn f3() {}".to_string());

    let task = TaskContext::from_query("run entry pipeline");

    let ctx_small = selector.select_structured_context(
        &graph,
        &[SymbolId(0)],
        40,
        &sources,
        Some(task.clone()),
    );
    let ctx_medium = selector.select_structured_context(
        &graph,
        &[SymbolId(0)],
        120,
        &sources,
        Some(task.clone()),
    );
    let ctx_large =
        selector.select_structured_context(&graph, &[SymbolId(0)], 400, &sources, Some(task));

    assert!(
        ctx_small.confidence_score <= ctx_medium.confidence_score + 1e-4,
        "Confidence score should not decrease with higher budget: small={:.3}, med={:.3}",
        ctx_small.confidence_score,
        ctx_medium.confidence_score
    );
    assert!(
        ctx_medium.confidence_score <= ctx_large.confidence_score + 1e-4,
        "Confidence score should not decrease with higher budget: med={:.3}, large={:.3}",
        ctx_medium.confidence_score,
        ctx_large.confidence_score
    );
    assert!(
        ctx_large.confidence_score >= 0.70,
        "Large budget context should achieve high confidence score: {:.3}",
        ctx_large.confidence_score
    );
}

#[test]
fn test_mermaid_diagram_formatting_and_sanitization() {
    let mut ctx = StructuredContext::new(1000);
    ctx.tokens_used = 100;
    ctx.confidence_score = 0.85;

    let sym1 = StructuredSymbol {
        id: SymbolId(0),
        name: "Parser<T>".to_string(),
        file_path: PathBuf::from("src/parser.rs"),
        span: TextSpan::new(0, 50, 1, 5),
        kind: SymbolKind::Struct,
        node_type: NodeType::Struct,
        lod: LodLevel::SignatureOnly,
        token_cost: 20,
        score: 0.90,
        container_name: None,
        trait_name: None,
        code: "struct Parser<T>".to_string(),
    };
    let sym2 = StructuredSymbol {
        id: SymbolId(1),
        name: "parse_\"json\"".to_string(),
        file_path: PathBuf::from("src/json.rs"),
        span: TextSpan::new(0, 50, 1, 5),
        kind: SymbolKind::Function,
        node_type: NodeType::Function,
        lod: LodLevel::SignatureOnly,
        token_cost: 20,
        score: 0.80,
        container_name: None,
        trait_name: None,
        code: "fn parse_json()".to_string(),
    };

    ctx.symbols.push(sym1);
    ctx.symbols.push(sym2);

    ctx.edges.push(StructuredEdge {
        source: SymbolId(0),
        target: SymbolId(1),
        relation: RelationType::Calls,
        weight: 0.85,
    });

    let diagram = ctx.generate_mermaid_flowchart();
    assert!(diagram.starts_with("```mermaid\ngraph TD\n"));
    assert!(diagram.ends_with("```\n"));
    // Double quotes in symbol names must be sanitized to single quotes
    assert!(!diagram.contains("parse_\"json\""));
    assert!(diagram.contains("parse_'json'"));
    assert!(diagram.contains("s0 -->|Calls| s1"));
}

proptest! {
    #[test]
    fn prop_structured_context_budget_and_confidence_invariants(
        budget in 50usize..500usize,
        relevance_scale in 0.1f32..1.0f32,
    ) {
        let s0 = make_test_symbol(0, "root", "src/root.rs", 25, "fn root()", "fn root() { c1(); }");
        let s1 = make_test_symbol(1, "c1", "src/c1.rs", 30, "fn c1()", "fn c1() {}");
        let edges = vec![
            ReferenceEdge { source: SymbolId(0), target_ident: "c1".to_string(), kind: EdgeKind::Call }
        ];

        let graph = MultiplexGraph::build(vec![s0, s1], &edges, LayerWeights::default());
        let selector = ContextSelector::default();

        let mut sources = HashMap::new();
        sources.insert(PathBuf::from("src/root.rs"), "fn root() { c1(); }".to_string());
        sources.insert(PathBuf::from("src/c1.rs"), "fn c1() {}".to_string());

        let seeds = vec![(SymbolId(0), relevance_scale)];
        let task = TaskContext::from_query("test query");
        let ctx = selector.select_structured_context_weighted(&graph, &seeds, budget, &sources, Some(task));

        prop_assert!(ctx.tokens_used <= budget, "tokens_used {} > budget {}", ctx.tokens_used, budget);
        prop_assert!(ctx.confidence_score >= 0.0 && ctx.confidence_score <= 1.0,
            "confidence_score out of bounds: {}", ctx.confidence_score);
    }
}
