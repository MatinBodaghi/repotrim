use thiserror::Error;

/// Errors produced during AST parsing, tree-sitter operations, or symbol extraction.
#[derive(Debug, Error)]
pub enum EngineError {
    #[error("Failed to parse AST with tree-sitter")]
    ParseError,
    #[error("Tree-sitter query error: {0}")]
    QueryError(#[from] tree_sitter::QueryError),
    #[error("UTF-8 decoding error: {0}")]
    Utf8Error(#[from] std::str::Utf8Error),
    #[error("I/O error: {0}")]
    IoError(#[from] std::io::Error),
}
