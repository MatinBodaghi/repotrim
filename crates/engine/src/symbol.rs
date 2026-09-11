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
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
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
    /// Name of the enclosing class, struct, or container if this symbol is a member/method.
    #[serde(default)]
    pub container_name: Option<String>,
    /// Name of the trait implemented by the enclosing block (e.g. `Clone` in `impl Clone for Foo`).
    #[serde(default)]
    pub trait_name: Option<String>,
}

impl SymbolNode {
    /// Derives the formal `NodeType` for this symbol node.
    pub fn node_type(&self) -> NodeType {
        NodeType::infer(&self.name, &self.file_path, self.kind)
    }
}

/// Kind of relationship represented by a reference edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EdgeKind {
    AstParent,
    Call,
    TypeRef,
    Import,
    CoEdit,
}

/// Directed reference from an extracted symbol to a target identifier.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ReferenceEdge {
    pub source: SymbolId,
    pub target_ident: String,
    pub kind: EdgeKind,
}

impl ReferenceEdge {
    /// Derives the formal `RelationType` for this reference edge.
    pub fn relation_type(&self) -> RelationType {
        RelationType::from(self.kind)
    }
}

/// Formal code entity type $\tau_V$ in the multiplex codebase graph.
///
/// Models first-class syntax constructs, packaging boundaries, tests,
/// configurations, and service endpoints for typed graph navigation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum NodeType {
    /// Packaging boundary (e.g. Crate, npm package, Go module).
    Package,
    /// Language module or submodule boundary (e.g. `mod foo`, Python package).
    Module,
    /// Physical source file container.
    File,
    /// Class declaration (e.g. Python class, TypeScript class).
    Class,
    /// Struct definition (e.g. Rust struct, Go struct).
    Struct,
    /// Interface or Trait contract (e.g. Rust trait, TypeScript interface).
    Interface,
    /// Free-standing procedural or mathematical function.
    Function,
    /// Associated member function or receiver method.
    Method,
    /// Type definition, alias, typedef, or enumeration.
    Type,
    /// Verification entity: unit test, integration test, or benchmark.
    Test,
    /// Project configuration or manifest entry (e.g. Cargo.toml, tsconfig.json).
    Config,
    /// Network API route, RPC handler, or external ingress endpoint.
    Endpoint,
}

impl NodeType {
    /// Infers a `NodeType` from a symbol's name, path, and syntactic `SymbolKind`.
    pub fn infer(name: &str, file_path: &std::path::Path, kind: SymbolKind) -> Self {
        let is_test_path = file_path.components().any(|c| {
            let s = c.as_os_str().to_string_lossy();
            s == "tests" || s == "test" || s.ends_with("_test.rs") || s.ends_with("_test.go")
        });

        let is_test_name = name.starts_with("test_")
            || name.starts_with("Test")
            || name.ends_with("_test")
            || name.ends_with("Test")
            || name.starts_with("prop_");

        if (is_test_path || is_test_name)
            && matches!(kind, SymbolKind::Function | SymbolKind::Method)
        {
            return NodeType::Test;
        }

        Self::from(kind)
    }

    /// Returns true if this entity represents an executable procedure or callable routine.
    pub fn is_callable(&self) -> bool {
        matches!(
            self,
            NodeType::Function | NodeType::Method | NodeType::Test | NodeType::Endpoint
        )
    }

    /// Returns true if this entity defines a data type, structure, or interface contract.
    pub fn is_type_definition(&self) -> bool {
        matches!(
            self,
            NodeType::Class | NodeType::Struct | NodeType::Interface | NodeType::Type
        )
    }

    /// Returns true if this entity acts as a container for other member symbols.
    pub fn is_container(&self) -> bool {
        matches!(
            self,
            NodeType::Package
                | NodeType::Module
                | NodeType::File
                | NodeType::Class
                | NodeType::Struct
                | NodeType::Interface
        )
    }

    /// Returns true if this entity represents a test or verification harness.
    pub fn is_test(&self) -> bool {
        matches!(self, NodeType::Test)
    }

    /// Returns true if this entity represents a project configuration or metadata manifest.
    pub fn is_config(&self) -> bool {
        matches!(self, NodeType::Config)
    }

    /// Human-readable label for Markdown outlines and diagrams.
    pub fn as_str(&self) -> &'static str {
        match self {
            NodeType::Package => "package",
            NodeType::Module => "module",
            NodeType::File => "file",
            NodeType::Class => "class",
            NodeType::Struct => "struct",
            NodeType::Interface => "interface",
            NodeType::Function => "function",
            NodeType::Method => "method",
            NodeType::Type => "type",
            NodeType::Test => "test",
            NodeType::Config => "config",
            NodeType::Endpoint => "endpoint",
        }
    }
}

impl fmt::Display for NodeType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl std::str::FromStr for NodeType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "package" => Ok(NodeType::Package),
            "module" | "mod" => Ok(NodeType::Module),
            "file" => Ok(NodeType::File),
            "class" => Ok(NodeType::Class),
            "struct" => Ok(NodeType::Struct),
            "interface" | "trait" => Ok(NodeType::Interface),
            "function" | "fn" => Ok(NodeType::Function),
            "method" => Ok(NodeType::Method),
            "type" | "enum" | "typealias" => Ok(NodeType::Type),
            "test" => Ok(NodeType::Test),
            "config" => Ok(NodeType::Config),
            "endpoint" | "route" => Ok(NodeType::Endpoint),
            other => Err(format!("Unknown NodeType: {}", other)),
        }
    }
}

impl From<SymbolKind> for NodeType {
    fn from(kind: SymbolKind) -> Self {
        match kind {
            SymbolKind::Function => NodeType::Function,
            SymbolKind::Method => NodeType::Method,
            SymbolKind::Struct => NodeType::Struct,
            SymbolKind::Enum | SymbolKind::TypeAlias => NodeType::Type,
            SymbolKind::Trait => NodeType::Interface,
            SymbolKind::Module => NodeType::Module,
        }
    }
}

/// Formal edge relation type $\tau_E$ in the multiplex codebase graph.
///
/// Models directional couplings between entities across AST containment,
/// call graphs, type dependencies, imports, tests, and historical co-edits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum RelationType {
    /// Explicit module or package import dependency.
    Imports,
    /// Procedural or method invocation call edge.
    Calls,
    /// Lexical or identifier reference without direct invocation.
    References,
    /// Class or structure inheritance relationship.
    Inherits,
    /// Interface or Trait implementation.
    Implements,
    /// Structural AST parent-to-child containment (e.g. Struct -> Method).
    Contains,
    /// Reverse containment or scope membership (e.g. Method -> Struct).
    BelongsTo,
    /// Return type contract dependency.
    Returns,
    /// Function or method input parameter type dependency.
    Accepts,
    /// Field or state read access.
    Reads,
    /// Field or state write or mutation access.
    Writes,
    /// Configuration or environment binding.
    Configures,
    /// Verification relationship connecting a test entity to its target subject.
    IsTestedBy,
    /// Mined historical co-change coupling from commit logs.
    CoChangesWith,
}

impl RelationType {
    /// Total number of distinct relation types modeled in the multiplex graph.
    pub const COUNT: usize = 14;

    /// Complete array of all modeled relation types.
    pub const ALL: [RelationType; 14] = [
        RelationType::Imports,
        RelationType::Calls,
        RelationType::References,
        RelationType::Inherits,
        RelationType::Implements,
        RelationType::Contains,
        RelationType::BelongsTo,
        RelationType::Returns,
        RelationType::Accepts,
        RelationType::Reads,
        RelationType::Writes,
        RelationType::Configures,
        RelationType::IsTestedBy,
        RelationType::CoChangesWith,
    ];

    /// Returns the 0-based dense index corresponding to this relation type.
    #[inline]
    pub const fn index(&self) -> usize {
        match self {
            RelationType::Imports => 0,
            RelationType::Calls => 1,
            RelationType::References => 2,
            RelationType::Inherits => 3,
            RelationType::Implements => 4,
            RelationType::Contains => 5,
            RelationType::BelongsTo => 6,
            RelationType::Returns => 7,
            RelationType::Accepts => 8,
            RelationType::Reads => 9,
            RelationType::Writes => 10,
            RelationType::Configures => 11,
            RelationType::IsTestedBy => 12,
            RelationType::CoChangesWith => 13,
        }
    }

    /// Resolves a dense index `0..14` back to its `RelationType`.
    #[inline]
    pub const fn from_index(idx: usize) -> Option<Self> {
        if idx < Self::COUNT {
            Some(Self::ALL[idx])
        } else {
            None
        }
    }

    /// Returns true if this relation is a structural hierarchy edge.
    pub fn is_structural(&self) -> bool {
        matches!(
            self,
            RelationType::Contains
                | RelationType::BelongsTo
                | RelationType::Inherits
                | RelationType::Implements
        )
    }

    /// Returns true if this relation represents a dynamic invocation or execution flow.
    pub fn is_behavioral(&self) -> bool {
        matches!(
            self,
            RelationType::Calls | RelationType::Reads | RelationType::Writes
        )
    }

    /// Returns true if this relation represents a type signature dependency.
    pub fn is_type_dependency(&self) -> bool {
        matches!(
            self,
            RelationType::Returns | RelationType::Accepts | RelationType::References
        )
    }

    /// Returns true if this relation represents empirical evidence (tests or co-edits).
    pub fn is_evidence(&self) -> bool {
        matches!(self, RelationType::IsTestedBy | RelationType::CoChangesWith)
    }

    /// Returns true if this relation represents an explicit import statement.
    pub fn is_import(&self) -> bool {
        matches!(self, RelationType::Imports)
    }

    /// Canonical relation name string.
    pub fn as_str(&self) -> &'static str {
        match self {
            RelationType::Imports => "imports",
            RelationType::Calls => "calls",
            RelationType::References => "references",
            RelationType::Inherits => "inherits",
            RelationType::Implements => "implements",
            RelationType::Contains => "contains",
            RelationType::BelongsTo => "belongs_to",
            RelationType::Returns => "returns",
            RelationType::Accepts => "accepts",
            RelationType::Reads => "reads",
            RelationType::Writes => "writes",
            RelationType::Configures => "configures",
            RelationType::IsTestedBy => "is_tested_by",
            RelationType::CoChangesWith => "co_changes_with",
        }
    }
}

impl fmt::Display for RelationType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl std::str::FromStr for RelationType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "imports" | "import" => Ok(RelationType::Imports),
            "calls" | "call" => Ok(RelationType::Calls),
            "references" | "ref" => Ok(RelationType::References),
            "inherits" | "extends" => Ok(RelationType::Inherits),
            "implements" | "impl" => Ok(RelationType::Implements),
            "contains" => Ok(RelationType::Contains),
            "belongs_to" | "member_of" => Ok(RelationType::BelongsTo),
            "returns" => Ok(RelationType::Returns),
            "accepts" | "param" => Ok(RelationType::Accepts),
            "reads" => Ok(RelationType::Reads),
            "writes" => Ok(RelationType::Writes),
            "configures" => Ok(RelationType::Configures),
            "is_tested_by" | "tested_by" => Ok(RelationType::IsTestedBy),
            "co_changes_with" | "coedit" => Ok(RelationType::CoChangesWith),
            other => Err(format!("Unknown RelationType: {}", other)),
        }
    }
}

impl From<EdgeKind> for RelationType {
    fn from(kind: EdgeKind) -> Self {
        match kind {
            EdgeKind::AstParent => RelationType::Contains,
            EdgeKind::Call => RelationType::Calls,
            EdgeKind::TypeRef => RelationType::References,
            EdgeKind::Import => RelationType::Imports,
            EdgeKind::CoEdit => RelationType::CoChangesWith,
        }
    }
}
