use serde::{Deserialize, Serialize};

use crate::csr::CsrMatrix;
use crate::import::FileImport;
use crate::resolver::ScopedResolver;
use crate::symbol::{EdgeKind, ReferenceEdge, SymbolId, SymbolNode};

/// Tensor weights assigned to orthogonal edge relationship layers in the Multiplex CPG.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct LayerWeights {
    /// Lexical containment weight (module -> struct -> method).
    pub ast_parent: f32,
    /// Explicit call invocation weight (f() -> g()).
    pub call: f32,
    /// Type signature dependency weight (inputs, return types).
    pub type_ref: f32,
    /// Module import dependency weight (use statements).
    pub import: f32,
}

impl Default for LayerWeights {
    fn default() -> Self {
        Self {
            ast_parent: 0.8,
            call: 1.0,
            type_ref: 0.6,
            import: 0.3,
        }
    }
}

impl LayerWeights {
    /// Returns the assigned weight multiplier for a given edge kind.
    #[inline]
    pub fn weight_for(&self, kind: EdgeKind) -> f32 {
        match kind {
            EdgeKind::AstParent => self.ast_parent,
            EdgeKind::Call => self.call,
            EdgeKind::TypeRef => self.type_ref,
            EdgeKind::Import => self.import,
        }
    }
}

/// In-memory Multiplex Code Property Graph.
///
/// Combines dense symbol declarations (`SymbolNode`) with a high-performance Compressed
/// Sparse Row (CSR) matrix representation. Edges across multiple layers (AST containment,
/// call graph, type dependencies, imports) are resolved via Bayesian scoping and weighted
/// into a unified row-stochastic transition matrix for Personalized PageRank diffusion.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultiplexGraph {
    /// Contiguous array of extracted symbol declarations, indexed by `SymbolId(0..N)`.
    symbols: Vec<SymbolNode>,
    /// Raw unnormalized CSR matrix containing accumulated multiplex edge weights.
    raw_csr: CsrMatrix,
    /// Row-stochastic transition probability matrix $\mathbf{P} = \mathbf{D}^{-1}\mathbf{A}$.
    transition_csr: CsrMatrix,
    /// Layer weights used to construct this graph.
    layer_weights: LayerWeights,
}

impl MultiplexGraph {
    /// Constructs a `MultiplexGraph` from extracted symbols, raw reference edges, and layer weights.
    ///
    /// Resolves identifier strings to target `SymbolId`s using the Bayesian `ScopedResolver`.
    pub fn build(
        symbols: Vec<SymbolNode>,
        raw_edges: &[ReferenceEdge],
        weights: LayerWeights,
    ) -> Self {
        Self::build_with_imports(symbols, raw_edges, &[], weights)
    }

    /// Constructs a `MultiplexGraph` from extracted symbols, raw reference edges, explicit AST imports,
    /// and layer weights.
    ///
    /// Resolves identifier strings to target `SymbolId`s using import-aware deterministic scoping.
    pub fn build_with_imports(
        symbols: Vec<SymbolNode>,
        raw_edges: &[ReferenceEdge],
        imports: &[FileImport],
        weights: LayerWeights,
    ) -> Self {
        let num_nodes = symbols.len();
        let resolver = ScopedResolver::with_imports(&symbols, imports);

        let mut directed_edges: Vec<(u32, u32, f32)> =
            Vec::with_capacity(raw_edges.len() + imports.len());

        for edge in raw_edges {
            let src_idx = edge.source.0 as usize;
            if src_idx >= num_nodes {
                continue;
            }

            let source_node = &symbols[src_idx];
            if let Some((target_id, confidence)) = resolver.resolve(source_node, &edge.target_ident)
            {
                if edge.source == target_id {
                    continue;
                }

                let layer_w = weights.weight_for(edge.kind);
                let combined_w = layer_w * confidence;
                directed_edges.push((edge.source.0, target_id.0, combined_w));

                // If this is an AST parent edge (e.g. method -> struct), also emit reverse
                // containment edge (struct -> method) so struct seeds diffuse to their member methods.
                if edge.kind == EdgeKind::AstParent {
                    directed_edges.push((target_id.0, edge.source.0, combined_w));
                }
            }
        }

        let raw_csr = CsrMatrix::from_edges(num_nodes, num_nodes, directed_edges);
        let transition_csr = raw_csr.row_normalized();

        Self {
            symbols,
            raw_csr,
            transition_csr,
            layer_weights: weights,
        }
    }

    /// Returns the total number of symbols in the graph.
    #[inline]
    pub fn num_symbols(&self) -> usize {
        self.symbols.len()
    }

    /// Returns the total number of directed edges in the unified CSR.
    #[inline]
    pub fn num_edges(&self) -> usize {
        self.raw_csr.num_edges()
    }

    /// Returns true if the graph contains no symbols.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.symbols.is_empty()
    }

    /// Returns a reference to the symbol with the given `SymbolId`.
    #[inline]
    pub fn symbol(&self, id: SymbolId) -> Option<&SymbolNode> {
        let idx = id.0 as usize;
        self.symbols.get(idx)
    }

    /// Returns an immutable slice of all symbols in the graph.
    #[inline]
    pub fn symbols(&self) -> &[SymbolNode] {
        &self.symbols
    }

    /// Returns the out-degree of a given symbol.
    #[inline]
    pub fn out_degree(&self, id: SymbolId) -> usize {
        self.transition_csr.out_degree(id.0)
    }

    /// Returns a borrowed slice of target `SymbolId`s reachable from `id`.
    #[inline]
    pub fn neighbors(&self, id: SymbolId) -> &[u32] {
        self.transition_csr.neighbors(id.0)
    }

    /// Returns a borrowed slice of transition probabilities for edges originating from `id`.
    #[inline]
    pub fn transition_probabilities(&self, id: SymbolId) -> &[f32] {
        self.transition_csr.weights(id.0)
    }

    /// Returns a reference to the unnormalized raw CSR matrix.
    #[inline]
    pub fn raw_csr(&self) -> &CsrMatrix {
        &self.raw_csr
    }

    /// Returns a reference to the row-normalized transition probability CSR matrix $\mathbf{P}$.
    #[inline]
    pub fn transition_csr(&self) -> &CsrMatrix {
        &self.transition_csr
    }

    /// Returns the layer weights applied to this graph.
    #[inline]
    pub fn layer_weights(&self) -> &LayerWeights {
        &self.layer_weights
    }

    /// Computes and returns the transposed (reverse) CSR matrix where edges point from callee to caller.
    #[inline]
    pub fn transpose_csr(&self) -> CsrMatrix {
        self.raw_csr.transpose()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::symbol::{SymbolKind, TextSpan};
    use std::path::PathBuf;

    fn make_test_symbol(id: u32, name: &str, file: &str) -> SymbolNode {
        SymbolNode {
            id: SymbolId(id),
            name: name.to_string(),
            kind: SymbolKind::Function,
            file_path: PathBuf::from(file),
            span: TextSpan::new(0, 10, 0, 1),
            signature: format!("pub fn {}()", name),
            docstring: None,
            token_cost: 5,
            ast_hash: [0u8; 32],
            container_name: None,
            trait_name: None,
        }
    }

    #[test]
    fn test_multiplex_graph_construction_and_normalization() {
        let s0 = make_test_symbol(0, "caller", "src/main.rs");
        let s1 = make_test_symbol(1, "callee_a", "src/math.rs");
        let s2 = make_test_symbol(2, "callee_b", "src/math.rs");

        let symbols = vec![s0, s1, s2];

        // Caller calls callee_a and callee_b
        let edges = vec![
            ReferenceEdge {
                source: SymbolId(0),
                target_ident: "callee_a".to_string(),
                kind: EdgeKind::Call,
            },
            ReferenceEdge {
                source: SymbolId(0),
                target_ident: "callee_b".to_string(),
                kind: EdgeKind::Call,
            },
        ];

        let graph = MultiplexGraph::build(symbols, &edges, LayerWeights::default());

        assert_eq!(graph.num_symbols(), 3);
        assert_eq!(graph.num_edges(), 2);

        // Check node 0 out-degree and neighbors
        assert_eq!(graph.out_degree(SymbolId(0)), 2);
        assert_eq!(graph.neighbors(SymbolId(0)), &[1, 2]);

        // Probabilities should sum to 1.0 (equal distance and weight)
        let probs = graph.transition_probabilities(SymbolId(0));
        assert_eq!(probs.len(), 2);
        assert!((probs[0] - 0.5).abs() < 1e-6);
        assert!((probs[1] - 0.5).abs() < 1e-6);

        // Nodes 1 and 2 are sinks
        assert_eq!(graph.out_degree(SymbolId(1)), 0);
        assert_eq!(graph.out_degree(SymbolId(2)), 0);
    }
}
