use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::PathBuf;

/// Dense numeric identifier for symbols.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Ord, PartialOrd, Serialize, Deserialize, Debug)]
pub struct SymbolId(pub u32);

impl fmt::Display for SymbolId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// The category or syntactic kind of a symbol.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SymbolKind {
    Function,
    Method,
    Struct,
    Enum,
    Trait,
    TypeAlias,
    Module,
}

/// Discrete Level-of-Detail (LOD) resolution for rendered symbols.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum LodLevel {
    /// LOD 0: Minimal signature declaration (e.g. `pub fn foo(...) -> Bar;`).
    SignatureOnly = 0,
    /// LOD 1: Signature preceded by docstrings.
    SignatureAndDoc = 1,
    /// LOD 2: Signature with sliced skeleton / control-flow outline.
    SlicedBody = 2,
    /// LOD 3: Complete source implementation from file content.
    FullBody = 3,
}

/// Precise source code location span for an extracted symbol.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TextSpan {
    pub start_byte: usize,
    pub end_byte: usize,
    pub start_row: usize,
    pub end_row: usize,
}

impl TextSpan {
    pub fn new(start_byte: usize, end_byte: usize, start_row: usize, end_row: usize) -> Self {
        Self {
            start_byte,
            end_byte,
            start_row,
            end_row,
        }
    }
}

/// Extracted AST symbol node with isolated signature, docstring, and incremental cache hash.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SymbolNode {
    pub id: SymbolId,
    pub name: String,
    pub kind: SymbolKind,
    pub file_path: PathBuf,
    pub span: TextSpan,
    pub signature: String,
    pub docstring: Option<String>,
    pub token_cost: usize,
    pub ast_hash: [u8; 32],
}

/// Kind of relationship represented by a reference edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EdgeKind {
    AstParent,
    Call,
    TypeRef,
    Import,
}

/// Directed reference from an extracted symbol to a target identifier.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ReferenceEdge {
    pub source: SymbolId,
    pub target_ident: String,
    pub kind: EdgeKind,
}
