//! Typed Multiplex Compressed Sparse Row (CSR) Code Property Graph representation.
//!
//! # Theoretical Formulation (De Domenico et al., 2013; Kivelä et al., 2014)
//! In a multilayer/multiplex graph $\mathcal{M} = (V, \mathbf{E}, \mathcal{R})$, each code relation
//! $r \in \mathcal{R}$ forms an independent sparse adjacency slice $A_r \in \mathbb{R}^{|V| \times |V|}$.
//! This module decouples orthogonal relation layers (calls, inheritance, type references, AST containment,
//! imports, and commit co-edits) into isolated Compressed Sparse Row (CSR) matrices.
//!
//! # Academic Citations
//! - De Domenico, M., Solé-Ribalta, A., Cozzo, E., Kivelä, M., Moreno, Y., Porter, M. A.,
//!   Gómez, S., & Arenas, A. (2013). "Mathematical Formulation of Multilayer Networks".
//!   *Physical Review X*, 3(4), 041022.
//! - Kivelä, M., Arenas, A., Barthelemy, M., Gleeson, J. P., Moreno, Y., & Porter, M. A. (2014).
//!   "Multilayer networks". *Journal of Complex Networks*, 2(3), 203-271.
//! - Andersen, R., Chung, F., & Lang, K. (2006). "Local Graph Partitioning using PageRank Vectors".
//!   *FOCS 2006*, pp. 475-486.

use serde::{Deserialize, Serialize};

use crate::csr::CsrMatrix;
use crate::import::FileImport;
use crate::resolver::ScopedResolver;
use crate::symbol::{EdgeKind, ReferenceEdge, RelationType, SymbolId, SymbolNode};

/// In-memory Typed Multiplex Compressed Sparse Row (CSR) Code Property Graph.
///
/// Stores extracted code symbols alongside an array of isolated CSR matrices corresponding to
/// each relation type in $\mathcal{R}$ (`RelationType::ALL`). Provides zero-allocation iterators
/// and degree queries across arbitrary relation subsets without pointer chasing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultiplexCsrGraph {
    /// Contiguous array of extracted symbol declarations, indexed by `SymbolId(0..N)`.
    symbols: Vec<SymbolNode>,
    /// Per-relation isolated CSR matrices indexed by `RelationType::index()`.
    slices: Vec<CsrMatrix>,
    /// Total edge count accumulated across all relation slices.
    total_edges: usize,
}

impl MultiplexCsrGraph {
    /// Constructs a `MultiplexCsrGraph` from symbols and pre-built CSR slices.
    ///
    /// # Panics
    /// Panics if `slices.len() != RelationType::COUNT`.
    pub fn new(symbols: Vec<SymbolNode>, slices: Vec<CsrMatrix>) -> Self {
        assert_eq!(
            slices.len(),
            RelationType::COUNT,
            "MultiplexCsrGraph requires exactly {} relation slices, got {}",
            RelationType::COUNT,
            slices.len()
        );

        let total_edges = slices.iter().map(|s| s.num_edges()).sum();
        Self {
            symbols,
            slices,
            total_edges,
        }
    }

    /// Constructs a `MultiplexCsrGraph` from a flat list of typed directed edges `(src, dst, relation, weight)`.
    pub fn from_typed_edges(
        symbols: Vec<SymbolNode>,
        edges: &[(u32, u32, RelationType, f32)],
    ) -> Self {
        let num_nodes = symbols.len();
        let mut per_rel_edges: Vec<Vec<(u32, u32, f32)>> = vec![Vec::new(); RelationType::COUNT];

        for &(src, dst, rel, weight) in edges {
            let src_idx = src as usize;
            let dst_idx = dst as usize;
            if src_idx < num_nodes && dst_idx < num_nodes && weight > 0.0 {
                per_rel_edges[rel.index()].push((src, dst, weight));
            }
        }

        let mut slices = Vec::with_capacity(RelationType::COUNT);
        for rel_edges in per_rel_edges {
            slices.push(CsrMatrix::from_edges(num_nodes, num_nodes, rel_edges));
        }

        let total_edges = slices.iter().map(|s| s.num_edges()).sum();
        Self {
            symbols,
            slices,
            total_edges,
        }
    }

    /// Constructs a `MultiplexCsrGraph` from raw reference edges.
    pub fn build(symbols: Vec<SymbolNode>, raw_edges: &[ReferenceEdge]) -> Self {
        Self::build_with_imports(symbols, raw_edges, &[])
    }

    /// Constructs a `MultiplexCsrGraph` from raw reference edges and explicit file imports.
    pub fn build_with_imports(
        symbols: Vec<SymbolNode>,
        raw_edges: &[ReferenceEdge],
        imports: &[FileImport],
    ) -> Self {
        Self::build_with_all(symbols, raw_edges, imports, &[])
    }

    /// Constructs a `MultiplexCsrGraph` resolving raw reference edges, AST imports, and Git co-edits.
    pub fn build_with_all(
        symbols: Vec<SymbolNode>,
        raw_edges: &[ReferenceEdge],
        imports: &[FileImport],
        coedit_edges: &[(SymbolId, SymbolId, f32)],
    ) -> Self {
        let num_nodes = symbols.len();
        let resolver = ScopedResolver::with_imports(&symbols, imports);

        let mut per_rel_edges: Vec<Vec<(u32, u32, f32)>> = vec![Vec::new(); RelationType::COUNT];

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

                let src = edge.source.0;
                let dst = target_id.0;
                let weight = confidence;

                match edge.kind {
                    EdgeKind::Call => {
                        per_rel_edges[RelationType::Calls.index()].push((src, dst, weight));
                    }
                    EdgeKind::TypeRef => {
                        per_rel_edges[RelationType::References.index()].push((src, dst, weight));
                    }
                    EdgeKind::Import => {
                        per_rel_edges[RelationType::Imports.index()].push((src, dst, weight));
                    }
                    EdgeKind::AstParent => {
                        // Forward: parent contains child (target -> source in AST parent reference)
                        // Reverse: child belongs to parent (source -> target)
                        per_rel_edges[RelationType::BelongsTo.index()].push((src, dst, weight));
                        per_rel_edges[RelationType::Contains.index()].push((dst, src, weight));
                    }
                    EdgeKind::CoEdit => {
                        per_rel_edges[RelationType::CoChangesWith.index()].push((src, dst, weight));
                    }
                }
            }
        }

        // Ingest mined Git co-edits
        for &(src_id, tgt_id, confidence) in coedit_edges {
            let src_idx = src_id.0 as usize;
            let tgt_idx = tgt_id.0 as usize;
            if src_idx < num_nodes && tgt_idx < num_nodes && src_id != tgt_id && confidence > 0.0 {
                per_rel_edges[RelationType::CoChangesWith.index()]
                    .push((src_id.0, tgt_id.0, confidence));
            }
        }

        let mut slices = Vec::with_capacity(RelationType::COUNT);
        for rel_edges in per_rel_edges {
            slices.push(CsrMatrix::from_edges(num_nodes, num_nodes, rel_edges));
        }

        let total_edges = slices.iter().map(|s| s.num_edges()).sum();
        Self {
            symbols,
            slices,
            total_edges,
        }
    }

    /// Returns the total number of code entity nodes in the graph.
    #[inline]
    pub fn num_symbols(&self) -> usize {
        self.symbols.len()
    }

    /// Returns true if the graph contains zero symbols.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.symbols.is_empty()
    }

    /// Returns a reference to the symbol node with given `SymbolId`.
    #[inline]
    pub fn symbol(&self, id: SymbolId) -> Option<&SymbolNode> {
        self.symbols.get(id.0 as usize)
    }

    /// Returns a slice of all symbol declarations.
    #[inline]
    pub fn symbols(&self) -> &[SymbolNode] {
        &self.symbols
    }

    /// Returns the total number of directed edges across all relation layers.
    #[inline]
    pub fn num_edges(&self) -> usize {
        self.total_edges
    }

    /// Returns the number of edges present in the specified relation layer $A_r$.
    #[inline]
    pub fn num_edges_for(&self, rel: RelationType) -> usize {
        self.slices[rel.index()].num_edges()
    }

    /// Returns a reference to the isolated CSR matrix for relation $r$.
    #[inline]
    pub fn slice(&self, rel: RelationType) -> &CsrMatrix {
        &self.slices[rel.index()]
    }

    /// Returns all relation slices in canonical `RelationType::ALL` order.
    #[inline]
    pub fn slices(&self) -> &[CsrMatrix] {
        &self.slices
    }

    /// Returns the out-degree of a symbol in a specific relation layer.
    #[inline]
    pub fn out_degree_for(&self, id: SymbolId, rel: RelationType) -> usize {
        self.slices[rel.index()].out_degree(id.0)
    }

    /// Returns the total out-degree of a symbol summed across all relation layers.
    #[inline]
    pub fn total_out_degree(&self, id: SymbolId) -> usize {
        self.slices.iter().map(|s| s.out_degree(id.0)).sum()
    }

    /// Returns a borrowed slice of target `SymbolId`s for relation $r$ originating from `id`.
    ///
    /// Zero heap allocations ($O(1)$ time).
    #[inline]
    pub fn neighbors_for(&self, id: SymbolId, rel: RelationType) -> &[u32] {
        self.slices[rel.index()].neighbors(id.0)
    }

    /// Returns a borrowed slice of edge weights for relation $r$ originating from `id`.
    ///
    /// Zero heap allocations ($O(1)$ time).
    #[inline]
    pub fn weights_for(&self, id: SymbolId, rel: RelationType) -> &[f32] {
        self.slices[rel.index()].weights(id.0)
    }

    /// Returns neighbor and weight slices simultaneously for relation $r$ originating from `id`.
    #[inline]
    pub fn row_slice_for(&self, id: SymbolId, rel: RelationType) -> (&[u32], &[f32]) {
        self.slices[rel.index()].row_slice(id.0)
    }

    /// Returns a zero-allocation iterator over all outgoing edges across all relation layers.
    pub fn typed_neighbors(
        &self,
        id: SymbolId,
    ) -> impl Iterator<Item = (RelationType, u32, f32)> + '_ {
        RelationType::ALL.into_iter().flat_map(move |rel| {
            let (neighbors, weights) = self.row_slice_for(id, rel);
            neighbors
                .iter()
                .zip(weights.iter())
                .map(move |(&dst, &w)| (rel, dst, w))
        })
    }

    /// Returns a zero-allocation iterator over outgoing edges for an arbitrary subset of relations.
    pub fn neighbors_for_relations<'a>(
        &'a self,
        id: SymbolId,
        relations: &'a [RelationType],
    ) -> impl Iterator<Item = (RelationType, u32, f32)> + 'a {
        relations.iter().flat_map(move |&rel| {
            let (neighbors, weights) = self.row_slice_for(id, rel);
            neighbors
                .iter()
                .zip(weights.iter())
                .map(move |(&dst, &w)| (rel, dst, w))
        })
    }

    /// Computes the transposed CSR matrix for a given relation slice.
    #[inline]
    pub fn transpose_slice(&self, rel: RelationType) -> CsrMatrix {
        self.slices[rel.index()].transpose()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::symbol::{SymbolKind, TextSpan};
    use std::path::PathBuf;

    fn make_test_node(id: u32, name: &str) -> SymbolNode {
        SymbolNode {
            id: SymbolId(id),
            name: name.to_string(),
            kind: SymbolKind::Function,
            file_path: PathBuf::from("test.rs"),
            span: TextSpan::new(0, 10, 0, 1),
            signature: format!("fn {}()", name),
            docstring: None,
            token_cost: 10,
            ast_hash: [0u8; 32],
            container_name: None,
            trait_name: None,
        }
    }

    #[test]
    fn test_multiplex_csr_graph_typed_slices() {
        let symbols = vec![
            make_test_node(0, "main"),
            make_test_node(1, "process"),
            make_test_node(2, "Helper"),
            make_test_node(3, "test_process"),
        ];

        let typed_edges = vec![
            (0, 1, RelationType::Calls, 1.0),
            (1, 2, RelationType::References, 0.8),
            (3, 1, RelationType::IsTestedBy, 1.5),
            (1, 0, RelationType::CoChangesWith, 0.5),
        ];

        let graph = MultiplexCsrGraph::from_typed_edges(symbols, &typed_edges);

        assert_eq!(graph.num_symbols(), 4);
        assert_eq!(graph.num_edges(), 4);

        // Calls slice
        assert_eq!(graph.num_edges_for(RelationType::Calls), 1);
        assert_eq!(graph.out_degree_for(SymbolId(0), RelationType::Calls), 1);
        assert_eq!(graph.neighbors_for(SymbolId(0), RelationType::Calls), &[1]);
        assert_eq!(graph.weights_for(SymbolId(0), RelationType::Calls), &[1.0]);

        // References slice
        assert_eq!(graph.num_edges_for(RelationType::References), 1);
        assert_eq!(
            graph.neighbors_for(SymbolId(1), RelationType::References),
            &[2]
        );

        // IsTestedBy slice
        assert_eq!(graph.num_edges_for(RelationType::IsTestedBy), 1);
        assert_eq!(
            graph.neighbors_for(SymbolId(3), RelationType::IsTestedBy),
            &[1]
        );

        // CoChangesWith slice
        assert_eq!(graph.num_edges_for(RelationType::CoChangesWith), 1);
        assert_eq!(
            graph.neighbors_for(SymbolId(1), RelationType::CoChangesWith),
            &[0]
        );

        // Empty slice check
        assert_eq!(graph.num_edges_for(RelationType::Imports), 0);
        assert_eq!(graph.neighbors_for(SymbolId(0), RelationType::Imports), &[]);

        // Typed neighbors iterator
        let neighbors_0: Vec<(RelationType, u32, f32)> =
            graph.typed_neighbors(SymbolId(0)).collect();
        assert_eq!(neighbors_0, vec![(RelationType::Calls, 1, 1.0)]);

        let neighbors_1: Vec<(RelationType, u32, f32)> =
            graph.typed_neighbors(SymbolId(1)).collect();
        assert_eq!(
            neighbors_1,
            vec![
                (RelationType::References, 2, 0.8),
                (RelationType::CoChangesWith, 0, 0.5),
            ]
        );

        // Filtered subset iterator
        let queried_rels = [RelationType::Calls, RelationType::IsTestedBy];
        let sub_1: Vec<(RelationType, u32, f32)> = graph
            .neighbors_for_relations(SymbolId(1), &queried_rels)
            .collect();
        assert!(sub_1.is_empty());

        let sub_3: Vec<(RelationType, u32, f32)> = graph
            .neighbors_for_relations(SymbolId(3), &queried_rels)
            .collect();
        assert_eq!(sub_3, vec![(RelationType::IsTestedBy, 1, 1.5)]);

        // Total out degree
        assert_eq!(graph.total_out_degree(SymbolId(1)), 2);
        assert_eq!(graph.total_out_degree(SymbolId(2)), 0);

        // Transpose slice
        let t_calls = graph.transpose_slice(RelationType::Calls);
        assert_eq!(t_calls.neighbors(1), &[0]);
        assert_eq!(t_calls.weights(1), &[1.0]);
    }
}
