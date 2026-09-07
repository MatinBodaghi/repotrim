use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::symbol::{SymbolId, SymbolNode};

/// Bayesian scoped identifier resolver.
///
/// Resolves raw string callee and type identifiers (e.g. `"sqrt"`, `"Point"`) to concrete
/// target `SymbolId`s across repository files without requiring a heavy compiler daemon.
///
/// Disambiguates identifier collisions using a spatial lexical distance prior:
/// candidate symbols located in the same file or parent module receive higher likelihood
/// than distant or external symbols.
#[derive(Debug, Clone)]
pub struct ScopedResolver {
    /// Maps symbol names to all known candidate `SymbolId`s across the workspace.
    name_to_ids: HashMap<String, Vec<SymbolId>>,
    /// Fast direct lookup by canonical file path and symbol name.
    path_name_to_id: HashMap<(PathBuf, String), SymbolId>,
    /// Dense symbol lookup table by `SymbolId`.
    id_to_symbol: HashMap<SymbolId, SymbolNode>,
}

impl ScopedResolver {
    /// Constructs a new `ScopedResolver` by indexing the extracted symbol declarations.
    pub fn new(symbols: &[SymbolNode]) -> Self {
        let mut name_to_ids: HashMap<String, Vec<SymbolId>> = HashMap::with_capacity(symbols.len());
        let mut path_name_to_id: HashMap<(PathBuf, String), SymbolId> =
            HashMap::with_capacity(symbols.len());
        let mut id_to_symbol: HashMap<SymbolId, SymbolNode> = HashMap::with_capacity(symbols.len());

        for sym in symbols {
            name_to_ids
                .entry(sym.name.clone())
                .or_default()
                .push(sym.id);

            path_name_to_id.insert((sym.file_path.clone(), sym.name.clone()), sym.id);
            id_to_symbol.insert(sym.id, sym.clone());
        }

        Self {
            name_to_ids,
            path_name_to_id,
            id_to_symbol,
        }
    }

    /// Looks up a symbol by exact file path and identifier name.
    #[inline]
    pub fn get_by_path_and_name(&self, path: &Path, name: &str) -> Option<SymbolId> {
        self.path_name_to_id
            .get(&(path.to_path_buf(), name.to_string()))
            .copied()
    }

    /// Resolves a reference identifier originating from `source_node` to the most likely target `SymbolId`.
    ///
    /// Returns the target `SymbolId` and a Bayesian confidence score in `(0.0, 1.0]`.
    /// If no candidate matches the identifier in the indexed workspace, returns `None`.
    pub fn resolve(&self, source_node: &SymbolNode, target_ident: &str) -> Option<(SymbolId, f32)> {
        // Normalize scoped paths like `math::sqrt` to extract base identifier and optional module hint
        let (module_hint, base_ident) = if let Some(idx) = target_ident.rfind("::") {
            (Some(&target_ident[..idx]), &target_ident[idx + 2..])
        } else {
            (None, target_ident)
        };

        let candidate_ids = self.name_to_ids.get(base_ident)?;
        if candidate_ids.is_empty() {
            return None;
        }

        // Fast path: unique symbol name in the entire codebase
        if candidate_ids.len() == 1 {
            let target_id = candidate_ids[0];
            if let Some(target_node) = self.id_to_symbol.get(&target_id) {
                let score = compute_scope_score(&source_node.file_path, &target_node.file_path);
                return Some((target_id, score));
            }
        }

        // Multiple candidates: disambiguate using Bayesian spatial prior
        let mut best_candidate: Option<SymbolId> = None;
        let mut highest_score = -1.0_f32;

        for &cand_id in candidate_ids {
            if let Some(cand_node) = self.id_to_symbol.get(&cand_id) {
                let mut score = compute_scope_score(&source_node.file_path, &cand_node.file_path);

                // Boost score if module hint matches candidate's path or name
                if let Some(hint) = module_hint {
                    let path_str = cand_node.file_path.to_string_lossy();
                    if path_str.contains(hint) {
                        score = (score + 0.3).min(1.0);
                    }
                }

                if score > highest_score {
                    highest_score = score;
                    best_candidate = Some(cand_id);
                }
            }
        }

        best_candidate.map(|id| (id, highest_score.max(0.1)))
    }
}

/// Computes the relative module/path distance between two file paths.
///
/// - `0`: Identical file
/// - `1`: Sibling files in the same directory
/// - `2`: Sibling directories (common grandparent)
/// - `3+`: Distant modules
pub fn compute_path_distance(p1: &Path, p2: &Path) -> usize {
    if p1 == p2 {
        return 0;
    }

    let parent1 = p1.parent();
    let parent2 = p2.parent();

    if parent1.is_some() && parent1 == parent2 {
        return 1;
    }

    let c1: Vec<_> = p1.components().collect();
    let c2: Vec<_> = p2.components().collect();

    let common_prefix_len = c1
        .iter()
        .zip(c2.iter())
        .take_while(|&(a, b)| a == b)
        .count();

    let diff1 = c1.len().saturating_sub(common_prefix_len);
    let diff2 = c2.len().saturating_sub(common_prefix_len);

    diff1 + diff2
}

/// Calculates a Bayesian spatial confidence score based on relative path distance.
fn compute_scope_score(p1: &Path, p2: &Path) -> f32 {
    let dist = compute_path_distance(p1, p2);
    match dist {
        0 => 1.00,
        1 => 0.85,
        2 => 0.60,
        _ => 0.30,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::symbol::{SymbolKind, TextSpan};

    fn make_test_node(id: u32, name: &str, file: &str) -> SymbolNode {
        SymbolNode {
            id: SymbolId(id),
            name: name.to_string(),
            kind: SymbolKind::Function,
            file_path: PathBuf::from(file),
            span: TextSpan::new(0, 10, 0, 1),
            signature: format!("fn {}()", name),
            docstring: None,
            token_cost: 5,
            ast_hash: [0u8; 32],
        }
    }

    #[test]
    fn test_path_distance_calculations() {
        let p_same = Path::new("src/parser.rs");
        assert_eq!(compute_path_distance(p_same, p_same), 0);

        let p_sibling = Path::new("src/tokens.rs");
        assert_eq!(compute_path_distance(p_same, p_sibling), 1);

        let p_child = Path::new("src/sub/mod.rs");
        assert_eq!(compute_path_distance(p_same, p_child), 3);
    }

    #[test]
    fn test_single_candidate_resolution() {
        let n1 = make_test_node(0, "compute", "src/math.rs");
        let n2 = make_test_node(1, "main", "src/main.rs");

        let resolver = ScopedResolver::new(&[n1.clone(), n2.clone()]);

        let resolved = resolver.resolve(&n2, "compute");
        assert!(resolved.is_some());
        let (target_id, score) = resolved.unwrap();
        assert_eq!(target_id, SymbolId(0));
        assert_eq!(score, 0.85); // Same directory
    }

    #[test]
    fn test_ambiguous_candidate_disambiguation() {
        // Two symbols named "init": one in local file, one in remote file
        let local_init = make_test_node(0, "init", "src/service.rs");
        let remote_init = make_test_node(1, "init", "crates/other/src/lib.rs");
        let caller = make_test_node(2, "run", "src/service.rs");

        let resolver = ScopedResolver::new(&[local_init, remote_init, caller.clone()]);

        // Caller should resolve "init" to local_init (SymbolId 0) with score 1.0
        let (target_id, score) = resolver.resolve(&caller, "init").expect("Should resolve");
        assert_eq!(target_id, SymbolId(0));
        assert_eq!(score, 1.0);
    }

    #[test]
    fn test_unresolvable_identifier() {
        let n1 = make_test_node(0, "main", "src/main.rs");
        let resolver = ScopedResolver::new(&[n1.clone()]);

        let result = resolver.resolve(&n1, "non_existent_function");
        assert!(result.is_none());
    }
}
