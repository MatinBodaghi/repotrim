use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use repotrim_engine::{
    AstExtractor, ContextSelector, LayerWeights, MultiplexGraph, ReferenceEdge, SymbolId,
    SymbolNode,
};

fn collect_rs_files(dir: &Path, files: &mut Vec<PathBuf>) {
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                collect_rs_files(&path, files);
            } else if path.extension().map_or(false, |ext| ext == "rs") {
                files.push(path);
            }
        }
    }
}

#[test]
fn test_dogfood_repotrim_engine() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let src_dir = manifest_dir.join("src");

    println!("\n================================================================================");
    println!("DOGFOODING REPOTRIM ON ITS OWN CODEBASE: {}", src_dir.display());
    println!("================================================================================\n");

    let mut rs_files = Vec::new();
    collect_rs_files(&src_dir, &mut rs_files);
    rs_files.sort();

    assert!(
        !rs_files.is_empty(),
        "Expected Rust source files in {}",
        src_dir.display()
    );

    let extractor = AstExtractor::new().expect("Failed to initialize AstExtractor");

    let mut all_symbols: Vec<SymbolNode> = Vec::new();
    let mut all_edges: Vec<ReferenceEdge> = Vec::new();
    let mut next_id = 0u32;
    let mut total_raw_bytes = 0usize;

    let parse_start = Instant::now();

    for file in &rs_files {
        let content = fs::read(file).expect("Failed to read source file");
        total_raw_bytes += content.len();

        // Use relative path for canonical display
        let rel_path = file.strip_prefix(&manifest_dir).unwrap_or(file);

        let (symbols, edges) = extractor
            .parse_file(rel_path, &content, &mut next_id)
            .unwrap_or_else(|e| panic!("Failed to parse {}: {:?}", rel_path.display(), e));

        all_symbols.extend(symbols);
        all_edges.extend(edges);
    }

    let parse_elapsed = parse_start.elapsed();

    let total_symbols = all_symbols.len();
    let total_edges = all_edges.len();
    let total_repo_tokens: usize = all_symbols.iter().map(|s| s.token_cost).sum();

    println!("1. PARSE & INGESTION STATS:");
    println!("   - Files Parsed:       {}", rs_files.len());
    println!("   - Total Source Bytes: {} bytes", total_raw_bytes);
    println!("   - Parse Duration:     {:?}", parse_elapsed);
    println!("   - Symbols Extracted:  {}", total_symbols);
    println!("   - Raw Ref Edges:      {}", total_edges);
    println!("   - Total Symbol Cost:  {} tokens\n", total_repo_tokens);

    assert!(total_symbols > 0);

    // Build the in-memory Multiplex Code Property Graph
    let graph_start = Instant::now();
    let graph = MultiplexGraph::build(all_symbols, &all_edges, LayerWeights::default());
    let graph_elapsed = graph_start.elapsed();

    println!("2. MULTIPLEX GRAPH & CSR MATRIX STATS:");
    println!("   - Graph Build Time:   {:?}", graph_elapsed);
    println!("   - Graph Symbols (V):  {}", graph.num_symbols());
    println!("   - Resolved Edges (E): {}", graph.num_edges());
    println!();

    let selector = ContextSelector::default();

    // Find key candidate seeds in the parsed codebase
    let selector_sym = graph
        .symbols()
        .iter()
        .find(|s| s.name == "ContextSelector")
        .expect("ContextSelector struct not found in engine codebase");

    let select_method_sym = graph
        .symbols()
        .iter()
        .find(|s| s.name == "select_context")
        .expect("select_context method not found in engine codebase");

    let ppr_sym = graph
        .symbols()
        .iter()
        .find(|s| s.name == "PprSolver")
        .expect("PprSolver struct not found in engine codebase");

    let csr_sym = graph
        .symbols()
        .iter()
        .find(|s| s.name == "CsrMatrix")
        .expect("CsrMatrix struct not found in engine codebase");

    // Test Scenario A1: Seed = select_context (method with call edges), Budget = 300 tokens
    run_scenario(
        &selector,
        &graph,
        "Scenario A1: Method select_context (Budget: 300 tokens)",
        &[select_method_sym.id],
        300,
        total_repo_tokens,
    );

    // Test Scenario A2: Seed = ContextSelector struct, Budget = 200 tokens
    run_scenario(
        &selector,
        &graph,
        "Scenario A2: ContextSelector Struct (Budget: 200 tokens)",
        &[selector_sym.id],
        200,
        total_repo_tokens,
    );

    // Test Scenario B: Seed = PprSolver, Budget = 400 tokens
    run_scenario(
        &selector,
        &graph,
        "Scenario B: PprSolver (Budget: 400 tokens)",
        &[ppr_sym.id],
        400,
        total_repo_tokens,
    );

    // Test Scenario C: Seed = CsrMatrix, Budget = 600 tokens
    run_scenario(
        &selector,
        &graph,
        "Scenario C: CsrMatrix (Budget: 600 tokens)",
        &[csr_sym.id],
        600,
        total_repo_tokens,
    );

    println!("================================================================================");
    println!("DOGFOODING VALIDATION COMPLETED SUCCESSFULLY");
    println!("================================================================================\n");
}

fn run_scenario(
    selector: &ContextSelector,
    graph: &MultiplexGraph,
    title: &str,
    seeds: &[SymbolId],
    budget: usize,
    total_repo_tokens: usize,
) {
    println!("--------------------------------------------------------------------------------");
    println!("{}", title);
    println!("--------------------------------------------------------------------------------");

    let start = Instant::now();
    let selected = selector.select_context(graph, seeds, budget);
    let elapsed = start.elapsed();

    let used_tokens: usize = selected.iter().map(|s| s.token_cost).sum();
    let reduction_pct = (1.0 - (used_tokens as f64 / total_repo_tokens as f64)) * 100.0;

    println!("   - Execution Latency:  {:?}", elapsed);
    println!("   - Tokens Used:        {} / {} budget", used_tokens, budget);
    println!(
        "   - Token Reduction:    {:.1}% (from {} total repo tokens)",
        reduction_pct, total_repo_tokens
    );
    println!("   - Selected Symbols:   {}", selected.len());

    assert!(
        used_tokens <= budget,
        "Used tokens {} exceeded budget {}",
        used_tokens,
        budget
    );
    assert!(!selected.is_empty(), "Expected non-empty context selection");

    println!("\n   Selected Context Skeleton:");
    for (idx, sym) in selected.iter().enumerate() {
        println!(
            "     [{:02}] {:<18} ({:>3} tokens) | {}:L{}",
            idx + 1,
            sym.name,
            sym.token_cost,
            sym.file_path.display(),
            sym.span.start_row + 1
        );
        println!("          Signature: {}", sym.signature);
    }
    println!();
}
