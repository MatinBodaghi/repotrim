use proptest::prelude::*;
use repotrim_engine::{
    EvidenceKernel, MultiplexCsrGraph, ProbabilisticCoverage, RelationType, SparseKernelMatrix,
    SymbolId, SymbolKind, SymbolNode, TaskContext, TextSpan,
};
use std::path::PathBuf;

fn mock_symbol(id: u32, name: &str, file: &str, container: Option<&str>) -> SymbolNode {
    SymbolNode {
        id: SymbolId(id),
        name: name.to_string(),
        kind: SymbolKind::Function,
        file_path: PathBuf::from(file),
        span: TextSpan::new(0, 50, 1, 5),
        signature: format!("pub fn {}()", name),
        docstring: None,
        token_cost: 25,
        ast_hash: [0u8; 32],
        container_name: container.map(|s| s.to_string()),
        trait_name: None,
    }
}

#[test]
fn test_kernel_normalization_and_self_coverage() {
    let syms = vec![
        mock_symbol(0, "init_engine", "src/core.rs", None),
        mock_symbol(1, "execute_step", "src/core.rs", Some("Engine")),
        mock_symbol(2, "log_telemetry", "src/logging.rs", None),
    ];
    let edges = vec![
        (0, 1, RelationType::Contains, 1.0),
        (1, 2, RelationType::Calls, 1.0),
    ];
    let graph = MultiplexCsrGraph::from_typed_edges(syms, &edges);
    let kernel = EvidenceKernel::default().compute(&graph, None);

    assert_eq!(kernel.num_symbols(), 3);
    for i in 0..3 {
        let sym_id = SymbolId(i);
        assert_eq!(
            kernel.get(sym_id, sym_id),
            1.0,
            "Self coverage must be identically 1.0"
        );
    }

    // Proximity decay
    let k_0_1 = kernel.get(SymbolId(0), SymbolId(1));
    let k_0_2 = kernel.get(SymbolId(0), SymbolId(2));
    assert!(k_0_1 > 0.0);
    assert!(k_0_2 > 0.0);
    assert!(
        k_0_1 > k_0_2,
        "1-hop direct containment must exceed 2-hop cross-file call: {} vs {}",
        k_0_1,
        k_0_2
    );
}

#[test]
fn test_boundedness_and_normalization() {
    let rows = vec![
        vec![(SymbolId(0), 1.0), (SymbolId(1), 0.5)],
        vec![(SymbolId(0), 0.4), (SymbolId(1), 1.0), (SymbolId(2), 0.8)],
        vec![(SymbolId(2), 1.0)],
    ];
    let kernel = SparseKernelMatrix::from_row_entries(3, &rows);
    let weights = vec![2.0, 3.0, 5.0];
    let total_w: f32 = weights.iter().sum();
    let cov = ProbabilisticCoverage::new(kernel, weights);

    assert_eq!(
        cov.evaluate_batch(&[]),
        0.0,
        "Empty set coverage must be 0.0"
    );

    let all_symbols = vec![SymbolId(0), SymbolId(1), SymbolId(2)];
    let full_cov = cov.evaluate_batch(&all_symbols);
    assert!(
        full_cov <= total_w + 1e-5,
        "Coverage cannot exceed sum of weights"
    );
    assert!(
        (full_cov - total_w).abs() < 1e-5,
        "With 1.0 self-coverage for all nodes, full set must achieve total weight"
    );
}

#[test]
fn test_evidence_kernel_task_conditioned_differentiation() {
    let syms = vec![
        mock_symbol(0, "compute_refund", "src/billing.rs", None),
        mock_symbol(1, "test_refund_success", "tests/billing_test.rs", None),
        mock_symbol(2, "billing_schema", "src/types.rs", None),
    ];
    let edges = vec![
        (0, 1, RelationType::IsTestedBy, 1.0),
        (0, 2, RelationType::References, 1.0),
    ];
    let graph = MultiplexCsrGraph::from_typed_edges(syms, &edges);

    // 1. Task: TestCreation
    let task_test = TaskContext::from_query("write tests for billing refund");
    let kernel_test = EvidenceKernel::default().compute(&graph, Some(&task_test));

    // 2. Task: Documentation / General
    let task_doc = TaskContext::from_query("document architecture");
    let kernel_doc = EvidenceKernel::default().compute(&graph, Some(&task_doc));

    let k_test_link = kernel_test.get(SymbolId(0), SymbolId(1));
    let k_doc_link = kernel_doc.get(SymbolId(0), SymbolId(1));

    assert!(
        k_test_link > k_doc_link,
        "TestCreation task must produce higher coverage probability along IsTestedBy edges: {} vs {}",
        k_test_link,
        k_doc_link
    );
}

// =========================================================================
// Property-Based Verification (QuickCheck / PropTest)
// =========================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    /// Verifies the fundamental submodular inequality (diminishing returns):
    /// For any A \subseteq B \subset V and candidate x \notin B:
    /// \Delta(x | A) \ge \Delta(x | B)
    #[test]
    fn prop_submodularity_diminishing_returns(
        n in 4usize..20,
        density in 0.1f32..0.8f32,
    ) {
        // Construct random valid sparse kernel
        let mut rows: Vec<Vec<(SymbolId, f32)>> = Vec::with_capacity(n);
        for i in 0..n {
            let mut row = vec![(SymbolId(i as u32), 1.0)];
            for j in 0..n {
                if i != j {
                    // Pseudorandom pseudo-deterministic entry
                    let pseudo_rand = ((i * 31 + j * 17) % 100) as f32 / 100.0;
                    if pseudo_rand < density {
                        let prob = (0.1 + 0.8 * pseudo_rand).clamp(0.01, 0.99);
                        row.push((SymbolId(j as u32), prob));
                    }
                }
            }
            row.sort_by_key(|&(id, _)| id);
            rows.push(row);
        }

        let kernel = SparseKernelMatrix::from_row_entries(n, &rows);
        let weights: Vec<f32> = (0..n).map(|i| 1.0 + (i as f32 % 5.0)).collect();
        let cov = ProbabilisticCoverage::new(kernel, weights);

        // Construct nested subsets A \subseteq B
        // Split indices: A = {0}, B = {0, 1, 2}, candidate x = 3
        if n >= 4 {
            let set_a = vec![SymbolId(0)];
            let set_b = vec![SymbolId(0), SymbolId(1), SymbolId(2)];
            let candidate = SymbolId(3);

            let mut state_a = cov.new_state();
            for &s in &set_a {
                cov.add_candidate(&mut state_a, s);
            }

            let mut state_b = cov.new_state();
            for &s in &set_b {
                cov.add_candidate(&mut state_b, s);
            }

            let gain_a = cov.marginal_gain(&state_a, candidate);
            let gain_b = cov.marginal_gain(&state_b, candidate);

            prop_assert!(
                gain_a >= gain_b - 1e-5,
                "Submodularity violation: gain_a ({}) < gain_b ({})",
                gain_a,
                gain_b
            );
        }
    }

    /// Verifies strict monotonicity:
    /// S \subseteq T \implies Cov(S) \le Cov(T)
    #[test]
    fn prop_monotonicity_across_nested_subsets(
        n in 4usize..20,
    ) {
        let mut rows: Vec<Vec<(SymbolId, f32)>> = Vec::with_capacity(n);
        for i in 0..n {
            let mut row = vec![(SymbolId(i as u32), 1.0)];
            for j in 0..n {
                if i != j && (i + j) % 2 == 0 {
                    row.push((SymbolId(j as u32), 0.5));
                }
            }
            row.sort_by_key(|&(id, _)| id);
            rows.push(row);
        }

        let kernel = SparseKernelMatrix::from_row_entries(n, &rows);
        let weights = vec![1.0; n];
        let cov = ProbabilisticCoverage::new(kernel, weights);

        let mut state = cov.new_state();
        let mut prev_cov = 0.0_f32;

        for i in 0..n {
            let candidate = SymbolId(i as u32);
            let gain = cov.marginal_gain(&state, candidate);
            prop_assert!(gain >= -1e-6, "Marginal gain must be non-negative");

            cov.add_candidate(&mut state, candidate);
            let curr_cov = state.total_coverage();

            prop_assert!(
                curr_cov >= prev_cov - 1e-6,
                "Monotonicity violation: curr ({}) < prev ({})",
                curr_cov,
                prev_cov
            );
            prev_cov = curr_cov;
        }
    }

    /// Verifies incremental log-space evaluation matches full batch evaluation.
    #[test]
    fn prop_incremental_vs_batch_parity(
        n in 3usize..15,
    ) {
        let mut rows: Vec<Vec<(SymbolId, f32)>> = Vec::with_capacity(n);
        for i in 0..n {
            let mut row = vec![(SymbolId(i as u32), 1.0)];
            for j in 0..n {
                if i != j && (i + j) % 3 == 0 {
                    row.push((SymbolId(j as u32), 0.4));
                }
            }
            row.sort_by_key(|&(id, _)| id);
            rows.push(row);
        }

        let kernel = SparseKernelMatrix::from_row_entries(n, &rows);
        let weights: Vec<f32> = (0..n).map(|i| 0.5 + (i as f32 * 0.2)).collect();
        let cov = ProbabilisticCoverage::new(kernel, weights);

        let mut state = cov.new_state();
        let mut selected = Vec::new();

        for i in 0..n {
            let candidate = SymbolId(i as u32);
            cov.add_candidate(&mut state, candidate);
            selected.push(candidate);

            let batch_val = cov.evaluate_batch(&selected);
            let inc_val = state.total_coverage();

            let diff = (batch_val - inc_val).abs();
            prop_assert!(
                diff < 1e-5,
                "Parity mismatch at step {}: batch={} vs inc={} (diff={})",
                i,
                batch_val,
                inc_val,
                diff
            );
        }
    }
}
