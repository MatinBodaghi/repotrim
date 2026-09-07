use repotrim_engine::{
    AstExtractor, EngineError, LayerWeights, MultiplexGraph, ReferenceEdge, SymbolNode,
};
use std::collections::HashMap;
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
];

/// Encapsulates parsed repository files and AST symbol/edge data.
#[derive(Debug)]
pub struct LoadedRepository {
    pub root_path: PathBuf,
    pub file_sources: HashMap<PathBuf, String>,
    pub symbols: Vec<SymbolNode>,
    pub edges: Vec<ReferenceEdge>,
    pub total_bytes: usize,
}

impl LoadedRepository {
    /// Recursively scans a root directory and parses all supported source files.
    pub fn load<P: AsRef<Path>>(root: P) -> Result<Self, EngineError> {
        let root_path = if root.as_ref().is_relative() {
            std::env::current_dir()
                .map_err(EngineError::IoError)?
                .join(root.as_ref())
        } else {
            root.as_ref().to_path_buf()
        };

        let mut source_files = Vec::new();
        scan_directory(&root_path, &root_path, &mut source_files)?;
        source_files.sort_by(|a, b| a.1.cmp(&b.1));

        let extractor = AstExtractor::new()?;
        let mut file_sources = HashMap::with_capacity(source_files.len());
        let mut symbols = Vec::new();
        let mut edges = Vec::new();
        let mut next_id = 0u32;
        let mut total_bytes = 0usize;

        for (abs_path, rel_path) in source_files {
            let content_bytes = fs::read(&abs_path).map_err(EngineError::IoError)?;
            total_bytes += content_bytes.len();

            let content_str = String::from_utf8_lossy(&content_bytes).to_string();
            file_sources.insert(rel_path.clone(), content_str);

            let (file_symbols, file_edges) =
                extractor.parse_file(&rel_path, &content_bytes, &mut next_id)?;

            symbols.extend(file_symbols);
            edges.extend(file_edges);
        }

        Ok(Self {
            root_path,
            file_sources,
            symbols,
            edges,
            total_bytes,
        })
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

        let file1 = temp_dir.join("src/lib.rs");
        let mut f1 = File::create(&file1).unwrap();
        writeln!(f1, "pub fn hello() -> u32 {{ 42 }}").unwrap();

        let file2 = temp_dir.join("target/ignored.rs");
        let mut f2 = File::create(&file2).unwrap();
        writeln!(f2, "pub fn should_be_ignored() {{}}").unwrap();

        let repo = LoadedRepository::load(&temp_dir).expect("Failed to load repo");
        assert_eq!(repo.file_sources.len(), 1);
        assert!(repo.file_sources.keys().any(|p| p.ends_with("lib.rs")));
        assert_eq!(repo.symbols.len(), 1);
        assert_eq!(repo.symbols[0].name, "hello");

        let graph = repo.build_graph();
        assert_eq!(graph.num_symbols(), 1);

        let _ = fs::remove_dir_all(&temp_dir);
    }
}
