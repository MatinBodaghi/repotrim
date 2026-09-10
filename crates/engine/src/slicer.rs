use tree_sitter::{Node, Parser};

use crate::parser::SupportedLanguage;
use crate::symbol::{SymbolKind, SymbolNode};

/// High-performance AST-aware program slicer.
///
/// Implements control-flow skeletonization and inter-procedural call extraction
/// based on the foundational program slicing paradigm introduced by Mark Weiser (1981):
///
/// > Weiser, M. (1981). *"Program Slicing"*. Proceedings of the 5th International
/// > Conference on Software Engineering (ICSE '81), pp. 439–449; IEEE Transactions on
/// > Software Engineering (TSE), SE-10(4): 352–357, 1984.
///
/// Preserves control flow branches (`if`, `else`, `match`, `switch`), loop constructs,
/// inter-procedural call invocations, error propagation, and exit statements (`return`, `throw`,
/// `panic!`, `?`), while eliding contiguous blocks of linear variable declarations and
/// local computations into concise line-count omission markers (`// ... [N lines elided] ...`).
pub struct AstSlicer;

impl AstSlicer {
    /// Slices a function or method AST symbol, returning its structural skeleton.
    pub fn slice_symbol(symbol: &SymbolNode, file_source: &str) -> String {
        if symbol.kind != SymbolKind::Function && symbol.kind != SymbolKind::Method {
            if symbol.span.end_byte <= file_source.len()
                && symbol.span.start_byte < symbol.span.end_byte
            {
                return file_source[symbol.span.start_byte..symbol.span.end_byte]
                    .trim()
                    .to_string();
            }
            return symbol.signature.clone();
        }

        if symbol.span.end_byte > file_source.len()
            || symbol.span.start_byte >= symbol.span.end_byte
        {
            return symbol.signature.clone();
        }

        let raw_func = file_source[symbol.span.start_byte..symbol.span.end_byte].trim();
        let lang = SupportedLanguage::from_path(&symbol.file_path);

        Self::slice_function_source(raw_func, lang)
    }

    /// Slices raw function source code into a control-flow outline.
    pub fn slice_function_source(source: &str, lang: Option<SupportedLanguage>) -> String {
        let lines: Vec<&str> = source.lines().collect();
        if lines.len() <= 6 {
            return source.to_string();
        }

        let lang = match lang {
            Some(l) => l,
            None => return source.to_string(),
        };

        let mut parser = Parser::new();
        let ts_lang = match lang {
            SupportedLanguage::Rust => tree_sitter_rust::LANGUAGE.into(),
            SupportedLanguage::Python => tree_sitter_python::LANGUAGE.into(),
            SupportedLanguage::TypeScript | SupportedLanguage::Tsx => {
                tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()
            }
            SupportedLanguage::Go => tree_sitter_go::LANGUAGE.into(),
        };

        if parser.set_language(&ts_lang).is_err() {
            return source.to_string();
        }

        // For TypeScript class methods, wrap in dummy class to ensure valid method parsing.
        // For Go functions/methods, prepend dummy package clause if missing.
        let (parse_text, offset) = if (lang == SupportedLanguage::TypeScript
            || lang == SupportedLanguage::Tsx)
            && !source.trim_start().starts_with("function")
            && !source.trim_start().starts_with("export function")
        {
            (format!("class _Dummy {{\n{}\n}}", source), 15)
        } else if lang == SupportedLanguage::Go && !source.trim_start().starts_with("package") {
            (format!("package _dummy\n{}", source), 15)
        } else {
            (source.to_string(), 0)
        };

        let tree = match parser.parse(&parse_text, None) {
            Some(t) => t,
            None => return source.to_string(),
        };

        let root = tree.root_node();
        let source_bytes = parse_text.as_bytes();

        // Locate function node
        let func_node = match Self::find_function_node(root, offset) {
            Some(n) => n,
            None => return source.to_string(),
        };

        // Locate body block
        let body_node = match func_node.child_by_field_name("body") {
            Some(b) => b,
            None => return source.to_string(),
        };

        let is_python = lang == SupportedLanguage::Python;
        let comment_prefix = if is_python { "#" } else { "//" };

        let header_end = body_node.start_byte() - offset;
        if header_end > source.len() {
            return source.to_string();
        }

        let header = source[..header_end].trim_end();

        // Extract body statements (unwrapping statement_list for Go if present)
        let mut target_body = body_node;
        let mut t_cursor = body_node.walk();
        for child in body_node.named_children(&mut t_cursor) {
            if child.kind() == "statement_list" {
                target_body = child;
                break;
            }
        }

        let mut cursor = target_body.walk();
        let statements: Vec<Node> = target_body.named_children(&mut cursor).collect();

        if statements.is_empty() {
            return source.to_string();
        }

        // Detect base indentation from the first statement
        let first_stmt_start = statements[0].start_byte() - offset;
        let base_indent = Self::detect_indentation(source, first_stmt_start);

        let mut rendered_elements: Vec<String> = Vec::new();
        let mut elided_start_line: Option<usize> = None;
        let mut elided_end_line: usize = 0;

        for (idx, stmt) in statements.iter().enumerate() {
            let stmt_start = stmt.start_byte().saturating_sub(offset);
            let stmt_end = stmt.end_byte().saturating_sub(offset);

            if stmt_start >= source.len() || stmt_end > source.len() {
                continue;
            }

            // In Python, preserve docstring expression as significant
            let is_doc = is_python
                && stmt.kind() == "expression_statement"
                && stmt.child(0).map(|c| c.kind() == "string").unwrap_or(false);

            // In Rust, preserve the final tail expression of a block (implicit return value)
            let is_rust_tail_expr = lang == SupportedLanguage::Rust
                && idx == statements.len() - 1
                && stmt.kind() != "let_declaration";

            let is_sig = is_doc
                || is_rust_tail_expr
                || Self::is_significant_statement(*stmt, source_bytes, offset);

            if is_sig {
                // Flush any accumulated elided lines
                if let Some(start_line) = elided_start_line {
                    let count = elided_end_line.saturating_sub(start_line) + 1;
                    rendered_elements.push(format!(
                        "{}{} ... [{} lines elided] ...",
                        base_indent, comment_prefix, count
                    ));
                    elided_start_line = None;
                }

                let stmt_text = source[stmt_start..stmt_end].trim_end();
                rendered_elements.push(stmt_text.to_string());
            } else {
                let start_row = stmt.start_position().row;
                let end_row = stmt.end_position().row;

                if elided_start_line.is_none() {
                    elided_start_line = Some(start_row);
                }
                elided_end_line = end_row;
            }
        }

        // Flush trailing elided statements
        if let Some(start_line) = elided_start_line {
            let count = elided_end_line.saturating_sub(start_line) + 1;
            rendered_elements.push(format!(
                "{}{} ... [{} lines elided] ...",
                base_indent, comment_prefix, count
            ));
        }

        let body_content = rendered_elements.join("\n");

        if is_python {
            format!("{}\n{}", header, body_content)
        } else {
            format!(
                "{} {{\n{}\n}}",
                header.trim_end_matches('{').trim(),
                body_content
            )
        }
    }

    /// Recursively identifies if an AST statement node represents critical control flow or calls.
    fn is_significant_statement(node: Node, source: &[u8], offset: usize) -> bool {
        let kind = node.kind();

        // Control flow keywords and branch constructs
        if kind.contains("if")
            || kind.contains("match")
            || kind.contains("switch")
            || kind.contains("for")
            || kind.contains("while")
            || kind.contains("loop")
            || kind.contains("try")
            || kind.contains("catch")
            || kind.contains("except")
            || kind.contains("finally")
            || kind.contains("return")
            || kind.contains("raise")
            || kind.contains("throw")
            || kind.contains("yield")
            || kind.contains("panic")
            || kind.contains("defer")
            || kind.contains("select")
            || kind.contains("guard")
        {
            return true;
        }

        // Check if statement contains function/method call expressions or error propagation
        Self::contains_call_or_exit(node, source, offset)
    }

    /// Recursively inspects a node for invocations or error propagation.
    fn contains_call_or_exit(node: Node, source: &[u8], _offset: usize) -> bool {
        let kind = node.kind();
        if kind == "call_expression"
            || kind == "call"
            || kind == "method_invocation"
            || kind == "try_expression"
            || kind == "macro_invocation"
        {
            return true;
        }

        // Rust question mark operator or panic call
        if let Ok(text) = node.utf8_text(source) {
            if text.contains('?') || text.contains("panic!") {
                return true;
            }
        }

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if Self::contains_call_or_exit(child, source, _offset) {
                return true;
            }
        }
        false
    }

    /// Locates the primary function node in a parsed tree.
    fn find_function_node(root: Node, offset: usize) -> Option<Node> {
        let mut cursor = root.walk();
        for child in root.children(&mut cursor) {
            let kind = child.kind();
            if kind == "function_item"
                || kind == "function_definition"
                || kind == "function_declaration"
                || kind == "method_declaration"
                || kind == "method_definition"
            {
                return Some(child);
            }
            if kind == "class_declaration" {
                if let Some(body) = child.child_by_field_name("body") {
                    let mut b_cursor = body.walk();
                    for b_child in body.children(&mut b_cursor) {
                        if b_child.kind() == "method_definition" && b_child.start_byte() >= offset {
                            return Some(b_child);
                        }
                    }
                }
            }
        }
        None
    }

    /// Detects leading indentation whitespace on the line containing byte offset.
    fn detect_indentation(source: &str, byte_offset: usize) -> String {
        let prefix = &source[..byte_offset.min(source.len())];
        if let Some(line_start) = prefix.rfind('\n') {
            let line = &prefix[line_start + 1..];
            let spaces = line
                .chars()
                .take_while(|c| c.is_whitespace())
                .collect::<String>();
            if !spaces.is_empty() {
                return spaces;
            }
        }
        "    ".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_slice_rust_function_with_branches_and_calls() {
        let src = r#"pub fn process_order(id: u64, amount: f64) -> Result<Receipt, Error> {
    let tax_rate = 0.08;
    let discount = 5.0;
    let shipping = 10.0;
    let adjusted = amount * (1.0 + tax_rate) - discount + shipping;
    if adjusted <= 0.0 {
        return Err(Error::InvalidAmount);
    }
    let fee = 2.5;
    let total = adjusted + fee;
    let receipt = gateway.charge(id, total)?;
    Ok(receipt)
}"#;

        let sliced = AstSlicer::slice_function_source(src, Some(SupportedLanguage::Rust));
        assert!(sliced
            .contains("pub fn process_order(id: u64, amount: f64) -> Result<Receipt, Error> {"));
        assert!(sliced.contains("// ... [4 lines elided] ..."));
        assert!(sliced.contains("if adjusted <= 0.0 {"));
        assert!(sliced.contains("return Err(Error::InvalidAmount);"));
        assert!(sliced.contains("// ... [2 lines elided] ..."));
        assert!(sliced.contains("gateway.charge(id, total)?"));
        assert!(sliced.contains("Ok(receipt)"));
        assert!(sliced.ends_with('}'));
    }

    #[test]
    fn test_slice_python_function_with_branches_and_calls() {
        let src = r#"def verify_user(user_id: int, token: str) -> bool:
    """Verifies user session token."""
    cache_ttl = 300
    retry_count = 3
    is_valid = False
    if not token:
        return False
    user = db.lookup_user(user_id)
    if user is None:
        raise UserNotFoundError()
    return True"#;

        let sliced = AstSlicer::slice_function_source(src, Some(SupportedLanguage::Python));
        assert!(sliced.contains("def verify_user(user_id: int, token: str) -> bool:"));
        assert!(sliced.contains("\"\"\"Verifies user session token.\"\"\""));
        assert!(sliced.contains("# ... [3 lines elided] ..."));
        assert!(sliced.contains("if not token:"));
        assert!(sliced.contains("db.lookup_user(user_id)"));
        assert!(sliced.contains("if user is None:"));
        assert!(sliced.contains("raise UserNotFoundError()"));
        assert!(sliced.contains("return True"));
    }

    #[test]
    fn test_slice_go_method_with_receiver_and_branches() {
        let src = r#"func (s *Server) Handle(req Request) (*Response, error) {
    timeout := 30
    retries := 3
    factor := 2
    if req.Body == nil {
        return nil, ErrNilBody
    }
    res, err := s.client.Do(req)
    if err != nil {
        return nil, err
    }
    return res, nil
}"#;

        let sliced = AstSlicer::slice_function_source(src, Some(SupportedLanguage::Go));
        assert!(sliced.contains("func (s *Server) Handle(req Request) (*Response, error) {"));
        assert!(sliced.contains("// ... [3 lines elided] ..."));
        assert!(sliced.contains("if req.Body == nil {"));
        assert!(sliced.contains("s.client.Do(req)"));
        assert!(sliced.contains("if err != nil {"));
        assert!(sliced.contains("return res, nil"));
        assert!(sliced.ends_with('}'));
    }

    #[test]
    fn test_slice_typescript_method() {
        let src = r#"public calculateFee(amount: number): number {
    const base = 5;
    const rate = 0.02;
    const surcharge = 1.5;
    if (amount <= 0) {
        throw new Error("Invalid");
    }
    const discount = computeDiscount(amount);
    return amount * rate + base - discount;
}"#;

        let sliced = AstSlicer::slice_function_source(src, Some(SupportedLanguage::TypeScript));
        assert!(sliced.contains("public calculateFee(amount: number): number {"));
        assert!(sliced.contains("// ... [3 lines elided] ..."));
        assert!(sliced.contains("if (amount <= 0) {"));
        assert!(sliced.contains("computeDiscount(amount)"));
        assert!(sliced.ends_with('}'));
    }

    #[test]
    fn test_short_function_preserved_in_full() {
        let src = "pub fn add(a: i32, b: i32) -> i32 {\n    let c = a + b;\n    c\n}";
        let sliced = AstSlicer::slice_function_source(src, Some(SupportedLanguage::Rust));
        assert_eq!(sliced, src);
    }
}
