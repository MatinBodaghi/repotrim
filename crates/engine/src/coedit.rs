//! Git Commit Co-Edit Mining and Logical Coupling Association.
//!
//! # Academic Foundations & Citations
//! - **Zimmermann, T., Weißgerber, P., Diehl, S., & Zeller, A. (2005)**.
//!   *Mining Version Histories to Guide Software Changes*.
//!   IEEE Transactions on Software Engineering, 31(6), 429-445.
//!   (Formulation of co-change association rules, support, and confidence across version archives).
//! - **Gall, H., Hajek, K., & Jazayeri, M. (1998)**.
//!   *Detection of Logical Coupling Based on Change Sets*.
//!   Proceedings of the 20th International Conference on Software Engineering (ICSE '98), 159-168.
//! - **Hassan, A. E. (2008)**.
//!   *The Road Ahead for Mining Software Repositories*.
//!   Frontiers of Software Engineering (FoSE), 48-57.
//!   (Threshold filtering of bulk formatting and megacommit noise in MSR).
//! - **Robbes, R., Pollet, D., & Lanza, M. (2008)**.
//!   *Logical Coupling Based on Fine-Grained Changes*.
//!   15th Working Conference on Reverse Engineering (WCRE), 171-180.
//!   (Temporal recency and exponential decay weighting of software evolution).

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::process::Command;

use crate::diff::DiffResolver;
use crate::error::EngineError;
use crate::symbol::{SymbolId, SymbolNode};

/// Configuration parameters for Git commit co-edit mining.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoeditConfig {
    /// Maximum number of historical commits to inspect from HEAD.
    pub max_commits: usize,
    /// Half-life in days for exponential recency decay $w(C) = 2^{-\Delta t / t_{\text{half}}}$.
    pub half_life_days: f64,
    /// Maximum number of modified files before a commit is discarded as bulk churn / megacommit.
    pub max_files_per_commit: usize,
    /// Maximum number of touched symbols before a commit is discarded as megacommit noise.
    pub max_symbols_per_commit: usize,
    /// Minimum raw co-edit occurrences required to consider two symbols logically coupled.
    pub min_support: usize,
    /// Minimum association confidence threshold $\text{conf}(u \to v) = \frac{c(u, v)}{c(u)}$.
    pub min_confidence: f32,
    /// Minimum Jaccard similarity threshold $J(u, v) = \frac{c(u, v)}{c(u) + c(v) - c(u, v)}$.
    pub min_jaccard: f32,
}

impl Default for CoeditConfig {
    fn default() -> Self {
        Self {
            max_commits: 200,
            half_life_days: 90.0,
            max_files_per_commit: 25,
            max_symbols_per_commit: 40,
            min_support: 2,
            min_confidence: 0.15,
            min_jaccard: 0.05,
        }
    }
}

/// A directed co-edit association between two symbols mined from Git history.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CoeditPair {
    /// Originating symbol.
    pub source: SymbolId,
    /// Target co-edited symbol.
    pub target: SymbolId,
    /// Raw unweighted number of commits where both symbols changed together.
    pub raw_count: usize,
    /// Time-decayed support metric $c(u, v) = \sum_{C: u,v \in C} w(C)$.
    pub support: f32,
    /// Association confidence $\text{conf}(u \to v) = \frac{c(u, v)}{c(u)}$.
    pub confidence: f32,
    /// Jaccard similarity $J(u, v) = \frac{c(u, v)}{c(u) + c(v) - c(u, v)}$.
    pub jaccard: f32,
}

/// In-memory graph of mined Git commit co-edits and logical couplings.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CoeditGraph {
    /// Mined directed co-edit pairs meeting minimum support and Jaccard thresholds.
    pub pairs: Vec<CoeditPair>,
    /// Accumulated time-decayed edit weights per symbol $c(u) = \sum_{C: u \in C} w(C)$.
    pub symbol_edit_counts: HashMap<SymbolId, f32>,
    /// Raw commit counts per symbol.
    pub symbol_raw_counts: HashMap<SymbolId, usize>,
    /// Total commits encountered in the mined window.
    pub total_commits_analyzed: usize,
    /// Commits discarded due to exceeding file or symbol churn thresholds.
    pub megacommits_filtered: usize,
    /// Valid commits contributing to co-edit mining.
    pub valid_commits: usize,
    /// HEAD commit hash when this co-edit graph was mined.
    pub head_hash: String,
}

impl CoeditGraph {
    /// Returns true if no co-edit associations were mined or the graph is empty.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.pairs.is_empty()
    }

    /// Returns the total number of directed co-edit edges.
    #[inline]
    pub fn num_pairs(&self) -> usize {
        self.pairs.len()
    }

    /// Converts mined co-edit pairs into directed edges `(source, target, confidence)`
    /// for integration into the `MultiplexGraph`.
    pub fn to_directed_edges(&self, min_confidence: f32) -> Vec<(SymbolId, SymbolId, f32)> {
        self.pairs
            .iter()
            .filter(|p| p.confidence >= min_confidence)
            .map(|p| (p.source, p.target, p.confidence))
            .collect()
    }

    /// Returns the top $N$ co-edit pairs sorted descending by decay-weighted support.
    pub fn top_pairs(&self, limit: usize) -> Vec<&CoeditPair> {
        let mut sorted: Vec<&CoeditPair> = self.pairs.iter().collect();
        sorted.sort_by(|a, b| {
            b.support
                .partial_cmp(&a.support)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| b.raw_count.cmp(&a.raw_count))
                .then_with(|| {
                    b.confidence
                        .partial_cmp(&a.confidence)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
        });
        sorted.truncate(limit);
        sorted
    }

    /// Returns all mined co-edit couplings originating from a given symbol.
    pub fn couplings_for(&self, symbol: SymbolId) -> Vec<&CoeditPair> {
        let mut couplings: Vec<&CoeditPair> =
            self.pairs.iter().filter(|p| p.source == symbol).collect();
        couplings.sort_by(|a, b| {
            b.confidence
                .partial_cmp(&a.confidence)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        couplings
    }

    /// Retrieves the time-decayed co-edit support between two symbols $c(u, v)$.
    pub fn coedit_support(&self, u: SymbolId, v: SymbolId) -> f32 {
        for p in &self.pairs {
            if (p.source == u && p.target == v) || (p.source == v && p.target == u) {
                return p.support;
            }
        }
        0.0
    }

    /// Retrieves the association confidence $\text{conf}(u \to v)$.
    pub fn coedit_confidence(&self, u: SymbolId, v: SymbolId) -> f32 {
        for p in &self.pairs {
            if p.source == u && p.target == v {
                return p.confidence;
            }
        }
        0.0
    }

    /// Retrieves the Jaccard similarity between two symbols $J(u, v)$.
    pub fn coedit_jaccard(&self, u: SymbolId, v: SymbolId) -> f32 {
        for p in &self.pairs {
            if (p.source == u && p.target == v) || (p.source == v && p.target == u) {
                return p.jaccard;
            }
        }
        0.0
    }
}

/// Mined commit record before symbol resolution.
#[derive(Debug, Clone)]
#[allow(dead_code)]
struct ParsedCommit {
    hash: String,
    timestamp: i64,
    diff_text: String,
}

/// Engine for mining Git commit history and constructing the co-edit graph.
pub struct GitCommitMiner;

impl GitCommitMiner {
    /// Inspects the current HEAD commit hash in the specified directory.
    pub fn get_head_hash(root: &Path) -> Result<String, EngineError> {
        let output = Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(root)
            .output();

        match output {
            Ok(out) if out.status.success() => {
                let hash = String::from_utf8_lossy(&out.stdout).trim().to_string();
                if hash.is_empty() {
                    Err(EngineError::GitError("HEAD commit hash is empty".into()))
                } else {
                    Ok(hash)
                }
            }
            Ok(out) => {
                let err_msg = String::from_utf8_lossy(&out.stderr).to_string();
                Err(EngineError::GitError(format!(
                    "git rev-parse HEAD exited with code {:?}: {}",
                    out.status.code(),
                    err_msg
                )))
            }
            Err(e) => Err(EngineError::GitError(format!(
                "Failed to run git rev-parse HEAD in '{}': {}",
                root.display(),
                e
            ))),
        }
    }

    /// Mines Git commit history from the workspace and constructs the `CoeditGraph`.
    ///
    /// Executes `git log -p --unified=0 -n <max_commits>` with delimiter formatting
    /// to parse fine-grained changesets without external library overhead.
    pub fn mine_repository(
        root: &Path,
        symbols: &[SymbolNode],
        config: &CoeditConfig,
    ) -> Result<CoeditGraph, EngineError> {
        let head_hash = Self::get_head_hash(root)?;

        let max_commits_str = config.max_commits.to_string();
        let output = Command::new("git")
            .args([
                "log",
                "-p",
                "--unified=0",
                "-n",
                &max_commits_str,
                "--format=COMMIT_DELIMITER %H %at",
            ])
            .current_dir(root)
            .output();

        let log_text = match output {
            Ok(out) if out.status.success() => String::from_utf8_lossy(&out.stdout).to_string(),
            Ok(out) => {
                let err_msg = String::from_utf8_lossy(&out.stderr).to_string();
                return Err(EngineError::GitError(format!(
                    "git log exited with code {:?}: {}",
                    out.status.code(),
                    err_msg
                )));
            }
            Err(e) => {
                return Err(EngineError::GitError(format!(
                    "Failed to execute git log in '{}': {}",
                    root.display(),
                    e
                )));
            }
        };

        Ok(Self::parse_git_log_output(
            &log_text, &head_hash, symbols, config,
        ))
    }

    /// Parses raw `git log` output into a structured `CoeditGraph`.
    ///
    /// Pure function suitable for deterministic testing with mock git outputs.
    pub fn parse_git_log_output(
        log_text: &str,
        head_hash: &str,
        symbols: &[SymbolNode],
        config: &CoeditConfig,
    ) -> CoeditGraph {
        let mut parsed_commits = Vec::new();
        let mut current_hash: Option<String> = None;
        let mut current_timestamp: i64 = 0;
        let mut current_diff_lines = Vec::new();

        for line in log_text.lines() {
            if let Some(rest) = line.strip_prefix("COMMIT_DELIMITER ") {
                // Flush previous commit if exists
                if let Some(h) = current_hash.take() {
                    parsed_commits.push(ParsedCommit {
                        hash: h,
                        timestamp: current_timestamp,
                        diff_text: current_diff_lines.join("\n"),
                    });
                    current_diff_lines.clear();
                }

                let mut parts = rest.split_whitespace();
                if let Some(h) = parts.next() {
                    current_hash = Some(h.to_string());
                    if let Some(ts_str) = parts.next() {
                        current_timestamp = ts_str.parse::<i64>().unwrap_or(0);
                    }
                }
            } else if current_hash.is_some() {
                current_diff_lines.push(line);
            }
        }

        // Flush final commit
        if let Some(h) = current_hash {
            parsed_commits.push(ParsedCommit {
                hash: h,
                timestamp: current_timestamp,
                diff_text: current_diff_lines.join("\n"),
            });
        }

        let total_commits_analyzed = parsed_commits.len();
        if parsed_commits.is_empty() {
            return CoeditGraph {
                head_hash: head_hash.to_string(),
                ..Default::default()
            };
        }

        // Find reference timestamp (most recent commit timestamp)
        let max_timestamp = parsed_commits
            .iter()
            .map(|c| c.timestamp)
            .max()
            .unwrap_or(0);

        let half_life_secs = (config.half_life_days * 86400.0).max(1.0);
        let mut megacommits_filtered = 0;
        let mut valid_commits = 0;

        let mut symbol_edit_counts: HashMap<SymbolId, f32> = HashMap::new();
        let mut symbol_raw_counts: HashMap<SymbolId, usize> = HashMap::new();
        let mut pair_raw_counts: HashMap<(u32, u32), usize> = HashMap::new();
        let mut pair_decay_weights: HashMap<(u32, u32), f32> = HashMap::new();

        for commit in parsed_commits {
            let modified_files = DiffResolver::parse_unified_diff(&commit.diff_text);
            if modified_files.is_empty() {
                continue;
            }

            // Megacommit check 1: File count threshold (Hassan 2008)
            if modified_files.len() > config.max_files_per_commit {
                megacommits_filtered += 1;
                continue;
            }

            let symbol_hits = DiffResolver::resolve_modified_symbols(symbols, &modified_files);
            let mut touched_symbols: Vec<SymbolId> =
                symbol_hits.into_iter().map(|(id, _)| id).collect();
            touched_symbols.sort_unstable();
            touched_symbols.dedup();

            // Megacommit check 2: Symbol count threshold
            if touched_symbols.len() > config.max_symbols_per_commit {
                megacommits_filtered += 1;
                continue;
            }

            // A commit touching < 2 symbols cannot form co-edits
            if touched_symbols.len() < 2 {
                continue;
            }

            valid_commits += 1;

            // Temporal recency weighting: w(C) = 2^(-delta_t / half_life) (Robbes et al. 2008)
            let delta_t = (max_timestamp - commit.timestamp).max(0) as f64;
            let weight = (2.0f64.powf(-delta_t / half_life_secs) as f32).clamp(0.001, 1.0);

            for &sym in &touched_symbols {
                *symbol_edit_counts.entry(sym).or_default() += weight;
                *symbol_raw_counts.entry(sym).or_default() += 1;
            }

            for i in 0..touched_symbols.len() {
                for j in (i + 1)..touched_symbols.len() {
                    let u = touched_symbols[i].0;
                    let v = touched_symbols[j].0;
                    let key = if u < v { (u, v) } else { (v, u) };

                    *pair_raw_counts.entry(key).or_default() += 1;
                    *pair_decay_weights.entry(key).or_default() += weight;
                }
            }
        }

        // Build filtered directed pairs
        let mut pairs = Vec::new();
        for (&(u_val, v_val), &raw_count) in &pair_raw_counts {
            if raw_count < config.min_support {
                continue;
            }

            let support = *pair_decay_weights.get(&(u_val, v_val)).unwrap_or(&0.0);
            let u_id = SymbolId(u_val);
            let v_id = SymbolId(v_val);

            let c_u = *symbol_edit_counts.get(&u_id).unwrap_or(&0.0);
            let c_v = *symbol_edit_counts.get(&v_id).unwrap_or(&0.0);

            let union_val = (c_u + c_v - support).max(1e-6);
            let jaccard = support / union_val;

            if jaccard < config.min_jaccard {
                continue;
            }

            let conf_u_to_v = (support / c_u.max(1e-6)).clamp(0.0, 1.0);
            let conf_v_to_u = (support / c_v.max(1e-6)).clamp(0.0, 1.0);

            pairs.push(CoeditPair {
                source: u_id,
                target: v_id,
                raw_count,
                support,
                confidence: conf_u_to_v,
                jaccard,
            });

            pairs.push(CoeditPair {
                source: v_id,
                target: u_id,
                raw_count,
                support,
                confidence: conf_v_to_u,
                jaccard,
            });
        }

        CoeditGraph {
            pairs,
            symbol_edit_counts,
            symbol_raw_counts,
            total_commits_analyzed,
            megacommits_filtered,
            valid_commits,
            head_hash: head_hash.to_string(),
        }
    }
}

/// Cache container for fast persistence of mined Git co-edit graphs.
pub struct CoeditCache;

impl CoeditCache {
    /// Loads cached `CoeditGraph` from disk if it exists and matches the expected Git HEAD hash.
    pub fn load_from_file(cache_path: &Path, expected_head: &str) -> Option<CoeditGraph> {
        if expected_head.is_empty() || !cache_path.exists() {
            return None;
        }

        let bytes = fs::read(cache_path).ok()?;
        let graph: CoeditGraph = bincode::deserialize(&bytes).ok()?;

        if graph.head_hash == expected_head {
            Some(graph)
        } else {
            None
        }
    }

    /// Saves the `CoeditGraph` to `.repotrim/coedit.bin`.
    pub fn save_to_file(cache_path: &Path, graph: &CoeditGraph) -> Result<(), EngineError> {
        if let Some(parent) = cache_path.parent() {
            fs::create_dir_all(parent).map_err(EngineError::IoError)?;
        }
        let bytes = bincode::serialize(graph).map_err(|e| EngineError::GitError(e.to_string()))?;
        fs::write(cache_path, bytes).map_err(EngineError::IoError)?;
        Ok(())
    }
}
