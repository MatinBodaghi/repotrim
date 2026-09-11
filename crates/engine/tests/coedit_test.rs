use repotrim_engine::{
    CoeditCache, CoeditConfig, CoeditGraph, EdgeKind, EdgeWeightLearner, GitCommitMiner,
    LayerWeights, MultiplexGraph, PprConfig, PprSolver, ReferenceEdge, SymbolId, SymbolKind,
    SymbolNode, TextSpan,
};
use std::collections::HashMap;
use std::path::PathBuf;

fn make_test_symbol(
    id: u32,
    name: &str,
    file: &str,
    start_row: usize,
    end_row: usize,
) -> SymbolNode {
    SymbolNode {
        id: SymbolId(id),
        name: name.to_string(),
        kind: SymbolKind::Function,
        file_path: PathBuf::from(file),
        span: TextSpan::new(0, 100, start_row, end_row),
        signature: format!("pub fn {}()", name),
        docstring: None,
        token_cost: 10,
        ast_hash: [0u8; 32],
        container_name: None,
        trait_name: None,
    }
}

#[test]
fn test_git_commit_miner_diff_parsing_and_coedits() {
    let s0 = make_test_symbol(0, "handle_request", "src/handler.rs", 10, 30);
    let s1 = make_test_symbol(1, "process_order", "src/service.rs", 10, 40);
    let s2 = make_test_symbol(2, "log_event", "src/logger.rs", 5, 20);
    let symbols = vec![s0, s1, s2];

    // Mock git log text with 2 commits where s0 and s1 change together
    let mock_log = r#"COMMIT_DELIMITER c1000000 1700000000
diff --git a/src/handler.rs b/src/handler.rs
--- a/src/handler.rs
+++ b/src/handler.rs
@@ -15,1 +15,2 @@
+// modified inside handle_request
+let x = 1;
diff --git a/src/service.rs b/src/service.rs
--- a/src/service.rs
+++ b/src/service.rs
@@ -20,1 +20,2 @@
+// modified inside process_order
+let y = 2;
COMMIT_DELIMITER c2000000 1700086400
diff --git a/src/handler.rs b/src/handler.rs
--- a/src/handler.rs
+++ b/src/handler.rs
@@ -18,1 +18,2 @@
+// another change inside handle_request
+let x2 = 3;
diff --git a/src/service.rs b/src/service.rs
--- a/src/service.rs
+++ b/src/service.rs
@@ -25,1 +25,2 @@
+// another change inside process_order
+let y2 = 4;
"#;

    let config = CoeditConfig {
        min_support: 2,
        min_confidence: 0.1,
        min_jaccard: 0.05,
        ..Default::default()
    };

    let graph = GitCommitMiner::parse_git_log_output(mock_log, "c2000000", &symbols, &config);

    assert_eq!(graph.total_commits_analyzed, 2);
    assert_eq!(graph.valid_commits, 2);
    assert_eq!(graph.megacommits_filtered, 0);
    assert_eq!(graph.head_hash, "c2000000");

    // We expect s0 (handle_request) and s1 (process_order) to be co-edited in 2 commits
    let couplings_0 = graph.couplings_for(SymbolId(0));
    assert_eq!(couplings_0.len(), 1);
    assert_eq!(couplings_0[0].target, SymbolId(1));
    assert_eq!(couplings_0[0].raw_count, 2);
    assert!(couplings_0[0].confidence > 0.9);
    assert!(couplings_0[0].jaccard > 0.9);

    let directed = graph.to_directed_edges(0.15);
    assert_eq!(directed.len(), 2); // 0 -> 1 and 1 -> 0
}

#[test]
fn test_megacommit_noise_filtering() {
    let s0 = make_test_symbol(0, "alpha", "src/alpha.rs", 10, 20);
    let s1 = make_test_symbol(1, "beta", "src/beta.rs", 10, 20);
    let symbols = vec![s0, s1];

    let mock_log = r#"COMMIT_DELIMITER c_bulk 1700000000
diff --git a/file1.rs b/file1.rs
--- a/file1.rs
+++ b/file1.rs
@@ -10,1 +10,1 @@
+x
diff --git a/file2.rs b/file2.rs
--- a/file2.rs
+++ b/file2.rs
@@ -10,1 +10,1 @@
+y
diff --git a/file3.rs b/file3.rs
--- a/file3.rs
+++ b/file3.rs
@@ -10,1 +10,1 @@
+z
"#;

    // Set max_files_per_commit to 2 (so 3 files triggers megacommit filter)
    let config = CoeditConfig {
        max_files_per_commit: 2,
        ..Default::default()
    };

    let graph = GitCommitMiner::parse_git_log_output(mock_log, "c_bulk", &symbols, &config);
    assert_eq!(graph.total_commits_analyzed, 1);
    assert_eq!(graph.megacommits_filtered, 1);
    assert_eq!(graph.valid_commits, 0);
    assert!(graph.pairs.is_empty());
}

#[test]
fn test_temporal_decay_half_life() {
    let s0 = make_test_symbol(0, "fn_a", "src/lib.rs", 10, 20);
    let s1 = make_test_symbol(1, "fn_b", "src/lib.rs", 30, 40);
    let symbols = vec![s0, s1];

    // Commit 1 is ancient (180 days before commit 2)
    // 180 days = 2 half-lives of 90 days => weight ~ 2^(-2) = 0.25
    let day_secs = 86400;
    let t2 = 1700000000;
    let t1 = t2 - 180 * day_secs;

    let mock_log = format!(
        r#"COMMIT_DELIMITER c_old {t1}
diff --git a/src/lib.rs b/src/lib.rs
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -15,1 +15,1 @@
+a
@@ -35,1 +35,1 @@
+b
COMMIT_DELIMITER c_new {t2}
diff --git a/src/lib.rs b/src/lib.rs
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -15,1 +15,1 @@
+a2
@@ -35,1 +35,1 @@
+b2
"#
    );

    let config = CoeditConfig {
        half_life_days: 90.0,
        min_support: 2,
        min_confidence: 0.1,
        min_jaccard: 0.05,
        ..Default::default()
    };

    let graph = GitCommitMiner::parse_git_log_output(&mock_log, "c_new", &symbols, &config);
    assert_eq!(graph.valid_commits, 2);

    let support = graph.coedit_support(SymbolId(0), SymbolId(1));
    // commit 2 weight = 1.0; commit 1 weight = 0.25 => support = 1.25
    assert!((support - 1.25).abs() < 0.05);
}

#[test]
fn test_coedit_multiplex_graph_ppr_diffusion() {
    let s0 = make_test_symbol(0, "route_checkout", "src/routes.rs", 1, 10);
    let s1 = make_test_symbol(1, "payment_webhook", "src/webhooks.rs", 1, 10);

    // No AST, Call, or TypeRef edges between s0 and s1
    let raw_edges = vec![];
    let imports = vec![];

    let default_weights = LayerWeights::default();

    // 1. Without co-edits: graph has 0 edges between 0 and 1
    let base_graph = MultiplexGraph::build_with_imports(
        vec![s0.clone(), s1.clone()],
        &raw_edges,
        &imports,
        default_weights,
    );

    let ppr = PprSolver::new(PprConfig::default());
    let base_scores = ppr.compute(&base_graph, &[(SymbolId(0), 1.0)]);
    assert_eq!(base_scores.get(&SymbolId(1)).copied().unwrap_or(0.0), 0.0);

    // 2. With co-edits: inject co-edit edge (0 -> 1, confidence 0.85)
    let coedit_edges = vec![(SymbolId(0), SymbolId(1), 0.85)];
    let coedit_graph = MultiplexGraph::build_with_all(
        vec![s0, s1],
        &raw_edges,
        &imports,
        &coedit_edges,
        default_weights,
    );

    let coedit_scores = ppr.compute(&coedit_graph, &[(SymbolId(0), 1.0)]);
    let score_1 = coedit_scores.get(&SymbolId(1)).copied().unwrap_or(0.0);
    assert!(score_1 > 0.0, "Expected PPR to diffuse across co-edit edge");
}

#[test]
fn test_layer_weight_learning_empirical() {
    let s0 = make_test_symbol(0, "caller", "src/main.rs", 1, 10);
    let s1 = make_test_symbol(1, "callee_call", "src/service.rs", 1, 10);
    let s2 = make_test_symbol(2, "type_dependency", "src/models.rs", 1, 10);

    let symbols = vec![s0, s1, s2];

    let raw_edges = vec![
        ReferenceEdge {
            source: SymbolId(0),
            target_ident: "callee_call".to_string(),
            kind: EdgeKind::Call,
        },
        ReferenceEdge {
            source: SymbolId(0),
            target_ident: "type_dependency".to_string(),
            kind: EdgeKind::TypeRef,
        },
    ];

    let mut coedit_graph = CoeditGraph {
        valid_commits: 100,
        total_commits_analyzed: 110,
        megacommits_filtered: 10,
        head_hash: "abc1234".to_string(),
        ..Default::default()
    };

    // Caller and callee_call have 5 co-edits (high co-change)
    coedit_graph.pairs.push(repotrim_engine::CoeditPair {
        source: SymbolId(0),
        target: SymbolId(1),
        raw_count: 5,
        support: 4.8,
        confidence: 0.9,
        jaccard: 0.8,
    });
    coedit_graph.pairs.push(repotrim_engine::CoeditPair {
        source: SymbolId(1),
        target: SymbolId(0),
        raw_count: 5,
        support: 4.8,
        confidence: 0.9,
        jaccard: 0.8,
    });

    let (learned_weights, report) = EdgeWeightLearner::learn_weights(
        &symbols,
        &raw_edges,
        &[],
        &coedit_graph,
        LayerWeights::default(),
    );

    // Call layer had co-edits, Type layer had none
    assert_eq!(report.valid_commits, 100);
    assert!(learned_weights.call >= learned_weights.type_ref);
    assert!(learned_weights.call <= 1.0);
    assert!(learned_weights.ast_parent >= 0.15);

    let md = report.to_markdown();
    assert!(md.contains("Git Co-Edit Mining & Principled Weight Learning Report"));
    assert!(md.contains("Call"));
}

#[test]
fn test_coedit_cache_persistence_and_invalidation() {
    let temp_dir =
        std::env::temp_dir().join(format!("repotrim_coedit_test_{}", std::process::id()));
    let cache_file = temp_dir.join("coedit.bin");

    let graph = CoeditGraph {
        head_hash: "commit_abc123".to_string(),
        total_commits_analyzed: 42,
        valid_commits: 40,
        megacommits_filtered: 2,
        pairs: vec![repotrim_engine::CoeditPair {
            source: SymbolId(0),
            target: SymbolId(1),
            raw_count: 3,
            support: 2.7,
            confidence: 0.8,
            jaccard: 0.65,
        }],
        symbol_edit_counts: HashMap::new(),
        symbol_raw_counts: HashMap::new(),
    };

    // Save to disk
    CoeditCache::save_to_file(&cache_file, &graph).expect("Save cache failed");

    // Load with matching HEAD hash
    let loaded = CoeditCache::load_from_file(&cache_file, "commit_abc123");
    assert!(loaded.is_some());
    let loaded_graph = loaded.unwrap();
    assert_eq!(loaded_graph.valid_commits, 40);
    assert_eq!(loaded_graph.pairs.len(), 1);

    // Load with mismatching HEAD hash -> should invalidate and return None
    let invalidated = CoeditCache::load_from_file(&cache_file, "commit_new_xyz");
    assert!(invalidated.is_none());

    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_mine_current_repository_dogfood() {
    let repo_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();

    if !repo_root.join(".git").exists() {
        return;
    }

    let loaded = repotrim_engine::LoadedRepository::load(repo_root)
        .expect("Failed to load repo for dogfood test");

    let config = CoeditConfig {
        max_commits: 50,
        min_support: 2,
        min_confidence: 0.1,
        min_jaccard: 0.05,
        ..Default::default()
    };

    let result = GitCommitMiner::mine_repository(repo_root, &loaded.symbols, &config);
    assert!(
        result.is_ok(),
        "Mining repository failed: {:?}",
        result.err()
    );

    let coedit_graph = result.unwrap();
    assert!(!coedit_graph.head_hash.is_empty());
    assert!(coedit_graph.total_commits_analyzed > 0);

    let (learned_weights, report) = EdgeWeightLearner::learn_weights(
        &loaded.symbols,
        &loaded.edges,
        &loaded.imports,
        &coedit_graph,
        LayerWeights::default(),
    );

    assert!(learned_weights.call > 0.0);
    assert!(learned_weights.ast_parent > 0.0);
    assert!(!report.layer_stats.is_empty());
}
