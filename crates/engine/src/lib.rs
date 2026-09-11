pub mod architecture;
pub mod cache;
pub mod celf;
pub mod coedit;
pub mod community;
pub mod csr;
pub mod diff;
pub mod error;
pub mod eval;
pub mod formatter;
pub mod graph;
pub mod impact;
pub mod import;
pub mod intent;
pub mod knee;
pub mod loader;
pub mod model;
pub mod parser;
pub mod ppr;
pub mod resolver;
pub mod retrieval;
pub mod selector;
pub mod slicer;
pub mod symbol;
pub mod tokens;
pub mod watcher;
pub mod weight_learning;

pub use architecture::{
    ArchitecturalHub, ArchitecturalLayer, ArchitectureReport, PublicApiSymbol, SubsystemCommunity,
};
pub use cache::{
    compute_blake3_hash, get_mtime_nanos, FileCacheEntry, RepositoryCache, CACHE_VERSION,
};
pub use celf::{
    BorderlinePair, CelfConfig, CelfOptimizer, CelfTraceStep, LodOption, LodWeights, MckpResult,
    MckpTraceStep, SensitivityReport,
};
pub use coedit::{CoeditCache, CoeditConfig, CoeditGraph, CoeditPair, GitCommitMiner};
pub use community::{
    ArchitecturalDrift, Community, CommunityConfig, CommunityDetector, CommunityHierarchy,
    CommunityResult,
};
pub use csr::CsrMatrix;
pub use diff::DiffResolver;
pub use error::EngineError;
pub use eval::{
    BenchmarkMetrics, BenchmarkRunner, BenchmarkScenario, BenchmarkSummary, ContextStrategy,
};
pub use formatter::ContextFormatter;
pub use graph::{LayerWeights, MultiplexGraph};
pub use impact::{ImpactAnalyzer, ImpactReport, ImpactSummary, RiskLevel};
pub use import::{normalize_path, resolve_module_path, FileImport};
pub use intent::IntentResolver;
pub use knee::{KneePoint, KneedleDetector};
pub use loader::{CacheReport, LoadedRepository};
pub use model::ModelProfile;
pub use parser::{AstExtractor, SupportedLanguage};
pub use ppr::{PprConfig, PprResult, PprSolver};
pub use resolver::ScopedResolver;
pub use retrieval::{
    Bm25Scorer, DenseEmbedder, HybridRetriever, PolyglotTokenizer, RetrievalConfig,
    RocchioExpander, SearchMode, SearchResult,
};
pub use selector::{AutoBudgetReport, ContextSelector};
pub use slicer::AstSlicer;
pub use symbol::{EdgeKind, LodLevel, ReferenceEdge, SymbolId, SymbolKind, SymbolNode, TextSpan};
pub use tokens::{count_tokens, estimate_tokens, estimate_tokens_calibrated, TokenizerModel};
pub use watcher::{is_ignored_path, RepositoryWatcher, WatcherEvent};
pub use weight_learning::{EdgeWeightLearner, LayerLearningStat, WeightLearningReport};

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_parse_functions_and_structs() {
        let extractor = AstExtractor::new().expect("Failed to initialize AstExtractor");

        let source = r#"
/// Documentation for Point.
pub struct Point {
    pub x: f64,
    pub y: f64,
}

/// Computes distance between two coordinates.
pub fn distance(p1: &Point, p2: &Point) -> f64 {
    let dx = p1.x - p2.x;
    let dy = p1.y - p2.y;
    sqrt(dx * dx + dy * dy)
}
"#;

        let mut next_id = 0;
        let (symbols, edges) = extractor
            .parse_file(
                Path::new("src/geometry.rs"),
                source.as_bytes(),
                &mut next_id,
            )
            .expect("Failed to parse file");

        // Verify dense numeric IDs
        assert_eq!(next_id, 2);
        assert_eq!(symbols.len(), 2);

        // Verify struct extraction
        let struct_sym = symbols
            .iter()
            .find(|s| s.name == "Point")
            .expect("Point struct not found");
        assert_eq!(struct_sym.id, SymbolId(0));
        assert_eq!(struct_sym.kind, SymbolKind::Struct);
        assert_eq!(struct_sym.file_path, Path::new("src/geometry.rs"));
        assert!(struct_sym.signature.contains("pub struct Point"));
        assert_eq!(
            struct_sym.docstring.as_deref(),
            Some("Documentation for Point.")
        );
        assert_eq!(
            struct_sym.ast_hash,
            *blake3::hash(struct_sym.signature.as_bytes()).as_bytes()
        );
        assert!(struct_sym.token_cost > 0);

        // Verify function extraction
        let fn_sym = symbols
            .iter()
            .find(|s| s.name == "distance")
            .expect("distance function not found");
        assert_eq!(fn_sym.id, SymbolId(1));
        assert_eq!(fn_sym.kind, SymbolKind::Function);
        assert_eq!(
            fn_sym.docstring.as_deref(),
            Some("Computes distance between two coordinates.")
        );
        assert_eq!(
            fn_sym.ast_hash,
            *blake3::hash(fn_sym.signature.as_bytes()).as_bytes()
        );
        assert!(fn_sym.token_cost > 0);

        // Verify execution body is stripped from function signature
        assert_eq!(
            fn_sym.signature,
            "pub fn distance(p1: &Point, p2: &Point) -> f64"
        );
        assert!(!fn_sym.signature.contains('{'));
        assert!(!fn_sym.signature.contains("dx"));
        assert!(!fn_sym.signature.contains("sqrt"));

        // Verify call reference edge
        let call_edge = edges
            .iter()
            .find(|e| e.kind == EdgeKind::Call)
            .expect("Call edge not found");
        assert_eq!(call_edge.source, fn_sym.id);
        assert_eq!(call_edge.target_ident, "sqrt");

        // Verify type reference edge (p1, p2: &Point)
        let type_edge = edges
            .iter()
            .find(|e| e.kind == EdgeKind::TypeRef)
            .expect("TypeRef edge not found");
        assert_eq!(type_edge.source, fn_sym.id);
        assert_eq!(type_edge.target_ident, "Point");
    }

    #[test]
    fn test_signature_body_isolation() {
        let extractor = AstExtractor::new().expect("Failed to initialize AstExtractor");

        let source = r#"
pub fn execute_algorithm<T: Ord + Clone>(items: &mut [T]) -> Option<T> {
    if items.is_empty() {
        return None;
    }
    items.sort();
    Some(items[0].clone())
}
"#;

        let mut next_id = 100;
        let (symbols, _edges) = extractor
            .parse_file(Path::new("src/algo.rs"), source.as_bytes(), &mut next_id)
            .expect("Failed to parse file");

        assert_eq!(symbols.len(), 1);
        let sym = &symbols[0];
        assert_eq!(sym.name, "execute_algorithm");
        assert_eq!(
            sym.signature,
            "pub fn execute_algorithm<T: Ord + Clone>(items: &mut [T]) -> Option<T>"
        );
        // Ensure function bodies ({ ... }) are stripped
        assert!(!sym.signature.contains('{'));
        assert!(!sym.signature.contains('}'));
        assert!(!sym.signature.contains("is_empty"));
        assert!(!sym.signature.contains("sort"));
        assert!(!sym.signature.contains("return"));
    }

    #[test]
    fn test_call_reference_edges_multiple_targets() {
        let extractor = AstExtractor::new().expect("Failed to initialize AstExtractor");

        let source = r#"
struct Service;

impl Service {
    pub fn handle_request(&self) {
        authenticate();
        self.validate();
        logger::log_info();
        transform::<u32>();
    }
}
"#;

        let mut next_id = 0;
        let (symbols, edges) = extractor
            .parse_file(Path::new("src/service.rs"), source.as_bytes(), &mut next_id)
            .expect("Failed to parse file");

        // Symbols: struct Service and method handle_request
        assert_eq!(symbols.len(), 2);
        let method_sym = symbols
            .iter()
            .find(|s| s.name == "handle_request")
            .expect("handle_request not found");
        assert_eq!(method_sym.kind, SymbolKind::Method);
        assert_eq!(method_sym.signature, "pub fn handle_request(&self)");

        // Reference edges
        let call_targets: Vec<&str> = edges
            .iter()
            .filter(|e| e.source == method_sym.id && e.kind == EdgeKind::Call)
            .map(|e| e.target_ident.as_str())
            .collect();

        assert_eq!(
            call_targets,
            vec!["authenticate", "validate", "log_info", "transform"]
        );
    }

    #[test]
    fn test_traits_and_docstrings() {
        let extractor = AstExtractor::new().expect("Failed to initialize AstExtractor");

        let source = r#"
/// Worker trait interface.
pub trait Worker {
    /// Perform task.
    fn work(&self);
}
"#;

        let mut next_id = 0;
        let (symbols, _edges) = extractor
            .parse_file(Path::new("src/worker.rs"), source.as_bytes(), &mut next_id)
            .expect("Failed to parse file");

        let trait_sym = symbols
            .iter()
            .find(|s| s.name == "Worker")
            .expect("Worker trait not found");
        assert_eq!(trait_sym.kind, SymbolKind::Trait);
        assert_eq!(
            trait_sym.docstring.as_deref(),
            Some("Worker trait interface.")
        );

        let fn_sym = symbols
            .iter()
            .find(|s| s.name == "work")
            .expect("work fn not found");
        assert_eq!(fn_sym.signature, "fn work(&self)");
        assert_eq!(fn_sym.docstring.as_deref(), Some("Perform task."));
    }

    #[test]
    fn test_blake3_hash_reproducibility() {
        let extractor = AstExtractor::new().expect("Failed to initialize AstExtractor");

        let source = "pub fn foo() {}";
        let mut id1 = 0;
        let (symbols1, _) = extractor
            .parse_file(Path::new("a.rs"), source.as_bytes(), &mut id1)
            .unwrap();

        let mut id2 = 42;
        let (symbols2, _) = extractor
            .parse_file(Path::new("b.rs"), source.as_bytes(), &mut id2)
            .unwrap();

        assert_eq!(symbols1[0].ast_hash, symbols2[0].ast_hash);
        assert_eq!(
            symbols1[0].ast_hash,
            *blake3::hash(b"pub fn foo()").as_bytes()
        );
    }

    #[test]
    fn test_invalid_utf8_error() {
        let extractor = AstExtractor::new().expect("Failed to initialize AstExtractor");
        let invalid_utf8 = [0xff, 0xfe, 0xfd];
        let mut next_id = 0;
        let result = extractor.parse_file(Path::new("invalid.rs"), &invalid_utf8, &mut next_id);
        assert!(matches!(result, Err(EngineError::Utf8Error(_))));
    }

    #[test]
    fn test_symbol_id_copy_and_traits() {
        let id1 = SymbolId(42);
        let id2 = id1; // Verifies Copy trait
        assert_eq!(id1, id2);
        assert_eq!(format!("{}", id1), "42");
    }

    #[test]
    fn test_end_to_end_ast_to_multiplex_graph() {
        let extractor = AstExtractor::new().expect("Failed to initialize AstExtractor");

        let mod_a = r#"
pub struct Config {
    pub timeout: u64,
}

pub fn initialize() -> Config {
    setup_logging();
    Config { timeout: 30 }
}
"#;

        let mod_b = r#"
pub fn setup_logging() {
    // initialize logger
}

pub fn run_server() {
    initialize();
}
"#;

        let mut next_id = 0;
        let (mut symbols_a, mut edges_a) = extractor
            .parse_file(Path::new("src/config.rs"), mod_a.as_bytes(), &mut next_id)
            .expect("Failed to parse config.rs");

        let (symbols_b, edges_b) = extractor
            .parse_file(Path::new("src/server.rs"), mod_b.as_bytes(), &mut next_id)
            .expect("Failed to parse server.rs");

        symbols_a.extend(symbols_b);
        edges_a.extend(edges_b);

        let graph = MultiplexGraph::build(symbols_a, &edges_a, LayerWeights::default());

        assert_eq!(graph.num_symbols(), 4);
        // Edges: initialize -> setup_logging (Call), initialize -> Config (TypeRef), run_server -> initialize (Call)
        assert_eq!(graph.num_edges(), 3);

        // Find symbols
        let run_sym = graph
            .symbols()
            .iter()
            .find(|s| s.name == "run_server")
            .expect("run_server not found");

        let init_sym = graph
            .symbols()
            .iter()
            .find(|s| s.name == "initialize")
            .expect("initialize not found");

        let logging_sym = graph
            .symbols()
            .iter()
            .find(|s| s.name == "setup_logging")
            .expect("setup_logging not found");

        let config_sym = graph
            .symbols()
            .iter()
            .find(|s| s.name == "Config")
            .expect("Config not found");

        // run_server should have 1 out-neighbor pointing to initialize
        let run_neighbors = graph.neighbors(run_sym.id);
        assert_eq!(run_neighbors, &[init_sym.id.0]);

        // initialize should have 2 out-neighbors pointing to Config and setup_logging
        let init_neighbors = graph.neighbors(init_sym.id);
        assert_eq!(init_neighbors.len(), 2);
        assert!(init_neighbors.contains(&config_sym.id.0));
        assert!(init_neighbors.contains(&logging_sym.id.0));

        // Transition probability from run_server to initialize is 1.0
        let probs = graph.transition_probabilities(run_sym.id);
        assert_eq!(probs.len(), 1);
        assert!((probs[0] - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_struct_methods_ast_parent_and_field_type_edges() {
        let extractor = AstExtractor::new().expect("Failed to initialize AstExtractor");

        let source = r#"
pub struct Matrix {
    pub rows: usize,
}

pub struct Engine {
    pub matrix: Matrix,
}

impl Engine {
    pub fn start(&self) {
        boot();
    }
}

pub fn boot() {}
"#;

        let mut next_id = 0;
        let (symbols, edges) = extractor
            .parse_file(Path::new("src/engine.rs"), source.as_bytes(), &mut next_id)
            .expect("Failed to parse engine.rs");

        // Verify AstParent edge from start() -> Engine
        let start_sym = symbols
            .iter()
            .find(|s| s.name == "start")
            .expect("start fn not found");
        let parent_edge = edges
            .iter()
            .find(|e| e.source == start_sym.id && e.kind == EdgeKind::AstParent)
            .expect("AstParent edge not found for start()");
        assert_eq!(parent_edge.target_ident, "Engine");

        // Verify TypeRef edge from Engine -> Matrix
        let engine_sym = symbols
            .iter()
            .find(|s| s.name == "Engine")
            .expect("Engine struct not found");
        let type_edge = edges
            .iter()
            .find(|e| e.source == engine_sym.id && e.kind == EdgeKind::TypeRef)
            .expect("TypeRef edge not found for Engine");
        assert_eq!(type_edge.target_ident, "Matrix");

        let start_id = start_sym.id;
        let engine_id = engine_sym.id;

        // Build MultiplexGraph and verify reverse containment: Engine -> start()
        let graph = MultiplexGraph::build(symbols, &edges, LayerWeights::default());
        let engine_neighbors = graph.neighbors(engine_id);

        let matrix_sym = graph
            .symbols()
            .iter()
            .find(|s| s.name == "Matrix")
            .expect("Matrix not found");

        // Engine must connect to both its member method start() and its field type Matrix
        assert!(engine_neighbors.contains(&start_id.0));
        assert!(engine_neighbors.contains(&matrix_sym.id.0));
    }
}
