//! Hybrid Lexical + Dense Semantic Query Retrieval Engine.
//!
//! Synthesizes statistically principled BM25+ lexical retrieval, in-engine subword
//! feature hashed dense embeddings with software domain concept taxonomy, Reciprocal
//! Rank Fusion (RRF), and Rocchio Pseudo-Relevance Feedback (PRF) query expansion.
//!
//! # Academic Foundations & References:
//! - **BM25+ Lexical Retrieval & Field Weighting**:
//!   - Robertson, S. E., & Zaragoza, H. (2009). *The Probabilistic Relevance Framework:
//!     BM25 and Beyond*. Foundations and Trends in Information Retrieval, 3(4), 333–389.
//!   - Lv, Y., & Zhai, C. (2011). *Lower Bounding the Length Normalization in BM25:
//!     An Alternative for Length Bias*. Proc. ACM CIKM '11, pp. 2005–2008.
//! - **Dense Subword Embeddings & Deterministic Feature Hashing**:
//!   - Bojanowski, P., Grave, E., Joulin, A., & Mikolov, T. (2017). *Enriching Word
//!     Vectors with Subword Information*. Transactions of the ACL (TACL), 5, 135–146.
//!   - Weinberger, K., Dasgupta, A., Langford, J., Smola, A., & Attenberg, J. (2009).
//!     *Feature Hashing for Large Scale Multitask Learning*. Proc. ICML '09, pp. 1113–1120.
//!   - Salton, G., & McGill, M. J. (1983). *Introduction to Modern Information Retrieval*.
//!     McGraw-Hill. (Cosine vector space model).
//! - **Reciprocal Rank Fusion (RRF)**:
//!   - Cormack, G. V., Clarke, C. L., & Büttcher, S. (2009). *Reciprocal Rank Fusion
//!     Outperforms Condorcet and Individual Rank Learning Methods*. Proc. ACM SIGIR '09, pp. 758–759.
//! - **Pseudo-Relevance Feedback (PRF) / Rocchio Query Expansion**:
//!   - Rocchio, J. J. (1971). *Relevance Feedback in Information Retrieval*. In G. Salton (Ed.),
//!     The SMART Retrieval System: Experiments in Automatic Document Processing, pp. 313–323.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::symbol::{SymbolId, SymbolKind, SymbolNode};

/// Search retrieval modality.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SearchMode {
    /// Fuses BM25+ lexical ranking and Dense semantic vector ranking via RRF (default).
    #[default]
    Hybrid,
    /// Pure BM25+ lexical and character n-gram ranking.
    LexicalOnly,
    /// Pure subword feature-hashed dense vector cosine similarity ranking.
    DenseOnly,
}

impl std::str::FromStr for SearchMode {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "hybrid" | "both" => Ok(Self::Hybrid),
            "lexical" | "lexical-only" | "bm25" => Ok(Self::LexicalOnly),
            "dense" | "dense-only" | "semantic" => Ok(Self::DenseOnly),
            other => Err(format!(
                "Invalid search mode '{}'. Supported: hybrid, lexical, dense",
                other
            )),
        }
    }
}

/// Configuration parameters for the hybrid retrieval engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetrievalConfig {
    /// Search modality (Hybrid, LexicalOnly, DenseOnly).
    pub mode: SearchMode,
    /// Maximum number of search results to return.
    pub top_k: usize,
    /// Reciprocal Rank Fusion smoothing parameter k (Cormack et al. 2009, default: 60).
    pub rrf_k: f32,
    /// Weight assigned to lexical BM25+ ranks in RRF (default: 1.0).
    pub lexical_weight: f32,
    /// Weight assigned to dense semantic ranks in RRF (default: 1.0).
    pub dense_weight: f32,
    /// BM25 term saturation parameter k1 (Robertson & Zaragoza 2009, default: 1.2).
    pub k1: f32,
    /// BM25 length normalization parameter b for symbol names (default: 0.5).
    pub b_name: f32,
    /// BM25 length normalization parameter b for signatures (default: 0.75).
    pub b_sig: f32,
    /// BM25 length normalization parameter b for file paths (default: 0.4).
    pub b_path: f32,
    /// BM25 length normalization parameter b for docstrings (default: 0.8).
    pub b_doc: f32,
    /// BM25+ lower-bound term frequency addition delta (Lv & Zhai 2011, default: 0.5).
    pub delta: f32,
    /// Whether to apply Rocchio pseudo-relevance feedback query expansion.
    pub query_expansion: bool,
    /// Number of top candidate documents to harvest expansion terms from.
    pub expansion_docs: usize,
    /// Maximum number of expansion terms to add.
    pub expansion_terms: usize,
    /// Weight of expansion terms relative to initial query terms.
    pub expansion_weight: f32,
}

impl Default for RetrievalConfig {
    fn default() -> Self {
        Self {
            mode: SearchMode::Hybrid,
            top_k: 10,
            rrf_k: 60.0,
            lexical_weight: 1.0,
            dense_weight: 1.0,
            k1: 1.2,
            b_name: 0.5,
            b_sig: 0.75,
            b_path: 0.4,
            b_doc: 0.8,
            delta: 0.5,
            query_expansion: false,
            expansion_docs: 3,
            expansion_terms: 4,
            expansion_weight: 0.5,
        }
    }
}

impl RetrievalConfig {
    /// Creates a hybrid retrieval configuration with custom top_k.
    pub fn hybrid(top_k: usize) -> Self {
        Self {
            mode: SearchMode::Hybrid,
            top_k,
            ..Default::default()
        }
    }

    /// Creates a lexical-only retrieval configuration.
    pub fn lexical_only(top_k: usize) -> Self {
        Self {
            mode: SearchMode::LexicalOnly,
            top_k,
            ..Default::default()
        }
    }

    /// Creates a dense-only semantic retrieval configuration.
    pub fn dense_only(top_k: usize) -> Self {
        Self {
            mode: SearchMode::DenseOnly,
            top_k,
            ..Default::default()
        }
    }

    /// Enables Rocchio pseudo-relevance feedback query expansion.
    pub fn with_expansion(mut self, enabled: bool) -> Self {
        self.query_expansion = enabled;
        self
    }
}

/// Detailed result of a query search.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    /// Unique symbol identifier.
    pub symbol_id: SymbolId,
    /// Identifier name of the matched symbol.
    pub symbol_name: String,
    /// Symbol syntactic category (Function, Struct, Trait, etc.).
    pub symbol_kind: SymbolKind,
    /// Source file declaring the symbol.
    pub file_path: PathBuf,
    /// Syntactic declaration signature.
    pub signature: String,
    /// Estimated token cost of the symbol declaration.
    pub token_cost: usize,
    /// Normalized overall relevance score in (0.0, 1.0].
    pub score: f32,
    /// Raw BM25+ lexical score.
    pub bm25_score: f32,
    /// Dense semantic cosine similarity in [-1.0, 1.0].
    pub dense_score: f32,
    /// Reciprocal Rank Fusion (RRF) score.
    pub rrf_score: f32,
    /// Matched query tokens and expansion terms found in the symbol.
    pub matched_terms: Vec<String>,
}

/// Polyglot identifier segmentation and text normalization.
pub struct PolyglotTokenizer;

/// Standard stopwords to suppress noise in natural language queries.
const STOPWORDS: &[&str] = &[
    "a",
    "about",
    "above",
    "after",
    "again",
    "all",
    "am",
    "an",
    "and",
    "any",
    "are",
    "as",
    "at",
    "be",
    "because",
    "been",
    "before",
    "being",
    "below",
    "between",
    "both",
    "but",
    "by",
    "could",
    "did",
    "do",
    "does",
    "doing",
    "down",
    "during",
    "each",
    "few",
    "for",
    "from",
    "further",
    "had",
    "has",
    "have",
    "having",
    "he",
    "her",
    "here",
    "hers",
    "herself",
    "him",
    "himself",
    "his",
    "how",
    "i",
    "if",
    "in",
    "into",
    "is",
    "it",
    "its",
    "itself",
    "just",
    "me",
    "more",
    "most",
    "my",
    "myself",
    "no",
    "nor",
    "not",
    "now",
    "of",
    "off",
    "on",
    "once",
    "only",
    "or",
    "other",
    "our",
    "ours",
    "ourselves",
    "out",
    "over",
    "own",
    "same",
    "she",
    "should",
    "so",
    "some",
    "such",
    "than",
    "that",
    "the",
    "their",
    "theirs",
    "them",
    "themselves",
    "then",
    "there",
    "these",
    "they",
    "this",
    "those",
    "through",
    "to",
    "too",
    "under",
    "until",
    "up",
    "very",
    "was",
    "we",
    "were",
    "what",
    "when",
    "where",
    "which",
    "while",
    "who",
    "whom",
    "why",
    "with",
    "would",
    "you",
    "your",
    "yours",
    "yourself",
    "yourselves",
];

impl PolyglotTokenizer {
    /// Tokenizes arbitrary source code identifiers and natural language query text.
    ///
    /// Handles:
    /// - CamelCase: `"ContextSelector"` -> `["context", "selector"]`
    /// - Snake_case: `"estimate_tokens"` -> `["estimate", "tokens"]`
    /// - Kebab-case: `"repotrim-engine"` -> `["repotrim", "engine"]`
    /// - Acronym Sequences: `"HTTPClient"` -> `["http", "client"]`, `"ASTVisitor"` -> `["ast", "visitor"]`
    /// - Alphanumeric Transitions: `"OAuth2Callback"` -> `["oauth", "2", "callback"]`, `"Sha256"` -> `["sha", "256"]`
    /// - Punctuation & Path delimiters: `"crates/engine/src/lib.rs"` -> `["crates", "engine", "src", "lib", "rs"]`
    pub fn tokenize(text: &str) -> Vec<String> {
        let mut tokens = Vec::new();
        let chars: Vec<char> = text.chars().collect();
        let n = chars.len();
        if n == 0 {
            return tokens;
        }

        let mut start = 0;
        while start < n {
            // Skip non-alphanumerics
            if !chars[start].is_alphanumeric() {
                start += 1;
                continue;
            }

            let mut end = start + 1;

            // Check if leading is digits
            if chars[start].is_ascii_digit() {
                while end < n && chars[end].is_ascii_digit() {
                    end += 1;
                }
                tokens.push(chars[start..end].iter().collect::<String>().to_lowercase());
                start = end;
                continue;
            }

            // Word segmentation with acronym sequence awareness
            while end < n && chars[end].is_alphanumeric() {
                let curr = chars[end];
                let prev = chars[end - 1];

                // Case 1: Transition from digit to alpha or vice-versa
                if prev.is_ascii_digit() != curr.is_ascii_digit() {
                    break;
                }

                // Case 2: lower/digit followed by Upper (standard camelCase: "fooBar")
                if prev.is_lowercase() && curr.is_uppercase() {
                    break;
                }

                // Case 3: Acronym sequence ending before lowercase: "HTTPClient" -> "HTTP", "Client"
                if prev.is_uppercase()
                    && curr.is_uppercase()
                    && (end - start >= 2)
                    && end + 1 < n
                    && chars[end + 1].is_lowercase()
                {
                    break;
                }

                end += 1;
            }

            let token: String = chars[start..end].iter().collect::<String>().to_lowercase();
            if !token.is_empty() {
                tokens.push(token);
            }
            start = end;
        }

        tokens
    }

    /// Tokenizes query text and filters out common non-code stopwords.
    pub fn tokenize_query_filtered(text: &str) -> Vec<String> {
        let stopword_set: HashSet<&str> = STOPWORDS.iter().copied().collect();
        Self::tokenize(text)
            .into_iter()
            .filter(|t| !stopword_set.contains(t.as_str()) && t.len() > 1)
            .collect()
    }

    /// Extracts character n-grams (3..=5) for morpho-syntactic fuzzy matching and subword hashing.
    pub fn char_ngrams(text: &str) -> HashSet<String> {
        let normalized = format!("<{}>", text.to_lowercase().trim());
        let chars: Vec<char> = normalized.chars().collect();
        let mut set = HashSet::new();

        for n in 3..=5 {
            if chars.len() >= n {
                for window in chars.windows(n) {
                    set.insert(window.iter().collect());
                }
            }
        }

        if set.is_empty() && !normalized.is_empty() {
            set.insert(normalized);
        }

        set
    }

    /// Computes Jaccard similarity between two character n-gram sets.
    pub fn ngram_jaccard(a: &HashSet<String>, b: &HashSet<String>) -> f32 {
        if a.is_empty() && b.is_empty() {
            return 1.0;
        }
        if a.is_empty() || b.is_empty() {
            return 0.0;
        }
        let intersection = a.intersection(b).count();
        let union = a.union(b).count();
        if union == 0 {
            0.0
        } else {
            intersection as f32 / union as f32
        }
    }
}

// -----------------------------------------------------------------------------
// 24-Axis Software Domain Concept Taxonomy
// -----------------------------------------------------------------------------

/// 24 Canonical Software Domain Concept Clusters mapping orthogonal semantic axes.
pub const SOFTWARE_TAXONOMY: &[(&str, &[&str])] = &[
    (
        "auth_security",
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
            "secret",
            "verify",
            "security",
            "secure",
            "access",
            "cert",
            "certificate",
            "role",
            "claims",
            "identity",
        ],
    ),
    (
        "database_storage",
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
            "postgres",
            "sqlite",
            "insert",
            "migration",
            "save",
            "storage",
            "record",
            "transaction",
            "crud",
            "schema",
            "column",
            "row",
            "orm",
        ],
    ),
    (
        "network_http",
        &[
            "http", "network", "request", "response", "client", "server", "endpoint", "api",
            "fetch", "route", "handler", "socket", "rpc", "rest", "gateway", "connect", "url",
            "webhook", "tcp", "udp", "ip", "port", "header", "payload",
        ],
    ),
    (
        "parser_ast",
        &[
            "parse",
            "ast",
            "syntax",
            "grammar",
            "token",
            "lexer",
            "visitor",
            "treesitter",
            "tree",
            "span",
            "traverse",
            "extract",
            "extractor",
            "node",
            "child",
            "parent",
            "statement",
            "expression",
            "declaration",
            "cursor",
            "query",
            "pattern",
        ],
    ),
    (
        "graph_topology",
        &[
            "graph",
            "node",
            "edge",
            "ppr",
            "pagerank",
            "modularity",
            "louvain",
            "celf",
            "csr",
            "adjacency",
            "community",
            "degree",
            "centrality",
            "diffusion",
            "walk",
            "teleport",
            "submodular",
            "partition",
            "cluster",
            "directed",
            "symmetrize",
        ],
    ),
    (
        "concurrency_async",
        &[
            "thread",
            "async",
            "await",
            "future",
            "spawn",
            "mutex",
            "channel",
            "pool",
            "worker",
            "lock",
            "atomic",
            "sync",
            "tokio",
            "task",
            "daemon",
            "queue",
            "event",
            "poll",
            "schedule",
            "routine",
            "concurrent",
            "parallel",
            "race",
        ],
    ),
    (
        "config_env",
        &[
            "config",
            "configuration",
            "setting",
            "env",
            "environment",
            "flag",
            "option",
            "param",
            "parameter",
            "toml",
            "yaml",
            "json",
            "properties",
            "variable",
            "profile",
            "default",
            "override",
        ],
    ),
    (
        "logging_diagnostics",
        &[
            "log",
            "logger",
            "warn",
            "warning",
            "error",
            "trace",
            "metric",
            "telemetry",
            "diagnostic",
            "report",
            "panic",
            "debug",
            "info",
            "stats",
            "inspect",
            "health",
            "benchmark",
            "monitor",
        ],
    ),
    (
        "memory_allocation",
        &[
            "alloc",
            "allocation",
            "buffer",
            "pointer",
            "ptr",
            "slice",
            "heap",
            "stack",
            "byte",
            "memory",
            "mem",
            "arena",
            "leak",
            "capacity",
            "dealloc",
            "free",
            "layout",
            "size",
        ],
    ),
    (
        "testing_mock",
        &[
            "test",
            "assert",
            "mock",
            "suite",
            "fixture",
            "stub",
            "criterion",
            "verify",
            "proptest",
            "spec",
            "case",
            "expectation",
            "runner",
            "check",
            "golden",
        ],
    ),
    (
        "crypto_hashing",
        &[
            "blake3",
            "sha",
            "hash",
            "cipher",
            "key",
            "sign",
            "signature",
            "hmac",
            "nonce",
            "digest",
            "encrypt",
            "decrypt",
            "rsa",
            "aes",
            "ed25519",
            "hasher",
            "merkle",
        ],
    ),
    (
        "serialization_codec",
        &[
            "serialize",
            "deserialize",
            "bincode",
            "serde",
            "proto",
            "codec",
            "encode",
            "decode",
            "marshal",
            "unmarshal",
            "binary",
            "text",
            "writer",
            "reader",
        ],
    ),
    (
        "routing_dispatch",
        &[
            "route",
            "router",
            "dispatch",
            "dispatcher",
            "match",
            "middleware",
            "mux",
            "pattern",
            "url",
            "path",
            "intercept",
            "forward",
            "redirect",
            "controller",
        ],
    ),
    (
        "caching_memoize",
        &[
            "cache",
            "memoize",
            "ttl",
            "lru",
            "evict",
            "eviction",
            "invalidate",
            "invalidation",
            "stale",
            "hit",
            "miss",
            "warm",
            "cold",
            "storage",
        ],
    ),
    (
        "program_slicing_cfg",
        &[
            "slice",
            "slicer",
            "cfg",
            "dataflow",
            "controlflow",
            "reaching",
            "dominate",
            "def",
            "use",
            "skeleton",
            "prune",
            "statement",
            "branch",
            "phi",
            "ssa",
            "dependency",
        ],
    ),
    (
        "cli_terminal",
        &[
            "cli",
            "arg",
            "flag",
            "command",
            "subcmd",
            "subcommand",
            "clap",
            "ansi",
            "color",
            "terminal",
            "stdin",
            "stdout",
            "stderr",
            "print",
            "prompt",
            "shell",
            "console",
        ],
    ),
    (
        "blueprint_harness",
        &[
            "blueprint",
            "skeleton",
            "signature",
            "outline",
            "template",
            "lod",
            "prompt",
            "harness",
            "agent",
            "context",
            "trimmer",
            "anchor",
            "seed",
            "budget",
            "token",
        ],
    ),
    (
        "perf_optimization",
        &[
            "optimize",
            "optimization",
            "perf",
            "performance",
            "fast",
            "latency",
            "throughput",
            "bottleneck",
            "speed",
            "efficient",
            "simd",
            "zero_copy",
        ],
    ),
    (
        "diff_vcs",
        &[
            "diff",
            "patch",
            "commit",
            "vcs",
            "git",
            "changeset",
            "hunk",
            "blame",
            "coedit",
            "merge",
            "rebase",
            "branch",
            "history",
            "log",
        ],
    ),
    (
        "pipeline_flow",
        &[
            "pipeline", "step", "stage", "flow", "job", "task", "batch", "runner", "stream",
            "consumer", "producer", "pipe", "source", "sink", "filter",
        ],
    ),
    (
        "os_filesystem",
        &[
            "file",
            "dir",
            "directory",
            "path",
            "io",
            "read",
            "write",
            "fs",
            "filesystem",
            "watcher",
            "notify",
            "metadata",
            "symlink",
            "stat",
            "open",
            "close",
        ],
    ),
    (
        "protocols_rpc",
        &[
            "jsonrpc",
            "mcp",
            "protocol",
            "wire",
            "message",
            "frame",
            "packet",
            "handshake",
            "notification",
            "call",
            "method",
            "transport",
            "stdio",
        ],
    ),
    (
        "search_ir",
        &[
            "search",
            "bm25",
            "query",
            "tfidf",
            "retrieve",
            "retrieval",
            "rank",
            "ranking",
            "recall",
            "precision",
            "cosine",
            "vector",
            "embedding",
            "hybrid",
            "rrf",
            "rocchio",
        ],
    ),
    (
        "tokenizer_nlp",
        &[
            "bpe",
            "tokenizer",
            "subword",
            "cl100k",
            "o200k",
            "vocabulary",
            "vocab",
            "encoding",
            "token_count",
            "estimate",
            "kneedle",
            "knee",
        ],
    ),
];

// -----------------------------------------------------------------------------
// Dense Subword Feature Hashed Embedder
// -----------------------------------------------------------------------------

/// 128-dimensional dense semantic vector embedding.
pub const EMBEDDING_DIM: usize = 128;
const HASH_DIM: usize = 104;

/// Computes a fast deterministic 64-bit hash (FNV-1a) for subword feature hashing.
#[inline]
fn fnv1a_64(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf29ce484222325_u64;
    for &b in bytes {
        hash ^= b as u64;
        hash = hash.wrapping_mul(0x100000001b3_u64);
    }
    hash
}

/// Zero-dependency in-engine dense semantic embedding model.
///
/// Combines:
/// 1. Subword character n-gram feature hashing into a 104-dimensional subspace
///    (Bojanowski et al. 2017 FastText; Weinberger et al. 2009).
/// 2. Projections onto the 24-axis Software Domain Concept Taxonomy.
///
/// Yields an L2-normalized 128-dimensional dense vector supporting cosine similarity.
#[derive(Debug, Clone)]
pub struct DenseEmbedder;

impl DenseEmbedder {
    /// Computes the 128-dimensional normalized dense embedding vector for any text.
    pub fn embed(text: &str) -> [f32; EMBEDDING_DIM] {
        let mut vec = [0.0_f32; EMBEDDING_DIM];
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return vec;
        }

        // 1. Subword n-gram hashing into the first 104 dimensions
        let ngrams = PolyglotTokenizer::char_ngrams(trimmed);
        for ng in &ngrams {
            let h = fnv1a_64(ng.as_bytes());
            let dim = (h as usize) % HASH_DIM;
            // Sign-consistent pseudo-random projection (+1 or -1)
            let sign = if (h >> 32) & 1 == 1 {
                1.0_f32
            } else {
                -1.0_f32
            };
            vec[dim] += sign;
        }

        // 2. 24-axis Software Taxonomy projection into the last 24 dimensions
        let tokens = PolyglotTokenizer::tokenize(trimmed);
        let token_set: HashSet<String> = tokens.into_iter().collect();

        for (idx, &(_axis_name, keywords)) in SOFTWARE_TAXONOMY.iter().enumerate() {
            let mut axis_score = 0.0_f32;
            for &kw in keywords {
                if token_set.contains(kw) {
                    axis_score += 2.0;
                } else {
                    // Check subword prefixes/suffixes
                    for tok in &token_set {
                        if tok.len() >= 4
                            && kw.len() >= 4
                            && (tok.starts_with(kw) || kw.starts_with(tok))
                        {
                            axis_score += 1.0;
                        }
                    }
                }
            }
            vec[HASH_DIM + idx] = axis_score * 2.5;
        }

        // 3. L2 Normalization
        let norm_sq: f32 = vec.iter().map(|&x| x * x).sum();
        if norm_sq > 1e-9 {
            let inv_norm = 1.0 / norm_sq.sqrt();
            for x in &mut vec {
                *x *= inv_norm;
            }
        }

        vec
    }

    /// Computes cosine similarity between two 128-dimensional normalized embedding vectors.
    #[inline]
    pub fn cosine_similarity(a: &[f32; EMBEDDING_DIM], b: &[f32; EMBEDDING_DIM]) -> f32 {
        let mut dot = 0.0_f32;
        for i in 0..EMBEDDING_DIM {
            dot += a[i] * b[i];
        }
        dot.clamp(-1.0, 1.0)
    }

    /// Embeds a symbol node across its name, file path, signature, and docstring.
    pub fn embed_symbol(symbol: &SymbolNode) -> [f32; EMBEDDING_DIM] {
        let doc = symbol.docstring.as_deref().unwrap_or("");
        let combined = format!(
            "{} {} {} {}",
            symbol.name,
            symbol.file_path.display(),
            symbol.signature,
            doc
        );
        Self::embed(&combined)
    }
}

// -----------------------------------------------------------------------------
// Field-Weighted BM25+ Lexical Scorer
// -----------------------------------------------------------------------------

/// Precomputed document length statistics for multi-field BM25+.
#[derive(Debug, Clone)]
struct CorpusStats {
    total_docs: usize,
    avgdl_name: f32,
    avgdl_sig: f32,
    avgdl_path: f32,
    avgdl_doc: f32,
}

/// Tokenized representation of a Symbol for fast BM25+ scoring.
#[derive(Debug, Clone)]
struct SymbolDoc {
    id: SymbolId,
    name_tokens: Vec<String>,
    sig_tokens: Vec<String>,
    path_tokens: Vec<String>,
    doc_tokens: Vec<String>,
    name_ngrams: HashSet<String>,
}

/// Multi-field BM25+ implementation with Robertson-Zaragoza IDF and length normalization.
pub struct Bm25Scorer {
    stats: CorpusStats,
    docs: Vec<SymbolDoc>,
    doc_freqs: HashMap<String, usize>,
}

impl Bm25Scorer {
    /// Builds an in-memory BM25 index over the provided symbols.
    pub fn new(symbols: &[SymbolNode]) -> Self {
        let total_docs = symbols.len();
        let mut docs = Vec::with_capacity(total_docs);
        let mut doc_freqs: HashMap<String, usize> = HashMap::new();

        let mut sum_name = 0usize;
        let mut sum_sig = 0usize;
        let mut sum_path = 0usize;
        let mut sum_doc = 0usize;

        for s in symbols {
            let name_tokens = PolyglotTokenizer::tokenize(&s.name);
            let sig_tokens = PolyglotTokenizer::tokenize(&s.signature);
            let path_tokens = PolyglotTokenizer::tokenize(&s.file_path.to_string_lossy());
            let doc_tokens = s
                .docstring
                .as_deref()
                .map(PolyglotTokenizer::tokenize)
                .unwrap_or_default();

            sum_name += name_tokens.len();
            sum_sig += sig_tokens.len();
            sum_path += path_tokens.len();
            sum_doc += doc_tokens.len();

            let mut all_tokens: HashSet<String> = HashSet::new();
            all_tokens.extend(name_tokens.iter().cloned());
            all_tokens.extend(sig_tokens.iter().cloned());
            all_tokens.extend(path_tokens.iter().cloned());
            all_tokens.extend(doc_tokens.iter().cloned());

            for tok in &all_tokens {
                *doc_freqs.entry(tok.clone()).or_insert(0) += 1;
            }

            let name_ngrams = PolyglotTokenizer::char_ngrams(&s.name);

            docs.push(SymbolDoc {
                id: s.id,
                name_tokens,
                sig_tokens,
                path_tokens,
                doc_tokens,
                name_ngrams,
            });
        }

        let n = total_docs.max(1) as f32;
        let stats = CorpusStats {
            total_docs,
            avgdl_name: sum_name as f32 / n,
            avgdl_sig: sum_sig as f32 / n,
            avgdl_path: sum_path as f32 / n,
            avgdl_doc: sum_doc as f32 / n,
        };

        Self {
            stats,
            docs,
            doc_freqs,
        }
    }

    /// Scores all documents against query tokens and returns raw BM25+ scores.
    pub fn score_query(
        &self,
        query: &str,
        query_tokens: &[String],
        config: &RetrievalConfig,
    ) -> Vec<(SymbolId, f32, Vec<String>)> {
        let n_docs = self.stats.total_docs as f32;
        let query_lower = query.trim().to_lowercase();
        let query_ngrams = PolyglotTokenizer::char_ngrams(&query_lower);

        let mut results = Vec::with_capacity(self.docs.len());

        for doc in &self.docs {
            let mut bm25_sum = 0.0_f32;
            let mut matched_terms = Vec::new();

            for t in query_tokens {
                let df = self.doc_freqs.get(t).copied().unwrap_or(0);
                if df == 0 {
                    continue;
                }

                // Robertson-Sparck Jones IDF with smoothing: ln(1 + (N - df + 0.5) / (df + 0.5))
                let idf = ((n_docs - df as f32 + 0.5) / (df as f32 + 0.5) + 1.0)
                    .ln()
                    .max(0.15);

                // Multi-field term count
                let tf_name = doc.name_tokens.iter().filter(|&x| x == t).count() as f32;
                let tf_sig = doc.sig_tokens.iter().filter(|&x| x == t).count() as f32;
                let tf_path = doc.path_tokens.iter().filter(|&x| x == t).count() as f32;
                let tf_doc = doc.doc_tokens.iter().filter(|&x| x == t).count() as f32;

                if tf_name + tf_sig + tf_path + tf_doc == 0.0 {
                    continue;
                }

                matched_terms.push(t.clone());

                // Field length normalization
                let norm_name = 1.0 - config.b_name
                    + config.b_name
                        * (doc.name_tokens.len() as f32 / self.stats.avgdl_name.max(1.0));
                let norm_sig = 1.0 - config.b_sig
                    + config.b_sig * (doc.sig_tokens.len() as f32 / self.stats.avgdl_sig.max(1.0));
                let norm_path = 1.0 - config.b_path
                    + config.b_path
                        * (doc.path_tokens.len() as f32 / self.stats.avgdl_path.max(1.0));
                let norm_doc = 1.0 - config.b_doc
                    + config.b_doc * (doc.doc_tokens.len() as f32 / self.stats.avgdl_doc.max(1.0));

                // Field-weighted normalized TF: Zaragoza et al. (2004) BM25F
                let weighted_tf = 4.0 * (tf_name / norm_name.max(0.2))
                    + 2.0 * (tf_sig / norm_sig.max(0.2))
                    + 1.5 * (tf_path / norm_path.max(0.2))
                    + 1.0 * (tf_doc / norm_doc.max(0.2));

                // BM25+ formula: idf * [ (tf * (k1 + 1)) / (tf + k1) + delta ]
                let term_score = idf
                    * ((weighted_tf * (config.k1 + 1.0)) / (weighted_tf + config.k1)
                        + config.delta);
                bm25_sum += term_score;
            }

            // Exact match anchor bonus
            let full_name = doc.name_tokens.join("");
            let snake_name = doc.name_tokens.join("_");
            if full_name == query_lower || snake_name == query_lower {
                bm25_sum += 40.0;
            } else if doc.name_tokens.iter().any(|t| t == &query_lower) {
                bm25_sum += 20.0;
            }

            // Character n-gram fuzzy similarity bonus for typo resilience
            let sim = PolyglotTokenizer::ngram_jaccard(&query_ngrams, &doc.name_ngrams);
            if sim > 0.35 {
                bm25_sum += sim * 15.0;
            }

            if bm25_sum > 0.0 {
                matched_terms.sort();
                matched_terms.dedup();
                results.push((doc.id, bm25_sum, matched_terms));
            }
        }

        results
    }
}

// -----------------------------------------------------------------------------
// Rocchio Pseudo-Relevance Feedback (PRF)
// -----------------------------------------------------------------------------

/// Implements Rocchio query expansion based on initial top-ranked feedback documents.
pub struct RocchioExpander;

impl RocchioExpander {
    /// Extracts discriminative candidate expansion terms from top feedback documents.
    pub fn expand_query(
        initial_tokens: &[String],
        feedback_docs: &[&SymbolNode],
        max_terms: usize,
    ) -> Vec<String> {
        let initial_set: HashSet<&str> = initial_tokens.iter().map(|s| s.as_str()).collect();
        let stopword_set: HashSet<&str> = STOPWORDS.iter().copied().collect();

        let mut term_scores: HashMap<String, f32> = HashMap::new();

        for (rank, doc) in feedback_docs.iter().enumerate() {
            let discount = 1.0 / ((rank + 1) as f32).sqrt();

            let doc_text = format!(
                "{} {} {}",
                doc.name,
                doc.signature,
                doc.docstring.as_deref().unwrap_or("")
            );
            let tokens = PolyglotTokenizer::tokenize(&doc_text);

            for tok in tokens {
                if tok.len() < 3
                    || initial_set.contains(tok.as_str())
                    || stopword_set.contains(tok.as_str())
                {
                    continue;
                }
                *term_scores.entry(tok).or_insert(0.0) += discount;
            }
        }

        let mut sorted_terms: Vec<(String, f32)> = term_scores.into_iter().collect();
        sorted_terms.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        sorted_terms
            .into_iter()
            .take(max_terms)
            .map(|(t, _)| t)
            .collect()
    }
}

// -----------------------------------------------------------------------------
// Main Unified Hybrid Retriever
// -----------------------------------------------------------------------------

/// Unified Hybrid Lexical and Dense Semantic Information Retrieval engine.
#[derive(Debug, Default)]
pub struct HybridRetriever {
    config: RetrievalConfig,
}

impl HybridRetriever {
    /// Creates a new retriever with specified configuration.
    pub fn with_config(config: RetrievalConfig) -> Self {
        Self { config }
    }

    /// Executes query retrieval across the provided corpus of code symbols.
    pub fn search(&self, symbols: &[SymbolNode], query: &str) -> Vec<SearchResult> {
        let query_trimmed = query.trim();
        if query_trimmed.is_empty() || symbols.is_empty() || self.config.top_k == 0 {
            return Vec::new();
        }

        let mut query_tokens = PolyglotTokenizer::tokenize_query_filtered(query_trimmed);
        if query_tokens.is_empty() {
            query_tokens = PolyglotTokenizer::tokenize(query_trimmed);
        }
        if query_tokens.is_empty() {
            return Vec::new();
        }

        let sym_map: HashMap<SymbolId, &SymbolNode> = symbols.iter().map(|s| (s.id, s)).collect();

        // Pass 1: Build BM25 index and compute Lexical scores
        let bm25_scorer = Bm25Scorer::new(symbols);
        let mut lexical_results =
            bm25_scorer.score_query(query_trimmed, &query_tokens, &self.config);

        // Optional: Rocchio Pseudo-Relevance Feedback expansion
        if self.config.query_expansion && !lexical_results.is_empty() {
            lexical_results
                .sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
            let feedback_docs: Vec<&SymbolNode> = lexical_results
                .iter()
                .take(self.config.expansion_docs)
                .filter_map(|(id, _, _)| sym_map.get(id).copied())
                .collect();

            let expanded_terms = RocchioExpander::expand_query(
                &query_tokens,
                &feedback_docs,
                self.config.expansion_terms,
            );

            if !expanded_terms.is_empty() {
                let mut combined_tokens = query_tokens.clone();
                combined_tokens.extend(expanded_terms);
                // Re-score with expanded query
                lexical_results =
                    bm25_scorer.score_query(query_trimmed, &combined_tokens, &self.config);
            }
        }

        // Precompute Dense embeddings if needed
        let need_dense = matches!(self.config.mode, SearchMode::Hybrid | SearchMode::DenseOnly);
        let query_dense = if need_dense {
            Some(DenseEmbedder::embed(query_trimmed))
        } else {
            None
        };

        let mut dense_scores: HashMap<SymbolId, f32> = HashMap::new();
        if let Some(ref q_vec) = query_dense {
            for s in symbols {
                let sym_vec = DenseEmbedder::embed_symbol(s);
                let sim = DenseEmbedder::cosine_similarity(q_vec, &sym_vec);
                if sim > 0.05 {
                    dense_scores.insert(s.id, sim);
                }
            }
        }

        // Map lexical scores and matched terms
        let mut lexical_scores: HashMap<SymbolId, f32> = HashMap::new();
        let mut term_matches: HashMap<SymbolId, Vec<String>> = HashMap::new();
        for (id, score, terms) in lexical_results {
            lexical_scores.insert(id, score);
            term_matches.insert(id, terms);
        }

        // Rank lists for Reciprocal Rank Fusion
        let mut lex_ranked: Vec<(SymbolId, f32)> =
            lexical_scores.iter().map(|(&k, &v)| (k, v)).collect();
        lex_ranked.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        let lex_rank_map: HashMap<SymbolId, usize> = lex_ranked
            .iter()
            .enumerate()
            .map(|(rank, &(id, _))| (id, rank + 1))
            .collect();

        let mut dense_ranked: Vec<(SymbolId, f32)> =
            dense_scores.iter().map(|(&k, &v)| (k, v)).collect();
        dense_ranked.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        let dense_rank_map: HashMap<SymbolId, usize> = dense_ranked
            .iter()
            .enumerate()
            .map(|(rank, &(id, _))| (id, rank + 1))
            .collect();

        // Candidates: Union of lexical and dense hits
        let mut candidates: HashSet<SymbolId> = HashSet::new();
        match self.config.mode {
            SearchMode::Hybrid => {
                candidates.extend(lexical_scores.keys());
                candidates.extend(dense_scores.keys());
            }
            SearchMode::LexicalOnly => {
                candidates.extend(lexical_scores.keys());
            }
            SearchMode::DenseOnly => {
                candidates.extend(dense_scores.keys());
            }
        }

        if candidates.is_empty() {
            return Vec::new();
        }

        let mut results = Vec::with_capacity(candidates.len());
        let k = self.config.rrf_k;

        for id in candidates {
            let sym = match sym_map.get(&id) {
                Some(&s) => s,
                None => continue,
            };

            let bm25_val = lexical_scores.get(&id).copied().unwrap_or(0.0);
            let dense_val = dense_scores.get(&id).copied().unwrap_or(0.0);

            // Compute RRF score
            let mut rrf = 0.0_f32;
            if let Some(&r_lex) = lex_rank_map.get(&id) {
                rrf += self.config.lexical_weight / (k + r_lex as f32);
            }
            if let Some(&r_dense) = dense_rank_map.get(&id) {
                rrf += self.config.dense_weight / (k + r_dense as f32);
            }

            // Determine final composite score based on mode
            let base_score = match self.config.mode {
                SearchMode::Hybrid => rrf,
                SearchMode::LexicalOnly => bm25_val,
                SearchMode::DenseOnly => dense_val.max(0.0),
            };

            // Exact identifier match boost and test symbol demotion
            let is_exact_name = sym.name.eq_ignore_ascii_case(query_trimmed)
                || sym.name.to_lowercase() == query_trimmed.to_lowercase().replace('_', "");
            let is_test = sym.name.starts_with("test_")
                || sym.file_path.to_string_lossy().contains("tests/")
                || sym.file_path.to_string_lossy().contains("tests\\");
            let query_has_test = query_tokens.iter().any(|t| t == "test");

            let exact_bonus = if is_exact_name { 2.5_f32 } else { 1.0_f32 };
            let test_penalty = if is_test && !query_has_test && !is_exact_name {
                0.7_f32
            } else {
                1.0_f32
            };
            let final_score = base_score * exact_bonus * test_penalty;

            let matches = term_matches.remove(&id).unwrap_or_default();

            results.push(SearchResult {
                symbol_id: id,
                symbol_name: sym.name.clone(),
                symbol_kind: sym.kind,
                file_path: sym.file_path.clone(),
                signature: sym.signature.clone(),
                token_cost: sym.token_cost,
                score: final_score,
                bm25_score: bm25_val,
                dense_score: dense_val,
                rrf_score: rrf,
                matched_terms: matches,
            });
        }

        // Sort descending by composite score
        results.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        results.truncate(self.config.top_k);

        // Normalize scores to (0.05, 1.0] for downstream consumer calibration
        if !results.is_empty() {
            let max_s = results[0].score.max(1e-6);
            for r in &mut results {
                r.score = (r.score / max_s).clamp(0.05, 1.0);
            }
        }

        results
    }
}
