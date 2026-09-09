use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Component, Path, PathBuf};

use crate::parser::SupportedLanguage;

/// Extracted AST import statement representing a cross-module dependency.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FileImport {
    /// Relative path of the file containing the import statement.
    pub file_path: PathBuf,
    /// Raw module path specifier (e.g. `"../hooks/useUserData"`, `"app.auth"`, `"crate::cache"`).
    pub module_specifier: String,
    /// Name of the symbol as exported from the target module.
    pub imported_name: String,
    /// Local identifier bound in this file (same as `imported_name` unless aliased via `as`).
    pub local_name: String,
    /// Whether this is a wildcard import (`import * as ...` or `use module::*`).
    pub is_wildcard: bool,
}

impl FileImport {
    /// Constructs a new explicit symbol import.
    pub fn new<P: Into<PathBuf>, S1: Into<String>, S2: Into<String>, S3: Into<String>>(
        file_path: P,
        module_specifier: S1,
        imported_name: S2,
        local_name: S3,
    ) -> Self {
        Self {
            file_path: file_path.into(),
            module_specifier: module_specifier.into(),
            imported_name: imported_name.into(),
            local_name: local_name.into(),
            is_wildcard: false,
        }
    }

    /// Constructs a new wildcard module import.
    pub fn wildcard<P: Into<PathBuf>, S1: Into<String>, S2: Into<String>>(
        file_path: P,
        module_specifier: S1,
        local_name: S2,
    ) -> Self {
        Self {
            file_path: file_path.into(),
            module_specifier: module_specifier.into(),
            imported_name: "*".to_string(),
            local_name: local_name.into(),
            is_wildcard: true,
        }
    }
}

/// Normalizes a path by resolving `.` and `..` components logically.
pub fn normalize_path(path: &Path) -> PathBuf {
    let mut components = Vec::new();
    for comp in path.components() {
        match comp {
            Component::CurDir => {}
            Component::ParentDir => {
                if let Some(Component::Normal(_)) = components.last() {
                    components.pop();
                } else {
                    components.push(comp);
                }
            }
            _ => components.push(comp),
        }
    }
    components.into_iter().collect()
}

/// Resolves a raw module specifier to an exact relative file path in the known workspace.
pub fn resolve_module_path(
    source_file: &Path,
    module_specifier: &str,
    known_files: &HashSet<PathBuf>,
    lang: SupportedLanguage,
) -> Option<PathBuf> {
    let source_dir = source_file.parent().unwrap_or_else(|| Path::new(""));

    match lang {
        SupportedLanguage::TypeScript | SupportedLanguage::Tsx => {
            resolve_typescript_module(source_dir, module_specifier, known_files)
        }
        SupportedLanguage::Python => {
            resolve_python_module(source_dir, module_specifier, known_files)
        }
        SupportedLanguage::Rust => resolve_rust_module(source_file, module_specifier, known_files),
        SupportedLanguage::Go => resolve_go_module(source_dir, module_specifier, known_files),
    }
}

/// Resolves a TypeScript/JavaScript import specifier against workspace files.
fn resolve_typescript_module(
    source_dir: &Path,
    specifier: &str,
    known_files: &HashSet<PathBuf>,
) -> Option<PathBuf> {
    // Only resolve relative specifiers (e.g. `./foo`, `../bar`)
    if !specifier.starts_with('.') {
        return None;
    }

    let base = normalize_path(&source_dir.join(specifier));

    // 1. Direct match (e.g. if specifier already had extension)
    if known_files.contains(&base) {
        return Some(base);
    }

    // 2. Candidate file extensions
    let extensions = [".ts", ".tsx", ".js", ".jsx", ".mjs", ".cjs"];
    for ext in extensions {
        let candidate = PathBuf::from(format!("{}{}", base.display(), ext));
        if known_files.contains(&candidate) {
            return Some(candidate);
        }
    }

    // 3. Directory index resolution (e.g. `./components` -> `./components/index.ts`)
    for ext in extensions {
        let candidate = base.join(format!("index{}", ext));
        if known_files.contains(&candidate) {
            return Some(candidate);
        }
    }

    None
}

/// Resolves a Python import specifier against workspace files.
fn resolve_python_module(
    source_dir: &Path,
    specifier: &str,
    known_files: &HashSet<PathBuf>,
) -> Option<PathBuf> {
    // Relative imports: `.models` or `..utils.helpers`
    if specifier.starts_with('.') {
        let mut leading_dots = 0;
        for ch in specifier.chars() {
            if ch == '.' {
                leading_dots += 1;
            } else {
                break;
            }
        }

        let mut current_dir = source_dir.to_path_buf();
        for _ in 1..leading_dots {
            if let Some(p) = current_dir.parent() {
                current_dir = p.to_path_buf();
            }
        }

        let remainder = &specifier[leading_dots..];
        let rel_path_str = remainder.replace('.', "/");
        let base = if rel_path_str.is_empty() {
            current_dir
        } else {
            normalize_path(&current_dir.join(rel_path_str))
        };

        let candidate_py = PathBuf::from(format!("{}.py", base.display()));
        if known_files.contains(&candidate_py) {
            return Some(candidate_py);
        }

        let candidate_init = base.join("__init__.py");
        if known_files.contains(&candidate_init) {
            return Some(candidate_init);
        }

        return None;
    }

    // Dotted module specifier: `app.auth` or `models`
    let path_str = specifier.replace('.', "/");
    let candidate_py = PathBuf::from(format!("{}.py", path_str));
    if known_files.contains(&candidate_py) {
        return Some(candidate_py);
    }

    let candidate_init = PathBuf::from(format!("{}/__init__.py", path_str));
    if known_files.contains(&candidate_init) {
        return Some(candidate_init);
    }

    // Also check if any known file matches suffix (e.g. `src/app/auth.py` matches `app.auth`)
    for known in known_files {
        let known_str = known.to_string_lossy().replace('\\', "/");
        if known_str.ends_with(&format!("{}.py", path_str))
            || known_str.ends_with(&format!("{}/__init__.py", path_str))
        {
            return Some(known.clone());
        }
    }

    None
}

/// Resolves a Rust module specifier (`crate::...`, `super::...`) against workspace files.
fn resolve_rust_module(
    source_file: &Path,
    specifier: &str,
    known_files: &HashSet<PathBuf>,
) -> Option<PathBuf> {
    let path_str = specifier.replace("::", "/");

    // `crate::...` specifier: search relative to the enclosing crate root
    if let Some(stripped) = specifier.strip_prefix("crate::") {
        let sub_path = stripped.replace("::", "/");

        // Find crate `src/` root for this source file
        let mut crate_src: Option<PathBuf> = None;
        let mut current = source_file.parent();
        while let Some(dir) = current {
            if dir.ends_with("src") {
                crate_src = Some(dir.to_path_buf());
                break;
            }
            current = dir.parent();
        }

        if let Some(src_root) = crate_src {
            let candidate_rs = src_root.join(format!("{}.rs", sub_path));
            if known_files.contains(&candidate_rs) {
                return Some(candidate_rs);
            }
            let candidate_mod = src_root.join(&sub_path).join("mod.rs");
            if known_files.contains(&candidate_mod) {
                return Some(candidate_mod);
            }
        }
    }

    // `super::...` specifier: search relative to parent module
    if let Some(stripped) = specifier.strip_prefix("super::") {
        let sub_path = stripped.replace("::", "/");
        if let Some(parent) = source_file.parent().and_then(|p| p.parent()) {
            let candidate_rs = parent.join(format!("{}.rs", sub_path));
            if known_files.contains(&candidate_rs) {
                return Some(candidate_rs);
            }
        }
    }

    // Direct module in same directory
    if let Some(dir) = source_file.parent() {
        let candidate_rs = dir.join(format!("{}.rs", path_str));
        if known_files.contains(&candidate_rs) {
            return Some(candidate_rs);
        }
        let candidate_mod = dir.join(&path_str).join("mod.rs");
        if known_files.contains(&candidate_mod) {
            return Some(candidate_mod);
        }
    }

    None
}

/// Resolves a Go package or relative import specifier against workspace files.
fn resolve_go_module(
    source_dir: &Path,
    specifier: &str,
    known_files: &HashSet<PathBuf>,
) -> Option<PathBuf> {
    // Relative imports: `./pkg` or `../pkg`
    if specifier.starts_with('.') {
        let candidate_dir = normalize_path(&source_dir.join(specifier));
        for file in known_files {
            if file.starts_with(&candidate_dir) && file.extension().is_some_and(|e| e == "go") {
                return Some(file.clone());
            }
        }
        return None;
    }

    // Absolute / module package imports: e.g. "myproject/pkg/auth" or "pkg/auth"
    let specifier_path = Path::new(specifier);
    for file in known_files {
        if let Some(parent) = file.parent() {
            if (parent.ends_with(specifier_path) || specifier_path.ends_with(parent))
                && file.extension().is_some_and(|e| e == "go")
            {
                return Some(file.clone());
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_path() {
        let p = Path::new("src/components/../hooks/./useUserData");
        assert_eq!(normalize_path(p), PathBuf::from("src/hooks/useUserData"));
    }

    #[test]
    fn test_resolve_typescript_module() {
        let mut known = HashSet::new();
        known.insert(PathBuf::from("src/hooks/useUserData.ts"));
        known.insert(PathBuf::from("src/types/user.ts"));
        known.insert(PathBuf::from("src/components/Card/index.tsx"));

        let source = Path::new("src/components/UserProfile.tsx");

        let resolved = resolve_module_path(
            source,
            "../hooks/useUserData",
            &known,
            SupportedLanguage::TypeScript,
        );
        assert_eq!(resolved, Some(PathBuf::from("src/hooks/useUserData.ts")));

        let resolved_index =
            resolve_module_path(source, "./Card", &known, SupportedLanguage::TypeScript);
        assert_eq!(
            resolved_index,
            Some(PathBuf::from("src/components/Card/index.tsx"))
        );
    }

    #[test]
    fn test_resolve_python_module() {
        let mut known = HashSet::new();
        known.insert(PathBuf::from("app/auth.py"));
        known.insert(PathBuf::from("app/models.py"));
        known.insert(PathBuf::from("app/db/__init__.py"));

        let source = Path::new("app/routes.py");

        let resolved = resolve_module_path(source, "app.auth", &known, SupportedLanguage::Python);
        assert_eq!(resolved, Some(PathBuf::from("app/auth.py")));

        let resolved_rel =
            resolve_module_path(source, ".models", &known, SupportedLanguage::Python);
        assert_eq!(resolved_rel, Some(PathBuf::from("app/models.py")));

        let resolved_pkg = resolve_module_path(source, "app.db", &known, SupportedLanguage::Python);
        assert_eq!(resolved_pkg, Some(PathBuf::from("app/db/__init__.py")));
    }

    #[test]
    fn test_resolve_go_module() {
        let mut known = HashSet::new();
        known.insert(PathBuf::from("pkg/auth/auth.go"));
        known.insert(PathBuf::from("pkg/models/user.go"));
        known.insert(PathBuf::from("internal/db/postgres.go"));

        let source = Path::new("cmd/server/main.go");

        // Relative import
        let resolved_rel =
            resolve_module_path(source, "../../pkg/auth", &known, SupportedLanguage::Go);
        assert_eq!(resolved_rel, Some(PathBuf::from("pkg/auth/auth.go")));

        // Package / module import matching workspace directory suffix
        let resolved_pkg = resolve_module_path(
            source,
            "myproject/pkg/models",
            &known,
            SupportedLanguage::Go,
        );
        assert_eq!(resolved_pkg, Some(PathBuf::from("pkg/models/user.go")));
    }
}
