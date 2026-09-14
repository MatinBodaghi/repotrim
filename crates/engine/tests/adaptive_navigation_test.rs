//! Comprehensive integration tests for Sequential Navigation & Action Engine (Phase 40).
//!
//! Validates:
//! 1. `NavigationState` initialization and invariant tracking
//! 2. `Inspect`, `Expand`, `Trace`, `TestLink`, and `Stop` action execution
//! 3. Strict budget decrementation and multi-factor cost accounting
//! 4. Exploration frontier evolution and neighbor discovery
//! 5. `ActionGenerator` candidate proposal and feasibility constraints
//! 6. `StructuredContext` synthesis from navigation trajectory
//! 7. JSON serialization and state persistence round-trips

use std::collections::HashMap;
use std::path::PathBuf;

use repotrim_engine::{
    ActionGenerator, ActionGeneratorConfig, AdaptiveGainConfig, AdaptiveGainEstimator,
    AdaptiveNavigator, CandidateAction, CodebaseIntelligence, EdgeKind, EngineError, LayerWeights,
    LodLevel, MultiplexGraph, NavigationAction, NavigationState, NavigationTrajectory,
    NavigatorConfig, Observation, ReferenceEdge, SymbolId, SymbolKind, SymbolNode, TaskContext,
    TextSpan,
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
        span: TextSpan::new(0, code.len(), 0, 10),
        signature: format!("pub fn {}()", name),
        docstring: None,
        token_cost: cost,
        ast_hash: [id as u8; 32],
        container_name: None,
        trait_name: None,
    }
}

fn build_navigation_harness() -> (MultiplexGraph, HashMap<PathBuf, String>) {
    let syms = vec![
        make_symbol(
            0,
            "handle_request",
            "src/api.rs",
            SymbolKind::Function,
            40,
            "pub fn handle_request() {\n    auth_user();\n}",
        ),
        make_symbol(
            1,
            "auth_user",
            "src/auth.rs",
            SymbolKind::Function,
            30,
            "pub fn auth_user() {\n    find_user();\n}",
        ),
        make_symbol(
            2,
            "find_user",
            "src/db.rs",
            SymbolKind::Function,
            50,
            "pub fn find_user() {\n    execute_query();\n}",
        ),
        make_symbol(
            3,
            "execute_query",
            "src/db.rs",
            SymbolKind::Function,
            60,
            "pub fn execute_query() {\n    // query execution\n}",
        ),
        make_symbol(
            4,
            "test_auth_flow",
            "tests/auth_test.rs",
            SymbolKind::Function,
            35,
            "#[test]\nfn test_auth_flow() {\n    auth_user();\n}",
        ),
        make_symbol(
            5,
            "UserConfig",
            "src/config.rs",
            SymbolKind::Struct,
            20,
            "pub struct UserConfig;\n",
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
    ];

    let mut sources = HashMap::new();
    sources.insert(
        PathBuf::from("src/api.rs"),
        "pub fn handle_request() {\n    auth_user();\n}".to_string(),
    );
    sources.insert(
        PathBuf::from("src/auth.rs"),
        "pub fn auth_user() {\n    find_user();\n}".to_string(),
    );
    sources.insert(
        PathBuf::from("src/db.rs"),
        "pub fn find_user() {\n    execute_query();\n}\npub fn execute_query() {\n    // query execution\n}".to_string(),
    );
    sources.insert(
        PathBuf::from("tests/auth_test.rs"),
        "#[test]\nfn test_auth_flow() {\n    auth_user();\n}".to_string(),
    );
    sources.insert(
        PathBuf::from("src/config.rs"),
        "pub struct UserConfig;\n".to_string(),
    );

    let graph = MultiplexGraph::build(syms, &edges, LayerWeights::default());
    (graph, sources)
}

#[test]
fn test_navigation_state_initialization() {
    let task = TaskContext::from_query("implement user authentication");
    let state = NavigationState::new(task.clone(), 1000);

    assert_eq!(state.initial_budget, 1000);
    assert_eq!(state.remaining_budget, 1000);
    assert_eq!(state.step_count(), 0);
    assert_eq!(state.total_cost_incurred(), 0);
    assert!(state.can_afford(500));
    assert!(!state.is_terminal);
    assert!(state.history.is_empty());
    assert!(state.observed_symbols.is_empty());
    assert!(state.observed_edges.is_empty());
    assert!(state.frontier.is_empty());

    let seeded_state = NavigationState::with_entrypoints(task, 800, &[SymbolId(0), SymbolId(1)]);
    assert_eq!(seeded_state.remaining_budget, 800);
    assert_eq!(seeded_state.frontier.len(), 2);
    assert!(seeded_state.frontier.contains(&SymbolId(0)));
    assert!(seeded_state.frontier.contains(&SymbolId(1)));
}

#[test]
fn test_navigation_action_inspection_step() {
    let (graph, sources) = build_navigation_harness();
    let intel = CodebaseIntelligence::new(&graph, &sources);
    let task = TaskContext::from_query("authenticate user request");
    let mut state = NavigationState::with_entrypoints(task, 1000, &[SymbolId(0)]);

    let step = state
        .apply_action(
            NavigationAction::Inspect {
                symbol_id: SymbolId(0),
            },
            &intel,
        )
        .expect("Inspect action must succeed");

    assert_eq!(step.step_index, 0);
    assert_eq!(step.action.action_type(), "INSPECT");
    assert!(step.cost.total_tokens > 0);
    assert_eq!(state.remaining_budget, 1000 - step.cost.total_tokens);
    assert_eq!(state.step_count(), 1);

    // Verify observation payload
    match &step.observation {
        Observation::Inspection(obs) => {
            assert_eq!(obs.symbol.id, SymbolId(0));
            assert_eq!(obs.symbol.name, "handle_request");
            assert!(!obs.code_snippet.is_empty());
            assert_eq!(obs.outgoing.len(), 1);
            assert_eq!(obs.outgoing[0].target.id, SymbolId(1));
        }
        _ => panic!("Expected Observation::Inspection"),
    }

    // Verify state mutation
    assert!(state.observed_symbols.contains_key(&SymbolId(0)));
    assert_eq!(state.observed_symbols[&SymbolId(0)], LodLevel::FullBody);
    assert!(!state.frontier.contains(&SymbolId(0)));
    assert!(state.frontier.contains(&SymbolId(1)));
    assert!(!state.observed_edges.is_empty());
}

#[test]
fn test_navigation_action_expand_step() {
    let (graph, sources) = build_navigation_harness();
    let intel = CodebaseIntelligence::new(&graph, &sources);
    let task = TaskContext::from_query("database query execution");
    let mut state = NavigationState::new(task, 800);

    let step = state
        .apply_action(
            NavigationAction::Expand {
                symbol_id: SymbolId(1),
                budget: 200,
            },
            &intel,
        )
        .expect("Expand action must succeed");

    match &step.observation {
        Observation::Expansion(obs) => {
            assert_eq!(obs.focal_symbol.id, SymbolId(1));
            assert!(!obs.selected_symbols.is_empty());
            assert!(obs.tokens_used <= 200);
            assert!(!obs.markdown_source.is_empty());
        }
        _ => panic!("Expected Observation::Expansion"),
    }

    assert!(state.observed_symbols.contains_key(&SymbolId(1)));
    assert!(state.remaining_budget < 800);
}

#[test]
fn test_navigation_action_trace_step() {
    let (graph, sources) = build_navigation_harness();
    let intel = CodebaseIntelligence::new(&graph, &sources);
    let task = TaskContext::from_query("trace handle_request to execute_query");
    let mut state = NavigationState::new(task, 600);

    let step = state
        .apply_action(
            NavigationAction::Trace {
                source_id: SymbolId(0),
                target_id: SymbolId(3),
            },
            &intel,
        )
        .expect("Trace action must succeed");

    match &step.observation {
        Observation::PathTrace(obs) => {
            assert_eq!(obs.source.id, SymbolId(0));
            assert_eq!(obs.target.id, SymbolId(3));
            assert!(obs.path_found);
            assert_eq!(obs.hops, 3);
            assert!(obs.mermaid_diagram.contains("sequenceDiagram"));
            assert!(obs.path.is_some());
        }
        _ => panic!("Expected Observation::PathTrace"),
    }

    assert_eq!(state.observed_paths.len(), 1);
    assert!(state.observed_symbols.contains_key(&SymbolId(0)));
    assert!(state.observed_symbols.contains_key(&SymbolId(3)));
}

#[test]
fn test_navigation_action_test_link_step() {
    let (graph, sources) = build_navigation_harness();
    let intel = CodebaseIntelligence::new(&graph, &sources);
    let task = TaskContext::from_query("verify auth flow with tests");
    let mut state = NavigationState::new(task, 500);

    let step = state
        .apply_action(
            NavigationAction::TestLink {
                symbol_id: SymbolId(1),
            },
            &intel,
        )
        .expect("TestLink action must succeed");

    match &step.observation {
        Observation::TestLinkage(obs) => {
            assert_eq!(obs.target_symbol.id, SymbolId(1));
            assert!(!obs.untested);
            assert_eq!(obs.tests.len(), 1);
            assert_eq!(obs.tests[0].name, "test_auth_flow");
        }
        _ => panic!("Expected Observation::TestLinkage"),
    }

    assert!(state.frontier.contains(&SymbolId(4)));
}

#[test]
fn test_budget_exhaustion_and_termination() {
    let (graph, sources) = build_navigation_harness();
    let intel = CodebaseIntelligence::new(&graph, &sources);
    let task = TaskContext::from_query("test budget bounds");

    // 1. Explicit Stop action terminates navigation
    let mut state = NavigationState::new(task.clone(), 500);
    state
        .apply_action(
            NavigationAction::Stop {
                reason: "User requested completion".to_string(),
            },
            &intel,
        )
        .expect("Stop must succeed");

    assert!(state.is_terminal);
    assert!(!state.can_afford(10));

    let follow_up = state.apply_action(
        NavigationAction::Inspect {
            symbol_id: SymbolId(0),
        },
        &intel,
    );
    assert!(matches!(follow_up, Err(EngineError::InvalidInput(_))));

    // 2. Insufficient budget rejects action
    let mut low_budget_state = NavigationState::new(task, 5);
    let unaffordable = low_budget_state.apply_action(
        NavigationAction::Inspect {
            symbol_id: SymbolId(0),
        },
        &intel,
    );
    assert!(matches!(
        unaffordable,
        Err(EngineError::BudgetExceeded(_, _))
    ));
}

#[test]
fn test_action_candidate_generator_feasibility() {
    let (graph, sources) = build_navigation_harness();
    let intel = CodebaseIntelligence::new(&graph, &sources);
    let task = TaskContext::from_query("handle authentication");
    let generator = ActionGenerator::new(ActionGeneratorConfig {
        max_candidates: 5,
        default_expand_budget: 150,
        enable_trace_candidates: true,
        enable_test_candidates: true,
    });

    // 1. Bootstrap: Proposes top entrypoints
    let mut state = NavigationState::new(task, 800);
    let candidates = generator
        .propose_actions(&state, &intel)
        .expect("Proposing actions must succeed");

    assert!(!candidates.is_empty());
    assert!(candidates.len() <= 5);
    assert!(candidates.iter().all(|c| c.estimated_cost <= 800));

    // Execute first proposal
    let first = candidates[0].action.clone();
    state
        .apply_action(first, &intel)
        .expect("Step execution must succeed");

    // 2. Frontier proposals: inspect, expand, testlink, stop
    let next_candidates = generator
        .propose_actions(&state, &intel)
        .expect("Proposing actions must succeed");

    assert!(!next_candidates.is_empty());
    assert!(next_candidates
        .iter()
        .any(|c| matches!(c.action, NavigationAction::Stop { .. })));
    assert!(next_candidates
        .iter()
        .all(|c| c.estimated_cost <= state.remaining_budget));
}

#[test]
fn test_synthesize_structured_context() {
    let (graph, sources) = build_navigation_harness();
    let intel = CodebaseIntelligence::new(&graph, &sources);
    let task = TaskContext::from_query("request pipeline");
    let mut state = NavigationState::with_entrypoints(task, 900, &[SymbolId(0)]);

    state
        .apply_action(
            NavigationAction::Inspect {
                symbol_id: SymbolId(0),
            },
            &intel,
        )
        .expect("Inspect step 1");

    state
        .apply_action(
            NavigationAction::Inspect {
                symbol_id: SymbolId(1),
            },
            &intel,
        )
        .expect("Inspect step 2");

    let structured = state
        .synthesize_context(&intel)
        .expect("Context synthesis must succeed");

    assert_eq!(structured.budget_limit, 900);
    assert!(structured.tokens_used > 0);
    assert_eq!(structured.symbols.len(), 2);
    assert!(!structured.edges.is_empty());
    assert!(structured.confidence_score > 0.0);
}

#[test]
fn test_navigation_state_json_roundtrip() {
    let (graph, sources) = build_navigation_harness();
    let intel = CodebaseIntelligence::new(&graph, &sources);
    let task = TaskContext::from_query("serialize navigation state");
    let mut state = NavigationState::with_entrypoints(task, 750, &[SymbolId(0)]);

    state
        .apply_action(
            NavigationAction::Inspect {
                symbol_id: SymbolId(0),
            },
            &intel,
        )
        .expect("Inspect step");

    let serialized = serde_json::to_string(&state).expect("JSON serialization must succeed");
    let deserialized: NavigationState =
        serde_json::from_str(&serialized).expect("JSON deserialization must succeed");

    assert_eq!(deserialized.initial_budget, state.initial_budget);
    assert_eq!(deserialized.remaining_budget, state.remaining_budget);
    assert_eq!(deserialized.step_count(), 1);
    assert_eq!(deserialized.observed_symbols.len(), 1);
    assert_eq!(deserialized.frontier.len(), state.frontier.len());
    assert_eq!(deserialized.is_terminal, state.is_terminal);
}

#[test]
fn test_adaptive_gain_estimator_marginal_utility() {
    let (graph, sources) = build_navigation_harness();
    let intel = CodebaseIntelligence::new(&graph, &sources);
    let task = TaskContext::from_query("authenticate user credentials");
    let state = NavigationState::with_entrypoints(task, 1000, &[SymbolId(0), SymbolId(1)]);

    let config = AdaptiveGainConfig::default();
    let estimator = AdaptiveGainEstimator::new(config);

    // 1. Inspecting an unobserved relevant symbol yields positive gain
    let inspect_auth = NavigationAction::Inspect {
        symbol_id: SymbolId(1), // authenticate
    };
    let gain_auth = estimator.estimate_marginal_gain(&inspect_auth, &state, &intel);
    assert!(
        gain_auth > 0.1,
        "Gain for relevant symbol must be substantial: {}",
        gain_auth
    );

    // 2. Candidate efficiency computation
    let candidate = CandidateAction {
        action: inspect_auth,
        estimated_cost: 60,
        priority: 0.9,
        rationale: "Authentication logic".to_string(),
    };
    let eff = estimator.evaluate_candidate_efficiency(&candidate, &state, &intel);
    assert_eq!(eff, gain_auth / 60.0);
    assert!(eff > 0.0);

    // 3. Tracing between symbols yields path gain
    let trace_action = NavigationAction::Trace {
        source_id: SymbolId(0),
        target_id: SymbolId(1),
    };
    let trace_gain = estimator.estimate_marginal_gain(&trace_action, &state, &intel);
    assert!(trace_gain > 0.0);

    // 4. Stop action utility when budget is healthy vs when exhausted
    let stop_action = NavigationAction::Stop {
        reason: "Done".to_string(),
    };
    let stop_gain_healthy = estimator.estimate_marginal_gain(&stop_action, &state, &intel);

    let exhausted_state = NavigationState::new(TaskContext::from_query("test"), 20);
    let stop_gain_exhausted =
        estimator.estimate_marginal_gain(&stop_action, &exhausted_state, &intel);
    assert!(stop_gain_exhausted > stop_gain_healthy);
}

#[test]
fn test_adaptive_gain_estimator_diminishing_returns() {
    let (graph, sources) = build_navigation_harness();
    let intel = CodebaseIntelligence::new(&graph, &sources);
    let task = TaskContext::from_query("handle request");
    let mut state = NavigationState::with_entrypoints(task, 1000, &[SymbolId(0)]);

    let estimator = AdaptiveGainEstimator::default();

    // Prior to observation, inspect has positive marginal gain
    let inspect_action = NavigationAction::Inspect {
        symbol_id: SymbolId(0),
    };
    let gain_before = estimator.estimate_marginal_gain(&inspect_action, &state, &intel);
    assert!(gain_before > 0.0);

    // Apply inspect
    state
        .apply_action(inspect_action.clone(), &intel)
        .expect("Inspect must succeed");

    // After observation at LodLevel::FullBody, marginal gain drops to 0.0 (diminishing returns)
    let gain_after = estimator.estimate_marginal_gain(&inspect_action, &state, &intel);
    assert_eq!(
        gain_after, 0.0,
        "Re-inspecting full body symbol must yield 0.0 marginal gain"
    );
}

#[test]
fn test_adaptive_navigator_step_execution() {
    let (graph, sources) = build_navigation_harness();
    let intel = CodebaseIntelligence::new(&graph, &sources);
    let task = TaskContext::from_query("handle incoming user request");
    let mut state = NavigationState::with_entrypoints(task, 1200, &[SymbolId(0)]);

    let navigator = AdaptiveNavigator::default();

    // Step 1: Should execute an action (e.g. Inspect)
    let step1 = navigator
        .step(&mut state, &intel)
        .expect("Step 1 must succeed")
        .expect("Must produce a step");

    assert_eq!(step1.step_index, 0);
    assert!(step1.cost.total_tokens > 0);
    assert_eq!(state.step_count(), 1);
    assert!(state.remaining_budget < 1200);

    // Step 2: Executes next action from frontier
    let step2 = navigator
        .step(&mut state, &intel)
        .expect("Step 2 must succeed")
        .expect("Must produce a step");

    assert_eq!(step2.step_index, 1);
    assert_eq!(state.step_count(), 2);
}

#[test]
fn test_adaptive_navigator_autonomous_run() {
    let (graph, sources) = build_navigation_harness();
    let intel = CodebaseIntelligence::new(&graph, &sources);
    let task = TaskContext::from_query("authenticate user credentials");

    let config = NavigatorConfig {
        max_steps: 6,
        min_efficiency_threshold: 0.0001,
        ..Default::default()
    };
    let navigator = AdaptiveNavigator::new(config);

    let trajectory: NavigationTrajectory = navigator
        .navigate(task, 1500, &intel)
        .expect("Autonomous navigation must succeed");

    assert!(trajectory.step_count() > 0);
    assert!(trajectory.step_count() <= 6);
    assert!(trajectory.tokens_used() > 0);
    assert!(trajectory.remaining_budget <= 1500);
    assert!(!trajectory.termination_reason.is_empty());
    assert!(!trajectory.steps.is_empty());

    // Verify structured context synthesis attached to trajectory
    let structured = &trajectory.structured_context;
    assert_eq!(structured.budget_limit, 1500);
    assert!(!structured.symbols.is_empty());
    assert!(structured.confidence_score > 0.0);
}
