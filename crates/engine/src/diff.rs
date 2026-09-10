use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::error::EngineError;
use crate::symbol::{SymbolId, SymbolNode};

/// Git diff parser and symbol resolver.
///
/// Parses standard unified diffs into modified line ranges per file, and maps
/// them to the enclosing AST `SymbolNode`s to seed Personalized PageRank.
pub struct DiffResolver;

impl DiffResolver {
    /// Parses a standard unified diff string into a mapping of file paths to 0-indexed modified line numbers.
    pub fn parse_unified_diff(diff_text: &str) -> HashMap<PathBuf, Vec<usize>> {
        let mut modified_files: HashMap<PathBuf, Vec<usize>> = HashMap::new();
        let mut current_file: Option<PathBuf> = None;
        let mut current_line: usize = 0;

        for line in diff_text.lines() {
            if let Some(stripped) = line.strip_prefix("+++ b/") {
                let file_str = stripped.trim();
                current_file = Some(PathBuf::from(file_str));
            } else if let Some(stripped) = line.strip_prefix("+++ ") {
                if !stripped.starts_with("/dev/null") {
                    let file_str = stripped.trim();
                    let clean_str = file_str.strip_prefix("b/").unwrap_or(file_str);
                    current_file = Some(PathBuf::from(clean_str));
                }
            } else if line.starts_with("@@ ") {
                // Parse hunk header: @@ -old_start,old_count +new_start,new_count @@
                // Or: @@ -old_start +new_start @@
                if let Some(plus_idx) = line.find('+') {
                    let after_plus = &line[plus_idx + 1..];
                    let end_idx = after_plus.find([' ', ',']).unwrap_or(after_plus.len());
                    if let Ok(new_start) = after_plus[..end_idx].parse::<usize>() {
                        current_line = new_start;
                    }
                }
            } else if let Some(file) = &current_file {
                if line.starts_with('+') && !line.starts_with("+++") {
                    // Added/modified line in the new file
                    let line_0_idx = current_line.saturating_sub(1);
                    modified_files
                        .entry(file.clone())
                        .or_default()
                        .push(line_0_idx);
                    current_line += 1;
                } else if line.starts_with('-') && !line.starts_with("---") {
                    // Deleted line at current line position
                    let line_0_idx = current_line.saturating_sub(1);
                    modified_files
                        .entry(file.clone())
                        .or_default()
                        .push(line_0_idx);
                } else if line.starts_with(' ') {
                    current_line += 1;
                }
            }
        }

        // Deduplicate and sort line numbers per file
        for lines in modified_files.values_mut() {
            lines.sort_unstable();
            lines.dedup();
        }

        modified_files
    }

    /// Resolves modified line ranges to enclosing `SymbolNode`s across the workspace.
    ///
    /// Symbols touching more modified lines receive higher relative weights.
    /// If changes in a file occur outside any symbol declaration (e.g. module imports),
    /// the file's top-level symbols are included with baseline weight.
    pub fn resolve_modified_symbols(
        symbols: &[SymbolNode],
        modified_lines_by_file: &HashMap<PathBuf, Vec<usize>>,
    ) -> Vec<(SymbolId, f32)> {
        if symbols.is_empty() || modified_lines_by_file.is_empty() {
            return Vec::new();
        }

        let mut symbol_hits: HashMap<SymbolId, usize> = HashMap::new();
        let mut files_with_zero_hits: HashMap<PathBuf, ()> = HashMap::new();

        for (diff_path, lines) in modified_lines_by_file {
            let diff_norm = diff_path
                .to_string_lossy()
                .replace('\\', "/")
                .trim_start_matches("./")
                .to_string();

            let mut any_symbol_hit = false;

            for sym in symbols {
                let sym_norm = sym
                    .file_path
                    .to_string_lossy()
                    .replace('\\', "/")
                    .trim_start_matches("./")
                    .to_string();

                // Match paths (either exact or suffix match, e.g. "crates/engine/src/diff.rs")
                if sym_norm == diff_norm
                    || sym_norm.ends_with(&diff_norm)
                    || diff_norm.ends_with(&sym_norm)
                {
                    let hits = lines
                        .iter()
                        .filter(|&&l| l >= sym.span.start_row && l <= sym.span.end_row)
                        .count();

                    if hits > 0 {
                        *symbol_hits.entry(sym.id).or_default() += hits;
                        any_symbol_hit = true;
                    }
                }
            }

            if !any_symbol_hit && !lines.is_empty() {
                files_with_zero_hits.insert(diff_path.clone(), ());
            }
        }

        // For files modified outside any symbol (e.g. imports), attach their symbols with a modest baseline
        for diff_path in files_with_zero_hits.keys() {
            let diff_norm = diff_path
                .to_string_lossy()
                .replace('\\', "/")
                .trim_start_matches("./")
                .to_string();

            for sym in symbols {
                let sym_norm = sym
                    .file_path
                    .to_string_lossy()
                    .replace('\\', "/")
                    .trim_start_matches("./")
                    .to_string();

                if sym_norm == diff_norm
                    || sym_norm.ends_with(&diff_norm)
                    || diff_norm.ends_with(&sym_norm)
                {
                    symbol_hits.entry(sym.id).or_insert(1);
                }
            }
        }

        if symbol_hits.is_empty() {
            return Vec::new();
        }

        // Sort symbols descending by hit count
        let mut results: Vec<(SymbolId, f32)> = symbol_hits
            .into_iter()
            .map(|(id, count)| (id, 1.0 + (count as f32) * 0.2))
            .collect();

        results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        // Normalize so highest hit symbol has weight 1.0
        let max_weight = results[0].1.max(1e-5);
        results
            .into_iter()
            .map(|(id, w)| (id, (w / max_weight).clamp(0.1, 1.0)))
            .collect()
    }

    /// Executes `git diff` in the specified directory to extract uncommitted changes (both staged and unstaged).
    pub fn get_git_diff(working_dir: &Path) -> Result<String, EngineError> {
        // Try `git diff HEAD` first to capture both staged and unstaged changes against HEAD
        let output = Command::new("git")
            .args(["diff", "HEAD"])
            .current_dir(working_dir)
            .output();

        match output {
            Ok(out) if out.status.success() => {
                let diff_str = String::from_utf8_lossy(&out.stdout).to_string();
                if !diff_str.trim().is_empty() {
                    return Ok(diff_str);
                }
                // If diff HEAD was empty, also try `git diff` alone in case HEAD was identical
                let unstaged_output = Command::new("git")
                    .args(["diff"])
                    .current_dir(working_dir)
                    .output();
                if let Ok(u_out) = unstaged_output {
                    let u_str = String::from_utf8_lossy(&u_out.stdout).to_string();
                    if !u_str.trim().is_empty() {
                        return Ok(u_str);
                    }
                }
                Ok(diff_str)
            }
            Ok(out) => {
                // If `git diff HEAD` failed (e.g. unborn branch), try `git diff` alone
                let fallback = Command::new("git")
                    .args(["diff"])
                    .current_dir(working_dir)
                    .output();
                if let Ok(fb) = fallback {
                    if fb.status.success() {
                        return Ok(String::from_utf8_lossy(&fb.stdout).to_string());
                    }
                }
                let err_msg = String::from_utf8_lossy(&out.stderr).to_string();
                Err(EngineError::GitError(format!(
                    "git diff exited with code {:?}: {}",
                    out.status.code(),
                    err_msg
                )))
            }
            Err(e) => Err(EngineError::GitError(format!(
                "Failed to execute git in '{}': {}",
                working_dir.display(),
                e
            ))),
        }
    }

    /// Executes `git diff <revision>` in the specified directory to extract changes against a branch or commit.
    pub fn get_git_diff_against(working_dir: &Path, revision: &str) -> Result<String, EngineError> {
        let output = Command::new("git")
            .args(["diff", revision])
            .current_dir(working_dir)
            .output();

        match output {
            Ok(out) if out.status.success() => Ok(String::from_utf8_lossy(&out.stdout).to_string()),
            Ok(out) => {
                let err_msg = String::from_utf8_lossy(&out.stderr).to_string();
                Err(EngineError::GitError(format!(
                    "git diff {} exited with code {:?}: {}",
                    revision,
                    out.status.code(),
                    err_msg
                )))
            }
            Err(e) => Err(EngineError::GitError(format!(
                "Failed to execute git in '{}': {}",
                working_dir.display(),
                e
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::symbol::{SymbolKind, TextSpan};

    #[test]
    fn test_parse_unified_diff_hunks() {
        let diff = r#"
diff --git a/src/lib.rs b/src/lib.rs
index abc..def 100644
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -10,4 +10,6 @@ pub fn existing() {
     let x = 1;
+    let y = 2;
+    let z = 3;
     let w = 4;
diff --git a/src/other.rs b/src/other.rs
index 111..222 100644
--- a/src/other.rs
+++ b/src/other.rs
@@ -5,3 +5,2 @@ pub fn delete_me() {
-    bad_call();
     good_call();
"#;

        let modified = DiffResolver::parse_unified_diff(diff);
        assert_eq!(modified.len(), 2);

        let lib_lines = modified
            .get(&PathBuf::from("src/lib.rs"))
            .expect("src/lib.rs not found");
        // Lines 11 and 12 (0-indexed: 10, 11)
        assert!(lib_lines.contains(&10));
        assert!(lib_lines.contains(&11));

        let other_lines = modified
            .get(&PathBuf::from("src/other.rs"))
            .expect("src/other.rs not found");
        // Line 5 deleted (0-indexed: 4)
        assert!(other_lines.contains(&4));
    }

    #[test]
    fn test_resolve_modified_symbols() {
        let s0 = SymbolNode {
            id: SymbolId(0),
            name: "calculate_total".to_string(),
            kind: SymbolKind::Function,
            file_path: PathBuf::from("src/calc.rs"),
            span: TextSpan::new(100, 300, 10, 25), // Rows 10 to 25
            signature: "pub fn calculate_total()".to_string(),
            docstring: None,
            token_cost: 20,
            ast_hash: [0u8; 32],
            container_name: None,
            trait_name: None,
        };

        let s1 = SymbolNode {
            id: SymbolId(1),
            name: "format_output".to_string(),
            kind: SymbolKind::Function,
            file_path: PathBuf::from("src/calc.rs"),
            span: TextSpan::new(400, 600, 30, 45), // Rows 30 to 45
            signature: "pub fn format_output()".to_string(),
            docstring: None,
            token_cost: 15,
            ast_hash: [0u8; 32],
            container_name: None,
            trait_name: None,
        };

        let mut diff_map = HashMap::new();
        // Modify lines 12, 14, 15 (inside s0: rows 10..25)
        diff_map.insert(PathBuf::from("src/calc.rs"), vec![12, 14, 15]);

        let resolved = DiffResolver::resolve_modified_symbols(&[s0, s1], &diff_map);
        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].0, SymbolId(0));
        assert_eq!(resolved[0].1, 1.0);
    }
}
