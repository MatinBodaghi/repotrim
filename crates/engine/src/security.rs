//! # Security Boundary & Filesystem Isolation
//!
//! Provides `RootGuard` for path confinement and canonicalization, preventing
//! unauthorized filesystem access, path traversal attacks, and symlink escapes
//! across CLI and Model Context Protocol (MCP) interactions.

use std::fs;
use std::path::{Component, Path, PathBuf};
use thiserror::Error;

/// Security violations and path confinement errors.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum SecurityError {
    /// Attempted path access escapes all configured allowed root directories.
    #[error("Path '{}' escapes allowed root boundaries: {allowed_roots:?}", requested.display())]
    PathEscapesRoot {
        requested: PathBuf,
        allowed_roots: Vec<PathBuf>,
    },

    /// The specified path does not exist on the filesystem.
    #[error("Path '{}' was not found on the filesystem", requested.display())]
    PathNotFound { requested: PathBuf },

    /// An I/O error occurred while resolving or canonicalizing the path.
    #[error("I/O error during security path resolution: {0}")]
    IoError(String),

    /// A subprocess argument violates strict sanitization rules.
    #[error("Prohibited argument in subprocess invocation: '{0}'")]
    InvalidSubprocessArgument(String),
}

/// Enforces canonical directory boundaries to prevent path traversal and arbitrary filesystem reads.
#[derive(Debug, Clone)]
pub struct RootGuard {
    allowed_roots: Vec<PathBuf>,
}

impl Default for RootGuard {
    fn default() -> Self {
        Self::from_current_dir()
    }
}

impl RootGuard {
    /// Creates a new `RootGuard` from an explicit list of allowed root directories.
    ///
    /// Non-existent directories are canonicalized as far as possible or stored normalized.
    pub fn new<I, P>(roots: I) -> Self
    where
        I: IntoIterator<Item = P>,
        P: AsRef<Path>,
    {
        let mut allowed = Vec::new();
        for r in roots {
            let p = r.as_ref();
            let canonical = canonicalize_clean(p).unwrap_or_else(|_| normalize_path_lexical(p));
            if !allowed.iter().any(|existing: &PathBuf| {
                path_starts_with(existing, &canonical) && path_starts_with(&canonical, existing)
            }) {
                allowed.push(canonical);
            }
        }

        if allowed.is_empty() {
            allowed.push(Self::default_root());
        }

        Self {
            allowed_roots: allowed,
        }
    }

    /// Creates a `RootGuard` containing only the current working directory.
    pub fn from_current_dir() -> Self {
        Self {
            allowed_roots: vec![Self::default_root()],
        }
    }

    /// Creates a `RootGuard` containing a single specified root.
    pub fn with_root<P: AsRef<Path>>(root: P) -> Self {
        Self::new(std::iter::once(root))
    }

    /// Adds an additional allowed root directory to the guard.
    pub fn add_allowed_root<P: AsRef<Path>>(&mut self, root: P) {
        let canonical = canonicalize_clean(root.as_ref())
            .unwrap_or_else(|_| normalize_path_lexical(root.as_ref()));
        if !self.allowed_roots.contains(&canonical) {
            self.allowed_roots.push(canonical);
        }
    }

    /// Returns a slice of the canonical allowed roots.
    pub fn allowed_roots(&self) -> &[PathBuf] {
        &self.allowed_roots
    }

    /// Resolves and validates a requested path against the configured allowed roots.
    ///
    /// Relative paths are anchored against the primary (first) allowed root.
    /// Symbolic links and `..` traversal components are canonicalized before validation.
    pub fn resolve<P: AsRef<Path>>(&self, requested: P) -> Result<PathBuf, SecurityError> {
        let raw = requested.as_ref();
        let primary_root = self
            .allowed_roots
            .first()
            .cloned()
            .unwrap_or_else(Self::default_root);

        // Anchor relative paths to primary root
        let absolute_candidate = if raw.is_absolute() {
            raw.to_path_buf()
        } else {
            primary_root.join(raw)
        };

        // Canonicalize with symlink resolution
        let canonical = match canonicalize_clean(&absolute_candidate) {
            Ok(canon) => canon,
            Err(_) => {
                // If path does not exist, resolve existing parent + normalize remaining
                resolve_nonexistent_safely(&absolute_candidate)?
            }
        };

        // Validate that the canonical path is contained within at least one allowed root
        for root in &self.allowed_roots {
            if path_starts_with(&canonical, root) {
                return Ok(canonical);
            }
        }

        Err(SecurityError::PathEscapesRoot {
            requested: raw.to_path_buf(),
            allowed_roots: self.allowed_roots.clone(),
        })
    }

    /// Quick boolean check whether a path is contained within the allowed roots.
    pub fn is_allowed<P: AsRef<Path>>(&self, path: P) -> bool {
        self.resolve(path).is_ok()
    }

    fn default_root() -> PathBuf {
        std::env::current_dir()
            .map(|p| canonicalize_clean(&p).unwrap_or(p))
            .unwrap_or_else(|_| PathBuf::from("."))
    }
}

/// Normalizes Windows verbatim prefixes (`\\?\`) and canonicalizes existing paths.
pub fn canonicalize_clean<P: AsRef<Path>>(path: P) -> Result<PathBuf, std::io::Error> {
    let canon = fs::canonicalize(path)?;
    Ok(normalize_verbatim(canon))
}

/// Strips Windows verbatim prefix (`\\?\`) if present.
pub fn normalize_verbatim(path: PathBuf) -> PathBuf {
    let s = path.to_string_lossy();
    if let Some(stripped) = s.strip_prefix(r"\\?\") {
        PathBuf::from(stripped)
    } else {
        path
    }
}

/// Performs purely lexical normalization of path components without touching the filesystem.
pub fn normalize_path_lexical<P: AsRef<Path>>(path: P) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.as_ref().components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            c => out.push(c.as_os_str()),
        }
    }
    out
}

/// Checks if `target` resides within `root`, accounting for Windows casing and separators.
fn path_starts_with(target: &Path, root: &Path) -> bool {
    if target.starts_with(root) {
        return true;
    }

    #[cfg(windows)]
    {
        let target_str = target.to_string_lossy().to_lowercase().replace('/', "\\");
        let root_str = root.to_string_lossy().to_lowercase().replace('/', "\\");
        let trimmed_root = root_str.trim_end_matches('\\');

        if target_str == trimmed_root {
            return true;
        }

        if let Some(remainder) = target_str.strip_prefix(trimmed_root) {
            return remainder.starts_with('\\');
        }
    }

    false
}

/// Resolves a non-existent path by canonicalizing its closest existing ancestor.
fn resolve_nonexistent_safely(path: &Path) -> Result<PathBuf, SecurityError> {
    let mut ancestors = Vec::new();
    let mut current = Some(path);

    while let Some(p) = current {
        if p.exists() {
            let canon = canonicalize_clean(p).map_err(|e| SecurityError::IoError(e.to_string()))?;
            let mut result = canon;
            for segment in ancestors.into_iter().rev() {
                result.push(segment);
            }
            return Ok(normalize_path_lexical(result));
        }
        if let Some(file_name) = p.file_name() {
            ancestors.push(file_name.to_os_string());
        }
        current = p.parent();
    }

    // Fall back to lexical normalization if no ancestor exists
    Ok(normalize_path_lexical(path))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;

    #[test]
    fn test_root_guard_allows_contained_path() {
        let temp = std::env::temp_dir().join(format!("repotrim_sec_test_{}", std::process::id()));
        fs::create_dir_all(temp.join("subdir")).unwrap();
        let file_path = temp.join("subdir").join("test.rs");
        File::create(&file_path).unwrap();

        let guard = RootGuard::with_root(&temp);
        let resolved = guard.resolve(&file_path);
        assert!(resolved.is_ok());

        let _ = fs::remove_dir_all(&temp);
    }

    #[test]
    fn test_root_guard_blocks_parent_escape() {
        let temp = std::env::temp_dir().join(format!("repotrim_sec_escape_{}", std::process::id()));
        let inside = temp.join("inside");
        fs::create_dir_all(&inside).unwrap();

        let guard = RootGuard::with_root(&inside);
        let escape_attempt = inside.join("../outside.rs");

        let result = guard.resolve(&escape_attempt);
        assert!(matches!(result, Err(SecurityError::PathEscapesRoot { .. })));

        let _ = fs::remove_dir_all(&temp);
    }

    #[test]
    fn test_root_guard_blocks_external_absolute_path() {
        let temp = std::env::temp_dir().join(format!("repotrim_sec_ext_{}", std::process::id()));
        fs::create_dir_all(&temp).unwrap();

        let guard = RootGuard::with_root(&temp);
        // An arbitrary external path
        #[cfg(windows)]
        let outside = PathBuf::from(r"C:\Windows\System32");
        #[cfg(not(windows))]
        let outside = PathBuf::from("/etc/passwd");

        if outside.exists() {
            let result = guard.resolve(&outside);
            assert!(matches!(result, Err(SecurityError::PathEscapesRoot { .. })));
        }

        let _ = fs::remove_dir_all(&temp);
    }

    #[test]
    fn test_multiple_allowed_roots() {
        let temp1 = std::env::temp_dir().join(format!("repotrim_sec_m1_{}", std::process::id()));
        let temp2 = std::env::temp_dir().join(format!("repotrim_sec_m2_{}", std::process::id()));
        fs::create_dir_all(&temp1).unwrap();
        fs::create_dir_all(&temp2).unwrap();

        let guard = RootGuard::new(vec![&temp1, &temp2]);

        let file1 = temp1.join("a.rs");
        let file2 = temp2.join("b.rs");
        File::create(&file1).unwrap();
        File::create(&file2).unwrap();

        assert!(guard.resolve(&file1).is_ok());
        assert!(guard.resolve(&file2).is_ok());

        let _ = fs::remove_dir_all(&temp1);
        let _ = fs::remove_dir_all(&temp2);
    }
}
