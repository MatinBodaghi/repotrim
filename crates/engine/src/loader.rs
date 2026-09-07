use crate::{
    compute_blake3_hash, get_mtime_nanos, AstExtractor, EngineError, FileCacheEntry, LayerWeights,
    MultiplexGraph, ReferenceEdge, RepositoryCache, SymbolNode,
};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

/// Directories ignored by default during repository scanning.
const IGNORED_DIRS: &[&str] = &[
    ".git",
    "target",
    "node_modules",
    ".cargo",
    "dist",
    "build",
    ".idea",
    ".vscode",
    ".gemini",
    ".repotrim",
];

/// Metrics describing cache hits and recomputed files during repository ingestion.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CacheReport {
    pub total_files: usize,
    pub cached_files: usize,
    pub recomputed_files: usize,
    pub hit_ratio: f32,
}

/// Encapsulates parsed repository files, AST symbol/edge data, and cache diagnostics.
#[derive(Debug)]
pub struct LoadedRepository {
    pub root_path: PathBuf,
    pub file_sources: HashMap<PathBuf, String>,
    pub symbols: Vec<SymbolNode>,
    pub edges: Vec<ReferenceEdge>,
    pub total_bytes: usize,
    pub cache_report: CacheReport,
}

impl LoadedRepository {
    /// Recursively scans a root directory and parses all supported source files,
    /// using `.repotrim/cache.bin` by default.
    pub fn load<P: AsRef<Path>>(root: P) -> Result<Self, EngineError> {
        Self::load_with_options(root, true)
    }

    /// Recursively scans a root directory with optional caching.
    pub fn load_with_options<P: AsRef<Path>>(
        root: P,
        use_cache: bool,
    ) -> Result<Self, EngineError> {
        let root_path = if root.as_ref().is_relative() {
            std::env::current_dir()
                .map_err(EngineError::IoError)?
                .join(root.as_ref())
        } else {
            root.as_ref().to_path_buf()
        };

        let cache_dir = root_path.join(".repotrim");
        let cache_file = cache_dir.join("cache.bin");

        let mut cache = if use_cache {
            RepositoryCache::load_from_file(&cache_file)
        } else {
            RepositoryCache::new()
        };

        let mut source_files = Vec::new();
        scan_directory(&root_path, &root_path, &mut source_files)?;
        source_files.sort_by(|a, b| a.1.cmp(&b.1));

        let mut existing_paths = HashSet::with_capacity(source_files.len());
        let mut cached_files = 0;
        let mut recomputed_files = 0;
        let mut extractor_opt: Option<AstExtractor> = None;
        let mut file_sources = HashMap::new();
        let mut total_bytes = 0usize;

        for (abs_path, rel_path) in &source_files {
            existing_paths.insert(rel_path.clone());
            let metadata = fs::metadata(abs_path).map_err(EngineError::IoError)?;
            let mtime = get_mtime_nanos(&metadata);

            let mut is_cached = false;
            if use_cache {
                if let Some(entry) = cache.get_valid_entry(rel_path, mtime, None) {
                    is_cached = true;
                    total_bytes += entry.source_bytes;
                }
            }

            if is_cached {
                cached_files += 1;
            } else {
                let content_bytes = fs::read(abs_path).map_err(EngineError::IoError)?;
                let content_hash = compute_blake3_hash(&content_bytes);
                total_bytes += content_bytes.len();

                let content_str = String::from_utf8_lossy(&content_bytes).to_string();
                file_sources.insert(rel_path.clone(), content_str);

                if use_cache {
                    if let Some(entry) = cache.get_valid_entry(rel_path, mtime, Some(content_hash))
                    {
                        let mut updated_entry = entry.clone();
                        updated_entry.mtime_nanos = mtime;
                        cache.insert(updated_entry);
                        cached_files += 1;
                        is_cached = true;
                    }
                }

                if !is_cached {
                    recomputed_files += 1;
                    if extractor_opt.is_none() {
                        extractor_opt = Some(AstExtractor::new()?);
                    }
                    let extractor = extractor_opt.as_ref().unwrap();

                    let mut dummy_id = 0u32;
                    let (file_symbols, file_edges) =
                        extractor.parse_file(rel_path, &content_bytes, &mut dummy_id)?;

                    let entry = FileCacheEntry {
                        relative_path: rel_path.clone(),
                        blake3_hash: content_hash,
                        mtime_nanos: mtime,
                        symbols: file_symbols,
                        edges: file_edges,
                        source_bytes: content_bytes.len(),
                    };
                    cache.insert(entry);
                }
            }
        }

        let initial_entry_count = cache.entries.len();
        cache.retain_existing(&existing_paths);
        let entries_pruned = initial_entry_count != cache.entries.len();

        if use_cache && (recomputed_files > 0 || entries_pruned) {
            let _ = cache.save_to_file(&cache_file);
        }

        let ordered_paths: Vec<PathBuf> = source_files.into_iter().map(|(_, rel)| rel).collect();
        let (symbols, edges) = cache.compile_symbols_and_edges(&ordered_paths);

        let total_files = ordered_paths.len();
        let hit_ratio = if total_files > 0 {
            cached_files as f32 / total_files as f32
        } else {
            0.0
        };

        let cache_report = CacheReport {
            total_files,
            cached_files,
            recomputed_files,
            hit_ratio,
        };

        Ok(Self {
            root_path,
            file_sources,
            symbols,
            edges,
            total_bytes,
            cache_report,
        })
    }

    /// Loads source file contents for specified file paths if not already loaded into memory.
    pub fn load_sources_for_files<I, P>(&mut self, paths: I) -> Result<(), EngineError>
    where
        I: IntoIterator<Item = P>,
        P: AsRef<Path>,
    {
        for p in paths {
            let p_ref = p.as_ref();
            if !self.file_sources.contains_key(p_ref) {
                let abs = self.root_path.join(p_ref);
                if let Ok(content) = fs::read_to_string(&abs) {
                    self.file_sources.insert(p_ref.to_path_buf(), content);
                }
            }
        }
        Ok(())
    }

    /// Loads all repository source files into memory.
    pub fn load_all_sources(&mut self) -> Result<(), EngineError> {
        let paths: Vec<PathBuf> = self.symbols.iter().map(|s| s.file_path.clone()).collect();
        self.load_sources_for_files(&paths)
    }

    /// Constructs the in-memory MultiplexGraph using default layer weights.
    pub fn build_graph(&self) -> MultiplexGraph {
        MultiplexGraph::build(self.symbols.clone(), &self.edges, LayerWeights::default())
    }
}

fn scan_directory(
    root: &Path,
    current_dir: &Path,
    files: &mut Vec<(PathBuf, PathBuf)>,
) -> Result<(), EngineError> {
    let entries = fs::read_dir(current_dir).map_err(EngineError::IoError)?;

    for entry in entries.flatten() {
        let path = entry.path();
        let file_name = entry.file_name();
        let file_name_str = file_name.to_string_lossy();

        if path.is_dir() {
            if !IGNORED_DIRS.contains(&file_name_str.as_ref()) {
                scan_directory(root, &path, files)?;
            }
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            let rel_path = path.strip_prefix(root).unwrap_or(&path).to_path_buf();
            files.push((path, rel_path));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;

    #[test]
    fn test_scan_and_load_repository() {
        let temp_dir = std::env::temp_dir().join(format!("repotrim_test_{}", std::process::id()));
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(temp_dir.join("src")).unwrap();
        fs::create_dir_all(temp_dir.join("target")).unwrap(); // Should be ignored
        fs::create_dir_all(temp_dir.join(".repotrim")).unwrap(); // Should be ignored

        let file1 = temp_dir.join("src/lib.rs");
        let mut f1 = File::create(&file1).unwrap();
        writeln!(f1, "pub fn hello() -> u32 {{ 42 }}").unwrap();

        let file2 = temp_dir.join("target/ignored.rs");
        let mut f2 = File::create(&file2).unwrap();
        writeln!(f2, "pub fn should_be_ignored() {{}}").unwrap();

        // First run: cold load (creates cache)
        let repo_cold = LoadedRepository::load(&temp_dir).expect("Failed to load repo cold");
        assert_eq!(repo_cold.file_sources.len(), 1);
        assert!(repo_cold.file_sources.keys().any(|p| p.ends_with("lib.rs")));
        assert_eq!(repo_cold.symbols.len(), 1);
        assert_eq!(repo_cold.symbols[0].name, "hello");
        assert_eq!(repo_cold.cache_report.recomputed_files, 1);
        assert_eq!(repo_cold.cache_report.cached_files, 0);

        let graph = repo_cold.build_graph();
        assert_eq!(graph.num_symbols(), 1);

        // Second run: warm load (100% cache hit)
        let mut repo_warm = LoadedRepository::load(&temp_dir).expect("Failed to load repo warm");
        assert_eq!(repo_warm.symbols.len(), 1);
        assert_eq!(repo_warm.symbols[0].name, "hello");
        assert_eq!(repo_warm.cache_report.recomputed_files, 0);
        assert_eq!(repo_warm.cache_report.cached_files, 1);
        assert_eq!(repo_warm.cache_report.hit_ratio, 1.0);

        // Verify lazy source loading
        repo_warm
            .load_all_sources()
            .expect("Failed to load sources");
        assert_eq!(repo_warm.file_sources.len(), 1);

        let _ = fs::remove_dir_all(&temp_dir);
    }
}
