use std::collections::HashSet;

use crate::symbol::{SymbolId, SymbolNode};

/// In-memory lexical, fuzzy, and semantic hybrid intent resolver.
///
/// Resolves natural language queries, error descriptions, or search terms into
/// prioritized `SymbolId` seeds weighted by relevance for Personalized PageRank.
///
/// Employs a dense-sparse hybrid expansion paradigm to alleviate the vocabulary mismatch
/// problem in source code retrieval without requiring a heavy remote neural embedding service:
/// - Exact and fuzzy character 3-gram matching (Jaccard similarity).
/// - Smoothed BM25 term frequency / inverse document frequency (Robertson & Zaragoza, 2009).
/// - Static domain ontology concept clusters for soft semantic affinity:
///   Formal, T., Lassance, C., Piwowarski, B., & Clinchant, S. (2021).
///   *SPLADE v2: Sparse Lexical and Expansion Model for Information Retrieval*. arXiv:2109.10086.
///   Gao, L., Dai, Z., & Callan, J. (2021).
///   *COIL: Efficient Dense-Sparse Hybrid Retrieval*. arXiv:2104.07186.
pub struct IntentResolver;

impl IntentResolver {
    /// Splits a string into normalized, lowercase tokens.
    ///
    /// Breaks on whitespace, punctuation, underscores, path separators,
    /// and CamelCase boundaries (e.g. `"ContextSelector"` -> `["context", "selector"]`).
    pub fn tokenize(text: &str) -> Vec<String> {
        let mut tokens = Vec::new();
        let mut current = String::new();
        let mut prev_char: Option<char> = None;

        for c in text.chars() {
            if c.is_alphanumeric() {
                if let Some(prev) = prev_char {
                    // Detect CamelCase transitions
                    let is_camel = (prev.is_lowercase() && c.is_uppercase())
                        || (prev.is_alphabetic() && c.is_ascii_digit())
                        || (prev.is_ascii_digit() && c.is_alphabetic());

                    if is_camel && !current.is_empty() {
                        tokens.push(current.to_lowercase());
                        current.clear();
                    }
                }
                current.push(c);
                prev_char = Some(c);
            } else {
                if !current.is_empty() {
                    tokens.push(current.to_lowercase());
                    current.clear();
                }
                prev_char = None;
            }
        }

        if !current.is_empty() {
            tokens.push(current.to_lowercase());
        }

        tokens
    }

    /// Generates character 3-grams for fuzzy string matching.
    pub fn trigrams(text: &str) -> HashSet<String> {
        let normalized = text.to_lowercase();
        let chars: Vec<char> = normalized.chars().collect();
        let mut set = HashSet::new();

        if chars.len() < 3 {
            if !normalized.is_empty() {
                set.insert(normalized);
            }
            return set;
        }

        for window in chars.windows(3) {
            set.insert(window.iter().collect());
        }

        set
    }

    /// Computes the Jaccard similarity coefficient between two character 3-gram sets.
    pub fn trigram_similarity(set_a: &HashSet<String>, set_b: &HashSet<String>) -> f32 {
        if set_a.is_empty() && set_b.is_empty() {
            return 1.0;
        }
        if set_a.is_empty() || set_b.is_empty() {
            return 0.0;
        }

        let intersection_count = set_a.intersection(set_b).count();
        let union_count = set_a.union(set_b).count();

        if union_count == 0 {
            0.0
        } else {
            intersection_count as f32 / union_count as f32
        }
    }

    /// Resolves a natural language query into ranked seed symbols with confidence scores in `(0.0, 1.0]`.
    ///
    /// Employs BM25-style term frequency/inverse document frequency (TF-IDF) scoring
    /// across symbol names, file paths, signatures, and docstrings, complemented by
    /// character trigram fuzzy matching for typo tolerance.
    pub fn resolve_query(
        symbols: &[SymbolNode],
        query: &str,
        top_k: usize,
    ) -> Vec<(SymbolId, f32)> {
        let config = crate::retrieval::RetrievalConfig::hybrid(top_k);
        let retriever = crate::retrieval::HybridRetriever::with_config(config);
        let results = retriever.search(symbols, query);
        results
            .into_iter()
            .map(|r| (r.symbol_id, r.score))
            .collect()
    }

    /// Resolves a query using a custom `RetrievalConfig`, returning detailed `SearchResult`s.
    pub fn resolve_query_with_config(
        symbols: &[SymbolNode],
        query: &str,
        config: &crate::retrieval::RetrievalConfig,
    ) -> Vec<crate::retrieval::SearchResult> {
        let retriever = crate::retrieval::HybridRetriever::with_config(config.clone());
        retriever.search(symbols, query)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::symbol::{SymbolKind, TextSpan};
    use std::path::PathBuf;

    fn make_test_sym(id: u32, name: &str, file: &str, sig: &str, doc: Option<&str>) -> SymbolNode {
        SymbolNode {
            id: SymbolId(id),
            name: name.to_string(),
            kind: SymbolKind::Function,
            file_path: PathBuf::from(file),
            span: TextSpan::new(0, 10, 0, 1),
            signature: sig.to_string(),
            docstring: doc.map(|d| d.to_string()),
            token_cost: 10,
            ast_hash: [0u8; 32],
            container_name: None,
            trait_name: None,
        }
    }

    #[test]
    fn test_tokenize_camel_and_snake() {
        let tokens = IntentResolver::tokenize("estimateTokens_from_BPE");
        assert_eq!(tokens, vec!["estimate", "tokens", "from", "bpe"]);

        let path_tokens = IntentResolver::tokenize("crates/engine/src/intent.rs");
        assert_eq!(path_tokens, vec!["crates", "engine", "src", "intent", "rs"]);
    }

    #[test]
    fn test_trigram_fuzzy_similarity() {
        let tri1 = IntentResolver::trigrams("authenticate");
        let tri2 = IntentResolver::trigrams("authenticat"); // 1 typo/omission
        let sim = IntentResolver::trigram_similarity(&tri1, &tri2);
        assert!(sim > 0.7, "Expected high similarity, got {}", sim);

        let tri3 = IntentResolver::trigrams("xyz123");
        let sim2 = IntentResolver::trigram_similarity(&tri1, &tri3);
        assert_eq!(sim2, 0.0);
    }

    #[test]
    fn test_resolve_query_ranking() {
        let s0 = make_test_sym(
            0,
            "estimate_tokens",
            "crates/engine/src/tokens.rs",
            "pub fn estimate_tokens(text: &str) -> usize",
            Some("Estimates the token count using fast BPE."),
        );
        let s1 = make_test_sym(
            1,
            "select_context",
            "crates/engine/src/selector.rs",
            "pub fn select_context(&self, graph: &MultiplexGraph) -> Vec<SymbolNode>",
            Some("Selects optimal code context."),
        );
        let s2 = make_test_sym(
            2,
            "authenticate_user",
            "crates/auth/src/service.rs",
            "pub fn authenticate_user(token: &str) -> bool",
            Some("Verifies JWT token."),
        );

        let symbols = vec![s0, s1, s2];

        // Query 1: exact/partial function match
        let results = IntentResolver::resolve_query(&symbols, "estimate tokens bpe", 2);
        assert!(!results.is_empty());
        assert_eq!(results[0].0, SymbolId(0));
        assert_eq!(results[0].1, 1.0); // Top candidate normalized to 1.0

        // Query 2: docstring and signature match for auth
        let results2 = IntentResolver::resolve_query(&symbols, "JWT verification", 2);
        assert!(!results2.is_empty());
        assert_eq!(results2[0].0, SymbolId(2));

        // Query 3: fuzzy query with typo
        let results3 = IntentResolver::resolve_query(&symbols, "estimat_token", 2);
        assert!(!results3.is_empty());
        assert_eq!(results3[0].0, SymbolId(0));
    }

    #[test]
    fn test_zero_overlap_semantic_query() {
        let s_db = make_test_sym(
            0,
            "persist_entity",
            "crates/db/src/repo.rs",
            "pub fn persist_entity(data: &[u8]) -> Result<(), Error>",
            Some("Writes records into the SQL table."),
        );
        let s_auth = make_test_sym(
            1,
            "verify_signature",
            "crates/crypto/src/sign.rs",
            "pub fn verify_signature(key: &[u8]) -> bool",
            Some("Validates HMAC secret."),
        );
        let s_calc = make_test_sym(
            2,
            "calculate_matrix_pagerank",
            "crates/algo/src/graph.rs",
            "pub fn calculate_matrix_pagerank()",
            None,
        );

        let symbols = vec![s_db, s_auth, s_calc];

        // Zero literal overlap query: "credentials and password authentication"
        // s_auth contains "verify", "signature", "key", "crypto", "secret" -> all in Auth/Security cluster!
        // Neither "credentials" nor "password" appears literally in s_auth.
        let results =
            IntentResolver::resolve_query(&symbols, "credentials password authentication", 1);
        assert!(
            !results.is_empty(),
            "Expected semantic hybrid resolution for zero-overlap query"
        );
        assert_eq!(results[0].0, SymbolId(1));
        assert!(results[0].1 > 0.0);

        // Zero literal overlap query: "database store migration"
        // s_db contains "persist", "entity", "db", "repo", "records", "sql", "table" -> Database cluster!
        let results_db = IntentResolver::resolve_query(&symbols, "database store migration", 1);
        assert!(
            !results_db.is_empty(),
            "Expected semantic hybrid resolution for database query"
        );
        assert_eq!(results_db[0].0, SymbolId(0));
        assert!(results_db[0].1 > 0.0);
    }
}
