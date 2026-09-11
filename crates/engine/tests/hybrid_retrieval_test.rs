use std::path::{Path, PathBuf};

use repotrim_engine::{
    DenseEmbedder, HybridRetriever, LoadedRepository, PolyglotTokenizer, RetrievalConfig,
    RocchioExpander, SearchMode, SymbolId, SymbolKind, SymbolNode, TextSpan,
};

fn make_dummy_symbol(
    id: u32,
    name: &str,
    file_path: &str,
    sig: &str,
    doc: Option<&str>,
    token_cost: usize,
) -> SymbolNode {
    SymbolNode {
        id: SymbolId(id),
        name: name.to_string(),
        kind: SymbolKind::Function,
        file_path: PathBuf::from(file_path),
        span: TextSpan::new(1, 10, 1, 1),
        signature: sig.to_string(),
        docstring: doc.map(|d| d.to_string()),
        token_cost,
        ast_hash: [0u8; 32],
        container_name: None,
        trait_name: None,
    }
}

#[test]
fn test_polyglot_acronym_and_camelcase_tokenization() {
    // 1. CamelCase and PascalCase
    let tokens1 = PolyglotTokenizer::tokenize("ContextSelector");
    assert_eq!(tokens1, vec!["context", "selector"]);

    // 2. Acronym Sequences
    let tokens2 = PolyglotTokenizer::tokenize("HTTPClientHandler");
    assert_eq!(tokens2, vec!["http", "client", "handler"]);

    let tokens3 = PolyglotTokenizer::tokenize("parseASTTree");
    assert_eq!(tokens3, vec!["parse", "ast", "tree"]);

    // 3. Alphanumeric Transitions
    let tokens4 = PolyglotTokenizer::tokenize("OAuth2Callback");
    assert_eq!(tokens4, vec!["oauth", "2", "callback"]);

    let tokens5 = PolyglotTokenizer::tokenize("sha256_digest");
    assert_eq!(tokens5, vec!["sha", "256", "digest"]);

    // 4. Snake and Kebab Case
    let tokens6 = PolyglotTokenizer::tokenize("estimate_tokens_bpe-fast");
    assert_eq!(tokens6, vec!["estimate", "tokens", "bpe", "fast"]);
}

#[test]
fn test_bm25_plus_length_normalization() {
    let s_short = make_dummy_symbol(
        0,
        "auth_verify",
        "src/auth.rs",
        "fn auth_verify()",
        Some("Verifies auth token."),
        10,
    );

    let s_long = make_dummy_symbol(
        1,
        "complex_auth_pipeline_verification_manager",
        "src/enterprise/auth/pipeline/verification.rs",
        "fn complex_auth_pipeline_verification_manager(ctx: &Context, cfg: &Config, payload: &[u8], timeout: Duration) -> Result<VerificationOutcome, EnterpriseAuthError>",
        Some("Comprehensive verification manager that coordinates OAuth2, JWT, Kerberos, and internal authentication checks across distributed microservices with fallback telemetry and audit logging."),
        200,
    );

    let symbols = vec![s_short, s_long];
    let config = RetrievalConfig::lexical_only(2);
    let retriever = HybridRetriever::with_config(config);

    // Query "auth verify" should match both, with BM25+ rewarding exact concise matches while not zeroing long docs
    let results = retriever.search(&symbols, "auth verify");
    assert_eq!(results.len(), 2);
    assert_eq!(results[0].symbol_id, SymbolId(0)); // Concise match ranks highest
    assert!(results[0].score >= results[1].score);
    assert!(results[1].bm25_score > 0.0); // BM25+ delta ensures long doc still scores well
}

#[test]
fn test_dense_subword_embedding_cosine_similarity() {
    // Morphologically & Conceptually related
    let v_auth1 = DenseEmbedder::embed("authenticate_user jwt token");
    let v_auth2 = DenseEmbedder::embed("login verify credential password");
    let v_db = DenseEmbedder::embed("postgres database sql query table insert");

    let sim_auth = DenseEmbedder::cosine_similarity(&v_auth1, &v_auth2);
    let sim_cross = DenseEmbedder::cosine_similarity(&v_auth1, &v_db);

    assert!(
        sim_auth > 0.40,
        "Auth queries should have high similarity, got {}",
        sim_auth
    );
    assert!(
        sim_auth > sim_cross,
        "Auth-Auth similarity ({}) should exceed Auth-DB similarity ({})",
        sim_auth,
        sim_cross
    );
}

#[test]
fn test_reciprocal_rank_fusion_properties() {
    let s0 = make_dummy_symbol(0, "save_user", "src/db.rs", "fn save_user()", None, 15);
    let s1 = make_dummy_symbol(
        1,
        "persist_record",
        "src/db.rs",
        "fn persist_record()",
        Some("Writes user record into database storage."),
        25,
    );
    let s2 = make_dummy_symbol(2, "logger", "src/log.rs", "fn logger()", None, 10);

    let symbols = vec![s0, s1, s2];

    // Hybrid mode fuses BM25 (matching "user") and Dense (matching "database storage persistence")
    let config = RetrievalConfig::hybrid(3);
    let retriever = HybridRetriever::with_config(config);

    let results = retriever.search(&symbols, "save user to database");
    assert!(!results.is_empty());
    assert!(results[0].rrf_score > 0.0);
    assert_eq!(results[0].score, 1.0); // Normalized to 1.0
}

#[test]
fn test_rocchio_pseudo_relevance_feedback() {
    let s0 = make_dummy_symbol(
        0,
        "jwt_validator",
        "src/auth/jwt.rs",
        "fn jwt_validator(token: &str) -> bool",
        Some("Decodes signature and validates claims."),
        30,
    );
    let s1 = make_dummy_symbol(
        1,
        "verify_signature",
        "src/crypto/sig.rs",
        "fn verify_signature()",
        Some("Cryptographic signature verification."),
        20,
    );
    let s2 = make_dummy_symbol(2, "file_reader", "src/fs.rs", "fn file_reader()", None, 10);

    let initial_tokens = vec!["jwt".to_string(), "token".to_string()];
    let feedback = vec![&s0];

    // Rocchio expansion should harvest terms like "signature", "claims", "validator"
    let expanded = RocchioExpander::expand_query(&initial_tokens, &feedback, 3);
    assert!(!expanded.is_empty());
    assert!(
        expanded.contains(&"signature".to_string())
            || expanded.contains(&"claims".to_string())
            || expanded.contains(&"validator".to_string())
    );

    // End-to-end search with Rocchio PRF enabled
    let symbols = vec![s0, s1, s2];
    let config = RetrievalConfig::hybrid(2).with_expansion(true);
    let retriever = HybridRetriever::with_config(config);

    let results = retriever.search(&symbols, "jwt token");
    assert!(!results.is_empty());
    assert_eq!(results[0].symbol_id, SymbolId(0));
}

#[test]
fn test_search_modes_hybrid_lexical_dense() {
    let s_literal = make_dummy_symbol(
        0,
        "exact_keyword_matcher",
        "src/exact.rs",
        "fn exact_keyword_matcher()",
        None,
        15,
    );
    let s_conceptual = make_dummy_symbol(
        1,
        "crypto_key_derivation",
        "src/crypto.rs",
        "fn crypto_key_derivation()",
        Some("Derives secret session credentials using argon2."),
        35,
    );

    let symbols = vec![s_literal, s_conceptual];

    // 1. Lexical Only: Searching "exact keyword"
    let lex_config = RetrievalConfig::lexical_only(2);
    assert_eq!(lex_config.mode, SearchMode::LexicalOnly);
    let lex_retriever = HybridRetriever::with_config(lex_config);
    let lex_res = lex_retriever.search(&symbols, "exact keyword");
    assert_eq!(lex_res[0].symbol_id, SymbolId(0));

    // 2. Dense Only: Searching "password authentication security"
    let dense_config = RetrievalConfig::dense_only(2);
    assert_eq!(dense_config.mode, SearchMode::DenseOnly);
    let dense_retriever = HybridRetriever::with_config(dense_config);
    let dense_res = dense_retriever.search(&symbols, "password authentication security");
    assert_eq!(dense_res[0].symbol_id, SymbolId(1));

    // 3. Hybrid Mode
    let hybrid_config = RetrievalConfig::hybrid(2);
    assert_eq!(hybrid_config.mode, SearchMode::Hybrid);
    let hybrid_retriever = HybridRetriever::with_config(hybrid_config);
    let hybrid_res = hybrid_retriever.search(&symbols, "crypto key derivation");
    assert_eq!(hybrid_res[0].symbol_id, SymbolId(1));
}

#[test]
fn test_hybrid_retrieval_dogfood_on_workspace() {
    let mut repo =
        LoadedRepository::load_with_options(Path::new("."), true).expect("Failed to load repo");
    repo.load_all_sources().expect("Failed to load sources");

    let retriever = HybridRetriever::default();

    // Query 1: Exact / technical identifier
    let res1 = retriever.search(&repo.symbols, "ContextSelector");
    assert!(!res1.is_empty(), "Should find ContextSelector");
    assert_eq!(res1[0].symbol_name, "ContextSelector");

    // Query 2: Token estimation / BPE
    let res2 = retriever.search(&repo.symbols, "estimate tokens BPE");
    assert!(!res2.is_empty(), "Should find token estimation utilities");
    assert!(res2
        .iter()
        .any(|r| r.symbol_name.contains("token") || r.symbol_name.contains("estimate")));

    // Query 3: Multi-word software concept
    let res3 = retriever.search(&repo.symbols, "community modularity resolution");
    assert!(
        !res3.is_empty(),
        "Should find community detection utilities"
    );
    assert!(res3.iter().any(|r| r.symbol_name.contains("Community")));
}
