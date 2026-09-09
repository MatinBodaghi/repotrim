use std::collections::{HashMap, HashSet};
use std::path::Path;
use tree_sitter::{Language, Node, Parser, Query, QueryCursor, StreamingIterator};

use crate::error::EngineError;
use crate::import::FileImport;
use crate::symbol::{EdgeKind, ReferenceEdge, SymbolId, SymbolKind, SymbolNode, TextSpan};
use crate::tokens::estimate_tokens;

/// Embedded declarative Tree-sitter queries for Rust symbol and reference extraction.
const RUST_QUERY_SOURCE: &str = include_str!("../queries/rust.scm");
/// Embedded declarative Tree-sitter queries for Python symbol and reference extraction.
const PYTHON_QUERY_SOURCE: &str = include_str!("../queries/python.scm");
/// Embedded declarative Tree-sitter queries for TypeScript symbol and reference extraction.
const TYPESCRIPT_QUERY_SOURCE: &str = include_str!("../queries/typescript.scm");
/// Embedded declarative Tree-sitter queries for Go symbol and reference extraction.
const GO_QUERY_SOURCE: &str = include_str!("../queries/go.scm");

/// Programming languages supported for AST symbol and reference extraction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SupportedLanguage {
    Rust,
    Python,
    TypeScript,
    Tsx,
    Go,
}

impl SupportedLanguage {
    /// Infers the programming language from a file's extension.
    pub fn from_path(path: &Path) -> Option<Self> {
        let ext = path.extension()?.to_str()?;
        match ext {
            "rs" => Some(SupportedLanguage::Rust),
            "py" => Some(SupportedLanguage::Python),
            "ts" => Some(SupportedLanguage::TypeScript),
            "tsx" => Some(SupportedLanguage::Tsx),
            "js" | "jsx" | "mjs" | "cjs" => Some(SupportedLanguage::Tsx),
            "go" => Some(SupportedLanguage::Go),
            _ => None,
        }
    }
}

struct LanguageBundle {
    language: Language,
    query: Query,
}

/// Multi-language AST symbol and reference extractor for source files.
pub struct AstExtractor {
    rust: LanguageBundle,
    python: LanguageBundle,
    typescript: LanguageBundle,
    tsx: LanguageBundle,
    go: LanguageBundle,
}

/// Type alias for raw parsed file output containing extracted symbols, edges, and imports.
pub type FileParseOutput = (Vec<SymbolNode>, Vec<ReferenceEdge>, Vec<FileImport>);

impl AstExtractor {
    /// Creates a new `AstExtractor` compiling the embedded queries for all supported languages.
    pub fn new() -> Result<Self, EngineError> {
        let rust_lang: Language = tree_sitter_rust::LANGUAGE.into();
        let rust_query = Query::new(&rust_lang, RUST_QUERY_SOURCE)?;

        let python_lang: Language = tree_sitter_python::LANGUAGE.into();
        let python_query = Query::new(&python_lang, PYTHON_QUERY_SOURCE)?;

        let ts_lang: Language = tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into();
        let ts_query = Query::new(&ts_lang, TYPESCRIPT_QUERY_SOURCE)?;

        let tsx_lang: Language = tree_sitter_typescript::LANGUAGE_TSX.into();
        let tsx_query = Query::new(&tsx_lang, TYPESCRIPT_QUERY_SOURCE)?;

        let go_lang: Language = tree_sitter_go::LANGUAGE.into();
        let go_query = Query::new(&go_lang, GO_QUERY_SOURCE)?;

        Ok(Self {
            rust: LanguageBundle {
                language: rust_lang,
                query: rust_query,
            },
            python: LanguageBundle {
                language: python_lang,
                query: python_query,
            },
            typescript: LanguageBundle {
                language: ts_lang,
                query: ts_query,
            },
            tsx: LanguageBundle {
                language: tsx_lang,
                query: tsx_query,
            },
            go: LanguageBundle {
                language: go_lang,
                query: go_query,
            },
        })
    }

    /// Parses a file's source bytes, extracting symbols (with stripped function bodies)
    /// and tracking reference edges to called identifiers and types.
    pub fn parse_file(
        &self,
        path: &Path,
        source: &[u8],
        next_id: &mut u32,
    ) -> Result<(Vec<SymbolNode>, Vec<ReferenceEdge>), EngineError> {
        let (symbols, edges, _) = self.parse_file_with_imports(path, source, next_id)?;
        Ok((symbols, edges))
    }

    /// Parses a file's source bytes, extracting symbols, reference edges, and explicit import statements.
    pub fn parse_file_with_imports(
        &self,
        path: &Path,
        source: &[u8],
        next_id: &mut u32,
    ) -> Result<FileParseOutput, EngineError> {
        let lang = match SupportedLanguage::from_path(path) {
            Some(l) => l,
            None => return Ok((Vec::new(), Vec::new(), Vec::new())),
        };

        let bundle = match lang {
            SupportedLanguage::Rust => &self.rust,
            SupportedLanguage::Python => &self.python,
            SupportedLanguage::TypeScript => &self.typescript,
            SupportedLanguage::Tsx => &self.tsx,
            SupportedLanguage::Go => &self.go,
        };

        let mut parser = Parser::new();
        parser
            .set_language(&bundle.language)
            .map_err(|_| EngineError::ParseError)?;

        let tree = parser.parse(source, None).ok_or(EngineError::ParseError)?;
        let root_node = tree.root_node();
        let source_str = std::str::from_utf8(source)?;

        let mut symbols = Vec::new();
        let mut edges = Vec::new();

        let mut fn_node_to_symbol_id: HashMap<usize, SymbolId> = HashMap::new();
        let mut seen_symbol_nodes: HashSet<usize> = HashSet::new();
        let mut seen_call_nodes: HashSet<usize> = HashSet::new();

        let fn_capture_idx = bundle.query.capture_index_for_name("function");
        let struct_capture_idx = bundle.query.capture_index_for_name("struct");
        let enum_capture_idx = bundle.query.capture_index_for_name("enum");
        let trait_capture_idx = bundle.query.capture_index_for_name("trait");
        let call_capture_idx = bundle.query.capture_index_for_name("call");
        let call_target_idx = bundle.query.capture_index_for_name("call.target");

        // Pass 1: Extract symbols (functions, methods, structs, enums, traits)
        let mut cursor = QueryCursor::new();
        let mut matches = cursor.matches(&bundle.query, root_node, source);

        while let Some(m) = matches.next() {
            for capture in m.captures {
                let node = capture.node;

                if Some(capture.index) == fn_capture_idx {
                    if !seen_symbol_nodes.insert(node.id()) {
                        continue;
                    }

                    let name = if let Some(name_node) = node.child_by_field_name("name") {
                        name_node.utf8_text(source)?.to_string()
                    } else if let Some(id_child) = find_child_by_kinds(
                        node,
                        &["identifier", "property_identifier", "field_identifier"],
                    ) {
                        id_child.utf8_text(source)?.to_string()
                    } else {
                        continue;
                    };

                    let kind = match lang {
                        SupportedLanguage::Rust => {
                            if is_inside_impl(node) {
                                SymbolKind::Method
                            } else {
                                SymbolKind::Function
                            }
                        }
                        SupportedLanguage::Python => {
                            if is_inside_class(node) {
                                SymbolKind::Method
                            } else {
                                SymbolKind::Function
                            }
                        }
                        SupportedLanguage::TypeScript | SupportedLanguage::Tsx => {
                            if node.kind() == "method_definition" || is_inside_class(node) {
                                SymbolKind::Method
                            } else {
                                SymbolKind::Function
                            }
                        }
                        SupportedLanguage::Go => {
                            if node.kind() == "method_declaration" {
                                SymbolKind::Method
                            } else {
                                SymbolKind::Function
                            }
                        }
                    };

                    let span = TextSpan::new(
                        node.start_byte(),
                        node.end_byte(),
                        node.start_position().row,
                        node.end_position().row,
                    );

                    // Isolate execution bodies: strip block contents, leaving only declaration header
                    let signature = if let Some(body_node) = node.child_by_field_name("body") {
                        source_str[node.start_byte()..body_node.start_byte()]
                            .trim()
                            .to_string()
                    } else {
                        source_str[node.start_byte()..node.end_byte()]
                            .trim()
                            .trim_end_matches(';')
                            .trim()
                            .to_string()
                    };

                    let docstring = if lang == SupportedLanguage::Python {
                        extract_python_docstring(node, source)
                            .or_else(|| extract_docstring(node, source))
                    } else {
                        extract_docstring(node, source)
                    };

                    let token_cost = estimate_tokens(&signature);
                    let ast_hash = *blake3::hash(signature.as_bytes()).as_bytes();

                    let id = SymbolId(*next_id);
                    *next_id += 1;

                    fn_node_to_symbol_id.insert(node.id(), id);

                    // Link method to enclosing class/struct/impl via AstParent edge
                    match lang {
                        SupportedLanguage::Rust => {
                            if is_inside_impl(node) {
                                if let Some(impl_type) = find_enclosing_impl_type(node, source) {
                                    edges.push(ReferenceEdge {
                                        source: id,
                                        target_ident: impl_type,
                                        kind: EdgeKind::AstParent,
                                    });
                                }
                                if let Some(impl_trait) = find_enclosing_impl_trait(node, source) {
                                    edges.push(ReferenceEdge {
                                        source: id,
                                        target_ident: impl_trait,
                                        kind: EdgeKind::TypeRef,
                                    });
                                }
                            }
                        }
                        SupportedLanguage::Python
                        | SupportedLanguage::TypeScript
                        | SupportedLanguage::Tsx => {
                            if let Some(enclosing_class) = find_enclosing_class_name(node, source) {
                                edges.push(ReferenceEdge {
                                    source: id,
                                    target_ident: enclosing_class,
                                    kind: EdgeKind::AstParent,
                                });
                            }
                        }
                        SupportedLanguage::Go => {
                            if let Some(receiver_type) = find_go_receiver_type(node, source) {
                                edges.push(ReferenceEdge {
                                    source: id,
                                    target_ident: receiver_type,
                                    kind: EdgeKind::AstParent,
                                });
                            }
                        }
                    }

                    // Extract type references from function parameters and return types
                    let mut type_idents = HashSet::new();
                    if let Some(params) = node.child_by_field_name("parameters") {
                        extract_type_identifiers_from_node(params, source, &mut type_idents);
                    }
                    if let Some(ret) = node.child_by_field_name("return_type") {
                        extract_type_identifiers_from_node(ret, source, &mut type_idents);
                    }
                    for type_ident in type_idents {
                        if type_ident != name {
                            edges.push(ReferenceEdge {
                                source: id,
                                target_ident: type_ident,
                                kind: EdgeKind::TypeRef,
                            });
                        }
                    }

                    symbols.push(SymbolNode {
                        id,
                        name,
                        kind,
                        file_path: path.to_path_buf(),
                        span,
                        signature,
                        docstring,
                        token_cost,
                        ast_hash,
                    });
                } else if Some(capture.index) == struct_capture_idx {
                    if !seen_symbol_nodes.insert(node.id()) {
                        continue;
                    }

                    let name = if let Some(name_node) = node.child_by_field_name("name") {
                        name_node.utf8_text(source)?.to_string()
                    } else if let Some(id_child) = find_child_by_kinds(
                        node,
                        &["type_identifier", "identifier", "property_identifier"],
                    ) {
                        id_child.utf8_text(source)?.to_string()
                    } else {
                        continue;
                    };

                    let span = TextSpan::new(
                        node.start_byte(),
                        node.end_byte(),
                        node.start_position().row,
                        node.end_position().row,
                    );

                    let signature = if lang == SupportedLanguage::Python {
                        if let Some(body_node) = node.child_by_field_name("body") {
                            source_str[node.start_byte()..body_node.start_byte()]
                                .trim()
                                .to_string()
                        } else {
                            source_str[node.start_byte()..node.end_byte()]
                                .trim()
                                .to_string()
                        }
                    } else {
                        source_str[node.start_byte()..node.end_byte()]
                            .trim()
                            .to_string()
                    };

                    let docstring = if lang == SupportedLanguage::Python {
                        extract_python_docstring(node, source)
                            .or_else(|| extract_docstring(node, source))
                    } else {
                        extract_docstring(node, source)
                    };

                    let token_cost = estimate_tokens(&signature);
                    let ast_hash = *blake3::hash(signature.as_bytes()).as_bytes();

                    let id = SymbolId(*next_id);
                    *next_id += 1;

                    // Extract type references from struct field declarations / inheritance
                    let mut type_idents = HashSet::new();
                    let mut cursor = node.walk();
                    for child in node.children(&mut cursor) {
                        if child.kind().contains("field")
                            || child.kind().contains("heritage")
                            || child.kind() == "superclasses"
                            || child.kind() == "argument_list"
                            || child.kind() == "declaration_list"
                        {
                            extract_type_identifiers_from_node(child, source, &mut type_idents);
                        }
                    }
                    for type_ident in type_idents {
                        if type_ident != name {
                            edges.push(ReferenceEdge {
                                source: id,
                                target_ident: type_ident,
                                kind: EdgeKind::TypeRef,
                            });
                        }
                    }

                    symbols.push(SymbolNode {
                        id,
                        name,
                        kind: SymbolKind::Struct,
                        file_path: path.to_path_buf(),
                        span,
                        signature,
                        docstring,
                        token_cost,
                        ast_hash,
                    });
                } else if Some(capture.index) == enum_capture_idx {
                    if !seen_symbol_nodes.insert(node.id()) {
                        continue;
                    }

                    let name = if let Some(name_node) = node.child_by_field_name("name") {
                        name_node.utf8_text(source)?.to_string()
                    } else if let Some(id_child) =
                        find_child_by_kinds(node, &["type_identifier", "identifier"])
                    {
                        id_child.utf8_text(source)?.to_string()
                    } else {
                        continue;
                    };

                    let span = TextSpan::new(
                        node.start_byte(),
                        node.end_byte(),
                        node.start_position().row,
                        node.end_position().row,
                    );

                    let signature = source_str[node.start_byte()..node.end_byte()]
                        .trim()
                        .to_string();

                    let docstring = extract_docstring(node, source);
                    let token_cost = estimate_tokens(&signature);
                    let ast_hash = *blake3::hash(signature.as_bytes()).as_bytes();

                    let id = SymbolId(*next_id);
                    *next_id += 1;

                    let kind = if node.kind() == "type_alias_declaration" {
                        SymbolKind::TypeAlias
                    } else {
                        SymbolKind::Enum
                    };

                    symbols.push(SymbolNode {
                        id,
                        name,
                        kind,
                        file_path: path.to_path_buf(),
                        span,
                        signature,
                        docstring,
                        token_cost,
                        ast_hash,
                    });
                } else if Some(capture.index) == trait_capture_idx {
                    if !seen_symbol_nodes.insert(node.id()) {
                        continue;
                    }

                    let name = if let Some(name_node) = node.child_by_field_name("name") {
                        name_node.utf8_text(source)?.to_string()
                    } else if let Some(id_child) =
                        find_child_by_kinds(node, &["type_identifier", "identifier"])
                    {
                        id_child.utf8_text(source)?.to_string()
                    } else {
                        continue;
                    };

                    let span = TextSpan::new(
                        node.start_byte(),
                        node.end_byte(),
                        node.start_position().row,
                        node.end_position().row,
                    );

                    let signature = if let Some(body_node) = node.child_by_field_name("body") {
                        source_str[node.start_byte()..body_node.start_byte()]
                            .trim()
                            .to_string()
                    } else {
                        source_str[node.start_byte()..node.end_byte()]
                            .trim()
                            .trim_end_matches(';')
                            .trim()
                            .to_string()
                    };

                    let docstring = extract_docstring(node, source);
                    let token_cost = estimate_tokens(&signature);
                    let ast_hash = *blake3::hash(signature.as_bytes()).as_bytes();

                    let id = SymbolId(*next_id);
                    *next_id += 1;

                    symbols.push(SymbolNode {
                        id,
                        name,
                        kind: SymbolKind::Trait,
                        file_path: path.to_path_buf(),
                        span,
                        signature,
                        docstring,
                        token_cost,
                        ast_hash,
                    });
                }
            }
        }

        // Pass 2: Extract call reference edges inside functions
        let mut call_cursor = QueryCursor::new();
        let mut call_matches = call_cursor.matches(&bundle.query, root_node, source);

        while let Some(m) = call_matches.next() {
            let mut call_node: Option<Node> = None;
            let mut target_node: Option<Node> = None;

            for capture in m.captures {
                if Some(capture.index) == call_capture_idx {
                    call_node = Some(capture.node);
                } else if Some(capture.index) == call_target_idx {
                    target_node = Some(capture.node);
                }
            }

            if let (Some(call), Some(target)) = (call_node, target_node) {
                if !seen_call_nodes.insert(call.id()) {
                    continue;
                }

                // Find nearest enclosing function / method symbol
                let mut current = call.parent();
                while let Some(parent) = current {
                    if let Some(&caller_id) = fn_node_to_symbol_id.get(&parent.id()) {
                        let target_ident = target.utf8_text(source)?.to_string();
                        edges.push(ReferenceEdge {
                            source: caller_id,
                            target_ident,
                            kind: EdgeKind::Call,
                        });
                        break;
                    }
                    current = parent.parent();
                }
            }
        }

        // Pass 3: Extract explicit import statements
        let mut imports = Vec::new();
        match lang {
            SupportedLanguage::Rust => {
                extract_rust_imports(root_node, source, path, &mut imports);
            }
            SupportedLanguage::Python => {
                extract_python_imports(root_node, source, path, &mut imports);
            }
            SupportedLanguage::TypeScript | SupportedLanguage::Tsx => {
                extract_typescript_imports(root_node, source, path, &mut imports);
            }
            SupportedLanguage::Go => {
                extract_go_imports(root_node, source, path, &mut imports);
            }
        }

        Ok((symbols, edges, imports))
    }
}

/// Helper searching children of a node matching specified kind names.
fn find_child_by_kinds<'a>(node: Node<'a>, kinds: &[&str]) -> Option<Node<'a>> {
    let mut cursor = node.walk();
    let found = node
        .children(&mut cursor)
        .find(|child| kinds.contains(&child.kind()));
    found
}

/// Determines if an AST node is contained within a class definition.
fn is_inside_class(node: Node) -> bool {
    let mut current = node.parent();
    while let Some(parent) = current {
        if parent.kind() == "class_definition" || parent.kind() == "class_declaration" {
            return true;
        }
        current = parent.parent();
    }
    false
}

/// Finds the name of an enclosing class definition.
fn find_enclosing_class_name(node: Node, source: &[u8]) -> Option<String> {
    let mut current = node.parent();
    while let Some(parent) = current {
        if parent.kind() == "class_definition" || parent.kind() == "class_declaration" {
            if let Some(name_node) = parent.child_by_field_name("name") {
                return name_node.utf8_text(source).ok().map(|s| s.to_string());
            }
        }
        current = parent.parent();
    }
    None
}

/// Determines if an AST node is contained within an `impl_item` block (Rust).
fn is_inside_impl(node: Node) -> bool {
    let mut current = node.parent();
    while let Some(parent) = current {
        if parent.kind() == "impl_item" {
            return true;
        }
        current = parent.parent();
    }
    false
}

/// Extracts Python docstrings located as the first expression in a function/class body.
fn extract_python_docstring(node: Node, source: &[u8]) -> Option<String> {
    let body = node.child_by_field_name("body")?;
    let mut cursor = body.walk();
    for child in body.children(&mut cursor) {
        if child.kind() == "expression_statement" {
            if let Some(first_child) = child.child(0) {
                if first_child.kind() == "string" {
                    let text = first_child.utf8_text(source).ok()?;
                    let trimmed = text.trim();
                    let stripped = trimmed
                        .strip_prefix("\"\"\"")
                        .and_then(|s| s.strip_suffix("\"\"\""))
                        .or_else(|| {
                            trimmed
                                .strip_prefix("'''")
                                .and_then(|s| s.strip_suffix("'''"))
                        })
                        .or_else(|| trimmed.strip_prefix('"').and_then(|s| s.strip_suffix('"')))
                        .or_else(|| {
                            trimmed
                                .strip_prefix('\'')
                                .and_then(|s| s.strip_suffix('\''))
                        })
                        .unwrap_or(trimmed);
                    return Some(stripped.trim().to_string());
                }
            }
        }
        if child.kind() != "comment" {
            break;
        }
    }
    None
}

/// Extracts documentation comments preceding an AST node.
fn extract_docstring(node: Node, source: &[u8]) -> Option<String> {
    let mut doc_lines = Vec::new();
    let mut current = node.prev_sibling();

    while let Some(sibling) = current {
        match sibling.kind() {
            "line_comment" | "comment" => {
                let text = sibling.utf8_text(source).unwrap_or("");
                let trimmed = text.trim();
                if let Some(content) = trimmed
                    .strip_prefix("///")
                    .or_else(|| trimmed.strip_prefix("//!"))
                    .or_else(|| trimmed.strip_prefix("//"))
                    .or_else(|| trimmed.strip_prefix("#"))
                {
                    doc_lines.push(content.trim().to_string());
                } else {
                    break;
                }
            }
            "block_comment" => {
                let text = sibling.utf8_text(source).unwrap_or("");
                let trimmed = text.trim();
                if trimmed.starts_with("/**") && !trimmed.starts_with("/***") {
                    let inner = &trimmed[3..trimmed.len().saturating_sub(2)];
                    doc_lines.push(inner.trim().to_string());
                } else {
                    break;
                }
            }
            "attribute_item" | "decorator" => {
                // Continue scanning past attributes and decorators to capture preceding docs
            }
            _ => break,
        }
        current = sibling.prev_sibling();
    }

    if doc_lines.is_empty() {
        None
    } else {
        doc_lines.reverse();
        Some(doc_lines.join("\n"))
    }
}

/// Common primitive types and keywords to ignore during type dependency extraction.
const IGNORED_TYPE_IDENTS: &[&str] = &[
    "Self", "bool", "u8", "u16", "u32", "u64", "u128", "usize", "i8", "i16", "i32", "i64", "i128",
    "isize", "f32", "f64", "str", "char", "int", "float", "str", "list", "dict", "set", "tuple",
    "number", "string", "boolean", "any", "void", "never", "unknown", "object",
];

/// Recursively traverses an AST node to extract all referenced `type_identifier`s or Python types.
fn extract_type_identifiers_from_node(node: Node, source: &[u8], types: &mut HashSet<String>) {
    if node.kind() == "type_identifier"
        || (node.kind() == "identifier"
            && node
                .parent()
                .is_some_and(|p| p.kind() == "type" || p.kind() == "type_annotation"))
    {
        if let Ok(text) = node.utf8_text(source) {
            let trimmed = text.trim();
            if !trimmed.is_empty() && !IGNORED_TYPE_IDENTS.contains(&trimmed) {
                types.insert(trimmed.to_string());
            }
        }
        return;
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        extract_type_identifiers_from_node(child, source, types);
    }
}

/// Finds the target struct/enum type name implemented by an enclosing `impl_item` (Rust).
fn find_enclosing_impl_type(node: Node, source: &[u8]) -> Option<String> {
    let mut current = node.parent();
    while let Some(parent) = current {
        if parent.kind() == "impl_item" {
            if let Some(type_node) = parent.child_by_field_name("type") {
                return extract_type_name(type_node, source);
            }
        }
        current = parent.parent();
    }
    None
}

/// Finds the trait name implemented by an enclosing `impl_item` (Rust).
fn find_enclosing_impl_trait(node: Node, source: &[u8]) -> Option<String> {
    let mut current = node.parent();
    while let Some(parent) = current {
        if parent.kind() == "impl_item" {
            if let Some(trait_node) = parent.child_by_field_name("trait") {
                return extract_type_name(trait_node, source);
            }
        }
        current = parent.parent();
    }
    None
}

/// Extracts a simple identifier or outer type name from a type node.
fn extract_type_name(node: Node, source: &[u8]) -> Option<String> {
    match node.kind() {
        "type_identifier" | "identifier" => node.utf8_text(source).ok().map(|s| s.to_string()),
        "generic_type" => {
            let inner = node.child_by_field_name("type")?;
            extract_type_name(inner, source)
        }
        _ => None,
    }
}

/// Extracts all Rust `use` statements from the AST into structured `FileImport` items.
fn extract_rust_imports(
    root: Node,
    source: &[u8],
    file_path: &Path,
    imports: &mut Vec<FileImport>,
) {
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        if node.kind() == "use_declaration" {
            extract_rust_use_tree(node, source, "", file_path, imports);
        } else {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                stack.push(child);
            }
        }
    }
}

fn extract_rust_use_tree(
    node: Node,
    source: &[u8],
    current_prefix: &str,
    file_path: &Path,
    imports: &mut Vec<FileImport>,
) {
    match node.kind() {
        "use_declaration" => {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.kind() != "use"
                    && child.kind() != ";"
                    && child.kind() != "visibility_modifier"
                {
                    extract_rust_use_tree(child, source, "", file_path, imports);
                }
            }
        }
        "scoped_use_list" => {
            let path_str = if let Some(path_node) = node.child_by_field_name("path") {
                path_node.utf8_text(source).unwrap_or("")
            } else {
                ""
            };
            let prefix = if current_prefix.is_empty() {
                path_str.to_string()
            } else if path_str.is_empty() {
                current_prefix.to_string()
            } else {
                format!("{}::{}", current_prefix, path_str)
            };

            if let Some(list_node) = node.child_by_field_name("list") {
                extract_rust_use_tree(list_node, source, &prefix, file_path, imports);
            }
        }
        "use_list" => {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.kind() != "{" && child.kind() != "}" && child.kind() != "," {
                    extract_rust_use_tree(child, source, current_prefix, file_path, imports);
                }
            }
        }
        "scoped_identifier" => {
            if let (Some(path_node), Some(name_node)) = (
                node.child_by_field_name("path"),
                node.child_by_field_name("name"),
            ) {
                let p = path_node.utf8_text(source).unwrap_or("");
                let full_prefix = if current_prefix.is_empty() {
                    p.to_string()
                } else {
                    format!("{}::{}", current_prefix, p)
                };
                let imported = name_node.utf8_text(source).unwrap_or("");
                if !imported.is_empty() {
                    imports.push(FileImport::new(file_path, &full_prefix, imported, imported));
                }
            }
        }
        "use_as_clause" => {
            if let (Some(path_node), Some(alias_node)) = (
                node.child_by_field_name("path"),
                node.child_by_field_name("alias"),
            ) {
                let alias = alias_node.utf8_text(source).unwrap_or("");
                if path_node.kind() == "scoped_identifier" {
                    if let (Some(inner_path), Some(name_node)) = (
                        path_node.child_by_field_name("path"),
                        path_node.child_by_field_name("name"),
                    ) {
                        let p = inner_path.utf8_text(source).unwrap_or("");
                        let full_prefix = if current_prefix.is_empty() {
                            p.to_string()
                        } else {
                            format!("{}::{}", current_prefix, p)
                        };
                        let imported = name_node.utf8_text(source).unwrap_or("");
                        imports.push(FileImport::new(file_path, &full_prefix, imported, alias));
                    }
                } else {
                    let imported = path_node.utf8_text(source).unwrap_or("");
                    imports.push(FileImport::new(file_path, current_prefix, imported, alias));
                }
            }
        }
        "use_wildcard" => {
            let path_str = if let Some(path_node) = node.child_by_field_name("path") {
                path_node.utf8_text(source).unwrap_or("")
            } else if let Some(child) = node.child(0) {
                if child.kind() == "scoped_identifier" || child.kind() == "identifier" {
                    child.utf8_text(source).unwrap_or("")
                } else {
                    ""
                }
            } else {
                ""
            };
            let full_prefix = if current_prefix.is_empty() {
                path_str.to_string()
            } else if path_str.is_empty() {
                current_prefix.to_string()
            } else {
                format!("{}::{}", current_prefix, path_str)
            };
            imports.push(FileImport::wildcard(file_path, &full_prefix, "*"));
        }
        "identifier" | "type_identifier" => {
            let ident = node.utf8_text(source).unwrap_or("");
            if !ident.is_empty() && ident != "self" {
                imports.push(FileImport::new(file_path, current_prefix, ident, ident));
            } else if ident == "self" && !current_prefix.is_empty() {
                let last_part = current_prefix.rsplit("::").next().unwrap_or(current_prefix);
                imports.push(FileImport::new(
                    file_path,
                    current_prefix,
                    "self",
                    last_part,
                ));
            }
        }
        _ => {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                extract_rust_use_tree(child, source, current_prefix, file_path, imports);
            }
        }
    }
}

/// Extracts all Python import statements (`import ...`, `from ... import ...`) from the AST.
fn extract_python_imports(
    root: Node,
    source: &[u8],
    file_path: &Path,
    imports: &mut Vec<FileImport>,
) {
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        match node.kind() {
            "import_statement" => {
                let mut inner_cursor = node.walk();
                for child in node.children(&mut inner_cursor) {
                    match child.kind() {
                        "dotted_name" => {
                            if let Ok(name) = child.utf8_text(source) {
                                let name = name.trim();
                                if !name.is_empty() {
                                    let local_name = name.split('.').next().unwrap_or(name);
                                    imports
                                        .push(FileImport::new(file_path, name, name, local_name));
                                    if local_name != name {
                                        imports.push(FileImport::new(file_path, name, name, name));
                                    }
                                }
                            }
                        }
                        "aliased_import" => {
                            if let (Some(name_node), Some(alias_node)) = (
                                child.child_by_field_name("name"),
                                child.child_by_field_name("alias"),
                            ) {
                                if let (Ok(name), Ok(alias)) =
                                    (name_node.utf8_text(source), alias_node.utf8_text(source))
                                {
                                    let name = name.trim();
                                    let alias = alias.trim();
                                    if !name.is_empty() && !alias.is_empty() {
                                        imports.push(FileImport::new(file_path, name, name, alias));
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
            "import_from_statement" => {
                let module_spec = if let Some(mod_node) = node.child_by_field_name("module_name") {
                    mod_node.utf8_text(source).unwrap_or("").trim().to_string()
                } else {
                    let node_text = node.utf8_text(source).unwrap_or("");
                    if let Some(from_idx) = node_text.find("from ") {
                        if let Some(import_idx) = node_text[from_idx + 5..].find("import") {
                            node_text[from_idx + 5..from_idx + 5 + import_idx]
                                .trim()
                                .to_string()
                        } else {
                            String::new()
                        }
                    } else {
                        String::new()
                    }
                };

                let mut passed_import = false;
                let mut inner_cursor = node.walk();
                for child in node.children(&mut inner_cursor) {
                    if child.kind() == "import" {
                        passed_import = true;
                        continue;
                    }
                    if !passed_import {
                        continue;
                    }
                    extract_python_import_targets(child, source, file_path, &module_spec, imports);
                }
            }
            _ => {
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    stack.push(child);
                }
            }
        }
    }
}

fn extract_python_import_targets(
    node: Node,
    source: &[u8],
    file_path: &Path,
    module_spec: &str,
    imports: &mut Vec<FileImport>,
) {
    match node.kind() {
        "dotted_name" | "identifier" => {
            if let Ok(name) = node.utf8_text(source) {
                let name = name.trim();
                if !name.is_empty() {
                    imports.push(FileImport::new(file_path, module_spec, name, name));
                }
            }
        }
        "aliased_import" => {
            if let (Some(name_node), Some(alias_node)) = (
                node.child_by_field_name("name"),
                node.child_by_field_name("alias"),
            ) {
                if let (Ok(name), Ok(alias)) =
                    (name_node.utf8_text(source), alias_node.utf8_text(source))
                {
                    let name = name.trim();
                    let alias = alias.trim();
                    if !name.is_empty() && !alias.is_empty() {
                        imports.push(FileImport::new(file_path, module_spec, name, alias));
                    }
                }
            }
        }
        "wildcard_import" | "*" => {
            imports.push(FileImport::wildcard(file_path, module_spec, "*"));
        }
        _ => {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.kind() != "(" && child.kind() != ")" && child.kind() != "," {
                    extract_python_import_targets(child, source, file_path, module_spec, imports);
                }
            }
        }
    }
}

/// Extracts all TypeScript / JavaScript import statements from the AST.
fn extract_typescript_imports(
    root: Node,
    source: &[u8],
    file_path: &Path,
    imports: &mut Vec<FileImport>,
) {
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        if node.kind() == "import_statement" {
            let source_node = node.child_by_field_name("source");
            let raw_source = if let Some(sn) = source_node {
                sn.utf8_text(source).unwrap_or("")
            } else {
                ""
            };
            let module_spec = raw_source
                .trim()
                .trim_matches(|c| c == '\'' || c == '"')
                .to_string();
            if module_spec.is_empty() {
                continue;
            }

            let import_clause = node
                .child_by_field_name("import_clause")
                .or_else(|| find_child_by_kinds(node, &["import_clause"]));

            if let Some(clause) = import_clause {
                let mut clause_cursor = clause.walk();
                for child in clause.children(&mut clause_cursor) {
                    match child.kind() {
                        "identifier" => {
                            if let Ok(ident) = child.utf8_text(source) {
                                let ident = ident.trim();
                                if !ident.is_empty() && ident != "type" {
                                    imports.push(FileImport::new(
                                        file_path,
                                        &module_spec,
                                        "default",
                                        ident,
                                    ));
                                }
                            }
                        }
                        "named_imports" => {
                            let mut named_cursor = child.walk();
                            for spec in child.children(&mut named_cursor) {
                                if spec.kind() == "import_specifier" {
                                    let name_node = spec.child_by_field_name("name");
                                    let alias_node = spec.child_by_field_name("alias");

                                    let (name, alias) = match (name_node, alias_node) {
                                        (Some(n), Some(a)) => (
                                            n.utf8_text(source).unwrap_or(""),
                                            a.utf8_text(source).unwrap_or(""),
                                        ),
                                        (Some(n), None) => {
                                            let n_text = n.utf8_text(source).unwrap_or("");
                                            (n_text, n_text)
                                        }
                                        (None, _) => {
                                            let mut ids = Vec::new();
                                            let mut id_cursor = spec.walk();
                                            for id_child in spec.children(&mut id_cursor) {
                                                if id_child.kind() == "identifier"
                                                    || id_child.kind() == "type_identifier"
                                                {
                                                    if let Ok(t) = id_child.utf8_text(source) {
                                                        if t != "type" && t != "as" {
                                                            ids.push(t);
                                                        }
                                                    }
                                                }
                                            }
                                            if ids.len() >= 2 {
                                                (ids[0], ids[1])
                                            } else if ids.len() == 1 {
                                                (ids[0], ids[0])
                                            } else {
                                                ("", "")
                                            }
                                        }
                                    };

                                    let name = name.trim();
                                    let alias = alias.trim();
                                    if !name.is_empty() && !alias.is_empty() {
                                        imports.push(FileImport::new(
                                            file_path,
                                            &module_spec,
                                            name,
                                            alias,
                                        ));
                                    }
                                }
                            }
                        }
                        "namespace_import" => {
                            let alias = if let Some(alias_node) = child.child_by_field_name("alias")
                            {
                                alias_node.utf8_text(source).unwrap_or("").trim()
                            } else {
                                let mut id_name = "";
                                let mut ns_cursor = child.walk();
                                for ns_child in child.children(&mut ns_cursor) {
                                    if ns_child.kind() == "identifier" {
                                        id_name = ns_child.utf8_text(source).unwrap_or("").trim();
                                    }
                                }
                                id_name
                            };

                            if !alias.is_empty() {
                                imports.push(FileImport::wildcard(file_path, &module_spec, alias));
                            }
                        }
                        _ => {}
                    }
                }
            } else {
                imports.push(FileImport::new(file_path, &module_spec, "*", "*"));
            }
        } else {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                stack.push(child);
            }
        }
    }
}

/// Extracts the receiver type name for a Go method declaration (e.g. `Server` from `func (s *Server) Start()`).
fn find_go_receiver_type(node: Node, source: &[u8]) -> Option<String> {
    let receiver = node.child_by_field_name("receiver")?;
    find_first_descendant_by_kind(receiver, "type_identifier", source)
}

fn find_first_descendant_by_kind(node: Node, kind: &str, source: &[u8]) -> Option<String> {
    if node.kind() == kind {
        return node.utf8_text(source).ok().map(|s| s.to_string());
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if let Some(found) = find_first_descendant_by_kind(child, kind, source) {
            return Some(found);
        }
    }
    None
}

/// Extracts Go import declarations into structured FileImport items.
fn extract_go_imports(root: Node, source: &[u8], file_path: &Path, imports: &mut Vec<FileImport>) {
    let mut cursor = root.walk();
    for child in root.children(&mut cursor) {
        if child.kind() == "import_declaration" {
            extract_go_import_specs(child, source, file_path, imports);
        }
    }
}

fn extract_go_import_specs(
    node: Node,
    source: &[u8],
    file_path: &Path,
    imports: &mut Vec<FileImport>,
) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "import_spec" {
            let path_node = child.child_by_field_name("path");
            let name_node = child.child_by_field_name("name");

            if let Some(p_node) = path_node {
                if let Ok(raw_path) = p_node.utf8_text(source) {
                    let module_specifier = raw_path.trim_matches('"').to_string();
                    let alias = name_node
                        .and_then(|n| n.utf8_text(source).ok())
                        .map(|s| s.trim().to_string());

                    if let Some(alias_name) = alias {
                        if alias_name == "." {
                            imports.push(FileImport::wildcard(file_path, &module_specifier, "*"));
                        } else if alias_name != "_" {
                            imports.push(FileImport::new(
                                file_path,
                                &module_specifier,
                                alias_name.clone(),
                                alias_name,
                            ));
                        }
                    } else {
                        let default_name = module_specifier
                            .split('/')
                            .next_back()
                            .unwrap_or(&module_specifier)
                            .to_string();
                        imports.push(FileImport::new(
                            file_path,
                            &module_specifier,
                            default_name.clone(),
                            default_name,
                        ));
                    }
                }
            }
        } else if child.kind() == "import_spec_list" {
            extract_go_import_specs(child, source, file_path, imports);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_extract_rust_imports() {
        let parser = AstExtractor::new().expect("Failed to create AstExtractor");
        let source = r#"
            use crate::engine::parser::CodeParser;
            use crate::models::{Symbol, Edge as ReferenceEdge, *};
            use std::collections::HashMap;

            fn foo() {
                let x = 1;
            }
        "#;
        let mut next_id = 1;
        let (_, _, imports) = parser
            .parse_file_with_imports(
                &PathBuf::from("src/test.rs"),
                source.as_bytes(),
                &mut next_id,
            )
            .expect("Failed to parse");

        assert!(imports
            .iter()
            .any(|i| i.imported_name == "CodeParser" && i.local_name == "CodeParser"));
        assert!(imports
            .iter()
            .any(|i| i.imported_name == "Symbol" && i.local_name == "Symbol"));
        assert!(imports
            .iter()
            .any(|i| i.imported_name == "Edge" && i.local_name == "ReferenceEdge"));
        assert!(imports
            .iter()
            .any(|i| i.is_wildcard && i.module_specifier == "crate::models"));
        assert!(imports.iter().any(|i| i.imported_name == "HashMap"));
    }

    #[test]
    fn test_extract_python_imports() {
        let parser = AstExtractor::new().expect("Failed to create AstExtractor");
        let source = r#"
import os
import sys as system
from models.user import User as AppUser, Role
from ..services import auth
from math import *

def main():
    pass
"#;
        let mut next_id = 1;
        let (_, _, imports) = parser
            .parse_file_with_imports(
                &PathBuf::from("app/main.py"),
                source.as_bytes(),
                &mut next_id,
            )
            .expect("Failed to parse");

        assert!(imports
            .iter()
            .any(|i| i.module_specifier == "os" && i.local_name == "os"));
        assert!(imports
            .iter()
            .any(|i| i.module_specifier == "sys" && i.local_name == "system"));
        assert!(imports.iter().any(|i| i.module_specifier == "models.user"
            && i.imported_name == "User"
            && i.local_name == "AppUser"));
        assert!(imports.iter().any(|i| i.module_specifier == "models.user"
            && i.imported_name == "Role"
            && i.local_name == "Role"));
        assert!(imports
            .iter()
            .any(|i| i.module_specifier == "..services" && i.imported_name == "auth"));
        assert!(imports
            .iter()
            .any(|i| i.module_specifier == "math" && i.is_wildcard));
    }

    #[test]
    fn test_extract_typescript_imports() {
        let parser = AstExtractor::new().expect("Failed to create AstExtractor");
        let source = r#"
import React, { useState, useEffect as useFx } from 'react';
import * as Path from 'path';
import { helper } from '../utils/helper';
import './styles.css';

export function Component() {
    return null;
}
"#;
        let mut next_id = 1;
        let (_, _, imports) = parser
            .parse_file_with_imports(
                &PathBuf::from("src/Component.tsx"),
                source.as_bytes(),
                &mut next_id,
            )
            .expect("Failed to parse");

        assert!(imports.iter().any(|i| i.module_specifier == "react"
            && i.imported_name == "default"
            && i.local_name == "React"));
        assert!(imports.iter().any(|i| i.module_specifier == "react"
            && i.imported_name == "useState"
            && i.local_name == "useState"));
        assert!(imports.iter().any(|i| i.module_specifier == "react"
            && i.imported_name == "useEffect"
            && i.local_name == "useFx"));
        assert!(imports
            .iter()
            .any(|i| i.module_specifier == "path" && i.is_wildcard && i.local_name == "Path"));
        assert!(imports
            .iter()
            .any(|i| i.module_specifier == "../utils/helper" && i.imported_name == "helper"));
        assert!(imports.iter().any(|i| i.module_specifier == "./styles.css"));
    }

    #[test]
    fn test_extract_go_functions_and_structs() {
        let extractor = AstExtractor::new().expect("Failed to initialize AstExtractor");

        let source = r#"
package main

import (
    "fmt"
    "net/http"
    gin "github.com/gin-gonic/gin"
)

// Server represents an HTTP service.
type Server struct {
    host string
    port int
}

// Start launches the server listener.
func (s *Server) Start() error {
    fmt.Println("Starting server")
    gin.New()
    return nil
}

// HealthCheck performs system health diagnostics.
func HealthCheck() string {
    return "OK"
}
"#;
        let mut next_id = 0;
        let (symbols, edges, imports) = extractor
            .parse_file_with_imports(Path::new("server.go"), source.as_bytes(), &mut next_id)
            .expect("Failed to parse Go source");

        assert_eq!(symbols.len(), 3);
        let names: Vec<_> = symbols.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"Server"));
        assert!(names.contains(&"Start"));
        assert!(names.contains(&"HealthCheck"));

        // Verify Server is Struct and Start is Method with AstParent edge to Server
        let server_sym = symbols.iter().find(|s| s.name == "Server").unwrap();
        assert_eq!(server_sym.kind, SymbolKind::Struct);

        let start_sym = symbols.iter().find(|s| s.name == "Start").unwrap();
        assert_eq!(start_sym.kind, SymbolKind::Method);
        assert!(edges.iter().any(|e| e.source == start_sym.id
            && e.target_ident == "Server"
            && e.kind == EdgeKind::AstParent));

        // Verify imports
        assert_eq!(imports.len(), 3);
        assert!(imports
            .iter()
            .any(|i| i.module_specifier == "fmt" && i.local_name == "fmt"));
        assert!(imports
            .iter()
            .any(|i| i.module_specifier == "net/http" && i.local_name == "http"));
        assert!(imports
            .iter()
            .any(|i| i.module_specifier == "github.com/gin-gonic/gin" && i.local_name == "gin"));
    }
}
