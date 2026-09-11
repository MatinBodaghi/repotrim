//! Structured Context Graph Object & Diagnostic Explanation Engine.
//!
//! # Theoretical Formulation
//! Traditional context engines dump unstructured text blobs, leaving AI coding harnesses
//! blind to dependency edges, execution pathways, token budget consumption, and omitted
//! alternatives. This module formalizes the extracted context into a typed graph object
//! $\mathcal{C} = (V_{\mathcal{C}}, E_{\mathcal{C}}, \mathcal{P}_{\mathcal{C}}, \mathcal{O}_{\mathcal{C}}, M_{\mathcal{C}})$
//! equipped with causal paths and transparent omission diagnostics.

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use crate::cost::CostBreakdown;
use crate::symbol::{LodLevel, NodeType, RelationType, SymbolId, SymbolKind, SymbolNode, TextSpan};
use crate::task::TaskContext;
use crate::tokens::TokenizerModel;

/// A structured, self-contained symbol node in the context selection.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StructuredSymbol {
    /// Dense symbol identifier.
    pub id: SymbolId,
    /// Extracted symbol identifier name.
    pub name: String,
    /// Source file path.
    pub file_path: PathBuf,
    /// Source code location span.
    pub span: TextSpan,
    /// Syntactic symbol kind.
    pub kind: SymbolKind,
    /// Semantic node category.
    pub node_type: NodeType,
    /// Level-of-Detail resolution selected for this symbol.
    pub lod: LodLevel,
    /// Estimated token cost at selected LOD.
    pub token_cost: usize,
    /// Prior relevance / attribution score.
    pub score: f32,
    /// Optional enclosing container (class, struct, trait impl).
    pub container_name: Option<String>,
    /// Optional trait name implemented by the enclosing block.
    pub trait_name: Option<String>,
    /// Rendered code text for this symbol at the selected LOD.
    pub code: String,
}

/// A typed directed edge between symbols included in the structured context.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StructuredEdge {
    /// Source symbol identifier.
    pub source: SymbolId,
    /// Target symbol identifier.
    pub target: SymbolId,
    /// Typed relation between source and target.
    pub relation: RelationType,
    /// Confidence or transition weight $\in (0, 1]$.
    pub weight: f32,
}

/// A human- and machine-readable trace of a task-relevant execution path.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PathTrace {
    /// Sequence of symbol IDs in the path.
    pub nodes: Vec<SymbolId>,
    /// Sequence of typed relations between successive nodes.
    pub relations: Vec<RelationType>,
    /// Formatted trace string (e.g. `auth_middleware --[Calls]--> user_service`).
    pub trace: String,
    /// Normalized Boltzmann probability $P(p \mid q, G)$.
    pub probability: f32,
    /// Rationale explaining the causal significance of this path.
    pub rationale: String,
}

/// The diagnostic reason why a candidate symbol was omitted from the final context.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum OmissionReason {
    /// Candidate would exceed the remaining token budget.
    BudgetExhausted,
    /// Candidate utility was discounted below viability due to mutual information redundancy.
    RedundancySuppressed,
    /// Candidate marginal utility gain was insufficient relative to alternative selections.
    MarginalUtilityDepleted,
    /// Candidate initial relevance was below the discovery cutoff threshold.
    BelowCutoffThreshold,
}

impl std::fmt::Display for OmissionReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BudgetExhausted => write!(f, "Budget Exhausted"),
            Self::RedundancySuppressed => write!(f, "Redundancy Suppressed"),
            Self::MarginalUtilityDepleted => write!(f, "Marginal Utility Depleted"),
            Self::BelowCutoffThreshold => write!(f, "Below Cutoff Threshold"),
        }
    }
}

/// Detailed diagnostic explanation for a symbol omitted during budgeted optimization.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OmissionDiagnostic {
    /// Candidate symbol identifier.
    pub symbol_id: SymbolId,
    /// Symbol name.
    pub name: String,
    /// File path where the candidate is declared.
    pub file_path: PathBuf,
    /// Required token cost of the candidate.
    pub token_cost: usize,
    /// Initial relevance score.
    pub relevance_score: f32,
    /// Evaluated marginal utility gain $\Delta_F(x \mid S)$.
    pub marginal_utility: f32,
    /// Formal diagnostic reason category.
    pub reason: OmissionReason,
    /// Human-readable explanation text.
    pub explanation: String,
}

/// Comprehensive, machine-readable structured context object.
///
/// Encapsulates all extracted symbols, typed relation edges, causal execution paths,
/// cost accounting breakdowns, and explicit omission diagnostics for AI coding harnesses.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StructuredContext {
    /// Operational task context and query parameters.
    pub task: Option<TaskContext>,
    /// List of included code symbols.
    pub symbols: Vec<StructuredSymbol>,
    /// Typed relations connecting included symbols.
    pub edges: Vec<StructuredEdge>,
    /// Inferred causal execution and dependency paths.
    pub paths: Vec<PathTrace>,
    /// Detailed omission diagnostics for pruned candidate symbols.
    pub omissions: Vec<OmissionDiagnostic>,
    /// Aggregate confidence score $\in [0, 1]$ measuring context coverage sufficiency.
    pub confidence_score: f32,
    /// Allocated token budget ceiling.
    pub budget_limit: usize,
    /// Actual total tokens consumed by included symbols.
    pub tokens_used: usize,
    /// Multi-factor token cost breakdown across code, metadata, relations, and framing.
    pub cost_breakdown: CostBreakdown,
}

impl StructuredContext {
    /// Creates a fresh empty `StructuredContext` for a specified token budget limit.
    pub fn new(budget_limit: usize) -> Self {
        Self {
            task: None,
            symbols: Vec::new(),
            edges: Vec::new(),
            paths: Vec::new(),
            omissions: Vec::new(),
            confidence_score: 0.0,
            budget_limit,
            tokens_used: 0,
            cost_breakdown: CostBreakdown::default(),
        }
    }

    /// Returns true if zero symbols are included in this context.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.symbols.is_empty()
    }

    /// Returns the number of symbols included in the context.
    #[inline]
    pub fn num_symbols(&self) -> usize {
        self.symbols.len()
    }

    /// Returns the number of typed edges included in the context.
    #[inline]
    pub fn num_edges(&self) -> usize {
        self.edges.len()
    }

    /// Returns the number of causal paths traced in the context.
    #[inline]
    pub fn num_paths(&self) -> usize {
        self.paths.len()
    }

    /// Returns the number of omitted candidates tracked.
    #[inline]
    pub fn num_omissions(&self) -> usize {
        self.omissions.len()
    }

    /// Looks up an included symbol by its `SymbolId`.
    pub fn symbol_by_id(&self, id: SymbolId) -> Option<&StructuredSymbol> {
        self.symbols.iter().find(|s| s.id == id)
    }

    /// Returns true if symbol `id` is included in this context.
    #[inline]
    pub fn contains_symbol(&self, id: SymbolId) -> bool {
        self.symbols.iter().any(|s| s.id == id)
    }

    /// Manually records a single omission diagnostic.
    #[allow(clippy::too_many_arguments)]
    pub fn record_omission(
        &mut self,
        symbol_id: SymbolId,
        name: impl Into<String>,
        file_path: PathBuf,
        token_cost: usize,
        relevance_score: f32,
        marginal_utility: f32,
        reason: OmissionReason,
        explanation: impl Into<String>,
    ) {
        self.omissions.push(OmissionDiagnostic {
            symbol_id,
            name: name.into(),
            file_path,
            token_cost,
            relevance_score,
            marginal_utility,
            reason,
            explanation: explanation.into(),
        });
    }

    /// Computes and sets the aggregate confidence score $\in [0, 1]$ measuring context sufficiency.
    ///
    /// Combines relevance mass preservation ratio, budget saturation, and causal path completeness.
    pub fn update_confidence_score(&mut self, total_relevance: f32) {
        let selected_relevance: f32 = self.symbols.iter().map(|s| s.score).sum();
        let relevance_ratio = if total_relevance > 1e-6 {
            (selected_relevance / total_relevance).clamp(0.0, 1.0)
        } else if !self.symbols.is_empty() {
            1.0
        } else {
            0.0
        };

        let budget_efficiency = if self.budget_limit > 0 {
            ((self.tokens_used as f32) / (self.budget_limit as f32)).min(1.0)
        } else {
            0.0
        };

        let path_factor = if !self.paths.is_empty() {
            let avg_prob: f32 =
                self.paths.iter().map(|p| p.probability).sum::<f32>() / (self.paths.len() as f32);
            0.10 * avg_prob.clamp(0.0, 1.0)
        } else {
            0.0
        };

        self.confidence_score =
            (0.70 * relevance_ratio + 0.20 * budget_efficiency + path_factor).clamp(0.0, 1.0);
    }

    /// Serializes this structured context to a JSON string.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    /// Serializes this structured context to a formatted, indented JSON string.
    pub fn to_json_pretty(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    /// Deserializes a `StructuredContext` from a JSON string.
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }
}

/// Diagnostician analyzing omitted candidates after budgeted knapsack selection.
pub struct OmissionDiagnostician;

impl OmissionDiagnostician {
    /// Evaluates and categorizes candidate symbols that were not selected into the context.
    ///
    /// # Diagnostic Criteria:
    /// 1. `BelowCutoffThreshold`: Candidate initial relevance $r(v) < \tau_{\min}$.
    /// 2. `BudgetExhausted`: Candidate token cost $c(v)$ exceeds the remaining token budget
    ///    $B - \sum_{u \in S} c(u)$ or standalone budget limit $B$.
    /// 3. `RedundancySuppressed`: Candidate marginal utility gain $\Delta(v \mid S)$ was heavily
    ///    discounted ($\le 0.15 \cdot r(v)$ or $\le 0$) due to mutual overlap with already selected symbols.
    /// 4. `MarginalUtilityDepleted`: Candidate was viable and fits budget, but had lower marginal
    ///    utility per token than the selected candidates.
    #[allow(clippy::too_many_arguments)]
    pub fn diagnose(
        candidates: &[SymbolNode],
        selected_ids: &HashSet<SymbolId>,
        ppr_scores: &HashMap<SymbolId, f32>,
        marginal_gains: &HashMap<SymbolId, f32>,
        remaining_budget: usize,
        budget_limit: usize,
        min_relevance_threshold: f32,
        tokenizer: TokenizerModel,
    ) -> Vec<OmissionDiagnostic> {
        let mut omissions = Vec::new();

        for sym in candidates {
            if selected_ids.contains(&sym.id) {
                continue;
            }

            let score = ppr_scores.get(&sym.id).copied().unwrap_or(0.0);
            let cost = crate::tokens::count_tokens(&sym.signature, tokenizer).max(1);
            let marginal_gain = marginal_gains.get(&sym.id).copied().unwrap_or(0.0);

            let (reason, explanation) = if score < min_relevance_threshold {
                (
                    OmissionReason::BelowCutoffThreshold,
                    format!(
                        "Relevance score ({:.6}) fell below discovery cutoff threshold ({:.6})",
                        score, min_relevance_threshold
                    ),
                )
            } else if cost > remaining_budget || cost > budget_limit {
                (
                    OmissionReason::BudgetExhausted,
                    format!(
                        "Candidate token cost ({} tokens) exceeds remaining budget ({} tokens available)",
                        cost, remaining_budget
                    ),
                )
            } else if marginal_gain <= 0.15 * score || marginal_gain <= 0.0 {
                (
                    OmissionReason::RedundancySuppressed,
                    format!(
                        "Marginal gain ({:.4}) suppressed due to redundant overlap with already selected symbols in {:?}",
                        marginal_gain, sym.file_path
                    ),
                )
            } else {
                (
                    OmissionReason::MarginalUtilityDepleted,
                    format!(
                        "Marginal utility density ({:.4}/tok) outranked by higher-priority candidates",
                        marginal_gain / (cost as f32)
                    ),
                )
            };

            omissions.push(OmissionDiagnostic {
                symbol_id: sym.id,
                name: sym.name.clone(),
                file_path: sym.file_path.clone(),
                token_cost: cost,
                relevance_score: score,
                marginal_utility: marginal_gain,
                reason,
                explanation,
            });
        }

        // Sort omissions by initial relevance score in descending order
        omissions.sort_by(|a, b| {
            b.relevance_score
                .partial_cmp(&a.relevance_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        omissions
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_structured_context_json_roundtrip() {
        let mut ctx = StructuredContext::new(2000);
        ctx.tokens_used = 450;
        ctx.confidence_score = 0.92;

        let sym = StructuredSymbol {
            id: SymbolId(0),
            name: "authenticate".to_string(),
            file_path: PathBuf::from("src/auth.rs"),
            span: TextSpan::new(10, 100, 2, 8),
            kind: SymbolKind::Function,
            node_type: NodeType::Function,
            lod: LodLevel::SignatureAndDoc,
            token_cost: 45,
            score: 0.88,
            container_name: None,
            trait_name: None,
            code: "pub fn authenticate(req: &Request) -> Result<User>".to_string(),
        };
        ctx.symbols.push(sym);

        let edge = StructuredEdge {
            source: SymbolId(0),
            target: SymbolId(1),
            relation: RelationType::Calls,
            weight: 0.95,
        };
        ctx.edges.push(edge);

        let path = PathTrace {
            nodes: vec![SymbolId(0), SymbolId(1)],
            relations: vec![RelationType::Calls],
            trace: "authenticate --[Calls]--> verify_jwt".to_string(),
            probability: 0.85,
            rationale: "Authentication flow entrypoint to JWT validation".to_string(),
        };
        ctx.paths.push(path);

        let omission = OmissionDiagnostic {
            symbol_id: SymbolId(2),
            name: "debug_logger".to_string(),
            file_path: PathBuf::from("src/log.rs"),
            token_cost: 150,
            relevance_score: 0.20,
            marginal_utility: 0.01,
            reason: OmissionReason::BudgetExhausted,
            explanation: "Candidate cost (150 tokens) exceeded remaining budget".to_string(),
        };
        ctx.omissions.push(omission);

        let json = ctx.to_json_pretty().expect("Failed to serialize to JSON");
        assert!(json.contains("authenticate"));
        assert!(json.contains("BudgetExhausted"));

        let deserialized =
            StructuredContext::from_json(&json).expect("Failed to deserialize from JSON");
        assert_eq!(ctx, deserialized);
        assert_eq!(deserialized.num_symbols(), 1);
        assert_eq!(deserialized.num_edges(), 1);
        assert_eq!(deserialized.num_paths(), 1);
        assert_eq!(deserialized.num_omissions(), 1);
        assert_eq!(deserialized.confidence_score, 0.92);
    }

    #[test]
    fn test_omission_diagnostician_categorization() {
        let sym1 = SymbolNode {
            id: SymbolId(1),
            name: "pruned_by_budget".to_string(),
            file_path: PathBuf::from("src/heavy.rs"),
            span: TextSpan::new(0, 50, 1, 5),
            kind: SymbolKind::Function,
            signature: "pub fn pruned_by_budget_with_extensive_parameter_list(a: usize, b: String, c: Vec<u8>, d: HashMap<String, Value>) -> Result<(), Error>".to_string(),
            docstring: None,
            token_cost: 30,
            ast_hash: [0u8; 32],
            container_name: None,
            trait_name: None,
        };
        let sym2 = SymbolNode {
            id: SymbolId(2),
            name: "pruned_by_redundancy".to_string(),
            file_path: PathBuf::from("src/dup.rs"),
            span: TextSpan::new(0, 50, 1, 5),
            kind: SymbolKind::Function,
            signature: "fn g()".to_string(),
            docstring: None,
            token_cost: 3,
            ast_hash: [0u8; 32],
            container_name: None,
            trait_name: None,
        };
        let sym3 = SymbolNode {
            id: SymbolId(3),
            name: "pruned_by_cutoff".to_string(),
            file_path: PathBuf::from("src/irrelevant.rs"),
            span: TextSpan::new(0, 50, 1, 5),
            kind: SymbolKind::Function,
            signature: "fn h()".to_string(),
            docstring: None,
            token_cost: 3,
            ast_hash: [0u8; 32],
            container_name: None,
            trait_name: None,
        };
        let sym4 = SymbolNode {
            id: SymbolId(4),
            name: "pruned_by_marginal_gain".to_string(),
            file_path: PathBuf::from("src/low_gain.rs"),
            span: TextSpan::new(0, 50, 1, 5),
            kind: SymbolKind::Function,
            signature: "fn i()".to_string(),
            docstring: None,
            token_cost: 3,
            ast_hash: [0u8; 32],
            container_name: None,
            trait_name: None,
        };

        let candidates = vec![sym1, sym2, sym3, sym4];
        let selected_ids = HashSet::new(); // none selected

        let mut ppr_scores = HashMap::new();
        ppr_scores.insert(SymbolId(1), 0.80);
        ppr_scores.insert(SymbolId(2), 0.70);
        ppr_scores.insert(SymbolId(3), 0.000001); // below cutoff
        ppr_scores.insert(SymbolId(4), 0.60);

        let mut marginal_gains = HashMap::new();
        marginal_gains.insert(SymbolId(1), 0.75);
        marginal_gains.insert(SymbolId(2), 0.02); // redundancy suppressed
        marginal_gains.insert(SymbolId(3), 0.00);
        marginal_gains.insert(SymbolId(4), 0.50); // marginal utility depleted

        let remaining_budget = 10; // sym1 (cost > 10) will be budget exhausted; sym2 and sym4 fit
        let budget_limit = 100;
        let min_cutoff = 0.0001;

        let omissions = OmissionDiagnostician::diagnose(
            &candidates,
            &selected_ids,
            &ppr_scores,
            &marginal_gains,
            remaining_budget,
            budget_limit,
            min_cutoff,
            TokenizerModel::FastHeuristic,
        );

        assert_eq!(omissions.len(), 4);

        let o_budget = omissions
            .iter()
            .find(|o| o.symbol_id == SymbolId(1))
            .unwrap();
        assert_eq!(o_budget.reason, OmissionReason::BudgetExhausted);

        let o_redundancy = omissions
            .iter()
            .find(|o| o.symbol_id == SymbolId(2))
            .unwrap();
        assert_eq!(o_redundancy.reason, OmissionReason::RedundancySuppressed);

        let o_cutoff = omissions
            .iter()
            .find(|o| o.symbol_id == SymbolId(3))
            .unwrap();
        assert_eq!(o_cutoff.reason, OmissionReason::BelowCutoffThreshold);

        let o_depleted = omissions
            .iter()
            .find(|o| o.symbol_id == SymbolId(4))
            .unwrap();
        assert_eq!(o_depleted.reason, OmissionReason::MarginalUtilityDepleted);
    }

    #[test]
    fn test_confidence_score_calculation() {
        let mut ctx = StructuredContext::new(1000);
        ctx.tokens_used = 800;

        let sym = StructuredSymbol {
            id: SymbolId(0),
            name: "core_func".to_string(),
            file_path: PathBuf::from("src/core.rs"),
            span: TextSpan::new(1, 10, 1, 2),
            kind: SymbolKind::Function,
            node_type: NodeType::Function,
            lod: LodLevel::FullBody,
            token_cost: 800,
            score: 0.90,
            container_name: None,
            trait_name: None,
            code: "fn core_func() {}".to_string(),
        };
        ctx.symbols.push(sym);

        // When total relevance was 1.0 and selected is 0.90
        ctx.update_confidence_score(1.0);
        // 0.70 * 0.90 + 0.20 * (800/1000) = 0.63 + 0.16 = 0.79
        assert!((ctx.confidence_score - 0.79).abs() < 1e-4);

        // Add a high probability path
        ctx.paths.push(PathTrace {
            nodes: vec![SymbolId(0)],
            relations: Vec::new(),
            trace: "core_func".to_string(),
            probability: 0.95,
            rationale: "Self trace".to_string(),
        });
        ctx.update_confidence_score(1.0);
        // 0.79 + 0.10 * 0.95 = 0.885
        assert!((ctx.confidence_score - 0.885).abs() < 1e-3);
    }
}
