//! Multi-resolution community detection and architectural modularity analysis.
//!
//! # Academic Literature & Citations
//! The algorithms, heuristics, and mathematical formulations in this module build on
//! foundational research in complex network modularity, multi-resolution Potts models,
//! and software architecture reconstruction:
//!
//! - **Resolution Limit of Modularity**:
//!   Fortunato, S., & Barthélemy, M. (2007). "Resolution limit in community detection."
//!   *Proceedings of the National Academy of Sciences (PNAS)*, 104(1), 36–41.
//!   [DOI: 10.1073/pnas.0605926104](https://doi.org/10.1073/pnas.0605926104).
//!
//! - **Multi-Resolution Modularity (Reichardt-Bornholdt Potts Model)**:
//!   Reichardt, J., & Bornholdt, S. (2004). "Statistical mechanics of community detection."
//!   *Physical Review E*, 74(1), 016110.
//!   [DOI: 10.1103/PhysRevE.74.016110](https://doi.org/10.1103/PhysRevE.74.016110).
//!   Formula: $Q(\gamma) = \sum_c \left[ \frac{e(c, c)}{2m} - \gamma \left( \frac{k_c}{2m} \right)^2 \right]$
//!
//! - **Multi-Scale Community Analysis**:
//!   Arenas, A., Fernández, A., & Gómez, S. (2008). "Analysis of the structure of complex
//!   networks at different levels of resolution." *New Journal of Physics*, 10(5), 053039.
//!   [DOI: 10.1088/1367-2630/10/5/053039](https://doi.org/10.1088/1367-2630/10/5/053039).
//!
//! - **Louvain Algorithm for Modularity Optimization**:
//!   Blondel, V. D., Guillaume, J. L., Lambiotte, R., & Lefebvre, E. (2008). "Fast unfolding
//!   of communities in large networks." *Journal of Statistical Mechanics: Theory and Experiment*, P10008.
//!   [DOI: 10.1088/1742-5468/2008/10/P10008](https://doi.org/10.1088/1742-5468/2008/10/P10008).
//!
//! - **From Louvain to Leiden (Community Connectivity Guarantees)**:
//!   Traag, V. A., Waltman, L., & van Eck, N. J. (2019). "From Louvain to Leiden: guaranteeing
//!   well-connected communities." *Scientific Reports*, 9, 5233.
//!   [DOI: 10.1038/s41598-019-41695-z](https://doi.org/10.1038/s41598-019-41695-z).
//!
//! - **Cluster Comparison & Normalized Mutual Information (NMI)**:
//!   Strehl, A., & Ghosh, J. (2002). "Cluster ensembles—a knowledge reuse framework for
//!   combining multiple partitions." *Journal of Machine Learning Research*, 3, 583–617.

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::graph::MultiplexGraph;
use crate::symbol::{SymbolId, SymbolKind};

/// Configuration parameters for multi-resolution community detection.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CommunityConfig {
    /// Modularity resolution parameter $\gamma \in (0, \infty)$ (Reichardt & Bornholdt, 2004).
    /// - $\gamma < 1.0$: Coarse-grained macro-subsystems (e.g. $\gamma = 0.5$).
    /// - $\gamma = 1.0$: Standard Newman-Girvan modularity.
    /// - $\gamma > 1.0$: Fine-grained functional modules and symbol cliques (e.g. $\gamma = 2.5$).
    pub resolution: f64,
    /// Maximum local moving passes before stopping.
    pub max_iterations: usize,
    /// Minimum modularity gain threshold $\Delta Q$ required to continue iterating.
    pub min_modularity_gain: f64,
    /// Whether to perform multi-pass hierarchical coarse-graining (Louvain phase 2).
    pub hierarchical: bool,
}

impl Default for CommunityConfig {
    fn default() -> Self {
        Self {
            resolution: 1.0,
            max_iterations: 25,
            min_modularity_gain: 1e-5,
            hierarchical: true,
        }
    }
}

impl CommunityConfig {
    /// Creates a configuration with a custom resolution parameter $\gamma$.
    pub fn with_resolution(resolution: f64) -> Self {
        Self {
            resolution: resolution.max(0.01),
            ..Default::default()
        }
    }

    /// Macro-level architecture configuration ($\gamma = 0.5$).
    pub fn macro_scale() -> Self {
        Self::with_resolution(0.5)
    }

    /// Standard meso-level feature module configuration ($\gamma = 1.0$).
    pub fn meso_scale() -> Self {
        Self::with_resolution(1.0)
    }

    /// Micro-level fine-grained component configuration ($\gamma = 2.5$).
    pub fn micro_scale() -> Self {
        Self::with_resolution(2.5)
    }
}

/// A detected cohesive topological community of symbols.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Community {
    /// Numerical community identifier (0-indexed).
    pub id: usize,
    /// Human-readable label auto-inferred from dominant directory and primary symbols.
    pub name: String,
    /// Modularity resolution $\gamma$ under which this community was partitioned.
    pub resolution: f64,
    /// Contained symbol IDs in this community.
    pub symbol_ids: Vec<SymbolId>,
    /// Total symbol count.
    pub symbol_count: usize,
    /// Estimated token cost of all member symbols.
    pub total_tokens: usize,
    /// Sum of edge weights internal to this community.
    pub internal_weight: f64,
    /// Total edge weight incident to symbols in this community.
    pub total_weight: f64,
    /// Internal edge density: $2 \cdot W_{\text{internal}} / (|V|(|V|-1))$ or $1.0$ for singletons.
    pub density: f64,
    /// Dominant directory containing the plurality of symbols in this community.
    pub dominant_directory: PathBuf,
    /// Proportion of member symbols belonging to the dominant directory ($[0.0, 1.0]$).
    pub directory_purity: f32,
    /// Central architectural hub symbols within this community (name, kind, pagerank).
    pub key_symbols: Vec<String>,
}

/// Detailed result of multi-resolution community detection.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CommunityResult {
    /// Modularity resolution parameter $\gamma$ used.
    pub resolution: f64,
    /// Total modularity score $Q(\gamma)$ achieved by this partition.
    pub modularity: f32,
    /// Discovered cohesive communities, sorted by symbol count descending.
    pub communities: Vec<Community>,
    /// Map from SymbolId to assigned Community ID.
    pub membership: HashMap<SymbolId, usize>,
    /// Number of local moving passes completed.
    pub iterations: usize,
}

/// Multi-scale hierarchical decomposition of a codebase across three canonical resolutions.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CommunityHierarchy {
    /// Macro-level subsystems ($\gamma = 0.5$).
    pub macro_communities: Vec<Community>,
    /// Macro-level modularity score.
    pub macro_modularity: f32,
    /// Meso-level feature modules ($\gamma = 1.0$).
    pub meso_communities: Vec<Community>,
    /// Meso-level modularity score.
    pub meso_modularity: f32,
    /// Micro-level fine-grained components ($\gamma = 2.5$).
    pub micro_communities: Vec<Community>,
    /// Micro-level modularity score.
    pub micro_modularity: f32,
}

/// Detected architectural drift where a symbol's topological coupling deviates from its disk directory.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ArchitecturalDrift {
    /// Symbol ID experiencing architectural drift.
    pub symbol_id: SymbolId,
    /// Identifier name of the symbol.
    pub symbol_name: String,
    /// Symbol kind (Function, Struct, Class, etc.).
    pub symbol_kind: SymbolKind,
    /// Relative file path where the symbol is declared on disk.
    pub file_path: PathBuf,
    /// Declared directory of the symbol.
    pub declared_directory: PathBuf,
    /// Community ID the symbol was grouped into.
    pub community_id: usize,
    /// Human-readable name of the assigned community.
    pub community_name: String,
    /// Dominant directory of the assigned community.
    pub community_dominant_dir: PathBuf,
    /// Architectural drift score in $[0.0, 1.0]$ (coupling to foreign community vs local directory).
    pub drift_score: f32,
}

/// Fast Multi-Resolution Louvain Community Detector.
#[derive(Debug, Clone, Default)]
pub struct CommunityDetector;

impl CommunityDetector {
    /// Detects topological communities in the given `MultiplexGraph` using multi-resolution modularity.
    pub fn detect(graph: &MultiplexGraph, config: &CommunityConfig) -> CommunityResult {
        let num_symbols = graph.num_symbols();
        if num_symbols == 0 {
            return CommunityResult {
                resolution: config.resolution,
                modularity: 0.0,
                communities: Vec::new(),
                membership: HashMap::new(),
                iterations: 0,
            };
        }

        // Build symmetrized adjacency graph from MultiplexGraph CSR
        // A_uv = W_uv + W_vu
        let mut adj: Vec<Vec<(usize, f64)>> = vec![Vec::new(); num_symbols];
        let mut node_degree: Vec<f64> = vec![0.0; num_symbols];
        let mut total_weight_sum = 0.0_f64;

        // Temporary edge map to combine directed pairs
        let mut edge_pairs: HashMap<(usize, usize), f64> = HashMap::new();
        for s in graph.symbols() {
            let u = s.id.0 as usize;
            for &v_raw in graph.neighbors(s.id) {
                let v = v_raw as usize;
                if u != v && v < num_symbols {
                    let w = 1.0_f64; // Base coupling
                    let pair = if u < v { (u, v) } else { (v, u) };
                    *edge_pairs.entry(pair).or_insert(0.0) += w;
                }
            }
        }

        for (&(u, v), &w) in &edge_pairs {
            adj[u].push((v, w));
            adj[v].push((u, w));
            node_degree[u] += w;
            node_degree[v] += w;
            total_weight_sum += 2.0 * w;
        }

        // Handle isolated nodes: give tiny degree epsilon to prevent division by zero
        if total_weight_sum <= 0.0 {
            total_weight_sum = num_symbols as f64 * 2.0;
            for d in &mut node_degree {
                if *d <= 0.0 {
                    *d = 1.0;
                }
            }
        }

        let m2 = total_weight_sum; // 2m in modularity formula
        let gamma = config.resolution;

        // Step 1: Initial local moving pass
        let mut community_assignment: Vec<usize> = (0..num_symbols).collect();
        let mut community_tot: Vec<f64> = node_degree.clone();

        let mut total_iterations = 0;
        for _ in 0..config.max_iterations {
            total_iterations += 1;
            let mut moves = 0;

            for i in 0..num_symbols {
                let current_comm = community_assignment[i];
                let k_i = node_degree[i];

                // Compute coupling from node i to adjacent communities
                let mut comm_coupling: HashMap<usize, f64> = HashMap::new();
                for &(neighbor, weight) in &adj[i] {
                    let neighbor_comm = community_assignment[neighbor];
                    *comm_coupling.entry(neighbor_comm).or_insert(0.0) += weight;
                }

                let k_i_in_curr = comm_coupling.get(&current_comm).copied().unwrap_or(0.0);

                // Try each neighbor community plus current community
                let mut best_comm = current_comm;
                let mut best_delta_q = 0.0_f64;

                // Modularity difference when removing i from current_comm:
                // Delta Q_remove = - [ k_{i, curr} / m2 - gamma * k_i * (tot_curr - k_i) / m2^2 ]
                let tot_curr = community_tot[current_comm];
                let q_remove_term =
                    (k_i_in_curr / m2) - gamma * (k_i * (tot_curr - k_i)) / (m2 * m2);

                for (&cand_comm, &cand_coupling) in &comm_coupling {
                    if cand_comm == current_comm {
                        continue;
                    }

                    let tot_cand = community_tot[cand_comm];
                    let q_insert_term = (cand_coupling / m2) - gamma * (k_i * tot_cand) / (m2 * m2);

                    let delta_q = q_insert_term - q_remove_term;
                    if delta_q > best_delta_q {
                        best_delta_q = delta_q;
                        best_comm = cand_comm;
                    }
                }

                if best_comm != current_comm && best_delta_q > config.min_modularity_gain {
                    // Execute the move
                    community_tot[current_comm] -= k_i;
                    community_tot[best_comm] += k_i;
                    community_assignment[i] = best_comm;
                    moves += 1;
                }
            }

            if moves == 0 {
                break;
            }
        }

        // Step 2: If hierarchical coarse-graining is enabled, perform pass 2 on reduced community graph
        if config.hierarchical && num_symbols > 1 {
            let unique_comms: Vec<usize> = {
                let set: HashSet<usize> = community_assignment.iter().copied().collect();
                let mut v: Vec<usize> = set.into_iter().collect();
                v.sort_unstable();
                v
            };

            if unique_comms.len() < num_symbols && unique_comms.len() > 1 {
                let comm_to_super: HashMap<usize, usize> = unique_comms
                    .iter()
                    .enumerate()
                    .map(|(idx, &comm)| (comm, idx))
                    .collect();

                let num_super = unique_comms.len();
                let mut super_adj: Vec<Vec<(usize, f64)>> = vec![Vec::new(); num_super];
                let mut super_degree: Vec<f64> = vec![0.0; num_super];
                let mut super_pairs: HashMap<(usize, usize), f64> = HashMap::new();

                for (node_idx, &comm) in community_assignment.iter().enumerate() {
                    let super_id = comm_to_super[&comm];
                    super_degree[super_id] += node_degree[node_idx];
                }

                for (&(u, v), &w) in &edge_pairs {
                    let cu = comm_to_super[&community_assignment[u]];
                    let cv = comm_to_super[&community_assignment[v]];
                    if cu != cv {
                        let pair = if cu < cv { (cu, cv) } else { (cv, cu) };
                        *super_pairs.entry(pair).or_insert(0.0) += w;
                    }
                }

                for (&(cu, cv), &w) in &super_pairs {
                    super_adj[cu].push((cv, w));
                    super_adj[cv].push((cu, w));
                }

                let mut super_assignment: Vec<usize> = (0..num_super).collect();
                let mut super_tot = super_degree.clone();

                for _ in 0..config.max_iterations {
                    let mut super_moves = 0;
                    for c in 0..num_super {
                        let curr_sc = super_assignment[c];
                        let k_c = super_degree[c];

                        let mut sc_coupling: HashMap<usize, f64> = HashMap::new();
                        for &(neighbor_sc, weight) in &super_adj[c] {
                            let sc_comm = super_assignment[neighbor_sc];
                            *sc_coupling.entry(sc_comm).or_insert(0.0) += weight;
                        }

                        let k_c_in_curr = sc_coupling.get(&curr_sc).copied().unwrap_or(0.0);
                        let tot_curr = super_tot[curr_sc];
                        let q_remove_term =
                            (k_c_in_curr / m2) - gamma * (k_c * (tot_curr - k_c)) / (m2 * m2);

                        let mut best_sc = curr_sc;
                        let mut best_delta_q = 0.0_f64;

                        for (&cand_sc, &cand_coupling) in &sc_coupling {
                            if cand_sc == curr_sc {
                                continue;
                            }
                            let tot_cand = super_tot[cand_sc];
                            let q_insert_term =
                                (cand_coupling / m2) - gamma * (k_c * tot_cand) / (m2 * m2);
                            let delta_q = q_insert_term - q_remove_term;
                            if delta_q > best_delta_q {
                                best_delta_q = delta_q;
                                best_sc = cand_sc;
                            }
                        }

                        if best_sc != curr_sc && best_delta_q > config.min_modularity_gain {
                            super_tot[curr_sc] -= k_c;
                            super_tot[best_sc] += k_c;
                            super_assignment[c] = best_sc;
                            super_moves += 1;
                        }
                    }

                    if super_moves == 0 {
                        break;
                    }
                }

                // Propagate super-community assignments back to original nodes
                for a in &mut community_assignment {
                    let super_id = comm_to_super[a];
                    *a = super_assignment[super_id];
                }
            }
        }

        // Remap raw community IDs to contiguous 0..K ordered by size
        let mut raw_groups: HashMap<usize, Vec<SymbolId>> = HashMap::new();
        for (i, &raw_c) in community_assignment.iter().enumerate() {
            raw_groups
                .entry(raw_c)
                .or_default()
                .push(SymbolId(i as u32));
        }

        let mut sorted_groups: Vec<Vec<SymbolId>> = raw_groups.into_values().collect();
        sorted_groups.sort_by_key(|b| std::cmp::Reverse(b.len()));

        let mut membership: HashMap<SymbolId, usize> = HashMap::with_capacity(num_symbols);
        let mut communities: Vec<Community> = Vec::with_capacity(sorted_groups.len());

        let mut total_modularity = 0.0_f64;

        for (new_id, sym_ids) in sorted_groups.iter().enumerate() {
            let id_set: HashSet<SymbolId> = sym_ids.iter().copied().collect();
            let symbol_count = sym_ids.len();

            let mut internal_w = 0.0_f64;
            let mut incident_w = 0.0_f64;
            let mut total_tokens = 0usize;
            let mut dir_counts: HashMap<PathBuf, usize> = HashMap::new();

            for &sid in sym_ids {
                membership.insert(sid, new_id);
                if let Some(s) = graph.symbol(sid) {
                    total_tokens += s.token_cost;

                    let parent_dir = s
                        .file_path
                        .parent()
                        .map(|p| p.to_path_buf())
                        .unwrap_or_else(|| PathBuf::from("."));
                    *dir_counts.entry(parent_dir).or_insert(0) += 1;

                    let neighbors = graph.neighbors(sid);
                    for &nid_raw in neighbors {
                        let nid = SymbolId(nid_raw);
                        incident_w += 1.0;
                        if id_set.contains(&nid) {
                            internal_w += 1.0;
                        }
                    }
                }
            }

            // Internal edges were counted once per directed edge
            let e_c = internal_w;
            let k_c = incident_w;
            if m2 > 0.0 {
                total_modularity += (e_c / (m2 / 2.0)) - gamma * ((k_c / m2) * (k_c / m2));
            }

            // Determine dominant directory and purity
            let (dominant_directory, dominant_count) = dir_counts
                .into_iter()
                .max_by_key(|&(_, count)| count)
                .unwrap_or((PathBuf::from("."), 0));

            let directory_purity = if symbol_count > 0 {
                dominant_count as f32 / symbol_count as f32
            } else {
                1.0
            };

            // Compute density
            let density = if symbol_count <= 1 {
                1.0
            } else {
                let max_edges = (symbol_count * (symbol_count - 1)) as f64;
                (internal_w / max_edges).min(1.0)
            };

            // Top key symbols (sorted by degree / PageRank)
            let mut sorted_syms: Vec<(SymbolId, usize)> = sym_ids
                .iter()
                .map(|&id| (id, graph.out_degree(id)))
                .collect();
            sorted_syms.sort_by_key(|b| std::cmp::Reverse(b.1));

            let key_symbols: Vec<String> = sorted_syms
                .iter()
                .take(3)
                .filter_map(|&(id, _)| graph.symbol(id))
                .map(|s| s.name.clone())
                .collect();

            // Name: Dominant Directory basename + top symbol
            let dir_name = dominant_directory
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("subsystem");
            let lead_symbol = key_symbols.first().map(|s| s.as_str()).unwrap_or("core");
            let community_name = format!("{}:{}", dir_name, lead_symbol);

            communities.push(Community {
                id: new_id,
                name: community_name,
                resolution: config.resolution,
                symbol_ids: sym_ids.clone(),
                symbol_count,
                total_tokens,
                internal_weight: internal_w,
                total_weight: incident_w,
                density,
                dominant_directory,
                directory_purity,
                key_symbols,
            });
        }

        CommunityResult {
            resolution: config.resolution,
            modularity: total_modularity as f32,
            communities,
            membership,
            iterations: total_iterations,
        }
    }

    /// Decomposes the codebase across three canonical resolution tiers:
    /// - **Macro ($\gamma = 0.5$)**: Broad architectural packages.
    /// - **Meso ($\gamma = 1.0$)**: Cohesive feature modules.
    /// - **Micro ($\gamma = 2.5$)**: Fine-grained functional cliques.
    pub fn detect_hierarchy(graph: &MultiplexGraph) -> CommunityHierarchy {
        let macro_res = Self::detect(graph, &CommunityConfig::macro_scale());
        let meso_res = Self::detect(graph, &CommunityConfig::meso_scale());
        let micro_res = Self::detect(graph, &CommunityConfig::micro_scale());

        CommunityHierarchy {
            macro_communities: macro_res.communities,
            macro_modularity: macro_res.modularity,
            meso_communities: meso_res.communities,
            meso_modularity: meso_res.modularity,
            micro_communities: micro_res.communities,
            micro_modularity: micro_res.modularity,
        }
    }

    /// Analyzes architectural drift by comparing topological community assignments against
    /// physical file/directory boundaries on disk.
    ///
    /// Identifies misplaced symbols that have higher functional coupling to a foreign
    /// subsystem than their declared source directory.
    pub fn analyze_drift(
        graph: &MultiplexGraph,
        communities: &[Community],
        root_path: &Path,
    ) -> Vec<ArchitecturalDrift> {
        let mut drifts = Vec::new();
        let num_symbols = graph.num_symbols();
        if num_symbols == 0 || communities.is_empty() {
            return drifts;
        }

        // Build community lookup map
        let mut symbol_to_comm: HashMap<SymbolId, &Community> = HashMap::with_capacity(num_symbols);
        for comm in communities {
            for &sid in &comm.symbol_ids {
                symbol_to_comm.insert(sid, comm);
            }
        }

        for s in graph.symbols() {
            let Some(assigned_comm) = symbol_to_comm.get(&s.id) else {
                continue;
            };

            // Relative directory of the symbol
            let rel_file = s
                .file_path
                .strip_prefix(root_path)
                .unwrap_or(&s.file_path)
                .to_path_buf();
            let declared_dir = rel_file
                .parent()
                .map(|p| p.to_path_buf())
                .unwrap_or_else(|| PathBuf::from("."));

            // Check if symbol's directory differs from the community's dominant directory
            let norm_declared = declared_dir.to_string_lossy().replace('\\', "/");
            let norm_dominant = assigned_comm
                .dominant_directory
                .to_string_lossy()
                .replace('\\', "/");

            if !norm_declared.is_empty()
                && !norm_dominant.is_empty()
                && norm_declared != norm_dominant
            {
                // Calculate coupling to dominant directory vs total coupling
                let neighbors = graph.neighbors(s.id);
                let total_degree = neighbors.len();
                if total_degree >= 2 {
                    let mut foreign_coupling = 0usize;
                    for &nid_raw in neighbors {
                        let nid = SymbolId(nid_raw);
                        if let Some(nsym) = graph.symbol(nid) {
                            let n_dir = nsym
                                .file_path
                                .strip_prefix(root_path)
                                .unwrap_or(&nsym.file_path);
                            let n_norm = n_dir
                                .parent()
                                .map(|p| p.to_string_lossy().replace('\\', "/"))
                                .unwrap_or_default();
                            if n_norm == norm_dominant {
                                foreign_coupling += 1;
                            }
                        }
                    }

                    let drift_score = foreign_coupling as f32 / total_degree as f32;
                    if drift_score >= 0.40 {
                        drifts.push(ArchitecturalDrift {
                            symbol_id: s.id,
                            symbol_name: s.name.clone(),
                            symbol_kind: s.kind,
                            file_path: rel_file,
                            declared_directory: declared_dir,
                            community_id: assigned_comm.id,
                            community_name: assigned_comm.name.clone(),
                            community_dominant_dir: assigned_comm.dominant_directory.clone(),
                            drift_score,
                        });
                    }
                }
            }
        }

        drifts.sort_by(|a, b| {
            b.drift_score
                .partial_cmp(&a.drift_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        drifts
    }
}
