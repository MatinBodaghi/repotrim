use bincode::Options;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use crate::error::EngineError;
use crate::import::FileImport;
use crate::symbol::{ReferenceEdge, SymbolId, SymbolNode};

/// Maximum allowable bincode payload size (64 MB) to prevent unbounded memory allocation.
pub const MAX_CACHE_SIZE_BYTES: u64 = 64 * 1024 * 1024;

/// Constructs standard hardened bincode options with a 64 MB size limit.
fn bincode_options() -> impl bincode::Options {
    bincode::DefaultOptions::new()
        .with_limit(MAX_CACHE_SIZE_BYTES)
        .allow_trailing_bytes()
}

/// Current cache schema version. Incremented whenever the binary structure changes.
pub const CACHE_VERSION: u32 = 4;

/// Returns the externalized user cache directory for a given codebase root,
/// preventing any unsolicited cache directory creation inside the analyzed codebase.
pub fn get_cache_dir_for_root(root: &Path) -> PathBuf {
    let canonical = crate::security::canonicalize_clean(root)
        .unwrap_or_else(|_| crate::security::normalize_path_lexical(root));
    let hash = blake3::hash(canonical.to_string_lossy().as_bytes());
    let hash_hex = hash.to_hex();
    let base_cache = dirs::cache_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("repotrim");
    base_cache.join(&hash_hex[..16])
}

/// Returns the path to `cache.bin` within the externalized cache directory for the given root.
pub fn get_cache_file_for_root(root: &Path) -> PathBuf {
    get_cache_dir_for_root(root).join("cache.bin")
}

/// Returns the path to `coedit.bin` within the externalized cache directory for the given root.
pub fn get_coedit_file_for_root(root: &Path) -> PathBuf {
    get_cache_dir_for_root(root).join("coedit.bin")
}

/// Extracted AST symbol and edge metadata cached per individual source file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileCacheEntry {
    pub relative_path: PathBuf,
    pub blake3_hash: [u8; 32],
    pub mtime_nanos: u128,
    pub symbols: Vec<SymbolNode>,
    pub edges: Vec<ReferenceEdge>,
    pub imports: Vec<FileImport>,
    pub source_bytes: usize,
}

/// Global repository cache containing per-file Merkle fingerprints and parsed AST symbols.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepositoryCache {
    pub version: u32,
    pub entries: HashMap<PathBuf, FileCacheEntry>,
}

/// Type alias for compiled repository symbols, reference edges, and AST imports.
pub type CompiledRepositoryData = (Vec<SymbolNode>, Vec<ReferenceEdge>, Vec<FileImport>);

impl Default for RepositoryCache {
    fn default() -> Self {
        Self::new()
    }
}

impl RepositoryCache {
    /// Creates an empty repository cache with the current schema version.
    pub fn new() -> Self {
        Self {
            version: CACHE_VERSION,
            entries: HashMap::new(),
        }
    }

    /// Loads the repository cache from disk using bincode.
    ///
    /// If the file does not exist, is corrupted, or has an outdated schema version,
    /// a fresh empty cache is returned without error.
    pub fn load_from_file<P: AsRef<Path>>(path: P) -> Self {
        let path_ref = path.as_ref();
        if !path_ref.exists() {
            return Self::new();
        }

        let bytes = match fs::read(path_ref) {
            Ok(b) => b,
            Err(_) => return Self::new(),
        };

        match bincode_options().deserialize::<RepositoryCache>(&bytes) {
            Ok(cache) if cache.version == CACHE_VERSION => cache,
            _ => Self::new(),
        }
    }

    /// Atomically persists the repository cache to disk using bincode.
    pub fn save_to_file<P: AsRef<Path>>(&self, path: P) -> Result<(), EngineError> {
        let path_ref = path.as_ref();
        if let Some(parent) = path_ref.parent() {
            fs::create_dir_all(parent).map_err(EngineError::IoError)?;
        }

        let encoded = bincode_options()
            .serialize(self)
            .map_err(|e| EngineError::IoError(std::io::Error::other(e)))?;

        // Write to a temporary sibling file and atomically rename to avoid partial writes
        let temp_path = path_ref.with_extension("tmp");
        fs::write(&temp_path, encoded).map_err(EngineError::IoError)?;
        fs::rename(&temp_path, path_ref).map_err(EngineError::IoError)?;

        Ok(())
    }

    /// Checks if a file's cache entry matches the given BLAKE3 content hash.
    ///
    /// Deprecates modification-time-only trust: the disk bytes' BLAKE3 hash must
    /// match the stored BLAKE3 hash to guarantee integrity against poisoned or tampered files.
    pub fn get_valid_entry(
        &self,
        rel_path: &Path,
        content_hash: [u8; 32],
    ) -> Option<&FileCacheEntry> {
        let entry = self.entries.get(rel_path)?;
        if entry.blake3_hash == content_hash {
            Some(entry)
        } else {
            None
        }
    }

    /// Inserts or updates a file's cache entry.
    pub fn insert(&mut self, entry: FileCacheEntry) {
        self.entries.insert(entry.relative_path.clone(), entry);
    }

    /// Removes cache entries for files that no longer exist on disk.
    pub fn retain_existing(&mut self, existing_paths: &HashSet<PathBuf>) {
        self.entries.retain(|k, _| existing_paths.contains(k));
    }

    /// Compiles all cached symbols and edges across the specified file order,
    /// re-indexing symbols with dense contiguous SymbolIds `0..N`.
    pub fn compile_symbols_and_edges(
        &self,
        ordered_paths: &[PathBuf],
    ) -> (Vec<SymbolNode>, Vec<ReferenceEdge>) {
        let (symbols, edges, _) = self.compile_symbols_edges_and_imports(ordered_paths);
        (symbols, edges)
    }

    /// Compiles all cached symbols, edges, and imports across the specified file order,
    /// re-indexing symbols with dense contiguous SymbolIds `0..N`.
    pub fn compile_symbols_edges_and_imports(
        &self,
        ordered_paths: &[PathBuf],
    ) -> CompiledRepositoryData {
        let mut compiled_symbols = Vec::new();
        let mut compiled_edges = Vec::new();
        let mut compiled_imports = Vec::new();
        let mut current_id = 0u32;

        for path in ordered_paths {
            if let Some(entry) = self.entries.get(path) {
                let mut id_map = HashMap::with_capacity(entry.symbols.len());

                for sym in &entry.symbols {
                    let new_id = SymbolId(current_id);
                    current_id += 1;
                    id_map.insert(sym.id, new_id);

                    let mut remapped_sym = sym.clone();
                    remapped_sym.id = new_id;
                    compiled_symbols.push(remapped_sym);
                }

                for edge in &entry.edges {
                    if let Some(&new_src) = id_map.get(&edge.source) {
                        let mut remapped_edge = edge.clone();
                        remapped_edge.source = new_src;
                        compiled_edges.push(remapped_edge);
                    }
                }

                compiled_imports.extend(entry.imports.clone());
            }
        }

        (compiled_symbols, compiled_edges, compiled_imports)
    }
}

/// Extracts file modification time as nanoseconds since UNIX epoch.
pub fn get_mtime_nanos(metadata: &fs::Metadata) -> u128 {
    metadata
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok().map(|d| d.as_nanos()))
        .unwrap_or(0)
}

/// Computes the 32-byte BLAKE3 cryptographic hash of a byte slice.
pub fn compute_blake3_hash(bytes: &[u8]) -> [u8; 32] {
    *blake3::hash(bytes).as_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::symbol::{EdgeKind, SymbolKind, TextSpan};

    fn dummy_symbol(id: u32, name: &str, file: &str) -> SymbolNode {
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
    fn test_cache_serialization_roundtrip() {
        let temp_dir =
            std::env::temp_dir().join(format!("repotrim_cache_test_{}", std::process::id()));
        let cache_file = temp_dir.join(".repotrim/cache.bin");

        let mut cache = RepositoryCache::new();
        let path_a = PathBuf::from("src/a.rs");
        let sym_a = dummy_symbol(0, "foo", "src/a.rs");
        let edge_a = ReferenceEdge {
            source: SymbolId(0),
            target_ident: "bar".to_string(),
            kind: EdgeKind::Call,
        };

        cache.insert(FileCacheEntry {
            relative_path: path_a.clone(),
            blake3_hash: [1u8; 32],
            mtime_nanos: 123456789,
            symbols: vec![sym_a],
            edges: vec![edge_a],
            imports: vec![],
            source_bytes: 50,
        });

        cache
            .save_to_file(&cache_file)
            .expect("Failed to save cache");
        assert!(cache_file.exists());

        let loaded = RepositoryCache::load_from_file(&cache_file);
        assert_eq!(loaded.version, CACHE_VERSION);
        assert_eq!(loaded.entries.len(), 1);

        let entry = loaded.entries.get(&path_a).expect("Entry not found");
        assert_eq!(entry.mtime_nanos, 123456789);
        assert_eq!(entry.symbols[0].name, "foo");

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_compile_symbols_contiguous_reindexing() {
        let mut cache = RepositoryCache::new();

        let path_a = PathBuf::from("src/a.rs");
        let path_b = PathBuf::from("src/b.rs");

        // File A has symbols with IDs [10, 20]
        let sym_a1 = dummy_symbol(10, "foo", "src/a.rs");
        let sym_a2 = dummy_symbol(20, "bar", "src/a.rs");
        let edge_a = ReferenceEdge {
            source: SymbolId(10),
            target_ident: "bar".to_string(),
            kind: EdgeKind::Call,
        };

        // File B has symbol with ID [99]
        let sym_b1 = dummy_symbol(99, "baz", "src/b.rs");
        let edge_b = ReferenceEdge {
            source: SymbolId(99),
            target_ident: "foo".to_string(),
            kind: EdgeKind::Call,
        };

        cache.insert(FileCacheEntry {
            relative_path: path_a.clone(),
            blake3_hash: [1u8; 32],
            mtime_nanos: 100,
            symbols: vec![sym_a1, sym_a2],
            edges: vec![edge_a],
            imports: vec![],
            source_bytes: 100,
        });

        cache.insert(FileCacheEntry {
            relative_path: path_b.clone(),
            blake3_hash: [2u8; 32],
            mtime_nanos: 200,
            symbols: vec![sym_b1],
            edges: vec![edge_b],
            imports: vec![],
            source_bytes: 100,
        });

        let (symbols, edges) = cache.compile_symbols_and_edges(&[path_a, path_b]);

        assert_eq!(symbols.len(), 3);
        assert_eq!(symbols[0].id, SymbolId(0));
        assert_eq!(symbols[1].id, SymbolId(1));
        assert_eq!(symbols[2].id, SymbolId(2));

        assert_eq!(edges.len(), 2);
        // edge from foo (was 10 -> now 0)
        assert_eq!(edges[0].source, SymbolId(0));
        // edge from baz (was 99 -> now 2)
        assert_eq!(edges[1].source, SymbolId(2));
    }

    #[test]
    fn test_get_valid_entry_blake3_verification() {
        let mut cache = RepositoryCache::new();
        let path = PathBuf::from("src/test.rs");
        let sym = dummy_symbol(1, "test_fn", "src/test.rs");
        let hash = [42u8; 32];

        cache.insert(FileCacheEntry {
            relative_path: path.clone(),
            blake3_hash: hash,
            mtime_nanos: 1000,
            symbols: vec![sym],
            edges: vec![],
            imports: vec![],
            source_bytes: 50,
        });

        // Exact match
        assert!(cache.get_valid_entry(&path, hash).is_some());

        // Mismatched hash returns None even if path exists
        let forged_hash = [99u8; 32];
        assert!(cache.get_valid_entry(&path, forged_hash).is_none());

        // Unknown path returns None
        assert!(cache
            .get_valid_entry(&PathBuf::from("src/other.rs"), hash)
            .is_none());
    }

    #[test]
    fn test_bincode_options_limit_rejection() {
        let temp_dir =
            std::env::temp_dir().join(format!("repotrim_bincode_test_{}", std::process::id()));
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();
        let cache_file = temp_dir.join("cache.bin");

        // Malicious/corrupted bincode payload
        let malicious_bytes = vec![0xFF; 1024];
        fs::write(&cache_file, &malicious_bytes).unwrap();

        // Loading should safely fall back to an empty cache without panicking
        let cache = RepositoryCache::load_from_file(&cache_file);
        assert_eq!(cache.entries.len(), 0);
        assert_eq!(cache.version, CACHE_VERSION);

        let _ = fs::remove_dir_all(&temp_dir);
    }
}

