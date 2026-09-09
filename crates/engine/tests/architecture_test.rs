use repotrim_engine::architecture::{ArchitecturalLayer, ArchitectureReport};
use repotrim_engine::LoadedRepository;
use std::path::PathBuf;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[test]
fn test_architecture_analysis_on_workspace() {
    let root = repo_root();
    let mut repo = LoadedRepository::load(&root).expect("Failed to load repo");
    repo.load_all_sources().expect("Failed to load sources");
    let graph = repo.build_graph();

    assert!(graph.num_symbols() >= 100, "Should have extracted symbols");
    assert!(graph.num_edges() >= 100, "Should have resolved edges");

    let report = ArchitectureReport::analyze(&graph, &root);

    // 1. Metrics & Subsystems
    assert_eq!(report.total_symbols, graph.num_symbols());
    assert_eq!(report.total_edges, graph.num_edges());
    assert!(
        report.subsystems.len() >= 2,
        "Should discover multiple crate subsystems (found {})",
        report.subsystems.len()
    );

    // Verify discovered subsystems
    let sub_names: Vec<&str> = report.subsystems.iter().map(|s| s.name.as_str()).collect();
    assert!(
        sub_names.iter().any(|n| n.contains("engine")),
        "Should discover engine crate subsystem: {:?}",
        sub_names
    );
    assert!(
        sub_names
            .iter()
            .any(|n| n.contains("cli") || n.contains("mcp")),
        "Should discover cli or mcp subsystem: {:?}",
        sub_names
    );

    // 2. Layering Check
    // Presentation layer should contain CLI or MCP
    let presentation_subs: Vec<&str> = report
        .subsystems
        .iter()
        .filter(|s| s.layer == ArchitecturalLayer::Presentation)
        .map(|s| s.name.as_str())
        .collect();
    assert!(
        !presentation_subs.is_empty(),
        "Presentation layer should contain entrypoint crates (CLI/MCP)"
    );

    // 3. Central Hubs Check
    assert!(
        !report.central_hubs.is_empty(),
        "Central hubs should be identified"
    );
    let top_hub = &report.central_hubs[0];
    assert!(
        top_hub.in_degree > 0 || top_hub.pagerank_score > 0.0,
        "Top hub should have positive centrality"
    );

    // 4. Public API Catalog
    assert!(
        !report.public_apis.is_empty(),
        "Public APIs should be cataloged"
    );

    // 5. Markdown & Mermaid Document Export
    let md = report.to_markdown();
    assert!(md.contains("# Repository Architecture & Subsystem Specification"));
    assert!(md.contains("## 1. Executive Summary & Graph Modularity"));
    assert!(md.contains("## 2. Architectural Dependency Graph"));
    assert!(md.contains("```mermaid\nflowchart TD"));
    assert!(md.contains("## 3. Subsystem Community Catalog"));
    assert!(md.contains("## 4. Architectural Layers & Responsibilities"));
    assert!(md.contains("## 5. Central Architectural Hubs"));
    assert!(md.contains("## 6. Public API & Key Interface Catalog"));
    assert!(md.contains("Newman-Girvan Modularity"));
    assert!(md.contains("Louvain Method"));
}
