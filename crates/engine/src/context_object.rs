//! Structured Context Graph Object & Diagnostic Explanation Engine.
//!
//! # Theoretical Formulation
//! Traditional context engines dump unstructured text blobs, leaving AI coding harnesses
//! blind to dependency edges, execution pathways, token budget consumption, and omitted
//! alternatives. This module formalizes the extracted context into a typed graph object
//! $\mathcal{C} = (V_{\mathcal{C}}, E_{\mathcal{C}}, \mathcal{P}_{\mathcal{C}}, \mathcal{O}_{\mathcal{C}}, M_{\mathcal{C}})$
//! equipped with causal paths and transparent omission diagnostics.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::cost::CostBreakdown;
use crate::symbol::{LodLevel, NodeType, RelationType, SymbolId, SymbolKind, TextSpan};
use crate::task::TaskContext;

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
}
