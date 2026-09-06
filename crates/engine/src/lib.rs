pub mod parser;
pub mod symbol;
pub mod tokens;

pub use parser::{AstExtractor, EngineError};
pub use symbol::{EdgeKind, ReferenceEdge, SymbolId, SymbolKind, SymbolNode, TextSpan};
pub use tokens::estimate_tokens;

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
            .parse_file(Path::new("src/geometry.rs"), source.as_bytes(), &mut next_id)
            .expect("Failed to parse file");

        // Verify dense numeric IDs
        assert_eq!(next_id, 2);
        assert_eq!(symbols.len(), 2);

        // Verify struct extraction
        let struct_sym = symbols.iter().find(|s| s.name == "Point").expect("Point struct not found");
        assert_eq!(struct_sym.id, SymbolId(0));
        assert_eq!(struct_sym.kind, SymbolKind::Struct);
        assert_eq!(struct_sym.file_path, Path::new("src/geometry.rs"));
        assert!(struct_sym.signature.contains("pub struct Point"));
        assert_eq!(struct_sym.docstring.as_deref(), Some("Documentation for Point."));
        assert_eq!(struct_sym.ast_hash, *blake3::hash(struct_sym.signature.as_bytes()).as_bytes());
        assert!(struct_sym.token_cost > 0);

        // Verify function extraction
        let fn_sym = symbols.iter().find(|s| s.name == "distance").expect("distance function not found");
        assert_eq!(fn_sym.id, SymbolId(1));
        assert_eq!(fn_sym.kind, SymbolKind::Function);
        assert_eq!(fn_sym.docstring.as_deref(), Some("Computes distance between two coordinates."));
        assert_eq!(fn_sym.ast_hash, *blake3::hash(fn_sym.signature.as_bytes()).as_bytes());
        assert!(fn_sym.token_cost > 0);

        // Verify execution body is stripped from function signature
        assert_eq!(fn_sym.signature, "pub fn distance(p1: &Point, p2: &Point) -> f64");
        assert!(!fn_sym.signature.contains('{'));
        assert!(!fn_sym.signature.contains("dx"));
        assert!(!fn_sym.signature.contains("sqrt"));

        // Verify call reference edge
        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0].source, fn_sym.id);
        assert_eq!(edges[0].target_ident, "sqrt");
        assert_eq!(edges[0].kind, EdgeKind::Call);
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
        let method_sym = symbols.iter().find(|s| s.name == "handle_request").expect("handle_request not found");
        assert_eq!(method_sym.kind, SymbolKind::Method);
        assert_eq!(method_sym.signature, "pub fn handle_request(&self)");

        // Reference edges
        let call_targets: Vec<&str> = edges
            .iter()
            .filter(|e| e.source == method_sym.id && e.kind == EdgeKind::Call)
            .map(|e| e.target_ident.as_str())
            .collect();

        assert_eq!(call_targets, vec!["authenticate", "validate", "log_info", "transform"]);
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

        let trait_sym = symbols.iter().find(|s| s.name == "Worker").expect("Worker trait not found");
        assert_eq!(trait_sym.kind, SymbolKind::Trait);
        assert_eq!(trait_sym.docstring.as_deref(), Some("Worker trait interface."));

        let fn_sym = symbols.iter().find(|s| s.name == "work").expect("work fn not found");
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
        assert_eq!(symbols1[0].ast_hash, *blake3::hash(b"pub fn foo()").as_bytes());
    }
}
