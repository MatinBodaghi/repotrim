use std::collections::{HashMap, HashSet};

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

/// Static domain ontology clusters for zero-overlap semantic expansion.
const CONCEPT_CLUSTERS: &[&[&str]] = &[
    // Authentication / Security / Crypto
    &[
        "auth",
        "authenticate",
        "login",
        "token",
        "credential",
        "jwt",
        "session",
        "password",
        "key",
        "permission",
        "oauth",
        "rbac",
        "crypto",
        "secret",
        "verify",
        "security",
        "secure",
        "hash",
        "signature",
        "certificate",
        "access",
    ],
    // Database / Storage / Persistence
    &[
        "db",
        "database",
        "query",
        "sql",
        "persist",
        "repository",
        "model",
        "store",
        "table",
        "entity",
        "cache",
        "redis",
        "postgres",
        "sqlite",
        "insert",
        "migration",
        "save",
        "storage",
        "record",
        "transaction",
    ],
    // Networking / HTTP / API / Web
    &[
        "http", "network", "request", "response", "client", "server", "endpoint", "api", "fetch",
        "route", "handler", "socket", "rpc", "rest", "gateway", "connect", "url", "webhook",
    ],
    // Parsing / Serialization / Pipeline / AST
    &[
        "parse",
        "transform",
        "process",
        "serialize",
        "deserialize",
        "json",
        "ast",
        "stream",
        "codec",
        "buffer",
        "extractor",
        "grammar",
        "syntax",
        "tree",
        "decode",
        "encode",
        "compile",
    ],
    // Graph / Algorithms / Mathematics / Ranking
    &[
        "calculate",
        "compute",
        "algorithm",
        "graph",
        "pagerank",
        "ppr",
        "celf",
        "matrix",
        "score",
        "rank",
        "metric",
        "distance",
        "cost",
        "budget",
        "vector",
        "similarity",
        "heuristic",
        "traverse",
        "bfs",
        "dfs",
    ],
    // Lifecycle / Concurrency / Asynchrony
    &[
        "init", "start", "stop", "close", "thread", "async", "spawn", "worker", "pool", "mutex",
        "channel", "run", "execute", "task", "daemon", "service", "schedule", "queue", "event",
    ],
    // Configuration / Environment / Flags
    &[
        "config",
        "configuration",
        "setting",
        "env",
        "environment",
        "option",
        "param",
        "parameter",
        "flag",
        "property",
        "preference",
    ],
    // Error / Logging / Diagnostics
    &[
        "error",
        "exception",
        "fault",
        "warn",
        "log",
        "logger",
        "trace",
        "metric",
        "diagnostic",
        "report",
        "panic",
        "debug",
    ],
];

/// Helper to test if a token matches any keyword in a concept cluster.
fn cluster_matches_token(cluster: &[&str], token: &str) -> bool {
    let t = token.to_lowercase();
    for &kw in cluster {
        if t == kw {
            return true;
        }
        if t.len() >= 4 && kw.len() >= 4 && (t.starts_with(kw) || kw.starts_with(&t)) {
            return true;
        }
    }
    false
}

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
        let query_trimmed = query.trim();
        if query_trimmed.is_empty() || symbols.is_empty() || top_k == 0 {
            return Vec::new();
        }

        let query_tokens = Self::tokenize(query_trimmed);
        if query_tokens.is_empty() {
            return Vec::new();
        }

        let query_trigrams = Self::trigrams(query_trimmed);
        let n_docs = symbols.len() as f32;

        // Precompute document frequencies for each query token
        let mut doc_freqs: HashMap<&str, usize> = HashMap::new();
        for token in &query_tokens {
            let count = symbols
                .iter()
                .filter(|s| {
                    s.name.to_lowercase().contains(token)
                        || s.file_path.to_string_lossy().to_lowercase().contains(token)
                        || s.signature.to_lowercase().contains(token)
                        || s.docstring
                            .as_deref()
                            .is_some_and(|d| d.to_lowercase().contains(token))
                })
                .count();
            doc_freqs.insert(token.as_str(), count);
        }

        // Precompute matching semantic concept clusters for the query
        let mut query_clusters = HashSet::new();
        for q_token in &query_tokens {
            for (idx, &cluster) in CONCEPT_CLUSTERS.iter().enumerate() {
                if cluster_matches_token(cluster, q_token) {
                    query_clusters.insert(idx);
                }
            }
        }

        let mut scores: Vec<(SymbolId, f32)> = Vec::with_capacity(symbols.len());

        for symbol in symbols {
            let mut score = 0.0_f32;
            let sym_name_lower = symbol.name.to_lowercase();
            let sym_name_tokens = Self::tokenize(&symbol.name);
            let path_tokens = Self::tokenize(&symbol.file_path.to_string_lossy());
            let sig_tokens = Self::tokenize(&symbol.signature);
            let doc_tokens = symbol
                .docstring
                .as_deref()
                .map(Self::tokenize)
                .unwrap_or_default();

            // 1. Exact or case-insensitive symbol name match gives an immediate anchor bonus
            if sym_name_lower == query_trimmed.to_lowercase() {
                score += 50.0;
            } else if query_tokens.iter().any(|q| q == &sym_name_lower) {
                score += 25.0;
            }

            // 2. Trigram similarity between query and symbol name (fuzzy tolerance)
            let sym_trigrams = Self::trigrams(&symbol.name);
            let tri_sim = Self::trigram_similarity(&query_trigrams, &sym_trigrams);
            if tri_sim > 0.3 {
                score += tri_sim * 15.0;
            }

            // 3. BM25-style term matching across fields
            for token in &query_tokens {
                let df = doc_freqs.get(token.as_str()).copied().unwrap_or(0);
                // Standard smoothed BM25 IDF: ln(1 + (N - df + 0.5) / (df + 0.5))
                let idf = ((n_docs - df as f32 + 0.5) / (df as f32 + 0.5) + 1.0)
                    .ln()
                    .max(0.2);

                // Term frequencies in each field
                let name_tf = sym_name_tokens.iter().filter(|&t| t == token).count() as f32;
                let path_tf = path_tokens.iter().filter(|&t| t == token).count() as f32;
                let sig_tf = sig_tokens.iter().filter(|&t| t == token).count() as f32;
                let doc_tf = doc_tokens.iter().filter(|&t| t == token).count() as f32;

                // Field multipliers: name > path > signature > docstring
                if name_tf > 0.0 {
                    score += idf * (name_tf * 8.0);
                }
                if path_tf > 0.0 {
                    score += idf * (path_tf * 3.0);
                }
                if sig_tf > 0.0 {
                    score += idf * (sig_tf * 2.0);
                }
                if doc_tf > 0.0 {
                    score += idf * (doc_tf * 1.0);
                }
            }

            // 4. Semantic Concept Expansion for zero-overlap domain affinity
            // (Formal et al., 2021; Gao et al., 2021)
            let mut semantic_score = 0.0_f32;
            for &cluster_idx in &query_clusters {
                let cluster = CONCEPT_CLUSTERS[cluster_idx];
                let name_count = sym_name_tokens
                    .iter()
                    .filter(|t| cluster_matches_token(cluster, t))
                    .count() as f32;
                let path_count = path_tokens
                    .iter()
                    .filter(|t| cluster_matches_token(cluster, t))
                    .count() as f32;
                let sig_count = sig_tokens
                    .iter()
                    .filter(|t| cluster_matches_token(cluster, t))
                    .count() as f32;
                let doc_count = doc_tokens
                    .iter()
                    .filter(|t| cluster_matches_token(cluster, t))
                    .count() as f32;

                semantic_score += name_count * 15.0;
                semantic_score += path_count * 8.0;
                semantic_score += sig_count * 5.0;
                semantic_score += doc_count * 3.0;
            }
            score += semantic_score;

            if score > 0.0 {
                scores.push((symbol.id, score));
            }
        }

        if scores.is_empty() {
            return Vec::new();
        }

        // Sort descending by score
        scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        // Retain top_k
        scores.truncate(top_k);

        // Normalize weights relative to the maximum score so the top candidate has weight 1.0
        let max_score = scores[0].1.max(1e-5);
        scores
            .into_iter()
            .map(|(id, s)| (id, (s / max_score).clamp(0.05, 1.0)))
            .collect()
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
