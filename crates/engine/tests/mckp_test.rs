//! Empirical evaluation and integration tests for Multiple-Choice Knapsack (MCKP)
//! Joint Symbol Selection and Level-of-Detail (LOD) Optimization.
//!
//! # Algorithmic Grounding
//! Evaluates the Multiple-Choice Knapsack Formulation (Kellerer, Pferschy & Pisinger, 2004,
//! *Knapsack Problems*, Springer, DOI: 10.1007/978-3-540-24777-7; Dyer, 1984, DOI: 10.1287/moor.9.4.627;
//! Zemel, 1980, DOI: 10.1287/moor.5.4.527).
//!
//! Rather than decoupling symbol selection and LOD assignment into two independent sequential steps,
//! RepoTrim models discrete LOD choices ($0 \to \text{Sig} \to \text{Doc} \to \text{Sliced} \to \text{Full}$)
//! as mutually exclusive items within symbol classes, pruning interior points using the upper convex hull.

use repotrim_engine::{
    ContextSelector, EdgeKind, LayerWeights, LodLevel, ModelProfile, MultiplexGraph, ReferenceEdge,
    SymbolId, SymbolKind, SymbolNode, TextSpan, TokenizerModel,
};
use std::collections::HashMap;
use std::path::PathBuf;

fn create_symbol(
    id: u32,
    name: &str,
    file_path: &str,
    docstring: Option<&str>,
    body: &str,
) -> (SymbolNode, String) {
    let doc_str = docstring
        .map(|d| format!("/// {}\n", d))
        .unwrap_or_default();
    let full_src = format!("{}pub fn {}() {{\n{}\n}}\n", doc_str, name, body);
    let span = TextSpan {
        start_byte: 0,
        end_byte: full_src.len(),
        start_row: 0,
        end_row: full_src.lines().count(),
    };
    let sig = format!("pub fn {}()", name);
    let cost = (full_src.len() / 4).max(1);

    let sym = SymbolNode {
        id: SymbolId(id),
        name: name.to_string(),
        kind: SymbolKind::Function,
        file_path: PathBuf::from(file_path),
        span,
        signature: sig,
        docstring: docstring.map(|s| s.to_string()),
        token_cost: cost,
        ast_hash: [0u8; 32],
        container_name: None,
        trait_name: None,
    };
    (sym, full_src)
}

#[test]
fn test_mckp_joint_vs_decoupled_baseline() {
    // Build a synthetic codebase with 8 interrelated symbols:
    // Some have heavy bodies (100+ tokens) but valuable signatures and docs.
    let (s0, src0) = create_symbol(
        0,
        "entry_point",
        "src/main.rs",
        Some("Top-level request handler for incoming events"),
        "    let auth = authenticate_user();\n    let data = fetch_records();\n    render_response(data);\n",
    );
    let (s1, src1) = create_symbol(
        1,
        "authenticate_user",
        "src/auth.rs",
        Some("Validates user credentials and cryptographic tokens against identity provider"),
        "    // Extensive authentication logic with hashing and retry\n"
            .repeat(8)
            .as_str(),
    );
    let (s2, src2) = create_symbol(
        2,
        "fetch_records",
        "src/db.rs",
        Some("Executes parameterized database queries with connection pooling"),
        "    // Heavy database query orchestration\n"
            .repeat(12)
            .as_str(),
    );
    let (s3, src3) = create_symbol(
        3,
        "render_response",
        "src/view.rs",
        Some("Serializes response data to JSON with validation"),
        "    // Complex template and serialization\n"
            .repeat(6)
            .as_str(),
    );
    let (s4, src4) = create_symbol(
        4,
        "token_validator",
        "src/auth.rs",
        Some("Cryptographic token verification helper"),
        "    // JWT signature validation\n".repeat(5).as_str(),
    );
    let (s5, src5) = create_symbol(
        5,
        "pool_manager",
        "src/db.rs",
        Some("Connection pool life cycle manager"),
        "    // Pool health check and lease management\n"
            .repeat(10)
            .as_str(),
    );
    let (s6, src6) = create_symbol(
        6,
        "audit_log",
        "src/audit.rs",
        Some("Appends security audit records"),
        "    // File append and HMAC signing\n".repeat(6).as_str(),
    );
    let (s7, src7) = create_symbol(
        7,
        "rate_limiter",
        "src/rate.rs",
        Some("Sliding window token bucket rate limiter"),
        "    // Atomic CAS token bucket\n".repeat(7).as_str(),
    );

    let edges = vec![
        ReferenceEdge {
            source: SymbolId(0),
            target_ident: "authenticate_user".to_string(),
            kind: EdgeKind::Call,
        },
        ReferenceEdge {
            source: SymbolId(0),
            target_ident: "fetch_records".to_string(),
            kind: EdgeKind::Call,
        },
        ReferenceEdge {
            source: SymbolId(0),
            target_ident: "render_response".to_string(),
            kind: EdgeKind::Call,
        },
        ReferenceEdge {
            source: SymbolId(1),
            target_ident: "token_validator".to_string(),
            kind: EdgeKind::Call,
        },
        ReferenceEdge {
            source: SymbolId(2),
            target_ident: "pool_manager".to_string(),
            kind: EdgeKind::Call,
        },
        ReferenceEdge {
            source: SymbolId(1),
            target_ident: "audit_log".to_string(),
            kind: EdgeKind::Call,
        },
        ReferenceEdge {
            source: SymbolId(0),
            target_ident: "rate_limiter".to_string(),
            kind: EdgeKind::Call,
        },
    ];

    let symbols = vec![s0, s1, s2, s3, s4, s5, s6, s7];
    let graph = MultiplexGraph::build(symbols, &edges, LayerWeights::default());

    let mut sources = HashMap::new();
    sources.insert(PathBuf::from("src/main.rs"), src0);
    sources.insert(PathBuf::from("src/auth.rs"), src1 + "\n" + &src4);
    sources.insert(PathBuf::from("src/db.rs"), src2 + "\n" + &src5);
    sources.insert(PathBuf::from("src/view.rs"), src3);
    sources.insert(PathBuf::from("src/audit.rs"), src6);
    sources.insert(PathBuf::from("src/rate.rs"), src7);

    let selector = ContextSelector::default().with_tokenizer(TokenizerModel::FastHeuristic);
    let seed_id = SymbolId(0);

    // Test across a tight budget (120 tokens)
    let tight_budget = 120usize;

    // Decoupled selection (standard)
    let (decoupled_syms, decoupled_md) = selector.select_and_format_context_weighted(
        &graph,
        &[(seed_id, 1.0)],
        tight_budget,
        &sources,
    );

    // Joint MCKP selection
    let (joint_syms, joint_md, mckp_result) = selector
        .select_and_format_context_weighted_joint_lod(
            &graph,
            &[(seed_id, 1.0)],
            tight_budget,
            &sources,
        );

    assert!(
        mckp_result.total_tokens <= tight_budget,
        "MCKP must strictly respect token budget: used {} vs budget {}",
        mckp_result.total_tokens,
        tight_budget
    );

    // Decoupled baseline evaluated each symbol at its full body token cost during knapsack selection,
    // admitting fewer total symbols into context.
    // MCKP admits symbols at lower LOD (Signature / Doc) when budget is tight, maximizing symbol coverage!
    assert!(
        joint_syms.len() >= decoupled_syms.len(),
        "Joint MCKP should admit at least as many symbols as decoupled selection (got {} vs {})",
        joint_syms.len(),
        decoupled_syms.len()
    );

    assert!(!joint_md.is_empty());
    assert!(!decoupled_md.is_empty());

    // Verify LOD distribution in MCKP result
    let has_signature_or_doc = mckp_result.selected_lods.values().any(|&l| {
        l == LodLevel::SignatureOnly || l == LodLevel::SignatureAndDoc || l == LodLevel::SlicedBody
    });
    assert!(
        has_signature_or_doc,
        "In tight budget, MCKP should choose compact LOD representations"
    );
}

#[test]
fn test_mckp_convex_hull_monotonic_slopes() {
    // Verify that the trace steps from MCKP optimization exhibit strictly non-negative marginal gains
    // and valid cumulative token progressions.
    let (s0, src0) = create_symbol(
        0,
        "kernel",
        "src/kernel.rs",
        Some("Operating system core scheduling loop"),
        "    schedule_next();\n    handle_interrupts();\n",
    );
    let (s1, src1) = create_symbol(
        1,
        "schedule_next",
        "src/sched.rs",
        Some("Determines the next highest priority runnable task"),
        "    // Pick task from runqueue\n".repeat(6).as_str(),
    );
    let (s2, src2) = create_symbol(
        2,
        "handle_interrupts",
        "src/irq.rs",
        Some("Processes pending hardware interrupt queues"),
        "    // Acknowledge interrupt controller\n"
            .repeat(8)
            .as_str(),
    );

    let edges = vec![
        ReferenceEdge {
            source: SymbolId(0),
            target_ident: "schedule_next".to_string(),
            kind: EdgeKind::Call,
        },
        ReferenceEdge {
            source: SymbolId(0),
            target_ident: "handle_interrupts".to_string(),
            kind: EdgeKind::Call,
        },
    ];

    let graph = MultiplexGraph::build(vec![s0, s1, s2], &edges, LayerWeights::default());
    let mut sources = HashMap::new();
    sources.insert(PathBuf::from("src/kernel.rs"), src0);
    sources.insert(PathBuf::from("src/sched.rs"), src1);
    sources.insert(PathBuf::from("src/irq.rs"), src2);

    let selector = ContextSelector::default();
    let (_, _, mckp) =
        selector.select_and_format_context_joint_lod(&graph, &[SymbolId(0)], 500, &sources);

    assert!(
        !mckp.trace.is_empty(),
        "MCKP trace should record upgrade steps"
    );

    let mut prev_tokens = 0usize;
    let mut prev_utility = 0.0f32;

    for step in &mckp.trace {
        assert!(
            step.cumulative_tokens >= prev_tokens,
            "Cumulative tokens must be monotonically non-decreasing: {} >= {}",
            step.cumulative_tokens,
            prev_tokens
        );
        assert!(
            step.cumulative_utility >= prev_utility,
            "Cumulative utility must be monotonically non-decreasing: {} >= {}",
            step.cumulative_utility,
            prev_utility
        );
        assert!(
            step.marginal_gain >= 0.0,
            "Marginal gain must be non-negative: {}",
            step.marginal_gain
        );
        prev_tokens = step.cumulative_tokens;
        prev_utility = step.cumulative_utility;
    }
}

#[test]
fn test_mckp_auto_budgeting_pipeline() {
    let (s0, src0) = create_symbol(
        0,
        "gateway",
        "src/gw.rs",
        Some("API Gateway request router"),
        "    dispatch();\n",
    );
    let (s1, src1) = create_symbol(
        1,
        "dispatch",
        "src/gw.rs",
        Some("Dispatches request to upstream service"),
        "    // Downstream forward\n".repeat(10).as_str(),
    );

    let edges = vec![ReferenceEdge {
        source: SymbolId(0),
        target_ident: "dispatch".to_string(),
        kind: EdgeKind::Call,
    }];

    let graph = MultiplexGraph::build(vec![s0, s1], &edges, LayerWeights::default());
    let mut sources = HashMap::new();
    sources.insert(PathBuf::from("src/gw.rs"), src0 + "\n" + &src1);

    let selector = ContextSelector::default();
    let profile = ModelProfile::claude_3_5_sonnet();

    let (syms, md, report, mckp) = selector.select_and_format_context_auto_joint_lod(
        &graph,
        &[SymbolId(0)],
        profile,
        &sources,
    );

    assert!(!syms.is_empty());
    assert!(!md.is_empty());
    assert!(report.optimal_budget >= profile.min_budget);
    assert!(report.optimal_budget <= profile.max_budget);
    assert!(mckp.total_tokens <= report.optimal_budget);
    assert_eq!(report.selected_count, syms.len());
}
