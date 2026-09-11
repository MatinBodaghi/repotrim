use repotrim_engine::{
    symbol::{EdgeKind, ReferenceEdge, SymbolId, SymbolKind, SymbolNode, TextSpan},
    CommunityConfig, CommunityDetector, ContextSelector, LayerWeights, LoadedRepository,
    MultiplexGraph,
};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

fn make_dummy_symbol(id: u32, name: &str, path: &str) -> SymbolNode {
    SymbolNode {
        id: SymbolId(id),
        name: name.to_string(),
        kind: SymbolKind::Function,
        file_path: PathBuf::from(path),
        span: TextSpan::new(
            id as usize * 10,
            (id as usize + 1) * 10,
            id as usize,
            id as usize + 1,
        ),
        signature: format!("fn {}()", name),
        docstring: None,
        token_cost: 20,
        ast_hash: [0u8; 32],
        container_name: None,
        trait_name: None,
    }
}

#[test]
fn test_two_clique_community_detection() {
    // Two distinct cliques: Clique 0 (symbols 0..4) in auth, Clique 1 (symbols 5..9) in billing
    // with a single bridge edge between 4 and 5.
    let mut symbols = Vec::new();
    for i in 0..5 {
        symbols.push(make_dummy_symbol(i, &format!("auth_{i}"), "src/auth.rs"));
    }
    for i in 5..10 {
        symbols.push(make_dummy_symbol(i, &format!("bill_{i}"), "src/billing.rs"));
    }

    let mut edges = Vec::new();
    // Clique 0 all-to-all
    for u in 0..5 {
        for v in 0..5 {
            if u != v {
                edges.push(ReferenceEdge {
                    source: SymbolId(u),
                    target_ident: format!("auth_{v}"),
                    kind: EdgeKind::Call,
                });
            }
        }
    }
    // Clique 1 all-to-all
    for u in 5..10 {
        for v in 5..10 {
            if u != v {
                edges.push(ReferenceEdge {
                    source: SymbolId(u),
                    target_ident: format!("bill_{v}"),
                    kind: EdgeKind::Call,
                });
            }
        }
    }
    // Bridge edge
    edges.push(ReferenceEdge {
        source: SymbolId(4),
        target_ident: "bill_5".to_string(),
        kind: EdgeKind::Call,
    });

    let graph = MultiplexGraph::build(symbols, &edges, LayerWeights::default());
    let res = CommunityDetector::detect(&graph, &CommunityConfig::meso_scale());

    assert_eq!(res.communities.len(), 2, "Expected 2 cohesive communities");
    assert!(
        res.modularity > 0.35,
        "Modularity should be high for clear two-clique structure, got {}",
        res.modularity
    );

    // Verify all auth symbols are in the same community, and all billing symbols are in the other
    let comm_0 = res.membership[&SymbolId(0)];
    for i in 1..5 {
        assert_eq!(
            res.membership[&SymbolId(i)],
            comm_0,
            "All auth symbols should share community"
        );
    }

    let comm_5 = res.membership[&SymbolId(5)];
    assert_ne!(
        comm_0, comm_5,
        "Auth and billing should be in different communities"
    );
    for i in 6..10 {
        assert_eq!(
            res.membership[&SymbolId(i)],
            comm_5,
            "All billing symbols should share community"
        );
    }
}

#[test]
fn test_multi_resolution_scaling() {
    // Build a graph where a community splits into fine sub-modules at higher resolution gamma
    // Sub-cluster A1: 0, 1, 2 (heavily coupled)
    // Sub-cluster A2: 3, 4, 5 (heavily coupled)
    // Moderate coupling between A1 and A2
    // Separate cluster B: 6, 7, 8
    let mut symbols = Vec::new();
    for i in 0..6 {
        symbols.push(make_dummy_symbol(
            i,
            &format!("mod_a_{i}"),
            "src/module_a.rs",
        ));
    }
    for i in 6..9 {
        symbols.push(make_dummy_symbol(
            i,
            &format!("mod_b_{i}"),
            "src/module_b.rs",
        ));
    }

    let mut edges = Vec::new();
    // A1 internal
    for u in 0..3 {
        for v in 0..3 {
            if u != v {
                edges.push(ReferenceEdge {
                    source: SymbolId(u),
                    target_ident: format!("mod_a_{v}"),
                    kind: EdgeKind::Call,
                });
            }
        }
    }
    // A2 internal
    for u in 3..6 {
        for v in 3..6 {
            if u != v {
                edges.push(ReferenceEdge {
                    source: SymbolId(u),
                    target_ident: format!("mod_a_{v}"),
                    kind: EdgeKind::Call,
                });
            }
        }
    }
    // A1 <-> A2 moderate link
    edges.push(ReferenceEdge {
        source: SymbolId(2),
        target_ident: "mod_a_3".to_string(),
        kind: EdgeKind::Call,
    });
    edges.push(ReferenceEdge {
        source: SymbolId(3),
        target_ident: "mod_a_2".to_string(),
        kind: EdgeKind::Call,
    });

    // B internal
    for u in 6..9 {
        for v in 6..9 {
            if u != v {
                edges.push(ReferenceEdge {
                    source: SymbolId(u),
                    target_ident: format!("mod_b_{v}"),
                    kind: EdgeKind::Call,
                });
            }
        }
    }

    let graph = MultiplexGraph::build(symbols, &edges, LayerWeights::default());

    // Macro scale (gamma = 0.5): A1 and A2 should merge into single macro module
    let macro_res = CommunityDetector::detect(&graph, &CommunityConfig::macro_scale());

    // Micro scale (gamma = 3.0): higher penalty on size splits A1 and A2 into separate sub-communities
    let micro_res = CommunityDetector::detect(&graph, &CommunityConfig::with_resolution(3.0));

    assert!(
        micro_res.communities.len() >= macro_res.communities.len(),
        "Higher resolution should produce at least as many communities: micro={}, macro={}",
        micro_res.communities.len(),
        macro_res.communities.len()
    );
}

#[test]
fn test_architectural_drift_detection() {
    // Symbol 3 is physically declared in `src/utils/helper.rs`, but has 100% of its calls with
    // symbols in `src/billing/engine.rs`.
    // Billing module has 5 cohesive symbols
    let s0 = make_dummy_symbol(0, "bill_a", "src/billing/engine.rs");
    let s1 = make_dummy_symbol(1, "bill_b", "src/billing/engine.rs");
    let s2 = make_dummy_symbol(2, "bill_c", "src/billing/engine.rs");
    let s4 = make_dummy_symbol(4, "bill_d", "src/billing/engine.rs");
    let s5 = make_dummy_symbol(5, "bill_e", "src/billing/engine.rs");
    // Foreign symbol in utils that heavily couples with billing
    let s3 = make_dummy_symbol(3, "misplaced_helper", "src/utils/helper.rs");

    let edges = vec![
        ReferenceEdge {
            source: SymbolId(0),
            target_ident: "bill_b".to_string(),
            kind: EdgeKind::Call,
        },
        ReferenceEdge {
            source: SymbolId(1),
            target_ident: "bill_c".to_string(),
            kind: EdgeKind::Call,
        },
        ReferenceEdge {
            source: SymbolId(2),
            target_ident: "bill_d".to_string(),
            kind: EdgeKind::Call,
        },
        ReferenceEdge {
            source: SymbolId(4),
            target_ident: "bill_e".to_string(),
            kind: EdgeKind::Call,
        },
        ReferenceEdge {
            source: SymbolId(5),
            target_ident: "bill_a".to_string(),
            kind: EdgeKind::Call,
        },
        ReferenceEdge {
            source: SymbolId(3),
            target_ident: "bill_a".to_string(),
            kind: EdgeKind::Call,
        },
        ReferenceEdge {
            source: SymbolId(3),
            target_ident: "bill_b".to_string(),
            kind: EdgeKind::Call,
        },
    ];

    let graph = MultiplexGraph::build(
        vec![s0, s1, s2, s3, s4, s5],
        &edges,
        LayerWeights::default(),
    );
    let res = CommunityDetector::detect(&graph, &CommunityConfig::meso_scale());

    let root_path = Path::new(".");
    let drifts = CommunityDetector::analyze_drift(&graph, &res.communities, root_path);

    assert!(
        !drifts.is_empty(),
        "Should detect architectural drift for misplaced_helper"
    );
    let drift = drifts
        .iter()
        .find(|d| d.symbol_name == "misplaced_helper")
        .expect("misplaced_helper should be in drifts");
    assert_eq!(drift.declared_directory, PathBuf::from("src/utils"));
    assert!(
        drift.drift_score >= 0.5,
        "Drift score should be high, got {}",
        drift.drift_score
    );
}

#[test]
fn test_community_hierarchy_detection() {
    let mut symbols = Vec::new();
    for i in 0..12 {
        symbols.push(make_dummy_symbol(
            i,
            &format!("sym_{i}"),
            &format!("src/sub_{}/file.rs", i / 4),
        ));
    }
    let mut edges = Vec::new();
    for i in 0..11 {
        edges.push(ReferenceEdge {
            source: SymbolId(i),
            target_ident: format!("sym_{}", i + 1),
            kind: EdgeKind::Call,
        });
    }

    let graph = MultiplexGraph::build(symbols, &edges, LayerWeights::default());
    let hierarchy = CommunityDetector::detect_hierarchy(&graph);

    assert!(!hierarchy.macro_communities.is_empty());
    assert!(!hierarchy.meso_communities.is_empty());
    assert!(!hierarchy.micro_communities.is_empty());
    assert!(hierarchy.micro_communities.len() >= hierarchy.macro_communities.len());
}

#[test]
fn test_community_boosted_context_selection() {
    // 2 communities: Community A (0, 1) and Community B (2, 3)
    let s0 = make_dummy_symbol(0, "entry", "src/auth/entry.rs");
    let s1 = make_dummy_symbol(1, "auth_helper", "src/auth/helper.rs");
    let s2 = make_dummy_symbol(2, "billing_hub", "src/billing/hub.rs");
    let s3 = make_dummy_symbol(3, "billing_calc", "src/billing/calc.rs");

    let edges = vec![
        ReferenceEdge {
            source: SymbolId(0),
            target_ident: "auth_helper".to_string(),
            kind: EdgeKind::Call,
        },
        ReferenceEdge {
            source: SymbolId(0),
            target_ident: "billing_hub".to_string(),
            kind: EdgeKind::Call,
        },
        ReferenceEdge {
            source: SymbolId(2),
            target_ident: "billing_calc".to_string(),
            kind: EdgeKind::Call,
        },
    ];

    let graph = MultiplexGraph::build(vec![s0, s1, s2, s3], &edges, LayerWeights::default());
    let selector = ContextSelector::default();

    let mut sources = HashMap::new();
    sources.insert(
        PathBuf::from("src/auth/entry.rs"),
        "fn entry() {}".to_string(),
    );
    sources.insert(
        PathBuf::from("src/auth/helper.rs"),
        "fn auth_helper() {}".to_string(),
    );
    sources.insert(
        PathBuf::from("src/billing/hub.rs"),
        "fn billing_hub() {}".to_string(),
    );
    sources.insert(
        PathBuf::from("src/billing/calc.rs"),
        "fn billing_calc() {}".to_string(),
    );

    // With community boost of 0.5, seeding at entry (SymbolId 0) should prioritize auth_helper
    let (selected, _) = selector.select_and_format_context_with_community(
        &graph,
        &[(SymbolId(0), 1.0)],
        50, // tight budget: fits seed + 1 other symbol
        0.5,
        &sources,
    );

    assert!(!selected.is_empty());
    assert_eq!(selected[0].name, "entry");
}

#[test]
fn test_community_detection_dogfood_on_workspace() {
    let mut repo =
        LoadedRepository::load_with_options(Path::new("."), true).expect("Failed to load repo");
    repo.load_all_sources().expect("Failed to load sources");
    let graph = repo.build_graph();

    let res = CommunityDetector::detect(&graph, &CommunityConfig::meso_scale());
    assert!(
        !res.communities.is_empty(),
        "Should discover communities on repotrim codebase"
    );
    assert!(
        res.modularity > 0.0,
        "Modularity should be positive, got {}",
        res.modularity
    );

    let hierarchy = CommunityDetector::detect_hierarchy(&graph);
    assert!(!hierarchy.macro_communities.is_empty());
    assert!(!hierarchy.micro_communities.is_empty());

    let drifts = CommunityDetector::analyze_drift(&graph, &res.communities, &repo.root_path);
    // Drifts can be 0 or more, but should execute without panic
    for drift in drifts.iter().take(5) {
        assert!(drift.drift_score >= 0.40);
        assert!(!drift.symbol_name.is_empty());
    }
}
