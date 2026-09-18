use crate::{
    compute_blake3_hash, get_cache_file_for_root, get_mtime_nanos, AstExtractor,
    CodebaseIntelligence, EngineError, FileCacheEntry, FileImport, LayerWeights, MultiplexGraph,
    ReferenceEdge, RepositoryCache, SupportedLanguage, SymbolNode,
};
use ignore::WalkBuilder;
use rayon::prelude::*;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

/// Directories ignored by default during repository scanning and watching.
pub const IGNORED_DIRS: &[&str] = &[
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
    ".venv",
    "venv",
    "__pycache__",
    "vendor",
    ".next",
    "out",
    "coverage",
    ".tox",
    "Pods",
];

/// Default maximum file size (2 MB) before skipping parsing to prevent out-of-memory errors.
pub const DEFAULT_MAX_FILE_BYTES: usize = 2 * 1024 * 1024;

/// Metrics describing cache hits and recomputed files during repository ingestion.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CacheReport {
    pub total_files: usize,
    pub cached_files: usize,
    pub recomputed_files: usize,
    pub skipped_files: usize,
    pub hit_ratio: f32,
}

enum ProcessedFile {
    Cached {
        rel_path: PathBuf,
        mtime: u128,
        needs_mtime_update: bool,
        cached_entry: FileCacheEntry,
    },
    Recomputed {
        rel_path: PathBuf,
        content_str: String,
        entry: FileCacheEntry,
    },
}

/// Encapsulates parsed repository files, AST symbol/edge data, and cache diagnostics.
#[derive(Debug)]
pub struct LoadedRepository {
    pub root_path: PathBuf,
    pub file_sources: HashMap<PathBuf, String>,
    pub symbols: Vec<SymbolNode>,
    pub edges: Vec<ReferenceEdge>,
    pub imports: Vec<FileImport>,
    pub total_bytes: usize,
    pub cache_report: CacheReport,
    pub cache: RepositoryCache,
    pub max_file_bytes: usize,
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
        Self::load_with_config(root, use_cache, DEFAULT_MAX_FILE_BYTES)
    }

    /// Recursively scans a root directory with optional caching and configurable max file size limit.
    pub fn load_with_config<P: AsRef<Path>>(
        root: P,
        use_cache: bool,
        max_file_bytes: usize,
    ) -> Result<Self, EngineError> {
        let root_path = if root.as_ref().is_relative() {
            std::env::current_dir()
                .map_err(EngineError::IoError)?
                .join(root.as_ref())
        } else {
            root.as_ref().to_path_buf()
        };

        #[cfg(not(windows))]
        let root_path = fs::canonicalize(&root_path).unwrap_or(root_path);

        let cache_file = get_cache_file_for_root(&root_path);

        let mut cache = if use_cache {
            RepositoryCache::load_from_file(&cache_file)
        } else {
            RepositoryCache::new()
        };

        let mut source_files = Vec::new();
        scan_directory(&root_path, &mut source_files)?;
        source_files.sort_by(|a, b| a.1.cmp(&b.1));

        let mut accepted_files = Vec::with_capacity(source_files.len());
        let mut skipped_files = 0;

        for (abs_path, rel_path) in source_files {
            let metadata = match fs::symlink_metadata(&abs_path) {
                Ok(m) => m,
                Err(err) => return Err(EngineError::IoError(err)),
            };

            if metadata.file_type().is_symlink() {
                continue;
            }

            if metadata.len() > max_file_bytes as u64 {
                skipped_files += 1;
                continue;
            }

            let mtime = get_mtime_nanos(&metadata);
            accepted_files.push((abs_path, rel_path, mtime));
        }

        let extractor = AstExtractor::new()?;

        let processed_results: Vec<ProcessedFile> = accepted_files
            .par_iter()
            .map(
                |(abs_path, rel_path, mtime)| -> Result<ProcessedFile, EngineError> {
                    let content_bytes = fs::read(abs_path).map_err(EngineError::IoError)?;
                    let content_hash = compute_blake3_hash(&content_bytes);

                    if use_cache {
                        if let Some(entry) = cache.get_valid_entry(rel_path, content_hash) {
                            let needs_mtime_update = entry.mtime_nanos != *mtime;
                            return Ok(ProcessedFile::Cached {
                                rel_path: rel_path.clone(),
                                mtime: *mtime,
                                needs_mtime_update,
                                cached_entry: entry.clone(),
                            });
                        }
                    }

                    let content_str = String::from_utf8_lossy(&content_bytes).to_string();
                    let mut dummy_id = 0u32;
                    let (file_symbols, file_edges, file_imports) = extractor
                        .parse_file_with_imports(rel_path, &content_bytes, &mut dummy_id)?;

                    let entry = FileCacheEntry {
                        relative_path: rel_path.clone(),
                        blake3_hash: content_hash,
                        mtime_nanos: *mtime,
                        symbols: file_symbols,
                        edges: file_edges,
                        imports: file_imports,
                        source_bytes: content_bytes.len(),
                    };

                    Ok(ProcessedFile::Recomputed {
                        rel_path: rel_path.clone(),
                        content_str,
                        entry,
                    })
                },
            )
            .collect::<Result<Vec<_>, EngineError>>()?;

        let mut existing_paths = HashSet::with_capacity(processed_results.len());
        let mut ordered_paths = Vec::with_capacity(processed_results.len());
        let mut cached_files = 0;
        let mut recomputed_files = 0;
        let mut file_sources = HashMap::new();
        let mut total_bytes = 0usize;

        for item in processed_results {
            match item {
                ProcessedFile::Cached {
                    rel_path,
                    mtime,
                    needs_mtime_update,
                    mut cached_entry,
                } => {
                    cached_files += 1;
                    total_bytes += cached_entry.source_bytes;
                    existing_paths.insert(rel_path.clone());
                    ordered_paths.push(rel_path);

                    if needs_mtime_update {
                        cached_entry.mtime_nanos = mtime;
                        cache.insert(cached_entry);
                    }
                }
                ProcessedFile::Recomputed {
                    rel_path,
                    content_str,
                    entry,
                } => {
                    recomputed_files += 1;
                    total_bytes += entry.source_bytes;
                    existing_paths.insert(rel_path.clone());
                    ordered_paths.push(rel_path.clone());
                    file_sources.insert(rel_path, content_str);
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

        let (symbols, edges, imports) = cache.compile_symbols_edges_and_imports(&ordered_paths);

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
            skipped_files,
            hit_ratio,
        };

        Ok(Self {
            root_path,
            file_sources,
            symbols,
            edges,
            imports,
            total_bytes,
            cache_report,
            cache,
            max_file_bytes,
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

    /// Loads source file contents on-demand only for the requested symbols,
    /// avoiding full repository memory residency.
    pub fn load_sources_for_symbols<'a, I>(&mut self, symbols: I) -> Result<(), EngineError>
    where
        I: IntoIterator<Item = &'a SymbolNode>,
    {
        let paths: HashSet<PathBuf> = symbols.into_iter().map(|s| s.file_path.clone()).collect();
        self.load_sources_for_files(&paths)
    }

    /// Reads a specific source text slice for a symbol directly by byte offset from disk,
    /// avoiding full-file in-memory residency.
    pub fn read_symbol_source(&self, symbol: &SymbolNode) -> Result<String, EngineError> {
        let abs_path = self.root_path.join(&symbol.file_path);
        let mut file = fs::File::open(&abs_path).map_err(EngineError::IoError)?;
        use std::io::{Read, Seek, SeekFrom};
        if symbol.span.start_byte >= symbol.span.end_byte {
            return Ok(String::new());
        }
        let len = symbol.span.end_byte - symbol.span.start_byte;
        file.seek(SeekFrom::Start(symbol.span.start_byte as u64))
            .map_err(EngineError::IoError)?;
        let mut buffer = vec![0u8; len];
        file.read_exact(&mut buffer).map_err(EngineError::IoError)?;
        Ok(String::from_utf8_lossy(&buffer).to_string())
    }

    /// Constructs the in-memory MultiplexGraph using default layer weights.
    pub fn build_graph(&self) -> MultiplexGraph {
        MultiplexGraph::build_with_imports(
            self.symbols.clone(),
            &self.edges,
            &self.imports,
            LayerWeights::default(),
        )
    }

    /// Constructs the in-memory MultiplexGraph using custom layer weights.
    pub fn build_graph_with_weights(&self, weights: LayerWeights) -> MultiplexGraph {
        MultiplexGraph::build_with_imports(
            self.symbols.clone(),
            &self.edges,
            &self.imports,
            weights,
        )
    }

    /// Constructs the in-memory MultiplexGraph fusing mined Git co-edit edges and layer weights.
    pub fn build_graph_with_coedits(
        &self,
        coedit_edges: &[(crate::symbol::SymbolId, crate::symbol::SymbolId, f32)],
        weights: LayerWeights,
    ) -> MultiplexGraph {
        MultiplexGraph::build_with_all(
            self.symbols.clone(),
            &self.edges,
            &self.imports,
            coedit_edges,
            weights,
        )
    }

    /// Creates a `CodebaseIntelligence` primitives engine over this loaded repository and a graph.
    pub fn intelligence<'a>(&'a self, graph: &'a MultiplexGraph) -> CodebaseIntelligence<'a> {
        CodebaseIntelligence::new(graph, &self.file_sources)
    }

    /// Converts an absolute or relative path to a repository-relative path,
    /// resolving symlinks and canonical paths (e.g. macOS /var -> /private/var).
    pub fn to_relative_path(&self, path: &Path) -> Option<PathBuf> {
        if !path.is_absolute() {
            return Some(path.to_path_buf());
        }
        if let Ok(p) = path.strip_prefix(&self.root_path) {
            return Some(p.to_path_buf());
        }
        if let Ok(canonical_root) = fs::canonicalize(&self.root_path) {
            if let Ok(p) = path.strip_prefix(&canonical_root) {
                return Some(p.to_path_buf());
            }
            if let Ok(canonical_path) = fs::canonicalize(path) {
                if let Ok(p) = canonical_path.strip_prefix(&canonical_root) {
                    return Some(p.to_path_buf());
                }
            }
        }
        None
    }

    /// Incrementally parses or updates a single file without rescanning the whole repository.
    ///
    /// Returns `Ok(true)` if the file was modified and symbols were re-indexed;
    /// `Ok(false)` if the file is ignored, unchanged, or not a supported language.
    pub fn patch_file<P: AsRef<Path>>(&mut self, path: P) -> Result<bool, EngineError> {
        let path = path.as_ref();
        let rel_path = match self.to_relative_path(path) {
            Some(p) => p,
            None => return Ok(false),
        };

        // Check ignored directories in path components
        for comp in rel_path.components() {
            if let std::path::Component::Normal(c) = comp {
                if IGNORED_DIRS.contains(&c.to_string_lossy().as_ref()) {
                    return Ok(false);
                }
            }
        }

        let abs_path = self.root_path.join(&rel_path);
        if !abs_path.exists() {
            return self.remove_file(&rel_path);
        }

        if SupportedLanguage::from_path(&rel_path).is_none() {
            return Ok(false);
        }

        let metadata = fs::symlink_metadata(&abs_path).map_err(EngineError::IoError)?;
        if metadata.is_dir()
            || metadata.file_type().is_symlink()
            || metadata.len() > self.max_file_bytes as u64
        {
            return Ok(false);
        }

        let mtime = get_mtime_nanos(&metadata);
        let content_bytes = fs::read(&abs_path).map_err(EngineError::IoError)?;
        let content_hash = compute_blake3_hash(&content_bytes);

        // If cached entry has the exact same content hash, no parse needed
        if let Some(entry) = self.cache.entries.get(&rel_path) {
            if entry.blake3_hash == content_hash {
                return Ok(false);
            }
        }

        let content_str = String::from_utf8_lossy(&content_bytes).to_string();
        let extractor = AstExtractor::new()?;
        let mut dummy_id = 0u32;
        let (file_symbols, file_edges, file_imports) =
            extractor.parse_file_with_imports(&rel_path, &content_bytes, &mut dummy_id)?;

        self.file_sources.insert(rel_path.clone(), content_str);
        let entry = FileCacheEntry {
            relative_path: rel_path,
            blake3_hash: content_hash,
            mtime_nanos: mtime,
            symbols: file_symbols,
            edges: file_edges,
            imports: file_imports,
            source_bytes: content_bytes.len(),
        };
        self.cache.insert(entry);

        let mut ordered_paths: Vec<PathBuf> = self.cache.entries.keys().cloned().collect();
        ordered_paths.sort();
        let (symbols, edges, imports) =
            self.cache.compile_symbols_edges_and_imports(&ordered_paths);
        self.symbols = symbols;
        self.edges = edges;
        self.imports = imports;
        self.total_bytes = self.cache.entries.values().map(|e| e.source_bytes).sum();

        let cache_file = get_cache_file_for_root(&self.root_path);
        let _ = self.cache.save_to_file(&cache_file);

        Ok(true)
    }

    /// Incrementally removes a file from the in-memory cache and re-indexes symbols.
    ///
    /// Returns `Ok(true)` if the file was tracked and removed; `Ok(false)` otherwise.
    pub fn remove_file<P: AsRef<Path>>(&mut self, path: P) -> Result<bool, EngineError> {
        let path = path.as_ref();
        let rel_path = match self.to_relative_path(path) {
            Some(p) => p,
            None => return Ok(false),
        };

        self.file_sources.remove(&rel_path);
        if self.cache.entries.remove(&rel_path).is_some() {
            let mut ordered_paths: Vec<PathBuf> = self.cache.entries.keys().cloned().collect();
            ordered_paths.sort();
            let (symbols, edges, imports) =
                self.cache.compile_symbols_edges_and_imports(&ordered_paths);
            self.symbols = symbols;
            self.edges = edges;
            self.imports = imports;
            self.total_bytes = self.cache.entries.values().map(|e| e.source_bytes).sum();

            let cache_file = get_cache_file_for_root(&self.root_path);
            let _ = self.cache.save_to_file(&cache_file);

            Ok(true)
        } else {
            Ok(false)
        }
    }
}

fn scan_directory(root: &Path, files: &mut Vec<(PathBuf, PathBuf)>) -> Result<(), EngineError> {
    let mut builder = WalkBuilder::new(root);
    builder
        .hidden(true)
        .git_ignore(true)
        .git_global(true)
        .git_exclude(true)
        .require_git(false)
        .follow_links(false)
        .filter_entry(|entry| {
            if entry.depth() > 0 && entry.file_type().is_some_and(|ft| ft.is_dir()) {
                if let Some(name) = entry.file_name().to_str() {
                    if IGNORED_DIRS.contains(&name) {
                        return false;
                    }
                }
            }
            true
        });

    for entry in builder.build().flatten() {
        if entry.file_type().is_some_and(|ft| ft.is_symlink()) {
            continue;
        }
        let path = entry.into_path();
        if path.is_file() && SupportedLanguage::from_path(&path).is_some() {
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

    #[test]
    fn test_incremental_patch_and_remove() {
        let temp_dir =
            std::env::temp_dir().join(format!("repotrim_patch_test_{}", std::process::id()));
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(temp_dir.join("src")).unwrap();

        let file1 = temp_dir.join("src/main.rs");
        fs::write(&file1, "pub fn foo() {}\n").unwrap();

        let mut repo = LoadedRepository::load(&temp_dir).expect("Failed to load repo");
        assert_eq!(repo.symbols.len(), 1);
        assert_eq!(repo.symbols[0].name, "foo");

        // Touch without content change -> Ok(false)
        let touched = repo.patch_file(Path::new("src/main.rs")).unwrap();
        assert!(!touched);
        assert_eq!(repo.symbols.len(), 1);

        // Edit file to add a new function -> Ok(true)
        fs::write(&file1, "pub fn foo() {}\npub fn bar() {}\n").unwrap();
        let patched = repo.patch_file(Path::new("src/main.rs")).unwrap();
        assert!(patched);
        assert_eq!(repo.symbols.len(), 2);
        let names: Vec<_> = repo.symbols.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"foo"));
        assert!(names.contains(&"bar"));

        // Add a new file -> Ok(true)
        let file2 = temp_dir.join("src/util.rs");
        fs::write(&file2, "pub fn helper() {}\n").unwrap();
        let added = repo.patch_file(Path::new("src/util.rs")).unwrap();
        assert!(added);
        assert_eq!(repo.symbols.len(), 3);

        // Ignored file / directory -> Ok(false)
        let ignored_file = temp_dir.join("target/debug/foo.rs");
        fs::create_dir_all(temp_dir.join("target/debug")).unwrap();
        fs::write(&ignored_file, "pub fn ignore_me() {}\n").unwrap();
        let ignored = repo.patch_file(Path::new("target/debug/foo.rs")).unwrap();
        assert!(!ignored);

        // Unsupported extension -> Ok(false)
        let txt_file = temp_dir.join("src/notes.txt");
        fs::write(&txt_file, "just notes\n").unwrap();
        let skipped = repo.patch_file(Path::new("src/notes.txt")).unwrap();
        assert!(!skipped);

        // Remove file on disk and call patch_file (auto-delegates to remove_file) -> Ok(true)
        fs::remove_file(&file2).unwrap();
        let removed = repo.patch_file(Path::new("src/util.rs")).unwrap();
        assert!(removed);
        assert_eq!(repo.symbols.len(), 2);

        // Explicit remove on already removed file -> Ok(false)
        let removed_again = repo.remove_file(Path::new("src/util.rs")).unwrap();
        assert!(!removed_again);

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_stream_source_by_byte_offset_and_selective_loading() {
        let temp_dir =
            std::env::temp_dir().join(format!("repotrim_stream_test_{}", std::process::id()));
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(temp_dir.join("src")).unwrap();

        let file1 = temp_dir.join("src/lib.rs");
        fs::write(
            &file1,
            "pub fn first_func() -> u32 {\n    42\n}\n\npub fn second_func() -> &'static str {\n    \"hello\"\n}\n",
        )
        .unwrap();

        let mut repo = LoadedRepository::load_with_options(&temp_dir, false).expect("load repo");
        assert_eq!(repo.symbols.len(), 2);

        // Test direct byte-offset slice streaming
        let first_sym = repo
            .symbols
            .iter()
            .find(|s| s.name == "first_func")
            .unwrap()
            .clone();
        let first_source = repo
            .read_symbol_source(&first_sym)
            .expect("read first symbol");
        assert!(first_source.contains("pub fn first_func"));
        assert!(first_source.contains("42"));

        let second_sym = repo
            .symbols
            .iter()
            .find(|s| s.name == "second_func")
            .unwrap()
            .clone();
        let second_source = repo
            .read_symbol_source(&second_sym)
            .expect("read second symbol");
        assert!(second_source.contains("pub fn second_func"));
        assert!(second_source.contains("\"hello\""));

        // Clear file_sources to simulate zero whole-file residency
        repo.file_sources.clear();
        assert!(repo.file_sources.is_empty());

        // Selectively load sources only for second symbol
        repo.load_sources_for_symbols(std::iter::once(&second_sym))
            .expect("load sources for symbol");
        assert_eq!(repo.file_sources.len(), 1);
        assert!(repo.file_sources.contains_key(Path::new("src/lib.rs")));

        let _ = fs::remove_dir_all(&temp_dir);
    }
}
