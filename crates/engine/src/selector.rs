use std::collections::HashMap;
use std::path::PathBuf;

use crate::celf::{CelfConfig, CelfOptimizer};
use crate::formatter::ContextFormatter;
use crate::graph::MultiplexGraph;
use crate::ppr::{PprConfig, PprSolver};
use crate::symbol::{SymbolId, SymbolNode};

/// End-to-end context selection pipeline.
///
/// Orchestrates Personalized PageRank (PPR) relevance diffusion and CELF submodular
/// knapsack selection to extract the mathematically optimal code context for AI coding agents.
#[derive(Debug, Clone, Default)]
pub struct ContextSelector {
    ppr: PprSolver,
    celf: CelfOptimizer,
}

impl ContextSelector {
    /// Creates a new `ContextSelector` with custom PPR and CELF configurations.
    pub fn new(ppr_config: PprConfig, celf_config: CelfConfig) -> Self {
        Self {
            ppr: PprSolver::new(ppr_config),
            celf: CelfOptimizer::new(celf_config),
        }
    }

    /// Selects the optimal set of symbols matching the `budget` (in tokens), seeded by `seed_ids`.
    ///
    /// The resulting `SymbolNode`s are sorted in canonical file and line order
    /// (`file_path`, then `start_row`, then `start_byte`) to produce natural, coherent reading
    /// order for language model prompts.
    pub fn select_context(
        &self,
        graph: &MultiplexGraph,
        seed_ids: &[SymbolId],
        budget: usize,
    ) -> Vec<SymbolNode> {
        let seeds: Vec<(SymbolId, f32)> = seed_ids.iter().map(|&id| (id, 1.0)).collect();
        self.select_context_weighted(graph, &seeds, budget)
    }

    /// Selects the optimal set of symbols matching the `budget` (in tokens), seeded by `weighted_seeds`.
    pub fn select_context_weighted(
        &self,
        graph: &MultiplexGraph,
        weighted_seeds: &[(SymbolId, f32)],
        budget: usize,
    ) -> Vec<SymbolNode> {
        if budget == 0 || graph.is_empty() || weighted_seeds.is_empty() {
            return Vec::new();
        }

        // 1. Compute local PPR relevance diffusion with weighted seeds
        let mut ppr_scores = self.ppr.compute(graph, weighted_seeds);

        // 2. Anchor boost: prioritize user focus seeds as roots of the context tree proportional to weights
        for &(seed_id, weight) in weighted_seeds {
            *ppr_scores.entry(seed_id).or_default() += weight;
        }

        // 3. Select optimal subset using CELF submodular knapsack
        let selected_ids = self.celf.optimize(graph, &ppr_scores, budget);

        // 4. Collect symbol nodes
        let mut selected_symbols: Vec<SymbolNode> = selected_ids
            .into_iter()
            .filter_map(|id| graph.symbol(id).cloned())
            .collect();

        // 5. Sort canonically: by file path, then line number, then start byte
        selected_symbols.sort_by(|a, b| {
            a.file_path
                .cmp(&b.file_path)
                .then_with(|| a.span.start_row.cmp(&b.span.start_row))
                .then_with(|| a.span.start_byte.cmp(&b.span.start_byte))
        });

        selected_symbols
    }

    /// End-to-end pipeline: selects mathematically optimal symbols and formats them
    /// into structured Markdown context with dynamic Level-of-Detail (LOD).
    pub fn select_and_format_context(
        &self,
        graph: &MultiplexGraph,
        seed_ids: &[SymbolId],
        budget: usize,
        file_sources: &HashMap<PathBuf, String>,
    ) -> (Vec<SymbolNode>, String) {
        let seeds: Vec<(SymbolId, f32)> = seed_ids.iter().map(|&id| (id, 1.0)).collect();
        self.select_and_format_context_weighted(graph, &seeds, budget, file_sources)
    }

    /// End-to-end pipeline with weighted seeds: selects mathematically optimal symbols and formats
    /// them into structured Markdown context with dynamic Level-of-Detail (LOD).
    pub fn select_and_format_context_weighted(
        &self,
        graph: &MultiplexGraph,
        weighted_seeds: &[(SymbolId, f32)],
        budget: usize,
        file_sources: &HashMap<PathBuf, String>,
    ) -> (Vec<SymbolNode>, String) {
        if budget == 0 || graph.is_empty() || weighted_seeds.is_empty() {
            return (Vec::new(), String::new());
        }

        // 1. Compute local PPR relevance diffusion with weighted seeds
        let mut ppr_scores = self.ppr.compute(graph, weighted_seeds);

        // 2. Anchor boost: prioritize user focus seeds as roots of the context tree proportional to weights
        for &(seed_id, weight) in weighted_seeds {
            *ppr_scores.entry(seed_id).or_default() += weight;
        }

        // 3. Select optimal subset using CELF submodular knapsack
        let selected_ids = self.celf.optimize(graph, &ppr_scores, budget);

        // 4. Collect and canonically sort symbol nodes
        let mut selected_symbols: Vec<SymbolNode> = selected_ids
            .into_iter()
            .filter_map(|id| graph.symbol(id).cloned())
            .collect();

        selected_symbols.sort_by(|a, b| {
            a.file_path
                .cmp(&b.file_path)
                .then_with(|| a.span.start_row.cmp(&b.span.start_row))
                .then_with(|| a.span.start_byte.cmp(&b.span.start_byte))
        });

        // 5. Dynamically assign LOD and format into Markdown
        let seed_id_list: Vec<SymbolId> = weighted_seeds.iter().map(|&(id, _)| id).collect();
        let lod_map = ContextFormatter::assign_lod(
            &selected_symbols,
            &ppr_scores,
            &seed_id_list,
            budget,
            file_sources,
        );
        let markdown = ContextFormatter::format_markdown(&selected_symbols, &lod_map, file_sources);

        (selected_symbols, markdown)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::LayerWeights;
    use crate::symbol::{EdgeKind, ReferenceEdge, SymbolKind, TextSpan};
    use std::path::PathBuf;

    fn make_test_symbol(id: u32, name: &str, file: &str, row: usize, cost: usize) -> SymbolNode {
        SymbolNode {
            id: SymbolId(id),
            name: name.to_string(),
            kind: SymbolKind::Function,
            file_path: PathBuf::from(file),
            span: TextSpan::new(row * 20, (row + 1) * 20, row, row + 1),
            signature: format!("fn {}()", name),
            docstring: None,
            token_cost: cost,
            ast_hash: [0u8; 32],
        }
    }

    #[test]
    fn test_context_selector_budget_and_canonical_sorting() {
        // Module with 3 functions in geometry.rs and 1 in main.rs
        let s0 = make_test_symbol(0, "main", "src/main.rs", 10, 25);
        let s1 = make_test_symbol(1, "distance", "src/geometry.rs", 20, 30);
        let s2 = make_test_symbol(2, "sqrt", "src/geometry.rs", 5, 15);
        let s3 = make_test_symbol(3, "area", "src/geometry.rs", 40, 40);

        let edges = vec![
            ReferenceEdge {
                source: SymbolId(0),
                target_ident: "distance".to_string(),
                kind: EdgeKind::Call,
            },
            ReferenceEdge {
                source: SymbolId(1),
                target_ident: "sqrt".to_string(),
                kind: EdgeKind::Call,
            },
        ];

        let graph = MultiplexGraph::build(vec![s0, s1, s2, s3], &edges, LayerWeights::default());
        let selector = ContextSelector::default();

        // Query with seed = main (SymbolId 0), budget = 75 tokens
        // main (25) + distance (30) + sqrt (15) = 70 tokens <= 75 tokens
        let context = selector.select_context(&graph, &[SymbolId(0)], 75);

        assert_eq!(context.len(), 3);
        let names: Vec<&str> = context.iter().map(|s| s.name.as_str()).collect();

        // Canonical ordering should place geometry.rs first (sorted by file path),
        // and inside geometry.rs, sqrt (row 5) before distance (row 20)!
        assert_eq!(names, vec!["sqrt", "distance", "main"]);

        let total_tokens: usize = context.iter().map(|s| s.token_cost).sum();
        assert!(total_tokens <= 75);
    }

    #[test]
    fn test_context_selector_strict_budget_pruning() {
        let s0 = make_test_symbol(0, "entry", "src/lib.rs", 0, 50);
        let s1 = make_test_symbol(1, "helper", "src/lib.rs", 10, 50);

        let edges = vec![ReferenceEdge {
            source: SymbolId(0),
            target_ident: "helper".to_string(),
            kind: EdgeKind::Call,
        }];

        let graph = MultiplexGraph::build(vec![s0, s1], &edges, LayerWeights::default());
        let selector = ContextSelector::default();

        // Budget of 60 tokens: only entry (50 tokens) can fit; helper (50) would exceed 60
        let context = selector.select_context(&graph, &[SymbolId(0)], 60);
        assert_eq!(context.len(), 1);
        assert_eq!(context[0].name, "entry");
    }

    #[test]
    fn test_context_selector_select_and_format_pipeline() {
        let s0 = make_test_symbol(0, "entry", "src/lib.rs", 0, 10);
        let s1 = make_test_symbol(1, "helper", "src/lib.rs", 10, 10);

        let edges = vec![ReferenceEdge {
            source: SymbolId(0),
            target_ident: "helper".to_string(),
            kind: EdgeKind::Call,
        }];

        let graph = MultiplexGraph::build(vec![s0, s1], &edges, LayerWeights::default());
        let selector = ContextSelector::default();

        let mut sources = HashMap::new();
        sources.insert(
            PathBuf::from("src/lib.rs"),
            "fn entry() {\n    helper();\n}\n\nfn helper() {\n}\n".to_string(),
        );

        let (symbols, markdown) =
            selector.select_and_format_context(&graph, &[SymbolId(0)], 100, &sources);

        assert_eq!(symbols.len(), 2);
        assert!(markdown.contains("### File: `src/lib.rs`"));
        assert!(markdown.contains("```rust"));
        assert!(markdown.contains("fn entry()"));
        assert!(markdown.contains("fn helper()"));
    }

    #[test]
    fn test_context_selector_weighted_seeds() {
        let s0 = make_test_symbol(0, "heavy_seed", "src/lib.rs", 0, 10);
        let s1 = make_test_symbol(1, "light_seed", "src/lib.rs", 10, 10);
        let s2 = make_test_symbol(2, "leaf", "src/lib.rs", 20, 10);

        let edges = vec![
            ReferenceEdge {
                source: SymbolId(0),
                target_ident: "leaf".to_string(),
                kind: EdgeKind::Call,
            },
            ReferenceEdge {
                source: SymbolId(1),
                target_ident: "leaf".to_string(),
                kind: EdgeKind::Call,
            },
        ];

        let graph = MultiplexGraph::build(vec![s0, s1, s2], &edges, LayerWeights::default());
        let selector = ContextSelector::default();

        let weighted_seeds = vec![(SymbolId(0), 10.0), (SymbolId(1), 0.1)];
        let selected = selector.select_context_weighted(&graph, &weighted_seeds, 25);

        // Budget of 25 fits 2 symbols (each is 10 tokens).
        // heavy_seed (weight 10.0) should be prioritized.
        assert_eq!(selected.len(), 2);
        assert!(selected.iter().any(|s| s.name == "heavy_seed"));
    }
}
