//! Multi-Factor Token Cost Accounting for Submodular Knapsack Optimization.
//!
//! # Mathematical Formulation (Krause & Golovin, 2014; Sviridenko, 2004)
//! In constrained submodular maximization (e.g. Modified Greedy Algorithm under Knapsack Constraints),
//! candidate selection depends fundamentally on the benefit-to-cost ratio:
//! $$\frac{\Delta(x \mid S)}{c(x)}$$
//!
//! Naive token accounting models only consider raw code characters ($c(v) \approx \text{len}(v)/4$).
//! In production prompt contexts, symbols incur multi-dimensional framing and metadata costs:
//! $$c(v) = c_{\text{text}}(v) + c_{\text{meta}}(v) + c_{\text{rel}}(v) + c_{\text{format}}(v)$$
//!
//! Where:
//! - $c_{\text{text}}(v)$: Tokens consumed by the code snippet at the assigned resolution LOD
//!   (Signature, Sliced body, Full body).
//! - $c_{\text{meta}}(v)$: Tokens consumed by structural metadata (identifier name, node type,
//!   container name, file path, line numbers).
//! - $c_{\text{rel}}(v)$: Tokens consumed by rendered relationship links (imports, callers, callees,
//!   verification test linkage).
//! - $c_{\text{format}}(v)$: Tokens consumed by framing markup (Markdown code fences, language tag,
//!   section headers, delimiters, indentation).
//!
//! # Academic Citations
//! - Krause, A., & Golovin, D. (2014). "Submodular Function Maximization". In *Tractability:
//!   Practical Approaches to Hard Problems*, Cambridge University Press, pp. 71–104.
//! - Sviridenko, M. (2004). "A note on maximizing a submodular set function subject to a knapsack
//!   constraint". *Operations Research Letters*, 32(1), 41–43.

use serde::{Deserialize, Serialize};

use crate::formatter::ContextFormatter;
use crate::symbol::{LodLevel, SymbolNode};
use crate::tokens::{count_tokens, TokenizerModel};

/// Multi-factor breakdown of token costs incurred by an entity representation in context.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct CostBreakdown {
    /// Tokens consumed by the code snippet at the target resolution LOD.
    pub text_cost: usize,
    /// Tokens consumed by entity metadata (name, kind, container, span, path).
    pub meta_cost: usize,
    /// Tokens consumed by rendered relationship links (callers, callees, tests, imports).
    pub rel_cost: usize,
    /// Tokens consumed by markdown framing, code fences, headers, and separators.
    pub format_cost: usize,
}

impl CostBreakdown {
    /// Creates a new `CostBreakdown` with explicit component values.
    pub const fn new(
        text_cost: usize,
        meta_cost: usize,
        rel_cost: usize,
        format_cost: usize,
    ) -> Self {
        Self {
            text_cost,
            meta_cost,
            rel_cost,
            format_cost,
        }
    }

    /// Returns the total combined token cost $c(v) = c_{\text{text}} + c_{\text{meta}} + c_{\text{rel}} + c_{\text{format}}$.
    #[inline]
    pub fn total(&self) -> usize {
        self.text_cost + self.meta_cost + self.rel_cost + self.format_cost
    }
}

/// Configuration parameters for multi-factor token cost estimation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenCostConfig {
    /// Tokenizer model used for token counting and estimation.
    pub model: TokenizerModel,
    /// Base framing overhead per symbol (Markdown code fence, language tag, newlines).
    pub base_format_overhead: usize,
    /// Estimated tokens per rendered relationship link (caller/callee/test/import).
    pub tokens_per_relation: usize,
    /// Amortized file header tokens per symbol (file path and markdown section heading).
    pub amortized_header_overhead: usize,
    /// Minimum guaranteed total token cost floor per symbol.
    pub min_cost_floor: usize,
}

impl Default for TokenCostConfig {
    fn default() -> Self {
        Self {
            model: TokenizerModel::FastHeuristic,
            base_format_overhead: 4,
            tokens_per_relation: 3,
            amortized_header_overhead: 3,
            min_cost_floor: 1,
        }
    }
}

/// Multi-factor token cost estimator.
///
/// Computes realistic token accounting across code content, structural metadata,
/// graph relationships, and Markdown/XML formatting overhead.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenCostEstimator {
    config: TokenCostConfig,
}

impl Default for TokenCostEstimator {
    fn default() -> Self {
        Self::new(TokenizerModel::default())
    }
}

impl TokenCostEstimator {
    /// Creates a new `TokenCostEstimator` with default configuration and given tokenizer model.
    pub fn new(model: TokenizerModel) -> Self {
        Self {
            config: TokenCostConfig {
                model,
                ..Default::default()
            },
        }
    }

    /// Creates a new `TokenCostEstimator` with custom configuration.
    pub fn with_config(config: TokenCostConfig) -> Self {
        Self { config }
    }

    /// Returns a reference to the active configuration.
    #[inline]
    pub fn config(&self) -> &TokenCostConfig {
        &self.config
    }

    /// Computes the comprehensive multi-factor cost breakdown for a `SymbolNode`
    /// at a given `LodLevel`, optional file content source, and known relation count.
    pub fn estimate_breakdown(
        &self,
        symbol: &SymbolNode,
        lod: LodLevel,
        file_source: Option<&str>,
        num_relations: usize,
    ) -> CostBreakdown {
        // 1. Text cost: rendered code body or signature
        let text_cost = self.compute_text_cost(symbol, lod, file_source);

        // 2. Metadata cost: name, kind, container, span
        let meta_cost = self.compute_metadata_cost(symbol);

        // 3. Relation cost
        let rel_cost = num_relations * self.config.tokens_per_relation;

        // 4. Formatting overhead
        let format_cost = self.config.base_format_overhead + self.config.amortized_header_overhead;

        let mut breakdown = CostBreakdown {
            text_cost,
            meta_cost,
            rel_cost,
            format_cost,
        };

        if breakdown.total() < self.config.min_cost_floor {
            breakdown.format_cost += self.config.min_cost_floor - breakdown.total();
        }

        breakdown
    }

    /// Computes the total combined token cost $c(v)$.
    pub fn estimate_total(
        &self,
        symbol: &SymbolNode,
        lod: LodLevel,
        file_source: Option<&str>,
        num_relations: usize,
    ) -> usize {
        self.estimate_breakdown(symbol, lod, file_source, num_relations)
            .total()
    }

    /// Quick multi-factor estimation without full file source loaded,
    /// utilizing pre-indexed `symbol.token_cost` and signature.
    pub fn estimate_quick(
        &self,
        symbol: &SymbolNode,
        lod: LodLevel,
        num_relations: usize,
    ) -> CostBreakdown {
        self.estimate_breakdown(symbol, lod, None, num_relations)
    }

    fn compute_text_cost(
        &self,
        symbol: &SymbolNode,
        lod: LodLevel,
        file_source: Option<&str>,
    ) -> usize {
        match file_source {
            Some(src) => {
                let rendered = ContextFormatter::render_symbol(symbol, lod, Some(src));
                count_tokens(&rendered, self.config.model).max(1)
            }
            None => match lod {
                LodLevel::SignatureOnly => {
                    let sig = symbol.signature.trim();
                    count_tokens(sig, self.config.model).max(1)
                }
                LodLevel::SignatureAndDoc => {
                    let mut tokens = count_tokens(symbol.signature.trim(), self.config.model);
                    if let Some(doc) = &symbol.docstring {
                        tokens += count_tokens(doc, self.config.model);
                    }
                    tokens.max(1)
                }
                LodLevel::SlicedBody => {
                    let full = symbol.token_cost.max(1);
                    let sig_tokens = count_tokens(symbol.signature.trim(), self.config.model);
                    let doc_tokens = symbol
                        .docstring
                        .as_deref()
                        .map(|d| count_tokens(d, self.config.model))
                        .unwrap_or(0);
                    let min_boundary = sig_tokens + doc_tokens;
                    ((full / 2) + 2).max(min_boundary)
                }
                LodLevel::FullBody => {
                    let sig_tokens = count_tokens(symbol.signature.trim(), self.config.model);
                    let doc_tokens = symbol
                        .docstring
                        .as_deref()
                        .map(|d| count_tokens(d, self.config.model))
                        .unwrap_or(0);
                    let min_boundary = sig_tokens + doc_tokens;
                    symbol.token_cost.max(min_boundary).max(1)
                }
            },
        }
    }

    fn compute_metadata_cost(&self, symbol: &SymbolNode) -> usize {
        let mut meta_text = format!("{} {}", symbol.node_type().as_str(), symbol.name);
        if let Some(c) = &symbol.container_name {
            meta_text.push_str(" in ");
            meta_text.push_str(c);
        }
        meta_text.push_str(&format!(
            " [L{}-L{}]",
            symbol.span.start_row, symbol.span.end_row
        ));
        count_tokens(&meta_text, self.config.model).max(1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::symbol::{SymbolId, SymbolKind, TextSpan};
    use std::path::PathBuf;

    fn make_test_symbol() -> SymbolNode {
        SymbolNode {
            id: SymbolId(1),
            name: "calculate_coverage".to_string(),
            kind: SymbolKind::Function,
            file_path: PathBuf::from("crates/engine/src/evidence.rs"),
            span: TextSpan::new(100, 350, 10, 25),
            signature: "pub fn calculate_coverage(kernel: &SparseKernelMatrix) -> f32".to_string(),
            docstring: Some("Computes coverage over target entities.".to_string()),
            token_cost: 45,
            ast_hash: [0u8; 32],
            container_name: Some("ProbabilisticCoverage".to_string()),
            trait_name: None,
        }
    }

    #[test]
    fn test_cost_breakdown_total() {
        let b = CostBreakdown::new(50, 10, 6, 7);
        assert_eq!(b.total(), 73);
    }

    #[test]
    fn test_estimate_breakdown_lod_monotonicity() {
        let sym = make_test_symbol();
        let estimator = TokenCostEstimator::default();

        let b_sig = estimator.estimate_quick(&sym, LodLevel::SignatureOnly, 2);
        let b_doc = estimator.estimate_quick(&sym, LodLevel::SignatureAndDoc, 2);
        let b_slice = estimator.estimate_quick(&sym, LodLevel::SlicedBody, 2);
        let b_full = estimator.estimate_quick(&sym, LodLevel::FullBody, 2);

        // Check text cost monotonically increases with detail
        assert!(b_sig.text_cost <= b_doc.text_cost);
        assert!(b_doc.text_cost <= b_slice.text_cost);
        assert!(b_slice.text_cost <= b_full.text_cost);

        // Total cost monotonically increases
        assert!(b_sig.total() <= b_doc.total());
        assert!(b_doc.total() <= b_slice.total());
        assert!(b_slice.total() <= b_full.total());

        // Metadata and formatting costs remain constant across LODs
        assert_eq!(b_sig.meta_cost, b_full.meta_cost);
        assert_eq!(b_sig.format_cost, b_full.format_cost);
        assert_eq!(b_sig.rel_cost, b_full.rel_cost);
        assert_eq!(b_sig.rel_cost, 2 * 3);
    }

    #[test]
    fn test_estimate_with_source() {
        let sym = make_test_symbol();
        let estimator = TokenCostEstimator::new(TokenizerModel::FastHeuristic);
        let file_src = "pub fn calculate_coverage(kernel: &SparseKernelMatrix) -> f32 {\n    // Implementation\n    42.0\n}\n";

        let b_full = estimator.estimate_breakdown(&sym, LodLevel::FullBody, Some(file_src), 0);
        assert!(b_full.text_cost > 0);
        assert_eq!(b_full.rel_cost, 0);
        assert_eq!(b_full.format_cost, 7); // 4 base + 3 header
        assert!(b_full.total() > b_full.text_cost);
    }
}
