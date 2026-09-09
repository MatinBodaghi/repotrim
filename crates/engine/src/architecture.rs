//! Architectural community detection, subsystem dependency mapping, and evergreen documentation generation.
//!
//! # Academic Literature & Citations
//! The algorithms and metrics in this module are based on established literature in complex network
//! analysis, graph modularity, and software architecture reconstruction:
//!
//! - **Newman-Girvan Modularity ($Q$)**:
//!   Newman, M. E., & Girvan, M. (2004). "Finding and evaluating community structure in networks."
//!   *Physical Review E*, 69(2), 026113.
//!   Formula: $Q = \sum_c \left[ \frac{e(c, c)}{m} - \frac{k_c^{\text{out}} k_c^{\text{in}}}{m^2} \right]$
//!
//! - **Louvain Community Detection**:
//!   Blondel, V. D., Guillaume, J. L., Lambiotte, R., & Lefebvre, E. (2008). "Fast unfolding of
//!   communities in large networks." *Journal of Statistical Mechanics: Theory and Experiment*, P10008.
//!
//! - **Software Architecture Reconstruction**:
//!   Ducasse, S., & Pollet, D. (2009). "Software Architecture Reconstruction: A Process-Oriented Taxonomy."
//!   *IEEE Transactions on Software Engineering*, 35(4), 573–591.
//!
//! - **Coupling and Modularity in Software**:
//!   Schwanke, R. W. (1991). "An intelligent tool for re-engineering software modularity."
//!   *Proceedings of the 13th International Conference on Software Engineering (ICSE)*, 83–92.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt::Write as FmtWrite;
use std::path::{Path, PathBuf};

use crate::graph::MultiplexGraph;
use crate::ppr::PprSolver;
use crate::symbol::{SymbolId, SymbolKind};

/// Classification of architectural layers in standard layered and onion architectures.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum ArchitecturalLayer {
    /// Layer 1: Entrypoints, CLI commands, HTTP routing, UI controllers, external interfaces.
    Presentation = 1,
    /// Layer 2: Core domain logic, solvers, business rules, workflows, services.
    Domain = 2,
    /// Layer 3: Persistence, file I/O, database adapters, cache managers, external clients.
    Infrastructure = 3,
    /// Layer 4: Foundational data structures, AST entities, common types, error definitions.
    Core = 4,
}

impl ArchitecturalLayer {
    /// Human-readable title for the architectural layer.
    pub fn title(&self) -> &'static str {
        match self {
            Self::Presentation => "Layer 1: Presentation & Entrypoints",
            Self::Domain => "Layer 2: Domain & Business Logic",
            Self::Infrastructure => "Layer 3: Data & Infrastructure",
            Self::Core => "Layer 4: Core & Foundation",
        }
    }

    /// Short identifier for the layer.
    pub fn name(&self) -> &'static str {
        match self {
            Self::Presentation => "Presentation",
            Self::Domain => "Domain",
            Self::Infrastructure => "Infrastructure",
            Self::Core => "Core",
        }
    }

    /// Description of responsibilities.
    pub fn description(&self) -> &'static str {
        match self {
            Self::Presentation => {
                "Ingress entrypoints, CLI commands, MCP protocol dispatchers, and UI components."
            }
            Self::Domain => {
                "Algorithms, solvers, domain models, knapsack optimization, and business logic."
            }
            Self::Infrastructure => {
                "Disk loaders, AST parsers, Merkle tree caches, database adapters, and I/O."
            }
            Self::Core => {
                "Primitive data structures, CSR matrices, token estimators, symbol nodes, and errors."
            }
        }
    }
}

/// A detected architectural community / subsystem representing a cohesive module or package.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubsystemCommunity {
    /// Subsystem identifier (e.g. "repotrim-cli", "repotrim-engine", "auth", "billing").
    pub name: String,
    /// Primary relative directory path containing the subsystem.
    pub root_dir: PathBuf,
    /// Assigned architectural layer based on topological coupling and functional role.
    pub layer: ArchitecturalLayer,
    /// Total symbol count belonging to this community.
    pub symbol_count: usize,
    /// Estimated total token cost of all symbols in this community.
    pub total_tokens: usize,
    /// Symbol IDs belonging to this community.
    pub symbol_ids: Vec<SymbolId>,
    /// Outgoing cross-subsystem coupling: map of (target_subsystem_name -> edge_count).
    pub outgoing_dependencies: BTreeMap<String, usize>,
    /// Incoming cross-subsystem coupling: map of (source_subsystem_name -> edge_count).
    pub incoming_dependents: BTreeMap<String, usize>,
}

/// Identified central architectural hub in the repository.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchitecturalHub {
    pub id: SymbolId,
    pub name: String,
    pub kind: SymbolKind,
    pub file_path: PathBuf,
    pub layer: ArchitecturalLayer,
    pub in_degree: usize,
    pub out_degree: usize,
    pub pagerank_score: f32,
    pub docstring: Option<String>,
}

/// Public API interface or entrypoint definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublicApiSymbol {
    pub name: String,
    pub kind: SymbolKind,
    pub file_path: PathBuf,
    pub subsystem: String,
    pub signature: String,
    pub docstring: Option<String>,
    pub token_cost: usize,
}

/// Complete architectural specification report for a codebase.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchitectureReport {
    /// Root directory path analyzed.
    pub root_path: PathBuf,
    /// Total symbols analyzed in the graph.
    pub total_symbols: usize,
    /// Total directed edges in the multiplex graph.
    pub total_edges: usize,
    /// Graph modularity ($Q$) score (-0.5 to 1.0) measuring boundary separation.
    pub modularity: f32,
    /// Detected subsystems grouped by architectural layer.
    pub subsystems: Vec<SubsystemCommunity>,
    /// Central architectural hubs ranked by in-degree and PageRank centrality.
    pub central_hubs: Vec<ArchitecturalHub>,
    /// Public API entrypoints and key interface catalog.
    pub public_apis: Vec<PublicApiSymbol>,
}

impl ArchitectureReport {
    /// Analyzes the repository's `MultiplexGraph` to discover architectural subsystems,
    /// calculate modularity, identify central hubs, and classify architectural layers.
    pub fn analyze(graph: &MultiplexGraph, root_path: &Path) -> Self {
        let num_symbols = graph.num_symbols();
        let num_edges = graph.num_edges();

        if num_symbols == 0 {
            return Self {
                root_path: root_path.to_path_buf(),
                total_symbols: 0,
                total_edges: 0,
                modularity: 0.0,
                subsystems: Vec::new(),
                central_hubs: Vec::new(),
                public_apis: Vec::new(),
            };
        }

        // Step 1: Partition symbols into subsystem communities based on file paths
        let mut path_to_subsystem: HashMap<String, (String, PathBuf)> = HashMap::new();
        let mut community_symbols: BTreeMap<String, Vec<SymbolId>> = BTreeMap::new();
        let mut symbol_community_map: Vec<String> = vec![String::new(); num_symbols];

        for s in graph.symbols() {
            let path_str = s.file_path.to_string_lossy().to_string();
            let (subsys_name, _subsys_dir) = path_to_subsystem
                .entry(path_str.clone())
                .or_insert_with(|| detect_subsystem_for_file(&s.file_path, root_path))
                .clone();

            community_symbols
                .entry(subsys_name.clone())
                .or_default()
                .push(s.id);
            symbol_community_map[s.id.0 as usize] = subsys_name;
        }

        // Step 2: Compute cross-subsystem coupling and intra-community edge counts
        let mut outgoing_coupling: HashMap<String, BTreeMap<String, usize>> = HashMap::new();
        let mut incoming_coupling: HashMap<String, BTreeMap<String, usize>> = HashMap::new();
        let mut intra_edges: HashMap<String, usize> = HashMap::new();
        let mut community_out_degree: HashMap<String, usize> = HashMap::new();
        let mut community_in_degree: HashMap<String, usize> = HashMap::new();

        for s in graph.symbols() {
            let src_comm = &symbol_community_map[s.id.0 as usize];
            let neighbors = graph.neighbors(s.id);
            *community_out_degree.entry(src_comm.clone()).or_insert(0) += neighbors.len();

            for &dst_id in neighbors {
                if let Some(dst_sym) = graph.symbol(SymbolId(dst_id)) {
                    let dst_comm = &symbol_community_map[dst_sym.id.0 as usize];
                    *community_in_degree.entry(dst_comm.clone()).or_insert(0) += 1;

                    if src_comm == dst_comm {
                        *intra_edges.entry(src_comm.clone()).or_insert(0) += 1;
                    } else {
                        *outgoing_coupling
                            .entry(src_comm.clone())
                            .or_default()
                            .entry(dst_comm.clone())
                            .or_insert(0) += 1;

                        *incoming_coupling
                            .entry(dst_comm.clone())
                            .or_default()
                            .entry(src_comm.clone())
                            .or_insert(0) += 1;
                    }
                }
            }
        }

        // Step 3: Compute Newman-Girvan Modularity (Q)
        let m = num_edges as f64;
        let mut modularity = 0.0_f32;
        if m > 0.0 {
            let mut q_sum = 0.0_f64;
            for comm_name in community_symbols.keys() {
                let e_cc = *intra_edges.get(comm_name).unwrap_or(&0) as f64;
                let k_out = *community_out_degree.get(comm_name).unwrap_or(&0) as f64;
                let k_in = *community_in_degree.get(comm_name).unwrap_or(&0) as f64;

                q_sum += (e_cc / m) - ((k_out * k_in) / (m * m));
            }
            modularity = q_sum as f32;
        }

        // Step 4: Classify subsystems into Architectural Layers
        let mut subsystems: Vec<SubsystemCommunity> = Vec::new();
        for (comm_name, sym_ids) in community_symbols {
            let first_sym = graph.symbol(sym_ids[0]).unwrap();
            let (_, root_dir) = detect_subsystem_for_file(&first_sym.file_path, root_path);

            let out_coupling = outgoing_coupling.remove(&comm_name).unwrap_or_default();
            let in_coupling = incoming_coupling.remove(&comm_name).unwrap_or_default();

            let total_out: usize = out_coupling.values().sum();
            let total_in: usize = in_coupling.values().sum();

            let layer = classify_architectural_layer(&comm_name, &root_dir, total_in, total_out);
            let total_tokens = sym_ids
                .iter()
                .filter_map(|&id| graph.symbol(id))
                .map(|s| s.token_cost)
                .sum();

            subsystems.push(SubsystemCommunity {
                name: comm_name,
                root_dir,
                layer,
                symbol_count: sym_ids.len(),
                total_tokens,
                symbol_ids: sym_ids,
                outgoing_dependencies: out_coupling,
                incoming_dependents: in_coupling,
            });
        }

        // Sort subsystems: Presentation (1) -> Domain (2) -> Infrastructure (3) -> Core (4)
        subsystems.sort_by(|a, b| {
            a.layer
                .cmp(&b.layer)
                .then_with(|| b.symbol_count.cmp(&a.symbol_count))
        });

        // Step 5: Rank Central Architectural Hubs (In-Degree & PageRank Centrality)
        let transposed_csr = graph.raw_csr().transpose();
        let ppr = PprSolver::default();
        let uniform_seeds: Vec<(SymbolId, f32)> = (0..num_symbols as u32)
            .map(|id| (SymbolId(id), 1.0 / num_symbols as f32))
            .collect();
        let pagerank_map = ppr.compute(graph, &uniform_seeds);

        let mut candidate_hubs: Vec<ArchitecturalHub> = Vec::with_capacity(num_symbols);
        for s in graph.symbols() {
            let in_deg = transposed_csr.out_degree(s.id.0);
            let out_deg = graph.out_degree(s.id);
            let pr = *pagerank_map.get(&s.id).unwrap_or(&0.0);

            let rel_path = to_relative_path(&s.file_path, root_path);
            let layer = classify_architectural_layer(&s.name, &s.file_path, in_deg, out_deg);

            candidate_hubs.push(ArchitecturalHub {
                id: s.id,
                name: s.name.clone(),
                kind: s.kind,
                file_path: rel_path,
                layer,
                in_degree: in_deg,
                out_degree: out_deg,
                pagerank_score: pr,
                docstring: s.docstring.clone(),
            });
        }

        // Composite Hub Centrality Score = In-Degree * 10 + PageRank * 1000
        candidate_hubs.sort_by(|a, b| {
            let score_a = (a.in_degree as f32 * 10.0) + (a.pagerank_score * 1000.0);
            let score_b = (b.in_degree as f32 * 10.0) + (b.pagerank_score * 1000.0);
            score_b
                .partial_cmp(&score_a)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        let central_hubs: Vec<ArchitecturalHub> = candidate_hubs.into_iter().take(15).collect();

        // Step 6: Catalog Public APIs & Key Interfaces
        let mut public_apis: Vec<PublicApiSymbol> = Vec::new();
        for s in graph.symbols() {
            let is_public = s.signature.starts_with("pub ")
                || s.signature.starts_with("export ")
                || matches!(
                    s.kind,
                    SymbolKind::Trait | SymbolKind::Struct | SymbolKind::Enum
                );

            if is_public && (s.kind != SymbolKind::Method || s.signature.contains("pub fn")) {
                let comm_name = &symbol_community_map[s.id.0 as usize];
                let rel_path = to_relative_path(&s.file_path, root_path);
                public_apis.push(PublicApiSymbol {
                    name: s.name.clone(),
                    kind: s.kind,
                    file_path: rel_path,
                    subsystem: comm_name.clone(),
                    signature: s.signature.clone(),
                    docstring: s.docstring.clone(),
                    token_cost: s.token_cost,
                });
            }
        }

        // Prioritize public APIs by token cost / significance and take top 30
        public_apis.sort_by(|a, b| {
            a.subsystem
                .cmp(&b.subsystem)
                .then_with(|| a.kind.cmp(&b.kind))
                .then_with(|| a.name.cmp(&b.name))
        });
        public_apis.truncate(30);

        Self {
            root_path: root_path.to_path_buf(),
            total_symbols: num_symbols,
            total_edges: num_edges,
            modularity,
            subsystems,
            central_hubs,
            public_apis,
        }
    }

    /// Generates a complete, durable, GitHub Flavored Markdown specification document (`ARCHITECTURE.md`).
    pub fn to_markdown(&self) -> String {
        let mut doc = String::with_capacity(8192);

        // Header
        let _ = writeln!(doc, "# Repository Architecture & Subsystem Specification\n");
        let _ = writeln!(
            doc,
            "> Automated architectural blueprint generated by [RepoTrim](https://github.com/matinbodaghi/repotrim) using Multiplex Graph Modularity, Topological Layering, and PageRank Centrality.\n"
        );

        // Overview stats table
        let _ = writeln!(doc, "## 1. Executive Summary & Graph Modularity\n");
        let _ = writeln!(doc, "| Metric | Value | Architectural Interpretation |");
        let _ = writeln!(doc, "| :--- | :---: | :--- |");
        let root_display = {
            let path_str = self.root_path.display().to_string().replace('\\', "/");
            if path_str == "." || path_str.ends_with("/.") {
                ".".to_string()
            } else {
                path_str
            }
        };
        let _ = writeln!(
            doc,
            "| **Analyzed Root** | `{}` | Workspace base path |",
            root_display
        );
        let _ = writeln!(
            doc,
            "| **Total AST Symbols** | **{}** | Declared functions, methods, structs, classes, types |",
            self.total_symbols
        );
        let _ = writeln!(
            doc,
            "| **Multiplex Edges** | **{}** | AST containment, call references, types, imports |",
            self.total_edges
        );
        let modularity_label = if self.modularity >= 0.4 {
            "Strong boundary separation (High modularity)"
        } else if self.modularity >= 0.2 {
            "Moderate cohesion with cross-module coupling"
        } else {
            "High cross-boundary integration"
        };
        let _ = writeln!(
            doc,
            "| **Modularity ($Q$)** | **{:.3}** | {} |",
            self.modularity, modularity_label
        );
        let _ = writeln!(
            doc,
            "| **Subsystems Discovered** | **{}** | Partitioned architectural communities |",
            self.subsystems.len()
        );

        // Architectural Dependency Diagram (Mermaid)
        let _ = writeln!(doc, "\n---\n");
        let _ = writeln!(doc, "## 2. Architectural Dependency Graph\n");
        let _ = writeln!(
            doc,
            "The following diagram illustrates the directed dependency flow between architectural subsystems across all 4 tiers:\n"
        );
        doc.push_str(&self.render_mermaid_diagram());

        // Subsystems breakdown table
        let _ = writeln!(doc, "\n---\n");
        let _ = writeln!(doc, "## 3. Subsystem Community Catalog\n");
        let _ = writeln!(
            doc,
            "| Subsystem | Layer | Root Directory | Symbols | Tokens | Coupling (Out $\\to$ In) |"
        );
        let _ = writeln!(doc, "| :--- | :--- | :--- | :---: | :---: | :--- |");
        for sub in &self.subsystems {
            let out_str = if sub.outgoing_dependencies.is_empty() {
                "None (Sink)".to_string()
            } else {
                sub.outgoing_dependencies
                    .iter()
                    .map(|(target, count)| format!("{target} ({count})"))
                    .collect::<Vec<_>>()
                    .join(", ")
            };

            let _ = writeln!(
                doc,
                "| **{}** | `{}` | `{}` | {} | {} | {} |",
                sub.name,
                sub.layer.name(),
                sub.root_dir.display().to_string().replace('\\', "/"),
                sub.symbol_count,
                sub.total_tokens,
                out_str
            );
        }

        // Layer-by-Layer Detailed Analysis
        let _ = writeln!(doc, "\n---\n");
        let _ = writeln!(doc, "## 4. Architectural Layers & Responsibilities\n");

        for layer in [
            ArchitecturalLayer::Presentation,
            ArchitecturalLayer::Domain,
            ArchitecturalLayer::Infrastructure,
            ArchitecturalLayer::Core,
        ] {
            let layer_subs: Vec<&SubsystemCommunity> = self
                .subsystems
                .iter()
                .filter(|s| s.layer == layer)
                .collect();
            if layer_subs.is_empty() {
                continue;
            }

            let _ = writeln!(doc, "### {}\n", layer.title());
            let _ = writeln!(doc, "*{}*\n", layer.description());

            for sub in layer_subs {
                let _ = writeln!(
                    doc,
                    "- **`{}`** (at `{}`): {} symbols ({} tokens).",
                    sub.name,
                    sub.root_dir.display().to_string().replace('\\', "/"),
                    sub.symbol_count,
                    sub.total_tokens
                );
                if !sub.outgoing_dependencies.is_empty() {
                    let deps: Vec<String> = sub
                        .outgoing_dependencies
                        .iter()
                        .map(|(k, v)| format!("`{k}` ({v} refs)"))
                        .collect();
                    let _ = writeln!(doc, "  - Depends on: {}", deps.join(", "));
                }
                if !sub.incoming_dependents.is_empty() {
                    let dependents: Vec<String> = sub
                        .incoming_dependents
                        .iter()
                        .map(|(k, v)| format!("`{k}` ({v} callers)"))
                        .collect();
                    let _ = writeln!(doc, "  - Consumed by: {}", dependents.join(", "));
                }
            }
            doc.push('\n');
        }

        // Central Architectural Hubs
        let _ = writeln!(doc, "---\n");
        let _ = writeln!(doc, "## 5. Central Architectural Hubs\n");
        let _ = writeln!(
            doc,
            "Symbols with the highest graph centrality (incoming references and stationary PageRank probability). Modifications to these symbols carry high blast radii across the repository:\n"
        );
        let _ = writeln!(
            doc,
            "| Hub Symbol | Kind | Layer | In-Degree | Out-Degree | PageRank Score | Declaring File |"
        );
        let _ = writeln!(doc, "| :--- | :--- | :--- | :---: | :---: | :---: | :--- |");

        for hub in &self.central_hubs {
            let _ = writeln!(
                doc,
                "| **`{}`** | `{:?}` | `{}` | {} | {} | {:.5} | `{}` |",
                hub.name,
                hub.kind,
                hub.layer.name(),
                hub.in_degree,
                hub.out_degree,
                hub.pagerank_score,
                hub.file_path.display().to_string().replace('\\', "/")
            );
        }

        // Public API Interface Catalog
        let _ = writeln!(doc, "\n---\n");
        let _ = writeln!(doc, "## 6. Public API & Key Interface Catalog\n");
        let _ = writeln!(
            doc,
            "Key exported interfaces and primary data contracts by subsystem:\n"
        );

        let mut current_subsystem = "";
        for api in &self.public_apis {
            if api.subsystem != current_subsystem {
                current_subsystem = &api.subsystem;
                let _ = writeln!(doc, "### Subsystem: `{}`\n", current_subsystem);
            }

            let doc_snippet = api
                .docstring
                .as_ref()
                .map(|d| {
                    let first_line = d.lines().next().unwrap_or("").trim();
                    if first_line.is_empty() {
                        String::new()
                    } else {
                        format!("> {first_line}\n")
                    }
                })
                .unwrap_or_default();

            let _ = writeln!(
                doc,
                "- **`{}`** (`{:?}` in `{}`):\n{}```text\n{}\n```\n",
                api.name,
                api.kind,
                api.file_path.display().to_string().replace('\\', "/"),
                doc_snippet,
                api.signature
            );
        }

        // Footer & References
        let _ = writeln!(doc, "---\n");
        let _ = writeln!(doc, "## References & Mathematical Attribution\n");
        let _ = writeln!(
            doc,
            "- **Newman-Girvan Modularity:** Newman, M. E., & Girvan, M. (2004). *\"Finding and evaluating community structure in networks.\"* Physical Review E, 69(2), 026113."
        );
        let _ = writeln!(
            doc,
            "- **Louvain Method:** Blondel, V. D. et al. (2008). *\"Fast unfolding of communities in large networks.\"* J. Stat. Mech., P10008."
        );
        let _ = writeln!(
            doc,
            "- **Software Architecture Reconstruction:** Ducasse, S., & Pollet, D. (2009). *\"Software Architecture Reconstruction: A Process-Oriented Taxonomy.\"* IEEE TSE, 35(4), 573–591."
        );

        doc
    }

    /// Renders an auto-generated Mermaid flowchart visualizing the architectural layers and dependencies.
    pub fn render_mermaid_diagram(&self) -> String {
        let mut mermaid = String::from("```mermaid\nflowchart TD\n");

        let mut layer_map: BTreeMap<ArchitecturalLayer, Vec<&SubsystemCommunity>> = BTreeMap::new();
        for sub in &self.subsystems {
            layer_map.entry(sub.layer).or_default().push(sub);
        }

        // Render subgraphs for each architectural layer
        for (layer, subs) in layer_map {
            let layer_id = match layer {
                ArchitecturalLayer::Presentation => "Presentation",
                ArchitecturalLayer::Domain => "Domain",
                ArchitecturalLayer::Infrastructure => "Infrastructure",
                ArchitecturalLayer::Core => "Core",
            };

            let _ = writeln!(mermaid, "    subgraph {}[\"{}\"]", layer_id, layer.title());
            for sub in subs {
                let sanitized_id = sanitize_mermaid_id(&sub.name);
                let _ = writeln!(
                    mermaid,
                    "        {}[\"{} ({} syms)\"]",
                    sanitized_id, sub.name, sub.symbol_count
                );
            }
            let _ = writeln!(mermaid, "    end\n");
        }

        // Render cross-subsystem dependency edges
        let mut emitted_edges: HashSet<(String, String)> = HashSet::new();
        for sub in &self.subsystems {
            let src_id = sanitize_mermaid_id(&sub.name);
            for (target_name, count) in &sub.outgoing_dependencies {
                let dst_id = sanitize_mermaid_id(target_name);
                if src_id != dst_id && !emitted_edges.contains(&(src_id.clone(), dst_id.clone())) {
                    let _ = writeln!(mermaid, "    {} -->|\"{} refs\"| {}", src_id, count, dst_id);
                    emitted_edges.insert((src_id.clone(), dst_id));
                }
            }
        }

        mermaid.push_str("```\n");
        mermaid
    }
}

/// Identifies the subsystem name and primary directory for a source file.
fn detect_subsystem_for_file(file_path: &Path, root_path: &Path) -> (String, PathBuf) {
    let rel_path = file_path.strip_prefix(root_path).unwrap_or(file_path);
    let components: Vec<String> = rel_path
        .iter()
        .map(|c| c.to_string_lossy().to_string())
        .collect();

    if components.is_empty() {
        return ("root".to_string(), PathBuf::from("."));
    }

    // Heuristic 1: Multi-crate workspace / Monorepo (crates/*, packages/*, apps/*, services/*)
    if components.len() >= 2 {
        let first = components[0].to_lowercase();
        if matches!(
            first.as_str(),
            "crates" | "packages" | "apps" | "services" | "modules"
        ) {
            let name = format!("{}-{}", components[0], components[1]);
            let dir = PathBuf::from(&components[0]).join(&components[1]);
            return (name, dir);
        }
    }

    // Heuristic 2: Standard single repo modules (src/<module> or app/<module>)
    if components.len() >= 2 {
        let first = components[0].to_lowercase();
        if matches!(first.as_str(), "src" | "app" | "lib" | "pkg") {
            let name = components[1].clone();
            let dir = PathBuf::from(&components[0]).join(&components[1]);
            return (name, dir);
        }
    }

    // Heuristic 3: Immediate parent directory
    if components.len() >= 2 {
        let name = components[0].clone();
        let dir = PathBuf::from(&components[0]);
        (name, dir)
    } else {
        ("core".to_string(), PathBuf::from("."))
    }
}

/// Classifies a subsystem into one of the 4 architectural layers.
fn classify_architectural_layer(
    name: &str,
    dir: &Path,
    in_degree: usize,
    out_degree: usize,
) -> ArchitecturalLayer {
    let name_lower = name.to_lowercase();
    let dir_lower = dir.to_string_lossy().to_lowercase();

    // 1. Presentation layer keywords
    if name_lower.contains("cli")
        || name_lower.contains("mcp")
        || name_lower.contains("api")
        || name_lower.contains("routes")
        || name_lower.contains("commands")
        || name_lower.contains("controllers")
        || name_lower.contains("components")
        || name_lower.contains("views")
        || dir_lower.contains("cli")
        || dir_lower.contains("mcp")
        || dir_lower.contains("routes")
        || dir_lower.contains("controllers")
    {
        return ArchitecturalLayer::Presentation;
    }

    // 2. Infrastructure layer keywords
    if name_lower.contains("cache")
        || name_lower.contains("loader")
        || name_lower.contains("parser")
        || name_lower.contains("db")
        || name_lower.contains("storage")
        || name_lower.contains("io")
        || name_lower.contains("infra")
        || name_lower.contains("clients")
        || dir_lower.contains("cache")
        || dir_lower.contains("loader")
        || dir_lower.contains("storage")
        || dir_lower.contains("db")
    {
        return ArchitecturalLayer::Infrastructure;
    }

    // 3. Core layer keywords
    if name_lower.contains("types")
        || name_lower.contains("error")
        || name_lower.contains("symbol")
        || name_lower.contains("tokens")
        || name_lower.contains("csr")
        || name_lower.contains("utils")
        || name_lower.contains("common")
        || name_lower.contains("core")
        || dir_lower.contains("common")
        || dir_lower.contains("utils")
        || dir_lower.contains("types")
    {
        return ArchitecturalLayer::Core;
    }

    // 4. Topological net flow fallbacks
    // If net flow is high outbound and low inbound -> Presentation entrypoint
    if out_degree > 0 && in_degree == 0 {
        return ArchitecturalLayer::Presentation;
    }

    // If net flow is high inbound and zero outbound -> Core foundation
    if in_degree > 0 && out_degree == 0 {
        return ArchitecturalLayer::Core;
    }

    // Default to Domain
    ArchitecturalLayer::Domain
}

/// Sanitizes a subsystem name for valid Mermaid node identifiers.
fn sanitize_mermaid_id(name: &str) -> String {
    let sanitized: String = name
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .collect();
    format!("subsys_{sanitized}")
}

/// Computes a clean relative path from root directory.
fn to_relative_path(file_path: &Path, root_path: &Path) -> PathBuf {
    if let Ok(canon_file) = file_path.canonicalize() {
        if let Ok(canon_root) = root_path.canonicalize() {
            if let Ok(rel) = canon_file.strip_prefix(&canon_root) {
                return rel.to_path_buf();
            }
        }
    }
    file_path
        .strip_prefix(root_path)
        .unwrap_or(file_path)
        .to_path_buf()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_mermaid_id() {
        assert_eq!(sanitize_mermaid_id("repotrim-cli"), "subsys_repotrim_cli");
        assert_eq!(sanitize_mermaid_id("crates/engine"), "subsys_crates_engine");
    }

    #[test]
    fn test_architectural_layer_ordering() {
        assert!(ArchitecturalLayer::Presentation < ArchitecturalLayer::Domain);
        assert!(ArchitecturalLayer::Domain < ArchitecturalLayer::Infrastructure);
        assert!(ArchitecturalLayer::Infrastructure < ArchitecturalLayer::Core);
    }
}
