//! Comprehensive property and integration tests for Multiplex CSR Graphs and Task-Conditioned PPR.
//!
//! Verifies:
//! 1. Mathematical probability mass conservation: sum(p) + sum(r) == 1.0.
//! 2. Teleportation lower bound invariant: p(s) >= alpha * s_0(s).
//! 3. Task-conditioned diffusion divergence: intent-driven divergence between Bugfix and Documentation tasks.
//! 4. Locality and sparsity scaling: support size decreases monotonically with increasing epsilon.
//! 5. Zero-allocation typed neighbor iteration consistency.
//! 6. Serialization roundtrip fidelity.

use std::path::PathBuf;

use repotrim_engine::{
    MultiplexCsrGraph, PprConfig, PprSolver, RelationType, RelationWeights, SymbolId, SymbolKind,
    SymbolNode, TaskContext, TaskKind, TextSpan,
};

fn make_node(id: u32, name: &str, kind: SymbolKind, file: &str) -> SymbolNode {
    SymbolNode {
        id: SymbolId(id),
        name: name.to_string(),
        kind,
        file_path: PathBuf::from(file),
        span: TextSpan::new(0, 20, 0, 2),
        signature: format!("pub fn {}()", name),
        docstring: None,
        token_cost: 15,
        ast_hash: [0u8; 32],
        container_name: None,
        trait_name: None,
    }
}

/// Helper: builds a rich polyglot multiplex graph with 6 nodes and diverse edge relation types.
///
/// Topology:
/// - 0: `api_controller` (Function)
/// - 1: `service_layer` (Function)
/// - 2: `data_model` (Struct)
/// - 3: `test_service` (Function)
/// - 4: `doc_readme` (Module)
/// - 5: `db_driver` (Function)
///
/// Relations:
/// - 0 -> 1: Calls
/// - 1 -> 2: References
/// - 1 -> 5: Calls
/// - 3 -> 1: IsTestedBy
/// - 4 -> 0: Contains
/// - 4 -> 1: Contains
/// - 1 -> 5: CoChangesWith
/// - 0 -> 5: Imports
fn build_sample_multiplex_graph() -> MultiplexCsrGraph {
    let symbols = vec![
        make_node(0, "api_controller", SymbolKind::Function, "src/api.rs"),
        make_node(1, "service_layer", SymbolKind::Function, "src/service.rs"),
        make_node(2, "data_model", SymbolKind::Struct, "src/model.rs"),
        make_node(
            3,
            "test_service",
            SymbolKind::Function,
            "tests/service_test.rs",
        ),
        make_node(4, "doc_readme", SymbolKind::Module, "docs/README.md"),
        make_node(5, "db_driver", SymbolKind::Function, "src/db.rs"),
    ];

    let edges = vec![
        (0, 1, RelationType::Calls, 1.0),
        (1, 2, RelationType::References, 0.8),
        (1, 5, RelationType::Calls, 1.2),
        (3, 1, RelationType::IsTestedBy, 2.0),
        (4, 0, RelationType::Contains, 1.0),
        (4, 1, RelationType::Contains, 1.0),
        (1, 5, RelationType::CoChangesWith, 1.5),
        (0, 5, RelationType::Imports, 0.7),
    ];

    MultiplexCsrGraph::from_typed_edges(symbols, &edges)
}

#[test]
fn test_multiplex_probability_conservation() {
    let graph = build_sample_multiplex_graph();
    let solver = PprSolver::new(PprConfig {
        alpha: 0.15,
        epsilon: 1e-5,
        max_iterations: 50_000,
    });

    let weights = RelationWeights::default();
    let p_matrix = graph.build_transition_matrix(&weights);

    // Test across each single-seed and multi-seed scenario
    for seed_node in 0..graph.num_symbols() {
        let seeds = vec![(SymbolId(seed_node as u32), 1.0)];
        let res = solver.compute_detailed_on_csr(graph.num_symbols(), &p_matrix, &seeds);

        let sum_p: f32 = res.scores.values().sum();
        let sum_r: f32 = res.residuals.values().sum();
        let total_mass = sum_p + sum_r;

        assert!(
            (total_mass - 1.0).abs() < 1e-4,
            "Seed {} total probability mass must equal 1.0, got {}",
            seed_node,
            total_mass
        );
    }
}

#[test]
fn test_multiplex_teleportation_lower_bound() {
    let graph = build_sample_multiplex_graph();
    let alpha = 0.20;
    let solver = PprSolver::new(PprConfig {
        alpha,
        epsilon: 1e-5,
        max_iterations: 50_000,
    });

    let weights = RelationWeights::default();
    let p_matrix = graph.build_transition_matrix(&weights);

    for seed_node in 0..graph.num_symbols() {
        let sid = SymbolId(seed_node as u32);
        let seeds = vec![(sid, 1.0)];
        let scores = solver.compute_on_csr(graph.num_symbols(), &p_matrix, &seeds);

        let seed_score = scores.get(&sid).copied().unwrap_or(0.0);
        assert!(
            seed_score >= alpha - 1e-4,
            "Seed {} score {} violated teleportation lower bound {}",
            seed_node,
            seed_score,
            alpha
        );
    }
}

#[test]
fn test_task_conditioned_diffusion_divergence() {
    let graph = build_sample_multiplex_graph();
    let solver = PprSolver::new(PprConfig {
        alpha: 0.15,
        epsilon: 1e-5,
        max_iterations: 50_000,
    });

    let bug_task = TaskContext::from_prompt("fix failure and bug in service layer");
    assert_eq!(bug_task.kind(), TaskKind::Bugfix);

    let doc_task = TaskContext::from_prompt("write architecture documentation and guide");
    assert_eq!(doc_task.kind(), TaskKind::Documentation);

    // Seed at service_layer (node 1)
    let seeds = vec![(SymbolId(1), 1.0)];

    let p_bug = solver.compute_multiplex(&graph, &bug_task, &seeds);
    let p_doc = solver.compute_multiplex(&graph, &doc_task, &seeds);

    // Compute cosine similarity between p_bug and p_doc
    let mut dot = 0.0_f32;
    let mut norm_bug = 0.0_f32;
    let mut norm_doc = 0.0_f32;

    for i in 0..graph.num_symbols() {
        let sid = SymbolId(i as u32);
        let b = p_bug.get(&sid).copied().unwrap_or(0.0);
        let d = p_doc.get(&sid).copied().unwrap_or(0.0);
        dot += b * d;
        norm_bug += b * b;
        norm_doc += d * d;
    }

    let cosine_sim = dot / (norm_bug.sqrt() * norm_doc.sqrt());

    // Divergence verification: the tasks must produce measurably different diffusion vectors
    assert!(
        cosine_sim < 0.99,
        "Expected task conditioning divergence, but cosine similarity was too high: {}",
        cosine_sim
    );

    // Node 5 (db_driver) is linked via Calls and CoChangesWith: should receive higher relative mass in Bugfix
    let score_5_bug = p_bug.get(&SymbolId(5)).copied().unwrap_or(0.0);
    let score_5_doc = p_doc.get(&SymbolId(5)).copied().unwrap_or(0.0);
    assert!(
        score_5_bug > score_5_doc,
        "Bugfix task should push more mass to co-changed/called db_driver than Documentation: {} vs {}",
        score_5_bug,
        score_5_doc
    );
}

#[test]
fn test_multiplex_sparsity_monotonicity_across_epsilon() {
    let graph = build_sample_multiplex_graph();
    let epsilons = [1e-5, 1e-4, 1e-3, 1e-2, 1e-1];
    let seeds = vec![(SymbolId(0), 1.0)];
    let weights = RelationWeights::default();
    let p_matrix = graph.build_transition_matrix(&weights);

    let mut previous_support = usize::MAX;

    for &eps in &epsilons {
        let solver = PprSolver::new(PprConfig {
            alpha: 0.15,
            epsilon: eps,
            max_iterations: 10_000,
        });

        let scores = solver.compute_on_csr(graph.num_symbols(), &p_matrix, &seeds);
        let non_zeros = scores.len();

        assert!(
            non_zeros <= previous_support,
            "Sparsity invariant violated: non_zeros {} > previous {} at epsilon {}",
            non_zeros,
            previous_support,
            eps
        );

        previous_support = non_zeros;
    }
}

#[test]
fn test_typed_neighbors_consistency() {
    let graph = build_sample_multiplex_graph();

    for i in 0..graph.num_symbols() {
        let sid = SymbolId(i as u32);
        let iterator_edges: Vec<(RelationType, u32, f32)> = graph.typed_neighbors(sid).collect();

        // Calculate expected edges from individual slices
        let mut expected_edges: Vec<(RelationType, u32, f32)> = Vec::new();
        for rel in RelationType::ALL {
            let (neighbors, weights) = graph.row_slice_for(sid, rel);
            for (&dst, &w) in neighbors.iter().zip(weights.iter()) {
                expected_edges.push((rel, dst, w));
            }
        }

        assert_eq!(
            iterator_edges.len(),
            expected_edges.len(),
            "Mismatch in typed neighbors count for symbol {}",
            i
        );

        for (actual, expected) in iterator_edges.iter().zip(expected_edges.iter()) {
            assert_eq!(actual.0, expected.0);
            assert_eq!(actual.1, expected.1);
            assert!((actual.2 - expected.2).abs() < 1e-6);
        }
    }
}

#[test]
fn test_multiplex_graph_serialization_roundtrip() {
    let graph = build_sample_multiplex_graph();
    let weights = RelationWeights::default();

    let json = serde_json::to_string_pretty(&graph).expect("Serialization failed");
    let deserialized: MultiplexCsrGraph =
        serde_json::from_str(&json).expect("Deserialization failed");

    assert_eq!(deserialized.num_symbols(), graph.num_symbols());
    assert_eq!(deserialized.num_edges(), graph.num_edges());

    let solver = PprSolver::default();
    let seeds = vec![(SymbolId(0), 1.0)];

    let p_orig = solver.compute_multiplex_with_weights(&graph, &weights, &seeds);
    let p_deser = solver.compute_multiplex_with_weights(&deserialized, &weights, &seeds);

    for (k, v) in &p_orig {
        let v_deser = p_deser.get(k).copied().unwrap_or(0.0);
        assert!((v - v_deser).abs() < 1e-6);
    }
}
