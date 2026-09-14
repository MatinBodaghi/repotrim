//! Comprehensive Integration and Efficiency Tests for Adaptive Submodular Navigator (Phase 41).
//!
//! Grounded in:
//! - Golovin, D., & Krause, A. (2011). "Adaptive Submodularity: Theory and Applications
//!   in Active Learning and Stochastic Optimization". *Journal of Artificial Intelligence Research*,
//!   42, 427–486.
//! - Nemhauser, G. L., Wolsey, L. A., & Fisher, M. L. (1978). "An analysis of approximations for
//!   maximizing submodular set functions—I". *Mathematical Programming*, 14(1), 265–294.

use std::collections::HashMap;
use std::path::PathBuf;

use repotrim_engine::{
    AdaptiveGainEstimator, AdaptiveNavigator, CodebaseIntelligence, EdgeKind, LayerWeights,
    MultiplexGraph, NavigationAction, NavigationState, NavigationTrajectory, NavigatorConfig,
    ReferenceEdge, SymbolId, SymbolKind, SymbolNode, TaskContext, TextSpan,
};

fn make_symbol(
    id: u32,
    name: &str,
    file: &str,
    kind: SymbolKind,
    start_row: usize,
    end_row: usize,
    token_cost: usize,
) -> SymbolNode {
    SymbolNode {
        id: SymbolId(id),
        name: name.to_string(),
        kind,
        file_path: PathBuf::from(file),
        span: TextSpan::new(0, 0, start_row, end_row),
        signature: format!("pub fn {}()", name),
        docstring: Some(format!("Documentation for {}", name)),
        token_cost,
        ast_hash: [id as u8; 32],
        container_name: None,
        trait_name: None,
    }
}

/// Constructs a realistic test repository topology:
///
/// Nodes:
/// - 0: `handle_request` (controller / entrypoint)
/// - 1: `authenticate_user` (auth service)
/// - 2: `validate_token` (token parser)
/// - 3: `query_database` (data store)
/// - 4: `test_auth_flow` (test case for auth)
/// - 5: `unrelated_utility` (unrelated utility function)
///
/// Edges:
/// - 0 -> 1: Calls
/// - 1 -> 2: Calls
/// - 1 -> 3: Calls
/// - 4 -> 1: IsTestedBy
/// - 5 -> 3: References
fn build_test_harness() -> (MultiplexGraph, HashMap<PathBuf, String>) {
    let s0 = make_symbol(
        0,
        "handle_request",
        "src/controller.rs",
        SymbolKind::Function,
        0,
        10,
        80,
    );
    let s1 = make_symbol(
        1,
        "authenticate_user",
        "src/auth.rs",
        SymbolKind::Function,
        0,
        15,
        120,
    );
    let s2 = make_symbol(
        2,
        "validate_token",
        "src/token.rs",
        SymbolKind::Function,
        0,
        8,
        70,
    );
    let s3 = make_symbol(
        3,
        "query_database",
        "src/db.rs",
        SymbolKind::Function,
        0,
        12,
        100,
    );
    let s4 = make_symbol(
        4,
        "test_auth_flow",
        "tests/auth_test.rs",
        SymbolKind::Function,
        0,
        14,
        90,
    );
    let s5 = make_symbol(
        5,
        "unrelated_utility",
        "src/util.rs",
        SymbolKind::Function,
        0,
        6,
        50,
    );

    let symbols = vec![s0, s1, s2, s3, s4, s5];

    let edges = vec![
        ReferenceEdge {
            source: SymbolId(0),
            target_ident: "authenticate_user".to_string(),
            kind: EdgeKind::Call,
        },
        ReferenceEdge {
            source: SymbolId(1),
            target_ident: "validate_token".to_string(),
            kind: EdgeKind::Call,
        },
        ReferenceEdge {
            source: SymbolId(1),
            target_ident: "query_database".to_string(),
            kind: EdgeKind::Call,
        },
        ReferenceEdge {
            source: SymbolId(4),
            target_ident: "authenticate_user".to_string(),
            kind: EdgeKind::Call,
        },
        ReferenceEdge {
            source: SymbolId(5),
            target_ident: "query_database".to_string(),
            kind: EdgeKind::Call,
        },
    ];

    let graph = MultiplexGraph::build(symbols, &edges, LayerWeights::default());

    let mut sources = HashMap::new();
    sources.insert(
        PathBuf::from("src/controller.rs"),
        "pub fn handle_request() {\n    authenticate_user();\n}\n".to_string(),
    );
    sources.insert(
        PathBuf::from("src/auth.rs"),
        "pub fn authenticate_user() {\n    validate_token();\n    query_database();\n}\n"
            .to_string(),
    );
    sources.insert(
        PathBuf::from("src/token.rs"),
        "pub fn validate_token() -> bool {\n    true\n}\n".to_string(),
    );
    sources.insert(
        PathBuf::from("src/db.rs"),
        "pub fn query_database() -> String {\n    \"db_record\".into()\n}\n".to_string(),
    );
    sources.insert(
        PathBuf::from("tests/auth_test.rs"),
        "#[test]\nfn test_auth_flow() {\n    authenticate_user();\n}\n".to_string(),
    );
    sources.insert(
        PathBuf::from("src/util.rs"),
        "pub fn unrelated_utility() {\n    query_database();\n}\n".to_string(),
    );

    (graph, sources)
}

#[test]
fn test_adaptive_gain_estimator_marginal_monotonicity() {
    let (graph, sources) = build_test_harness();
    let intel = CodebaseIntelligence::new(&graph, &sources);
    let task = TaskContext::from_query("authenticate user token");

    let estimator = AdaptiveGainEstimator::default();
    let mut state = NavigationState::with_entrypoints(task, 1500, &[SymbolId(0)]);

    // Action: Inspect authenticate_user
    let action = NavigationAction::Inspect {
        symbol_id: SymbolId(1),
    };

    // Prior to observation, marginal utility is high
    let delta_initial = estimator.estimate_marginal_gain(&action, &state, &intel);
    assert!(
        delta_initial > 0.1,
        "Initial inspection gain must be substantial: {}",
        delta_initial
    );

    // After inspecting, apply action
    state
        .apply_action(action.clone(), &intel)
        .expect("Inspect must succeed");

    // Once inspected at LodLevel::FullBody, marginal utility must diminish to 0.0
    let delta_after = estimator.estimate_marginal_gain(&action, &state, &intel);
    assert_eq!(
        delta_after, 0.0,
        "Marginal gain of already fully observed symbol must be 0.0"
    );
}

#[test]
fn test_adaptive_navigator_budget_respect_and_trajectory() {
    let (graph, sources) = build_test_harness();
    let intel = CodebaseIntelligence::new(&graph, &sources);
    let task = TaskContext::from_query("authenticate user token");

    for &budget in &[200, 500, 1200] {
        let config = NavigatorConfig {
            max_steps: 10,
            min_efficiency_threshold: 0.0001,
            ..Default::default()
        };
        let navigator = AdaptiveNavigator::new(config);

        let trajectory: NavigationTrajectory = navigator
            .navigate(task.clone(), budget, &intel)
            .unwrap_or_else(|e| panic!("Navigation failed for budget {}: {}", budget, e));

        // Strict Budget Invariant
        assert!(
            trajectory.tokens_used() <= budget,
            "Tokens used ({}) exceeded budget ({})",
            trajectory.tokens_used(),
            budget
        );
        assert!(
            trajectory.remaining_budget <= budget,
            "Remaining budget invalid"
        );
        assert!(
            trajectory.step_count() <= 10,
            "Step count exceeded max_steps"
        );
        assert!(!trajectory.termination_reason.is_empty());
        assert!(!trajectory.steps.is_empty());
    }
}

#[test]
fn test_adaptive_navigator_causal_path_discovery() {
    let (graph, sources) = build_test_harness();
    let intel = CodebaseIntelligence::new(&graph, &sources);
    let task = TaskContext::from_query("authenticate user");

    let config = NavigatorConfig {
        max_steps: 8,
        min_efficiency_threshold: 0.0001,
        ..Default::default()
    };
    let navigator = AdaptiveNavigator::new(config);

    let trajectory = navigator
        .navigate(task, 1500, &intel)
        .expect("Navigation must succeed");

    // Verify discovered symbols include relevant core entities
    let observed_names: Vec<&str> = trajectory
        .structured_context
        .symbols
        .iter()
        .map(|s| s.name.as_str())
        .collect();

    assert!(
        observed_names.contains(&"handle_request") || observed_names.contains(&"authenticate_user"),
        "Discovered symbols must contain core task entrypoints: {:?}",
        observed_names
    );
}

#[test]
fn test_adaptive_vs_static_efficiency() {
    let (graph, sources) = build_test_harness();
    let intel = CodebaseIntelligence::new(&graph, &sources);
    let task = TaskContext::from_query("validate user token");

    // Adaptive navigation with tight budget (e.g. 350 tokens)
    let config = NavigatorConfig {
        max_steps: 4,
        ..Default::default()
    };
    let navigator = AdaptiveNavigator::new(config);

    let trajectory = navigator
        .navigate(task, 350, &intel)
        .expect("Adaptive navigation must succeed");

    // Verify adaptive exploration only pulls relevant symbols within budget
    let selected_symbols = &trajectory.structured_context.symbols;
    assert!(!selected_symbols.is_empty());
    assert!(trajectory.tokens_used() <= 350);

    // Unrelated utility should not be prioritized over token validation
    let has_unrelated = selected_symbols
        .iter()
        .any(|s| s.name == "unrelated_utility");
    assert!(
        !has_unrelated,
        "Adaptive exploration must not greedily select unrelated utility when budget is tight"
    );
}

#[test]
fn test_trajectory_serialization_roundtrip() {
    let (graph, sources) = build_test_harness();
    let intel = CodebaseIntelligence::new(&graph, &sources);
    let task = TaskContext::from_query("test auth flow");

    let navigator = AdaptiveNavigator::new(NavigatorConfig {
        max_steps: 5,
        ..Default::default()
    });

    let trajectory = navigator
        .navigate(task, 800, &intel)
        .expect("Navigation must succeed");

    let serialized = serde_json::to_string_pretty(&trajectory)
        .expect("Trajectory JSON serialization must succeed");
    assert!(!serialized.is_empty());

    let deserialized: NavigationTrajectory =
        serde_json::from_str(&serialized).expect("Trajectory JSON deserialization must succeed");

    assert_eq!(deserialized.initial_budget, trajectory.initial_budget);
    assert_eq!(deserialized.remaining_budget, trajectory.remaining_budget);
    assert_eq!(deserialized.step_count(), trajectory.step_count());
    assert_eq!(
        deserialized.termination_reason,
        trajectory.termination_reason
    );
    assert_eq!(
        deserialized.structured_context.symbols.len(),
        trajectory.structured_context.symbols.len()
    );
}
