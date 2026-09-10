use repotrim_engine::{
    ContextSelector, EdgeKind, MultiplexGraph, PprConfig, PprSolver, ReferenceEdge, SymbolId,
    SymbolKind, SymbolNode, TextSpan,
};
use std::collections::HashMap;
use std::path::PathBuf;

fn make_test_symbol(id: u32, name: &str, file: &str, cost: usize) -> SymbolNode {
    SymbolNode {
        id: SymbolId(id),
        name: name.to_string(),
        kind: SymbolKind::Function,
        file_path: PathBuf::from(file),
        span: TextSpan::new(0, 100, 1, 10),
        signature: format!("fn {}()", name),
        docstring: None,
        token_cost: cost,
        ast_hash: [0u8; 32],
        container_name: None,
        trait_name: None,
    }
}

#[test]
fn test_sensitivity_stability_index_convergence_across_epsilon() {
    // Construct a star graph where seed 0 calls 10 workers
    let n = 11;
    let mut symbols = Vec::new();
    symbols.push(make_test_symbol(0, "dispatcher", "src/dispatcher.rs", 20));
    for i in 1..n {
        symbols.push(make_test_symbol(
            i as u32,
            &format!("worker_{}", i),
            "src/workers.rs",
            15,
        ));
    }

    let mut edges = Vec::new();
    for i in 1..n {
        edges.push(ReferenceEdge {
            source: SymbolId(0),
            target_ident: format!("worker_{}", i),
            kind: EdgeKind::Call,
        });
    }

    let graph = MultiplexGraph::build(symbols, &edges, Default::default());

    // Sweep epsilon from coarse (1e-2) to fine (1e-6)
    let epsilons = [1e-2_f32, 1e-4_f32, 1e-6_f32];
    let mut previous_max_residual = f32::MAX;

    for &eps in &epsilons {
        let solver = PprSolver::new(PprConfig {
            alpha: 0.15,
            epsilon: eps,
            max_iterations: 100_000,
        });

        let ppr_result = solver.compute_detailed(&graph, &[(SymbolId(0), 1.0)]);
        assert!(!ppr_result.truncated);
        assert!(ppr_result.max_residual < eps);

        // Theoretical residuals must monotonically decrease with epsilon
        assert!(
            ppr_result.max_residual <= previous_max_residual,
            "Residual {:e} did not decrease from {:e}",
            ppr_result.max_residual,
            previous_max_residual
        );
        previous_max_residual = ppr_result.max_residual;

        let selector = ContextSelector::default();
        let selected = vec![SymbolId(0), SymbolId(1)];

        let report =
            selector
                .celf()
                .analyze_sensitivity(&graph, &ppr_result, &selected, 40, 0.15, eps);

        assert_eq!(report.selected_count, 2);
        assert!(report.stability_index >= 0.0 && report.stability_index <= 1.0);
    }
}

#[test]
fn test_sensitivity_borderline_candidate_identification() {
    // 3 symbols: seed (s0), selected candidate (s1), unselected rival (s2)
    // s1 and s2 have identical costs and near-identical incoming edges in separate files
    let s0 = make_test_symbol(0, "orchestrator", "src/main.rs", 10);
    let s1 = make_test_symbol(1, "service_a", "src/service_a.rs", 10);
    let s2 = make_test_symbol(2, "service_b", "src/service_b.rs", 10);

    let edges = vec![
        ReferenceEdge {
            source: SymbolId(0),
            target_ident: "service_a".to_string(),
            kind: EdgeKind::Call,
        },
        ReferenceEdge {
            source: SymbolId(0),
            target_ident: "service_b".to_string(),
            kind: EdgeKind::Call,
        },
    ];

    let graph = MultiplexGraph::build(vec![s0, s1, s2], &edges, Default::default());

    let solver = PprSolver::new(PprConfig {
        alpha: 0.15,
        epsilon: 1e-3,
        max_iterations: 50_000,
    });

    let ppr_result = solver.compute_detailed(&graph, &[(SymbolId(0), 1.0)]);
    let selector = ContextSelector::default();

    // Budget allows only s0 and s1 (budget 20, costs: 10 + 10 = 20)
    let selected = vec![SymbolId(0), SymbolId(1)];

    let report =
        selector
            .celf()
            .analyze_sensitivity(&graph, &ppr_result, &selected, 20, 0.15, 1e-3);

    assert_eq!(report.selected_count, 2);
    // Because service_a and service_b are structurally symmetric, service_b is a borderline rival to service_a!
    let found_borderline = report
        .borderline_pairs
        .iter()
        .any(|p| p.unselected_symbol == SymbolId(2));
    assert!(
        found_borderline,
        "Expected service_b to be detected as a borderline candidate for service_a"
    );
}

#[test]
fn test_end_to_end_context_selection_with_sensitivity() {
    let s0 = make_test_symbol(0, "api_handler", "src/api.rs", 15);
    let s1 = make_test_symbol(1, "authenticate", "src/auth.rs", 25);
    let s2 = make_test_symbol(2, "verify_token", "src/auth.rs", 30);

    let edges = vec![
        ReferenceEdge {
            source: SymbolId(0),
            target_ident: "authenticate".to_string(),
            kind: EdgeKind::Call,
        },
        ReferenceEdge {
            source: SymbolId(1),
            target_ident: "verify_token".to_string(),
            kind: EdgeKind::Call,
        },
    ];

    let graph = MultiplexGraph::build(vec![s0, s1, s2], &edges, Default::default());
    let selector = ContextSelector::default();

    let mut sources = HashMap::new();
    sources.insert(
        PathBuf::from("src/api.rs"),
        "fn api_handler() {}".to_string(),
    );
    sources.insert(
        PathBuf::from("src/auth.rs"),
        "fn authenticate() {}\nfn verify_token() {}".to_string(),
    );

    let (selected, markdown, sensitivity) =
        selector.select_and_format_context_with_sensitivity(&graph, &[SymbolId(0)], 50, &sources);

    assert!(!selected.is_empty());
    assert!(!markdown.is_empty());
    assert_eq!(sensitivity.selected_count, selected.len());
    assert!(sensitivity.stability_index >= 0.0 && sensitivity.stability_index <= 1.0);
    assert!(sensitivity.max_error_bound >= 0.0);
}
