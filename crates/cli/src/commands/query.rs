use clap::{Args, ValueEnum};
use colored::Colorize;
use repotrim_engine::{HybridRetriever, RetrievalConfig, SearchMode, SearchResult};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::Instant;

use crate::loader::LoadedRepository;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QueryModeCli {
    Hybrid,
    Lexical,
    Dense,
}

impl From<QueryModeCli> for SearchMode {
    fn from(mode: QueryModeCli) -> Self {
        match mode {
            QueryModeCli::Hybrid => SearchMode::Hybrid,
            QueryModeCli::Lexical => SearchMode::LexicalOnly,
            QueryModeCli::Dense => SearchMode::DenseOnly,
        }
    }
}

#[derive(Args, Debug)]
pub struct QueryArgs {
    /// Natural language query, symbol name, or search description
    pub query: String,

    /// Maximum number of search results to return
    #[arg(short = 'n', long = "limit", default_value = "10")]
    pub limit: usize,

    /// Retrieval modality: hybrid (BM25+ and Dense RRF), lexical (BM25+), or dense (subword embeddings)
    #[arg(short = 'm', long = "mode", value_enum, default_value_t = QueryModeCli::Hybrid)]
    pub mode: QueryModeCli,

    /// Enable Rocchio pseudo-relevance feedback (PRF) query expansion
    #[arg(long = "expand")]
    pub expand: bool,

    /// Display detailed score breakdown (BM25, Dense Cosine, RRF, and matched terms)
    #[arg(long = "explain")]
    pub explain: bool,

    /// Target codebase directory to search
    #[arg(short = 'p', long = "path", default_value = ".")]
    pub path: PathBuf,

    /// Disable incremental AST caching and force full re-parsing
    #[arg(long = "no-cache")]
    pub no_cache: bool,

    /// Output results in machine-readable JSON format
    #[arg(long = "json")]
    pub json: bool,
}

#[derive(Serialize)]
struct QueryCliReport {
    query: String,
    mode: QueryModeCli,
    query_expansion: bool,
    total_symbols: usize,
    results: Vec<SearchResult>,
}

pub fn execute(args: QueryArgs) -> Result<(), Box<dyn std::error::Error>> {
    let start_time = Instant::now();

    if !args.json {
        eprintln!(
            "{} Searching codebase at '{}' for: '{}' (mode: {:?}, expand: {})...",
            "⚙".cyan().bold(),
            args.path.display().to_string().bold(),
            args.query.bold(),
            args.mode,
            args.expand
        );
    }

    let mut repo = LoadedRepository::load_with_options(&args.path, !args.no_cache)?;
    repo.load_all_sources()?;

    let config = RetrievalConfig {
        mode: args.mode.into(),
        top_k: args.limit,
        query_expansion: args.expand,
        ..Default::default()
    };

    let retriever = HybridRetriever::with_config(config);
    let results = retriever.search(&repo.symbols, &args.query);
    let elapsed = start_time.elapsed();

    if args.json {
        let report = QueryCliReport {
            query: args.query,
            mode: args.mode,
            query_expansion: args.expand,
            total_symbols: repo.symbols.len(),
            results,
        };
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }

    eprintln!(
        "{} Found {} relevant symbols across {} indexed entities in {:.2}ms\n",
        "✓".green().bold(),
        results.len().to_string().bold(),
        repo.symbols.len().to_string().bold(),
        elapsed.as_secs_f64() * 1000.0
    );

    if results.is_empty() {
        println!("{}", "No matching symbols found.".yellow());
        return Ok(());
    }

    // Format results table
    println!(
        "{:>4}  {:>7}  {:<32}  {:<10}  {:<36}  {:>6}",
        "Rank".bold().dimmed(),
        "Score".bold().dimmed(),
        "Symbol Name".bold().dimmed(),
        "Kind".bold().dimmed(),
        "File Location".bold().dimmed(),
        "Tokens".bold().dimmed()
    );
    println!(
        "{:>4}  {:>7}  {:<32}  {:<10}  {:<36}  {:>6}",
        "----".dimmed(),
        "-----".dimmed(),
        "-----------".dimmed(),
        "----".dimmed(),
        "-------------".dimmed(),
        "------".dimmed()
    );

    for (idx, r) in results.iter().enumerate() {
        let rank_str = format!("#{}", idx + 1);
        let score_pct = format!("{:.0}%", r.score * 100.0);

        let kind_str = format!("{:?}", r.symbol_kind);
        let loc_str = r.file_path.display().to_string().replace('\\', "/");
        let loc_trunc = if loc_str.len() > 36 {
            format!("...{}", &loc_str[loc_str.len() - 33..])
        } else {
            loc_str
        };

        let sym_name_trunc = if r.symbol_name.len() > 32 {
            format!("{}...", &r.symbol_name[..29])
        } else {
            r.symbol_name.clone()
        };

        println!(
            "{:>4}  {:>7}  {:<32}  {:<10}  {:<36}  {:>6}",
            rank_str.cyan(),
            score_pct.green().bold(),
            sym_name_trunc.bold(),
            kind_str.yellow(),
            loc_trunc.dimmed(),
            r.token_cost.to_string().magenta()
        );

        if args.explain {
            let terms_str = if r.matched_terms.is_empty() {
                "none".to_string()
            } else {
                r.matched_terms.join(", ")
            };
            println!(
                "      {} BM25: {:.3} | Dense: {:.3} | RRF: {:.5} | Matched: [{}]",
                "└─".dimmed(),
                r.bm25_score,
                r.dense_score,
                r.rrf_score,
                terms_str.cyan()
            );
            println!("         {} {}", "sig:".dimmed(), r.signature.dimmed());
        }
    }

    Ok(())
}
