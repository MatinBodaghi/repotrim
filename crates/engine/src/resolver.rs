use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::import::{resolve_module_path, FileImport};
use crate::parser::SupportedLanguage;
use crate::symbol::{SymbolId, SymbolNode};

/// Bayesian scoped identifier resolver with deterministic import-scoped resolution.
///
/// Resolves raw string callee and type identifiers (e.g. `"sqrt"`, `"Point"`) to concrete
/// target `SymbolId`s across repository files without requiring a heavy compiler daemon.
///
/// Resolution priority:
/// 1. Direct AST imports (`FileImport`) binding local names to exact cross-file targets (confidence 1.0).
/// 2. Receiver namespace / member access (`ns.member` or `ns::member`) (confidence 1.0).
/// 3. Local file definitions (confidence 1.0).
/// 4. Wildcard imports (`use ...::*`, `from ... import *`) (confidence 0.90).
/// 5. Receiver method disambiguation for imported modules (confidence 0.95).
/// 6. Bayesian spatial distance heuristic disambiguation (confidence 0.30 .. 0.85).
#[derive(Debug, Clone)]
pub struct ScopedResolver {
    /// Maps symbol names to all known candidate `SymbolId`s across the workspace.
    name_to_ids: HashMap<String, Vec<SymbolId>>,
    /// Fast direct lookup by canonical file path and symbol name.
    path_name_to_id: HashMap<(PathBuf, String), SymbolId>,
    /// Dense symbol lookup table by `SymbolId`.
    id_to_symbol: HashMap<SymbolId, SymbolNode>,
    /// Explicit import bindings: `(source_file, local_identifier) -> target SymbolId`.
    explicit_bindings: HashMap<(PathBuf, String), SymbolId>,
    /// Wildcard imported files per source file: `source_file -> Vec<target_file>`.
    wildcard_files: HashMap<PathBuf, Vec<PathBuf>>,
    /// Namespace imports: `(source_file, namespace_alias) -> target_file`.
    namespace_imports: HashMap<(PathBuf, String), PathBuf>,
    /// Set of all target files imported by a given source file.
    imported_files_per_file: HashMap<PathBuf, HashSet<PathBuf>>,
}

impl ScopedResolver {
    /// Constructs a `ScopedResolver` without import statements, falling back entirely to spatial heuristics.
    pub fn new(symbols: &[SymbolNode]) -> Self {
        Self::with_imports(symbols, &[])
    }

    /// Constructs a `ScopedResolver` indexing symbols and pre-resolving explicit AST imports.
    pub fn with_imports(symbols: &[SymbolNode], imports: &[FileImport]) -> Self {
        let mut name_to_ids: HashMap<String, Vec<SymbolId>> = HashMap::with_capacity(symbols.len());
        let mut path_name_to_id: HashMap<(PathBuf, String), SymbolId> =
            HashMap::with_capacity(symbols.len());
        let mut id_to_symbol: HashMap<SymbolId, SymbolNode> = HashMap::with_capacity(symbols.len());
        let mut known_files = HashSet::new();

        for sym in symbols {
            name_to_ids
                .entry(sym.name.clone())
                .or_default()
                .push(sym.id);

            path_name_to_id.insert((sym.file_path.clone(), sym.name.clone()), sym.id);
            id_to_symbol.insert(sym.id, sym.clone());
            known_files.insert(sym.file_path.clone());
        }

        let mut explicit_bindings = HashMap::new();
        let mut wildcard_files = HashMap::new();
        let mut namespace_imports = HashMap::new();
        let mut imported_files_per_file = HashMap::new();

        for import in imports {
            let source_file = &import.file_path;
            let lang = match SupportedLanguage::from_path(source_file) {
                Some(l) => l,
                None => continue,
            };

            if let Some(target_file) =
                resolve_module_path(source_file, &import.module_specifier, &known_files, lang)
            {
                imported_files_per_file
                    .entry(source_file.clone())
                    .or_insert_with(HashSet::new)
                    .insert(target_file.clone());

                if import.is_wildcard {
                    if import.local_name == "*" {
                        wildcard_files
                            .entry(source_file.clone())
                            .or_insert_with(Vec::new)
                            .push(target_file.clone());
                    } else {
                        namespace_imports.insert(
                            (source_file.clone(), import.local_name.clone()),
                            target_file.clone(),
                        );
                    }
                } else if import.imported_name == "default" {
                    let stem = target_file
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or("");
                    let target_id = path_name_to_id
                        .get(&(target_file.clone(), "default".to_string()))
                        .or_else(|| {
                            path_name_to_id.get(&(target_file.clone(), import.local_name.clone()))
                        })
                        .or_else(|| path_name_to_id.get(&(target_file.clone(), stem.to_string())))
                        .copied();

                    if let Some(id) = target_id {
                        explicit_bindings
                            .insert((source_file.clone(), import.local_name.clone()), id);
                    }
                } else if let Some(&target_id) =
                    path_name_to_id.get(&(target_file.clone(), import.imported_name.clone()))
                {
                    explicit_bindings
                        .insert((source_file.clone(), import.local_name.clone()), target_id);
                } else {
                    namespace_imports.insert(
                        (source_file.clone(), import.local_name.clone()),
                        target_file.clone(),
                    );
                }
            }
        }

        Self {
            name_to_ids,
            path_name_to_id,
            id_to_symbol,
            explicit_bindings,
            wildcard_files,
            namespace_imports,
            imported_files_per_file,
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
    /// Returns the target `SymbolId` and a confidence score in `(0.0, 1.0]`.
    /// If no candidate matches the identifier in the indexed workspace, returns `None`.
    pub fn resolve(&self, source_node: &SymbolNode, target_ident: &str) -> Option<(SymbolId, f32)> {
        // Priority 1: Exact explicit import binding (confidence 1.0)
        if let Some(&target_id) = self
            .explicit_bindings
            .get(&(source_node.file_path.clone(), target_ident.to_string()))
        {
            return Some((target_id, 1.0));
        }

        // Priority 2: Namespace access `ns.func` or `ns::func` (confidence 1.0)
        let split_res = target_ident
            .split_once('.')
            .or_else(|| target_ident.split_once("::"));
        if let Some((ns, member)) = split_res {
            if let Some(target_file) = self
                .namespace_imports
                .get(&(source_node.file_path.clone(), ns.to_string()))
            {
                if let Some(&target_id) = self
                    .path_name_to_id
                    .get(&(target_file.clone(), member.to_string()))
                {
                    return Some((target_id, 1.0));
                }

                // Check sibling files in the same package directory (e.g. Go multi-file packages)
                if let Some(target_parent) = target_file.parent() {
                    if let Some(candidate_ids) = self.name_to_ids.get(member) {
                        for &cand_id in candidate_ids {
                            if let Some(cand_node) = self.id_to_symbol.get(&cand_id) {
                                if cand_node.file_path.parent() == Some(target_parent) {
                                    return Some((cand_id, 1.0));
                                }
                            }
                        }
                    }
                }
            }
        }

        // Priority 3: Local definition in the same file (confidence 1.0)
        if let Some(&local_id) = self
            .path_name_to_id
            .get(&(source_node.file_path.clone(), target_ident.to_string()))
        {
            return Some((local_id, 1.0));
        }

        // Priority 3.5: Intra-package sibling definition in the same directory for Go (confidence 1.0)
        if source_node.file_path.extension().and_then(|s| s.to_str()) == Some("go") {
            if let Some(source_parent) = source_node.file_path.parent() {
                if let Some(candidate_ids) = self.name_to_ids.get(target_ident) {
                    for &cand_id in candidate_ids {
                        if let Some(cand_node) = self.id_to_symbol.get(&cand_id) {
                            if cand_node.file_path.parent() == Some(source_parent)
                                && cand_node.file_path.extension().and_then(|s| s.to_str())
                                    == Some("go")
                            {
                                return Some((cand_id, 1.0));
                            }
                        }
                    }
                }
            }
        }

        // Normalize scoped paths like `math::sqrt` to extract base identifier and optional module hint
        let (module_hint, base_ident) = if let Some(idx) = target_ident.rfind("::") {
            (Some(&target_ident[..idx]), &target_ident[idx + 2..])
        } else if let Some(idx) = target_ident.rfind('.') {
            (Some(&target_ident[..idx]), &target_ident[idx + 1..])
        } else {
            (None, target_ident)
        };

        // Priority 4: Wildcard imports (confidence 0.90)
        if let Some(wildcards) = self.wildcard_files.get(&source_node.file_path) {
            for wf in wildcards {
                if let Some(&target_id) = self
                    .path_name_to_id
                    .get(&(wf.clone(), base_ident.to_string()))
                {
                    return Some((target_id, 0.90));
                }
            }
        }

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

        // Priority 5: Disambiguate candidates using imported modules
        if let Some(imported_files) = self.imported_files_per_file.get(&source_node.file_path) {
            let mut imported_candidates = Vec::new();
            for &cand_id in candidate_ids {
                if let Some(cand_node) = self.id_to_symbol.get(&cand_id) {
                    if imported_files.contains(&cand_node.file_path) {
                        imported_candidates.push(cand_id);
                    }
                }
            }
            if imported_candidates.len() == 1 {
                return Some((imported_candidates[0], 0.95));
            }
        }

        // Priority 6: Multiple candidates - disambiguate using Bayesian spatial prior
        let mut best_candidate: Option<SymbolId> = None;
        let mut highest_score = -1.0_f32;

        let imported_files_opt = self.imported_files_per_file.get(&source_node.file_path);

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

                // Boost score if candidate's file is in imported files
                if let Some(imported_files) = imported_files_opt {
                    if imported_files.contains(&cand_node.file_path) {
                        score = (score + 0.4).min(1.0);
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
            container_name: None,
            trait_name: None,
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
        let resolver = ScopedResolver::new(std::slice::from_ref(&n1));

        let result = resolver.resolve(&n1, "non_existent_function");
        assert!(result.is_none());
    }

    #[test]
    fn test_go_package_and_intra_package_resolution() {
        // Go package auth with two files: auth.go and session.go
        let hash_pw = make_test_node(0, "HashPassword", "pkg/auth/auth.go");
        let make_session = make_test_node(1, "CreateSession", "pkg/auth/session.go");

        // Go main.go in cmd/main.go
        let main_fn = make_test_node(2, "main", "cmd/main.go");

        // main.go imports pkg/auth
        let import = FileImport::new(
            Path::new("cmd/main.go"),
            "myproject/pkg/auth".to_string(),
            "auth".to_string(),
            "auth".to_string(),
        );

        let resolver = ScopedResolver::with_imports(
            &[hash_pw.clone(), make_session.clone(), main_fn.clone()],
            &[import],
        );

        // 1. Cross-package call to auth.go symbol: auth.HashPassword -> score 1.0
        let res1 = resolver.resolve(&main_fn, "auth.HashPassword");
        assert!(res1.is_some());
        let (id1, score1) = res1.unwrap();
        assert_eq!(id1, SymbolId(0));
        assert_eq!(score1, 1.0);

        // 2. Cross-package call to session.go sibling symbol: auth.CreateSession -> score 1.0
        let res2 = resolver.resolve(&main_fn, "auth.CreateSession");
        assert!(res2.is_some());
        let (id2, score2) = res2.unwrap();
        assert_eq!(id2, SymbolId(1));
        assert_eq!(score2, 1.0);

        // 3. Intra-package call from session.go to auth.go without qualification: HashPassword -> score 1.0
        let res3 = resolver.resolve(&make_session, "HashPassword");
        assert!(res3.is_some());
        let (id3, score3) = res3.unwrap();
        assert_eq!(id3, SymbolId(0));
        assert_eq!(score3, 1.0);
    }
}
