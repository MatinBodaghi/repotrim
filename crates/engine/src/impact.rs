use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;

use crate::diff::DiffResolver;
use crate::formatter::ContextFormatter;
use crate::graph::MultiplexGraph;
use crate::symbol::{LodLevel, SymbolId, SymbolNode};
use crate::tokens::{count_tokens, TokenizerModel};

/// Architectural risk assessment for a change set.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum RiskLevel {
    /// Isolated modification with zero external callers or dependents.
    Low = 0,
    /// Localized modifications with 1-5 callers confined to immediate module/file.
    Medium = 1,
    /// Significant ripple effects crossing 3+ files or impacting >10 downstream symbols.
    High = 2,
    /// Critical architectural modification mutating primary hubs or breaking core APIs.
    Critical = 3,
}

impl RiskLevel {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Low => "LOW",
            Self::Medium => "MEDIUM",
            Self::High => "HIGH",
            Self::Critical => "CRITICAL",
        }
    }
}

/// Quantitative metrics summarizing the change impact analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImpactSummary {
    pub risk_level: RiskLevel,
    pub risk_score: f32,
    pub mutated_count: usize,
    pub direct_impact_count: usize,
    pub transitive_impact_count: usize,
    pub affected_tests_count: usize,
    pub affected_files_count: usize,
}

/// Comprehensive change impact report detailing mutations, ripple effects, and affected tests.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImpactReport {
    pub summary: ImpactSummary,
    pub mutated_symbols: Vec<SymbolNode>,
    pub direct_impact: Vec<SymbolNode>,
    pub transitive_impact: Vec<SymbolNode>,
    pub affected_tests: Vec<SymbolNode>,
    pub context_markdown: String,
}

/// High-performance Semantic Change Impact Analysis (CIA) Engine.
///
/// Traces forward ripple effects and calculates architectural blast radiuses
/// based on foundational software change impact analysis literature:
///
/// > Arnold, R. S., & Bohner, S. A. (1993). *"Software Change Impact Analysis"*.
/// > IEEE Computer Society Press, Los Alamitos, CA.
/// >
/// > Ren, X., Shah, F., Tip, F., Ryder, B. G., & Chesley, O. (2004).
/// > *"Chianti: A Tool for Change Impact Analysis of Java Programs"*.
/// > Proceedings of the 19th ACM SIGPLAN Conference on Object-Oriented
/// > Programming, Systems, Languages, and Applications (OOPSLA '04), pp. 432–448.
///
/// Uses transposed CSR matrix traversals to compute direct 1st-order callers,
/// transitive downstream ripple effects, and candidate regression test targets.
pub struct ImpactAnalyzer;

impl ImpactAnalyzer {
    /// Analyzes the change impact and blast radius for a list of mutated seed symbol IDs using default fast heuristic.
    pub fn analyze_symbols(
        graph: &MultiplexGraph,
        mutated_ids: &[SymbolId],
        budget: usize,
        file_sources: &HashMap<PathBuf, String>,
    ) -> ImpactReport {
        Self::analyze_symbols_with_model(
            graph,
            mutated_ids,
            budget,
            file_sources,
            TokenizerModel::default(),
        )
    }

    /// Analyzes the change impact and blast radius for a list of mutated seed symbol IDs using a specific tokenizer model.
    pub fn analyze_symbols_with_model(
        graph: &MultiplexGraph,
        mutated_ids: &[SymbolId],
        budget: usize,
        file_sources: &HashMap<PathBuf, String>,
        model: TokenizerModel,
    ) -> ImpactReport {
        if mutated_ids.is_empty() || graph.is_empty() {
            return Self::empty_report();
        }

        let num_nodes = graph.num_symbols();
        let transposed = graph.transpose_csr();

        let mutated_set: HashSet<SymbolId> = mutated_ids
            .iter()
            .copied()
            .filter(|id| (id.0 as usize) < num_nodes)
            .collect();

        if mutated_set.is_empty() {
            return Self::empty_report();
        }

        // 1. Identify 1st-order direct callers (distance = 1 in transposed CSR)
        let mut direct_set: HashSet<SymbolId> = HashSet::new();
        for &m_id in &mutated_set {
            for &caller_idx in transposed.neighbors(m_id.0) {
                let caller_id = SymbolId(caller_idx);
                if !mutated_set.contains(&caller_id) {
                    direct_set.insert(caller_id);
                }
            }
        }

        // 2. Transitive ripple effects (BFS on transposed graph starting from direct callers)
        let mut transitive_set: HashSet<SymbolId> = HashSet::new();
        let mut visited: HashSet<SymbolId> =
            mutated_set.iter().chain(&direct_set).copied().collect();
        let mut queue: VecDeque<(SymbolId, usize)> = direct_set.iter().map(|&id| (id, 1)).collect();

        const MAX_DEPTH: usize = 8;

        while let Some((curr_id, depth)) = queue.pop_front() {
            if depth >= MAX_DEPTH {
                continue;
            }
            for &caller_idx in transposed.neighbors(curr_id.0) {
                let next_id = SymbolId(caller_idx);
                if visited.insert(next_id) {
                    transitive_set.insert(next_id);
                    queue.push_back((next_id, depth + 1));
                }
            }
        }

        // 3. Classify symbols into Mutated, Direct, Transitive, and Affected Tests
        let mut mutated_symbols: Vec<SymbolNode> = Vec::new();
        let mut affected_files: HashSet<PathBuf> = HashSet::new();

        for &id in &mutated_set {
            if let Some(s) = graph.symbol(id) {
                mutated_symbols.push(s.clone());
                affected_files.insert(s.file_path.clone());
            }
        }

        let mut direct_impact = Vec::new();
        let mut transitive_impact = Vec::new();
        let mut affected_tests = Vec::new();

        // Process direct callers
        for &id in &direct_set {
            if let Some(s) = graph.symbol(id) {
                if Self::is_test_symbol(s) {
                    affected_tests.push(s.clone());
                } else {
                    direct_impact.push(s.clone());
                }
                affected_files.insert(s.file_path.clone());
            }
        }

        // Process transitive dependents
        for &id in &transitive_set {
            if let Some(s) = graph.symbol(id) {
                if Self::is_test_symbol(s) {
                    affected_tests.push(s.clone());
                } else {
                    transitive_impact.push(s.clone());
                }
                affected_files.insert(s.file_path.clone());
            }
        }

        // Sort symbols deterministically by path and row
        mutated_symbols.sort_by(|a, b| {
            a.file_path
                .cmp(&b.file_path)
                .then_with(|| a.span.start_row.cmp(&b.span.start_row))
        });
        direct_impact.sort_by(|a, b| {
            a.file_path
                .cmp(&b.file_path)
                .then_with(|| a.span.start_row.cmp(&b.span.start_row))
        });
        transitive_impact.sort_by(|a, b| {
            a.file_path
                .cmp(&b.file_path)
                .then_with(|| a.span.start_row.cmp(&b.span.start_row))
        });
        affected_tests.sort_by(|a, b| {
            a.file_path
                .cmp(&b.file_path)
                .then_with(|| a.span.start_row.cmp(&b.span.start_row))
        });

        // 4. Compute Architectural Risk Score and Level
        let max_in_degree = mutated_set
            .iter()
            .map(|&id| transposed.neighbors(id.0).len())
            .max()
            .unwrap_or(0);

        let affected_files_count = affected_files.len();

        let (risk_level, risk_score) = Self::compute_risk_level(
            mutated_symbols.len(),
            direct_impact.len(),
            transitive_impact.len(),
            affected_tests.len(),
            affected_files_count,
            max_in_degree,
        );

        let summary = ImpactSummary {
            risk_level,
            risk_score,
            mutated_count: mutated_symbols.len(),
            direct_impact_count: direct_impact.len(),
            transitive_impact_count: transitive_impact.len(),
            affected_tests_count: affected_tests.len(),
            affected_files_count,
        };

        // 5. Generate Markdown Blast Radius Outline
        let context_markdown = Self::render_impact_markdown(
            &summary,
            &mutated_symbols,
            &direct_impact,
            &transitive_impact,
            &affected_tests,
            budget,
            file_sources,
            model,
        );

        ImpactReport {
            summary,
            mutated_symbols,
            direct_impact,
            transitive_impact,
            affected_tests,
            context_markdown,
        }
    }

    /// Evaluates impact for a specific symbol by name using default fast heuristic.
    pub fn analyze_symbol(
        graph: &MultiplexGraph,
        symbol_name: &str,
        budget: usize,
        file_sources: &HashMap<PathBuf, String>,
    ) -> ImpactReport {
        Self::analyze_symbol_with_model(
            graph,
            symbol_name,
            budget,
            file_sources,
            TokenizerModel::default(),
        )
    }

    /// Evaluates impact for a specific symbol by name using a specific tokenizer model.
    pub fn analyze_symbol_with_model(
        graph: &MultiplexGraph,
        symbol_name: &str,
        budget: usize,
        file_sources: &HashMap<PathBuf, String>,
        model: TokenizerModel,
    ) -> ImpactReport {
        let matching: Vec<SymbolId> = graph
            .symbols()
            .iter()
            .filter(|s| s.name == symbol_name)
            .map(|s| s.id)
            .collect();

        Self::analyze_symbols_with_model(graph, &matching, budget, file_sources, model)
    }

    /// Evaluates impact across all symbols touched by a unified git diff using default fast heuristic.
    pub fn analyze_diff(
        graph: &MultiplexGraph,
        diff_text: &str,
        budget: usize,
        file_sources: &HashMap<PathBuf, String>,
    ) -> ImpactReport {
        Self::analyze_diff_with_model(
            graph,
            diff_text,
            budget,
            file_sources,
            TokenizerModel::default(),
        )
    }

    /// Evaluates impact across all symbols touched by a unified git diff using a specific tokenizer model.
    pub fn analyze_diff_with_model(
        graph: &MultiplexGraph,
        diff_text: &str,
        budget: usize,
        file_sources: &HashMap<PathBuf, String>,
        model: TokenizerModel,
    ) -> ImpactReport {
        let modified_lines = DiffResolver::parse_unified_diff(diff_text);
        if modified_lines.is_empty() {
            return Self::empty_report();
        }

        let weighted_symbols =
            DiffResolver::resolve_modified_symbols(graph.symbols(), &modified_lines);
        let mutated_ids: Vec<SymbolId> = weighted_symbols.into_iter().map(|(id, _)| id).collect();

        Self::analyze_symbols_with_model(graph, &mutated_ids, budget, file_sources, model)
    }

    /// Returns an empty impact report.
    pub fn empty_report() -> ImpactReport {
        ImpactReport {
            summary: ImpactSummary {
                risk_level: RiskLevel::Low,
                risk_score: 0.0,
                mutated_count: 0,
                direct_impact_count: 0,
                transitive_impact_count: 0,
                affected_tests_count: 0,
                affected_files_count: 0,
            },
            mutated_symbols: Vec::new(),
            direct_impact: Vec::new(),
            transitive_impact: Vec::new(),
            affected_tests: Vec::new(),
            context_markdown: "# Semantic Change Impact Analysis Report\n\n## Executive Risk Summary\n\n- **Architectural Risk Level**: `[LOW]` (Score: `0.00`)\n- **Directly Mutated Symbols**: 0\n- **1st-Order Callers Impacted**: 0\n- **Transitive Ripple Dependents**: 0\n- **Affected Test Targets**: 0\n- **Files in Blast Radius**: 0\n".to_string(),
        }
    }

    /// Determines if a symbol represents a unit or integration test.
    pub fn is_test_symbol(symbol: &SymbolNode) -> bool {
        let p_str = symbol.file_path.to_string_lossy();
        let lower_p = p_str.to_lowercase();
        let lower_name = symbol.name.to_lowercase();

        // Path-based indicators
        if lower_p.contains("/tests/")
            || lower_p.contains("\\tests\\")
            || lower_p.starts_with("tests/")
            || lower_p.starts_with("tests\\")
            || lower_p.ends_with("_test.rs")
            || lower_p.ends_with("_test.go")
            || lower_p.ends_with(".test.ts")
            || lower_p.ends_with(".spec.ts")
            || lower_p.ends_with(".test.tsx")
            || lower_p.ends_with(".spec.tsx")
            || lower_p.ends_with(".test.js")
            || lower_p.ends_with(".spec.js")
            || lower_p.contains("test_")
        {
            return true;
        }

        // Name-based indicators
        if lower_name.starts_with("test_")
            || symbol.name.starts_with("Test")
            || lower_name.ends_with("_test")
        {
            return true;
        }

        // Rust #[test] or #[tokio::test] attributes in signature or docstring
        if symbol.signature.contains("#[test]")
            || symbol.signature.contains("#[tokio::test]")
            || symbol
                .docstring
                .as_deref()
                .map(|d| d.contains("#[test]") || d.contains("#[tokio::test]"))
                .unwrap_or(false)
        {
            return true;
        }

        false
    }

    /// Computes the architectural risk level and normalized risk score in [0.0, 1.0].
    fn compute_risk_level(
        mutated_count: usize,
        direct_callers: usize,
        transitive_dependents: usize,
        _affected_tests: usize,
        distinct_files: usize,
        max_in_degree: usize,
    ) -> (RiskLevel, f32) {
        if mutated_count == 0 {
            return (RiskLevel::Low, 0.0);
        }

        // Weighted risk score formulation
        let raw_score = (direct_callers as f32 * 0.15)
            + (transitive_dependents as f32 * 0.05)
            + (max_in_degree as f32 * 0.10)
            + (distinct_files as f32 * 0.10);

        let normalized = (raw_score / 3.0).clamp(0.0, 1.0);

        let level = if max_in_degree >= 15 || normalized >= 0.75 {
            RiskLevel::Critical
        } else if transitive_dependents >= 10 || distinct_files >= 3 || normalized >= 0.50 {
            RiskLevel::High
        } else if direct_callers >= 1 || normalized >= 0.25 {
            RiskLevel::Medium
        } else {
            RiskLevel::Low
        };

        (level, normalized)
    }

    /// Renders the impact report into a formatted Markdown document.
    #[allow(clippy::too_many_arguments)]
    fn render_impact_markdown(
        summary: &ImpactSummary,
        mutated: &[SymbolNode],
        direct: &[SymbolNode],
        transitive: &[SymbolNode],
        tests: &[SymbolNode],
        budget: usize,
        file_sources: &HashMap<PathBuf, String>,
        model: TokenizerModel,
    ) -> String {
        let mut md = String::new();

        md.push_str("# Semantic Change Impact Analysis Report\n\n");

        // Section 1: Executive Summary
        md.push_str("## Executive Risk Summary\n\n");
        md.push_str(&format!(
            "- **Architectural Risk Level**: `[{}]` (Score: `{:.2}`)\n",
            summary.risk_level.as_str(),
            summary.risk_score
        ));
        md.push_str(&format!(
            "- **Directly Mutated Symbols**: {}\n",
            summary.mutated_count
        ));
        for m in mutated {
            md.push_str(&format!(
                "  - `{}` in `{}:{}`\n",
                m.name,
                m.file_path.display(),
                m.span.start_row + 1
            ));
        }
        md.push_str(&format!(
            "- **1st-Order Callers Impacted**: {}\n",
            summary.direct_impact_count
        ));
        md.push_str(&format!(
            "- **Transitive Ripple Dependents**: {}\n",
            summary.transitive_impact_count
        ));
        md.push_str(&format!(
            "- **Affected Test Targets**: {}\n",
            summary.affected_tests_count
        ));
        md.push_str(&format!(
            "- **Files in Blast Radius**: {}\n\n",
            summary.affected_files_count
        ));

        // Section 2: Recommended Test Candidates
        if !tests.is_empty() {
            md.push_str("## Recommended Test Suite Candidates\n\n");
            md.push_str("The following tests transitively exercise the modified code and should be executed:\n\n");
            let display_limit = 10;
            for t in tests.iter().take(display_limit) {
                md.push_str(&format!(
                    "- `{}` in `{}:{}`\n",
                    t.name,
                    t.file_path.display(),
                    t.span.start_row + 1
                ));
            }
            if tests.len() > display_limit {
                md.push_str(&format!(
                    "- ... and {} more test targets\n",
                    tests.len() - display_limit
                ));
            }
            md.push('\n');
        }

        // Section 3: Blast Radius Context
        md.push_str("## Blast Radius Context Outline\n\n");

        let mut all_symbols: Vec<SymbolNode> = Vec::new();
        let mut lod_map: HashMap<SymbolId, LodLevel> = HashMap::new();

        // Mutated symbols get SlicedBody or FullBody
        for s in mutated {
            all_symbols.push(s.clone());
            lod_map.insert(s.id, LodLevel::SlicedBody);
        }

        // Direct callers get SlicedBody
        for s in direct {
            all_symbols.push(s.clone());
            lod_map.insert(s.id, LodLevel::SlicedBody);
        }

        // Transitive dependents get SignatureAndDoc
        for s in transitive {
            all_symbols.push(s.clone());
            lod_map.insert(s.id, LodLevel::SignatureAndDoc);
        }

        // Tests get SignatureOnly
        for s in tests {
            all_symbols.push(s.clone());
            lod_map.insert(s.id, LodLevel::SignatureOnly);
        }

        let header_tokens = count_tokens(&md, model);
        let code_budget = budget.saturating_sub(header_tokens);

        let mut ppr_scores = HashMap::new();
        for (i, s) in mutated.iter().enumerate() {
            ppr_scores.insert(s.id, 1000.0 - i as f32);
        }
        for (i, s) in direct.iter().enumerate() {
            ppr_scores.insert(s.id, 500.0 - i as f32);
        }
        for (i, s) in transitive.iter().enumerate() {
            ppr_scores.insert(s.id, 100.0 - i as f32);
        }
        for (i, s) in tests.iter().enumerate() {
            ppr_scores.insert(s.id, 10.0 - i as f32);
        }
        let seed_ids: Vec<SymbolId> = mutated.iter().map(|s| s.id).collect();

        let (_, rendered_code, _) = ContextFormatter::format_markdown_budgeted(
            &all_symbols,
            &lod_map,
            file_sources,
            code_budget,
            model,
            &ppr_scores,
            &seed_ids,
        );
        md.push_str(&rendered_code);
        md.push('\n');

        md
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::LayerWeights;
    use crate::symbol::{EdgeKind, ReferenceEdge, SymbolKind, TextSpan};

    fn make_test_sym(id: u32, name: &str, file: &str) -> SymbolNode {
        SymbolNode {
            id: SymbolId(id),
            name: name.to_string(),
            kind: SymbolKind::Function,
            file_path: PathBuf::from(file),
            span: TextSpan::new(0, 50, 0, 3),
            signature: format!("pub fn {}()", name),
            docstring: None,
            token_cost: 15,
            ast_hash: [0u8; 32],
            container_name: None,
            trait_name: None,
        }
    }

    fn build_test_graph() -> MultiplexGraph {
        // Graph structure:
        // 0: leaf_fn (src/leaf.rs)
        // 1: mid_fn (src/service.rs) -> calls leaf_fn (0)
        // 2: top_fn (src/main.rs) -> calls mid_fn (1)
        // 3: test_service (tests/service_test.rs) -> calls mid_fn (1)
        // 4: isolated_fn (src/utils.rs)
        let sym0 = make_test_sym(0, "leaf_fn", "src/leaf.rs");
        let sym1 = make_test_sym(1, "mid_fn", "src/service.rs");
        let sym2 = make_test_sym(2, "top_fn", "src/main.rs");
        let sym3 = make_test_sym(3, "test_service", "tests/service_test.rs");
        let sym4 = make_test_sym(4, "isolated_fn", "src/utils.rs");

        let symbols = vec![sym0, sym1, sym2, sym3, sym4];
        let edges = vec![
            ReferenceEdge {
                source: SymbolId(1), // mid_fn -> leaf_fn
                target_ident: "leaf_fn".to_string(),
                kind: EdgeKind::Call,
            },
            ReferenceEdge {
                source: SymbolId(2), // top_fn -> mid_fn
                target_ident: "mid_fn".to_string(),
                kind: EdgeKind::Call,
            },
            ReferenceEdge {
                source: SymbolId(3), // test_service -> mid_fn
                target_ident: "mid_fn".to_string(),
                kind: EdgeKind::Call,
            },
        ];

        MultiplexGraph::build_with_imports(symbols, &edges, &[], LayerWeights::default())
    }

    #[test]
    fn test_direct_and_transitive_impact_propagation() {
        let graph = build_test_graph();
        let sources = HashMap::new();

        // Mutating leaf_fn (id: 0)
        let report = ImpactAnalyzer::analyze_symbol(&graph, "leaf_fn", 2000, &sources);

        // Mutated symbol
        assert_eq!(report.summary.mutated_count, 1);
        assert_eq!(report.mutated_symbols[0].name, "leaf_fn");

        // Direct impact: mid_fn
        assert_eq!(report.summary.direct_impact_count, 1);
        assert_eq!(report.direct_impact[0].name, "mid_fn");

        // Transitive impact: top_fn
        assert_eq!(report.summary.transitive_impact_count, 1);
        assert_eq!(report.transitive_impact[0].name, "top_fn");

        // Affected tests: test_service
        assert_eq!(report.summary.affected_tests_count, 1);
        assert_eq!(report.affected_tests[0].name, "test_service");

        // Total files in blast radius: leaf.rs, service.rs, main.rs, service_test.rs = 4
        assert_eq!(report.summary.affected_files_count, 4);
        assert_eq!(report.summary.risk_level, RiskLevel::High);
    }

    #[test]
    fn test_isolated_leaf_risk_level() {
        let graph = build_test_graph();
        let sources = HashMap::new();

        // Mutating isolated_fn (id: 4)
        let report = ImpactAnalyzer::analyze_symbol(&graph, "isolated_fn", 2000, &sources);

        assert_eq!(report.summary.mutated_count, 1);
        assert_eq!(report.summary.direct_impact_count, 0);
        assert_eq!(report.summary.transitive_impact_count, 0);
        assert_eq!(report.summary.affected_tests_count, 0);
        assert_eq!(report.summary.risk_level, RiskLevel::Low);
        assert_eq!(report.summary.affected_files_count, 1);
    }

    #[test]
    fn test_is_test_symbol_detection() {
        let rust_test = make_test_sym(0, "test_authentication", "src/auth.rs");
        assert!(ImpactAnalyzer::is_test_symbol(&rust_test));

        let go_test = make_test_sym(1, "TestServerRun", "server_test.go");
        assert!(ImpactAnalyzer::is_test_symbol(&go_test));

        let ts_test = make_test_sym(2, "renderWidget", "components/widget.test.ts");
        assert!(ImpactAnalyzer::is_test_symbol(&ts_test));

        let py_test = make_test_sym(3, "test_database_query", "tests/test_db.py");
        assert!(ImpactAnalyzer::is_test_symbol(&py_test));

        let prod_fn = make_test_sym(4, "calculate_hash", "src/crypto.rs");
        assert!(!ImpactAnalyzer::is_test_symbol(&prod_fn));
    }

    #[test]
    fn test_analyze_diff_impact() {
        let graph = build_test_graph();
        let sources = HashMap::new();

        let diff = r#"
--- a/src/leaf.rs
+++ b/src/leaf.rs
@@ -1,3 +1,3 @@
 pub fn leaf_fn() {
-    let x = 1;
+    let x = 2;
 }
"#;

        let report = ImpactAnalyzer::analyze_diff(&graph, diff, 2000, &sources);
        assert_eq!(report.summary.mutated_count, 1);
        assert_eq!(report.mutated_symbols[0].name, "leaf_fn");
        assert_eq!(report.summary.direct_impact_count, 1);
        assert!(report
            .context_markdown
            .contains("Recommended Test Suite Candidates"));
        assert!(report.context_markdown.contains("test_service"));
    }
}
