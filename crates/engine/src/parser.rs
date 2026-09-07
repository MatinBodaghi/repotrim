use std::collections::{HashMap, HashSet};
use std::path::Path;
use tree_sitter::{Language, Node, Parser, Query, QueryCursor, StreamingIterator};

use crate::error::EngineError;
use crate::symbol::{EdgeKind, ReferenceEdge, SymbolId, SymbolKind, SymbolNode, TextSpan};
use crate::tokens::estimate_tokens;

/// Embedded declarative Tree-sitter queries for Rust symbol and reference extraction.
const RUST_QUERY_SOURCE: &str = include_str!("../queries/rust.scm");

/// AST symbol and reference extractor for source files.
pub struct AstExtractor {
    language: Language,
    query: Query,
}

impl AstExtractor {
    /// Creates a new `AstExtractor` compiling the embedded Rust queries.
    pub fn new() -> Result<Self, EngineError> {
        let language: Language = tree_sitter_rust::LANGUAGE.into();
        let query = Query::new(&language, RUST_QUERY_SOURCE)?;
        Ok(Self { language, query })
    }

    /// Parses a file's source bytes, extracting symbols (with stripped function bodies)
    /// and tracking reference edges to called identifiers.
    pub fn parse_file(
        &self,
        path: &Path,
        source: &[u8],
        next_id: &mut u32,
    ) -> Result<(Vec<SymbolNode>, Vec<ReferenceEdge>), EngineError> {
        let mut parser = Parser::new();
        parser
            .set_language(&self.language)
            .map_err(|_| EngineError::ParseError)?;

        let tree = parser.parse(source, None).ok_or(EngineError::ParseError)?;
        let root_node = tree.root_node();
        let source_str = std::str::from_utf8(source)?;

        let mut symbols = Vec::new();
        let mut edges = Vec::new();

        let mut fn_node_to_symbol_id: HashMap<usize, SymbolId> = HashMap::new();
        let mut seen_symbol_nodes: HashSet<usize> = HashSet::new();
        let mut seen_call_nodes: HashSet<usize> = HashSet::new();

        let fn_capture_idx = self.query.capture_index_for_name("function");
        let struct_capture_idx = self.query.capture_index_for_name("struct");
        let enum_capture_idx = self.query.capture_index_for_name("enum");
        let trait_capture_idx = self.query.capture_index_for_name("trait");
        let call_capture_idx = self.query.capture_index_for_name("call");
        let call_target_idx = self.query.capture_index_for_name("call.target");

        // Pass 1: Extract symbols (functions, methods, structs, enums, traits)
        let mut cursor = QueryCursor::new();
        let mut matches = cursor.matches(&self.query, root_node, source);

        while let Some(m) = matches.next() {
            for capture in m.captures {
                let node = capture.node;

                if Some(capture.index) == fn_capture_idx {
                    if !seen_symbol_nodes.insert(node.id()) {
                        continue;
                    }

                    let name = if let Some(name_node) = node.child_by_field_name("name") {
                        name_node.utf8_text(source)?.to_string()
                    } else {
                        continue;
                    };

                    let kind = if is_inside_impl(node) {
                        SymbolKind::Method
                    } else {
                        SymbolKind::Function
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

                    let docstring = extract_docstring(node, source);
                    let token_cost = estimate_tokens(&signature);
                    let ast_hash = *blake3::hash(signature.as_bytes()).as_bytes();

                    let id = SymbolId(*next_id);
                    *next_id += 1;

                    fn_node_to_symbol_id.insert(node.id(), id);

                    // If method is inside an impl block, emit AstParent edge to the enclosing struct/type
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

                    // Extract type references from struct field declarations
                    let mut type_idents = HashSet::new();
                    let mut cursor = node.walk();
                    for child in node.children(&mut cursor) {
                        if child.kind().contains("field") || child.kind() == "declaration_list" {
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

                    // Extract type references from enum variant fields
                    let mut type_idents = HashSet::new();
                    let mut cursor = node.walk();
                    for child in node.children(&mut cursor) {
                        if child.kind().contains("variant")
                            || child.kind().contains("field")
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
                        kind: SymbolKind::Enum,
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
        let mut call_matches = call_cursor.matches(&self.query, root_node, source);

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
                let mut parent_symbol_id = None;

                while let Some(ancestor) = current {
                    if let Some(&sym_id) = fn_node_to_symbol_id.get(&ancestor.id()) {
                        parent_symbol_id = Some(sym_id);
                        break;
                    }
                    current = ancestor.parent();
                }

                if let Some(source_id) = parent_symbol_id {
                    let target_ident = target.utf8_text(source)?.to_string();
                    edges.push(ReferenceEdge {
                        source: source_id,
                        target_ident,
                        kind: EdgeKind::Call,
                    });
                }
            }
        }

        Ok((symbols, edges))
    }
}

impl Default for AstExtractor {
    fn default() -> Self {
        Self::new().expect("Failed to initialize AstExtractor with embedded Rust queries")
    }
}

/// Checks whether a node is inside an `impl_item`.
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

/// Extracts docstrings from comments preceding an AST node.
fn extract_docstring(node: Node, source: &[u8]) -> Option<String> {
    let mut doc_lines = Vec::new();
    let mut current = node.prev_sibling();

    while let Some(sibling) = current {
        match sibling.kind() {
            "line_comment" => {
                let text = sibling.utf8_text(source).unwrap_or("");
                let trimmed = text.trim();
                if let Some(content) = trimmed
                    .strip_prefix("///")
                    .or_else(|| trimmed.strip_prefix("//!"))
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
            "attribute_item" => {
                // Continue scanning past outer attributes (e.g. #[inline]) to capture preceding docs
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
    "isize", "f32", "f64", "str", "char",
];

/// Recursively traverses an AST node to extract all referenced `type_identifier`s.
fn extract_type_identifiers_from_node(node: Node, source: &[u8], types: &mut HashSet<String>) {
    if node.kind() == "type_identifier" {
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

/// Finds the target struct/enum type name implemented by an enclosing `impl_item`.
fn find_enclosing_impl_type(node: Node, source: &[u8]) -> Option<String> {
    let mut current = node.parent();
    while let Some(parent) = current {
        if parent.kind() == "impl_item" {
            if let Some(type_node) = parent.child_by_field_name("type") {
                return extract_type_name(type_node, source);
            }
            return None;
        }
        current = parent.parent();
    }
    None
}

/// Finds the trait name implemented by an enclosing `impl Trait for Type` item.
fn find_enclosing_impl_trait(node: Node, source: &[u8]) -> Option<String> {
    let mut current = node.parent();
    while let Some(parent) = current {
        if parent.kind() == "impl_item" {
            if let Some(trait_node) = parent.child_by_field_name("trait") {
                return extract_type_name(trait_node, source);
            }
            return None;
        }
        current = parent.parent();
    }
    None
}

/// Extracts the base identifier from a type node (e.g. `Foo<T>` -> `Foo`, `crate::Foo` -> `Foo`).
fn extract_type_name(node: Node, source: &[u8]) -> Option<String> {
    match node.kind() {
        "type_identifier" => node.utf8_text(source).ok().map(|s| s.trim().to_string()),
        "generic_type" => node
            .child_by_field_name("type")
            .and_then(|inner| extract_type_name(inner, source)),
        "scoped_type_identifier" => node
            .child_by_field_name("name")
            .and_then(|inner| extract_type_name(inner, source)),
        _ => None,
    }
}
