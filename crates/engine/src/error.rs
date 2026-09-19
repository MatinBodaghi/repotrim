use thiserror::Error;

/// Errors produced during AST parsing, tree-sitter operations, or symbol extraction.
#[derive(Debug, Error)]
pub enum EngineError {
    /// Failed to parse AST with tree-sitter.
    #[error("Failed to parse AST with tree-sitter")]
    ParseError,
    /// Tree-sitter query compilation or execution error.
    #[error("Tree-sitter query error: {0}")]
    QueryError(#[from] tree_sitter::QueryError),
    /// UTF-8 string decoding failure.
    #[error("UTF-8 decoding error: {0}")]
    Utf8Error(#[from] std::str::Utf8Error),
    /// Standard I/O read/write error.
    #[error("I/O error: {0}")]
    IoError(#[from] std::io::Error),
    /// Git command or revision resolution error.
    #[error("Git error: {0}")]
    GitError(String),
    /// File watcher subsystem error.
    #[error("Watcher error: {0}")]
    WatcherError(String),
    /// Requested symbol ID does not exist in repository graph.
    #[error("Symbol with ID {0} was not found in graph")]
    SymbolNotFound(u32),
    /// Token budget constraint exceeded.
    #[error("Budget exceeded: required {0} tokens, remaining {1} tokens")]
    BudgetExceeded(usize, usize),
    /// Invalid input parameter or argument.
    #[error("Invalid input argument: {0}")]
    InvalidInput(String),
    /// Filesystem or sandboxing security violation.
    #[error("Security violation: {0}")]
    SecurityError(#[from] crate::security::SecurityError),
    /// Unrecoverable internal engine error.
    #[error("Internal engine error: {0}")]
    InternalError(String),
}

impl From<notify::Error> for EngineError {
    fn from(err: notify::Error) -> Self {
        EngineError::WatcherError(err.to_string())
    }
}
