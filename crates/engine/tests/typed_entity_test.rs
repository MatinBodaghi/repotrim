use std::path::PathBuf;
use std::str::FromStr;

use repotrim_engine::graph::LayerWeights;
use repotrim_engine::parser::SupportedLanguage;
use repotrim_engine::symbol::{EdgeKind, NodeType, RelationType, SymbolKind};
use repotrim_engine::task::{TaskContext, TaskKind, TaskMetadata};

#[test]
fn test_node_type_properties_and_invariants() {
    let all_types = [
        NodeType::Package,
        NodeType::Module,
        NodeType::File,
        NodeType::Class,
        NodeType::Struct,
        NodeType::Interface,
        NodeType::Function,
        NodeType::Method,
        NodeType::Type,
        NodeType::Test,
        NodeType::Config,
        NodeType::Endpoint,
    ];

    for node_type in all_types {
        // Display and FromStr roundtrip
        let s = node_type.to_string();
        let parsed = NodeType::from_str(&s).expect("Failed to parse NodeType string");
        assert_eq!(parsed, node_type);

        // JSON serialization roundtrip
        let json = serde_json::to_string(&node_type).expect("Serialization failed");
        let deserialized: NodeType = serde_json::from_str(&json).expect("Deserialization failed");
        assert_eq!(deserialized, node_type);
    }

    // Semantic queries
    assert!(NodeType::Function.is_callable());
    assert!(NodeType::Method.is_callable());
    assert!(NodeType::Endpoint.is_callable());
    assert!(NodeType::Test.is_callable());
    assert!(!NodeType::Struct.is_callable());

    assert!(NodeType::Struct.is_type_definition());
    assert!(NodeType::Class.is_type_definition());
    assert!(NodeType::Interface.is_type_definition());
    assert!(NodeType::Type.is_type_definition());
    assert!(!NodeType::Function.is_type_definition());

    assert!(NodeType::Module.is_container());
    assert!(NodeType::Class.is_container());
    assert!(NodeType::Struct.is_container());
    assert!(!NodeType::Function.is_container());

    assert!(NodeType::Test.is_test());
    assert!(!NodeType::Method.is_test());

    assert!(NodeType::Config.is_config());
    assert!(!NodeType::File.is_config());
}

#[test]
fn test_node_type_inference() {
    let test_path = PathBuf::from("crates/engine/tests/eval_test.rs");
    let normal_path = PathBuf::from("crates/engine/src/eval.rs");

    // Functions in test paths become NodeType::Test
    assert_eq!(
        NodeType::infer("test_eval_runner", &test_path, SymbolKind::Function),
        NodeType::Test
    );
    // Functions with test prefixes become NodeType::Test even in src
    assert_eq!(
        NodeType::infer("test_something", &normal_path, SymbolKind::Function),
        NodeType::Test
    );
    assert_eq!(
        NodeType::infer("prop_submodularity", &normal_path, SymbolKind::Function),
        NodeType::Test
    );

    // Standard functions and types
    assert_eq!(
        NodeType::infer("calculate_score", &normal_path, SymbolKind::Function),
        NodeType::Function
    );
    assert_eq!(
        NodeType::infer("ContextSelector", &normal_path, SymbolKind::Struct),
        NodeType::Struct
    );
    assert_eq!(
        NodeType::infer("RelevanceModel", &normal_path, SymbolKind::Trait),
        NodeType::Interface
    );
}

#[test]
fn test_relation_type_properties_and_invariants() {
    let all_relations = [
        RelationType::Imports,
        RelationType::Calls,
        RelationType::References,
        RelationType::Inherits,
        RelationType::Implements,
        RelationType::Contains,
        RelationType::BelongsTo,
        RelationType::Returns,
        RelationType::Accepts,
        RelationType::Reads,
        RelationType::Writes,
        RelationType::Configures,
        RelationType::IsTestedBy,
        RelationType::CoChangesWith,
    ];

    for rel in all_relations {
        // Display and FromStr roundtrip
        let s = rel.to_string();
        let parsed = RelationType::from_str(&s).expect("Failed to parse RelationType string");
        assert_eq!(parsed, rel);

        // JSON serialization roundtrip
        let json = serde_json::to_string(&rel).expect("Serialization failed");
        let deserialized: RelationType =
            serde_json::from_str(&json).expect("Deserialization failed");
        assert_eq!(deserialized, rel);
    }

    // Semantic categorization
    assert!(RelationType::Contains.is_structural());
    assert!(RelationType::Inherits.is_structural());
    assert!(RelationType::Implements.is_structural());
    assert!(!RelationType::Calls.is_structural());

    assert!(RelationType::Calls.is_behavioral());
    assert!(RelationType::Reads.is_behavioral());
    assert!(RelationType::Writes.is_behavioral());
    assert!(!RelationType::Contains.is_behavioral());

    assert!(RelationType::Returns.is_type_dependency());
    assert!(RelationType::Accepts.is_type_dependency());
    assert!(RelationType::References.is_type_dependency());

    assert!(RelationType::IsTestedBy.is_evidence());
    assert!(RelationType::CoChangesWith.is_evidence());

    assert!(RelationType::Imports.is_import());

    // Conversion from EdgeKind
    assert_eq!(RelationType::from(EdgeKind::Call), RelationType::Calls);
    assert_eq!(
        RelationType::from(EdgeKind::AstParent),
        RelationType::Contains
    );
    assert_eq!(
        RelationType::from(EdgeKind::TypeRef),
        RelationType::References
    );
    assert_eq!(RelationType::from(EdgeKind::Import), RelationType::Imports);
    assert_eq!(
        RelationType::from(EdgeKind::CoEdit),
        RelationType::CoChangesWith
    );
}

#[test]
fn test_task_kind_inference_and_properties() {
    assert_eq!(
        TaskKind::infer("fix null pointer exception in parser"),
        TaskKind::Bugfix
    );
    assert_eq!(
        TaskKind::infer("resolve panic on empty token stream"),
        TaskKind::Bugfix
    );
    assert_eq!(
        TaskKind::infer("add unit tests for CELF knapsack optimizer"),
        TaskKind::TestCreation
    );
    assert_eq!(
        TaskKind::infer("refactor multiplex graph representation"),
        TaskKind::Refactor
    );
    assert_eq!(
        TaskKind::infer("implement Go language symbol extraction"),
        TaskKind::Feature
    );
    assert_eq!(
        TaskKind::infer("document architectural layers in README"),
        TaskKind::Documentation
    );
    assert_eq!(
        TaskKind::infer("where is SymbolNode defined"),
        TaskKind::Exploration
    );
    assert_eq!(
        TaskKind::infer("optimize matrix multiply"),
        TaskKind::General
    );
}

#[test]
fn test_task_context_construction_and_layer_weights() {
    let query = "Fix authentication token expiration in oauth middleware";
    let ctx = TaskContext::from_query(query);

    assert_eq!(ctx.kind(), TaskKind::Bugfix);
    assert!(ctx.concepts.contains(&"fix".to_string()));
    assert!(ctx.concepts.contains(&"authentication".to_string()));
    assert!(ctx.concepts.contains(&"token".to_string()));

    let base_weights = LayerWeights::default();
    let conditioned = ctx.conditioned_layer_weights(&base_weights);

    // Bugfix boosts calls and co-edits
    assert!(conditioned.call > base_weights.call);
    assert!(conditioned.co_edit > base_weights.co_edit);
    assert!(conditioned.ast_parent < base_weights.ast_parent);

    // All weights must be strictly positive
    assert!(conditioned.ast_parent > 0.0);
    assert!(conditioned.call > 0.0);
    assert!(conditioned.type_ref > 0.0);
    assert!(conditioned.import > 0.0);
    assert!(conditioned.co_edit > 0.0);
}

#[test]
fn test_task_context_serialization_roundtrip() {
    let query = "Refactor ScopedResolver to use canonical module paths";
    let seeds = vec!["ScopedResolver".to_string(), "FileImport".to_string()];
    let mut metadata = TaskMetadata {
        kind: Some(TaskKind::Refactor),
        target_files: vec![PathBuf::from("crates/engine/src/resolver.rs")],
        target_language: Some(SupportedLanguage::Rust),
        max_budget: Some(2500),
        ..Default::default()
    };
    metadata
        .attributes
        .insert("author".to_string(), "RepoTrim".to_string());

    let ctx = TaskContext::with_seeds(query, seeds).with_metadata(metadata);

    let json = serde_json::to_string_pretty(&ctx).expect("Serialization failed");
    let deserialized: TaskContext = serde_json::from_str(&json).expect("Deserialization failed");

    assert_eq!(deserialized.query, ctx.query);
    assert_eq!(deserialized.concepts, ctx.concepts);
    assert_eq!(deserialized.seed_hints, ctx.seed_hints);
    assert_eq!(deserialized.metadata.kind, Some(TaskKind::Refactor));
    assert_eq!(
        deserialized.metadata.target_language,
        Some(SupportedLanguage::Rust)
    );
    assert_eq!(deserialized.metadata.max_budget, Some(2500));
    assert_eq!(
        deserialized
            .metadata
            .attributes
            .get("author")
            .map(|s| s.as_str()),
        Some("RepoTrim")
    );
}
