use std::collections::HashMap;
use std::path::PathBuf;

use crate::symbol::{LodLevel, SymbolId, SymbolKind, SymbolNode};
use crate::tokens::estimate_tokens;

/// High-performance Multi-Resolution Level-of-Detail (LOD) context formatter.
///
/// Dynamically assigns code resolution levels (Signature, Doc, Sliced, Full)
/// to selected symbols based on their seed centrality and Personalized PageRank
/// diffusion scores, and renders deterministic Markdown context organized by file.
pub struct ContextFormatter;

impl ContextFormatter {
    /// Renders a single symbol at a given Level-of-Detail (LOD).
    pub fn render_symbol(symbol: &SymbolNode, lod: LodLevel, file_source: Option<&str>) -> String {
        let ext = symbol
            .file_path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");
        let is_python = ext == "py";
        let is_go = ext == "go";

        match lod {
            LodLevel::SignatureOnly => {
                let sig = symbol.signature.trim();
                if !is_python
                    && !is_go
                    && (symbol.kind == SymbolKind::Function || symbol.kind == SymbolKind::Method)
                {
                    if !sig.ends_with(';') {
                        format!("{};", sig)
                    } else {
                        sig.to_string()
                    }
                } else {
                    sig.to_string()
                }
            }
            LodLevel::SignatureAndDoc => {
                let sig = Self::render_symbol(symbol, LodLevel::SignatureOnly, file_source);
                if let Some(doc) = &symbol.docstring {
                    let comment_prefix = if is_python { "#" } else { "///" };
                    let doc_formatted: String = doc
                        .lines()
                        .map(|l| format!("{} {}", comment_prefix, l))
                        .collect::<Vec<_>>()
                        .join("\n");
                    format!("{}\n{}", doc_formatted, sig)
                } else {
                    sig
                }
            }
            LodLevel::SlicedBody => {
                if let Some(src) = file_source {
                    if symbol.span.end_byte <= src.len()
                        && symbol.span.start_byte < symbol.span.end_byte
                    {
                        let full = src[symbol.span.start_byte..symbol.span.end_byte].trim();
                        let line_count = full.lines().count();
                        if line_count <= 5
                            || (symbol.kind != SymbolKind::Function
                                && symbol.kind != SymbolKind::Method)
                        {
                            return full.to_string();
                        }
                        if is_python {
                            let sig = symbol.signature.trim_end_matches(':').trim();
                            return format!(
                                "{}:\n    # ... [implementation body sliced for budget] ...\n    pass",
                                sig
                            );
                        } else {
                            let sig = symbol.signature.trim_end_matches(';').trim();
                            return format!(
                                "{} {{\n    // ... [implementation body sliced for budget] ...\n}}",
                                sig
                            );
                        }
                    }
                }
                Self::render_symbol(symbol, LodLevel::SignatureAndDoc, file_source)
            }
            LodLevel::FullBody => {
                if let Some(src) = file_source {
                    if symbol.span.end_byte <= src.len()
                        && symbol.span.start_byte < symbol.span.end_byte
                    {
                        return src[symbol.span.start_byte..symbol.span.end_byte]
                            .trim()
                            .to_string();
                    }
                }
                Self::render_symbol(symbol, LodLevel::SignatureAndDoc, file_source)
            }
        }
    }

    /// Dynamically allocates Level-of-Detail (LOD) to selected symbols based on seed proximity,
    /// PPR score priority, and the available token budget.
    pub fn assign_lod(
        symbols: &[SymbolNode],
        ppr_scores: &HashMap<SymbolId, f32>,
        seed_ids: &[SymbolId],
        budget: usize,
        file_sources: &HashMap<PathBuf, String>,
    ) -> HashMap<SymbolId, LodLevel> {
        let mut lod_map: HashMap<SymbolId, LodLevel> = HashMap::with_capacity(symbols.len());
        let seed_set: std::collections::HashSet<SymbolId> = seed_ids.iter().copied().collect();

        // Pass 1: Initialize all symbols at baseline SignatureOnly
        for sym in symbols {
            lod_map.insert(sym.id, LodLevel::SignatureOnly);
        }

        // Calculate current baseline token consumption
        let mut current_tokens: usize = symbols
            .iter()
            .map(|s| {
                let src = file_sources.get(&s.file_path).map(|f| f.as_str());
                let rendered = Self::render_symbol(s, LodLevel::SignatureOnly, src);
                estimate_tokens(&rendered)
            })
            .sum();

        // If baseline already exceeds budget, keep at SignatureOnly
        if current_tokens >= budget {
            return lod_map;
        }

        // Pass 2: Upgrade root seeds to FullBody or SlicedBody
        for sym in symbols {
            if seed_set.contains(&sym.id) {
                let src = file_sources.get(&sym.file_path).map(|f| f.as_str());
                let full_tokens =
                    estimate_tokens(&Self::render_symbol(sym, LodLevel::FullBody, src));
                let sig_tokens =
                    estimate_tokens(&Self::render_symbol(sym, LodLevel::SignatureOnly, src));
                let extra_tokens = full_tokens.saturating_sub(sig_tokens);

                if current_tokens + extra_tokens <= budget {
                    lod_map.insert(sym.id, LodLevel::FullBody);
                    current_tokens += extra_tokens;
                } else {
                    let sliced_tokens =
                        estimate_tokens(&Self::render_symbol(sym, LodLevel::SlicedBody, src));
                    let extra_sliced = sliced_tokens.saturating_sub(sig_tokens);
                    if current_tokens + extra_sliced <= budget {
                        lod_map.insert(sym.id, LodLevel::SlicedBody);
                        current_tokens += extra_sliced;
                    }
                }
            }
        }

        // Pass 3: Greedily upgrade non-seed symbols by descending PPR score to SignatureAndDoc
        let mut non_seeds: Vec<&SymbolNode> = symbols
            .iter()
            .filter(|s| !seed_set.contains(&s.id))
            .collect();
        non_seeds.sort_by(|a, b| {
            let score_a = ppr_scores.get(&a.id).copied().unwrap_or(0.0);
            let score_b = ppr_scores.get(&b.id).copied().unwrap_or(0.0);
            score_b
                .partial_cmp(&score_a)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        for sym in non_seeds {
            if current_tokens >= budget {
                break;
            }
            let src = file_sources.get(&sym.file_path).map(|f| f.as_str());
            let doc_tokens =
                estimate_tokens(&Self::render_symbol(sym, LodLevel::SignatureAndDoc, src));
            let sig_tokens =
                estimate_tokens(&Self::render_symbol(sym, LodLevel::SignatureOnly, src));
            let extra = doc_tokens.saturating_sub(sig_tokens);

            if current_tokens + extra <= budget {
                lod_map.insert(sym.id, LodLevel::SignatureAndDoc);
                current_tokens += extra;
            }
        }

        lod_map
    }

    /// Renders selected symbols into structured Markdown with file paths, line numbers,
    /// and container-scoped nesting (struct/class/trait impl blocks).
    pub fn format_markdown(
        symbols: &[SymbolNode],
        lod_map: &HashMap<SymbolId, LodLevel>,
        file_sources: &HashMap<PathBuf, String>,
    ) -> String {
        if symbols.is_empty() {
            return String::new();
        }

        let mut by_file: HashMap<&PathBuf, Vec<&SymbolNode>> = HashMap::new();
        for sym in symbols {
            by_file.entry(&sym.file_path).or_default().push(sym);
        }

        let mut file_paths: Vec<&PathBuf> = by_file.keys().copied().collect();
        file_paths.sort();

        let mut output = String::new();

        for path in file_paths {
            let file_syms = &by_file[path];
            let file_src = file_sources.get(path).map(|s| s.as_str());
            let lang_tag = Self::language_tag_for_path(path);
            let comment_prefix = if lang_tag == "python" { "#" } else { "//" };

            output.push_str(&format!("### File: `{}`\n", path.display()));
            output.push_str(&format!("```{}\n", lang_tag));

            if lang_tag == "go" {
                let mut sorted_syms = file_syms.clone();
                sorted_syms.sort_by_key(|s| s.span.start_row);

                for (idx, sym) in sorted_syms.iter().enumerate() {
                    let lod = lod_map
                        .get(&sym.id)
                        .copied()
                        .unwrap_or(LodLevel::SignatureOnly);
                    let rendered = Self::render_symbol(sym, lod, file_src);

                    let start_line = sym.span.start_row + 1;
                    let end_line = sym.span.end_row + 1;

                    if idx > 0 {
                        output.push('\n');
                    }
                    output.push_str(&format!(
                        "{} Lines {}-{}\n",
                        comment_prefix, start_line, end_line
                    ));
                    output.push_str(&rendered);
                    output.push('\n');
                }
                output.push_str("```\n\n");
                continue;
            }

            // Group symbols by container: key = (container_name, trait_name)
            let mut container_methods: HashMap<(String, Option<String>), Vec<&SymbolNode>> =
                HashMap::new();
            let mut standalone_syms: Vec<&SymbolNode> = Vec::new();

            for &sym in file_syms {
                if let Some(c_name) = &sym.container_name {
                    container_methods
                        .entry((c_name.clone(), sym.trait_name.clone()))
                        .or_default()
                        .push(sym);
                } else {
                    standalone_syms.push(sym);
                }
            }

            // In Python, check if a class struct symbol is present to serve as the container header
            let mut python_class_syms: HashMap<String, &SymbolNode> = HashMap::new();
            if lang_tag == "python" {
                standalone_syms.retain(|sym| {
                    if sym.kind == SymbolKind::Struct
                        && container_methods.contains_key(&(sym.name.clone(), None))
                    {
                        let lod = lod_map
                            .get(&sym.id)
                            .copied()
                            .unwrap_or(LodLevel::SignatureOnly);
                        if lod != LodLevel::FullBody {
                            python_class_syms.insert(sym.name.clone(), sym);
                            return false;
                        }
                    }
                    true
                });
            }

            struct RenderItem {
                min_row: usize,
                content: String,
            }

            let mut items: Vec<RenderItem> = Vec::new();

            // 1. Standalone symbols
            for sym in standalone_syms {
                let lod = lod_map
                    .get(&sym.id)
                    .copied()
                    .unwrap_or(LodLevel::SignatureOnly);
                let rendered = Self::render_symbol(sym, lod, file_src);
                let start_line = sym.span.start_row + 1;
                let end_line = sym.span.end_row + 1;
                let content = format!(
                    "{} Lines {}-{}\n{}",
                    comment_prefix, start_line, end_line, rendered
                );
                items.push(RenderItem {
                    min_row: sym.span.start_row,
                    content,
                });
            }

            // 2. Container groups
            for ((container_name, trait_name), mut methods) in container_methods {
                methods.sort_by_key(|s| s.span.start_row);
                let min_row = if let Some(class_sym) = python_class_syms.get(&container_name) {
                    class_sym.span.start_row
                } else {
                    methods.first().map(|m| m.span.start_row).unwrap_or(0)
                };

                let mut rendered_methods = Vec::new();
                for m in &methods {
                    let lod = lod_map
                        .get(&m.id)
                        .copied()
                        .unwrap_or(LodLevel::SignatureOnly);
                    let rendered = Self::render_symbol(m, lod, file_src);
                    let start_line = m.span.start_row + 1;
                    let end_line = m.span.end_row + 1;
                    let method_block = format!(
                        "{} Lines {}-{}\n{}",
                        comment_prefix, start_line, end_line, rendered
                    );
                    let indented = Self::indent_lines(&method_block, "    ");
                    rendered_methods.push(indented);
                }
                let methods_body = rendered_methods.join("\n\n");

                let content = if lang_tag == "python" {
                    if let Some(class_sym) = python_class_syms.get(&container_name) {
                        let lod = lod_map
                            .get(&class_sym.id)
                            .copied()
                            .unwrap_or(LodLevel::SignatureOnly);
                        let rendered_class = Self::render_symbol(class_sym, lod, file_src);
                        let start_line = class_sym.span.start_row + 1;
                        let end_line = class_sym.span.end_row + 1;
                        format!(
                            "{} Lines {}-{}\n{}\n{}",
                            comment_prefix, start_line, end_line, rendered_class, methods_body
                        )
                    } else {
                        format!("class {}:\n{}", container_name, methods_body)
                    }
                } else {
                    let (header, footer) = Self::container_header_and_footer(
                        lang_tag,
                        &container_name,
                        trait_name.as_deref(),
                    );
                    if let Some(footer) = footer {
                        format!("{}\n{}\n{}", header, methods_body, footer)
                    } else {
                        format!("{}\n{}", header, methods_body)
                    }
                };

                items.push(RenderItem { min_row, content });
            }

            // Sort items canonically by source line appearance
            items.sort_by_key(|item| item.min_row);

            for (idx, item) in items.iter().enumerate() {
                if idx > 0 {
                    output.push('\n');
                }
                output.push_str(&item.content);
                output.push('\n');
            }

            output.push_str("```\n\n");
        }

        output.trim_end().to_string()
    }

    /// Indents each non-empty line of a string with the given indentation prefix.
    pub fn indent_lines(text: &str, indent: &str) -> String {
        text.lines()
            .map(|line| {
                if line.trim().is_empty() {
                    String::new()
                } else {
                    format!("{}{}", indent, line)
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Synthesizes the container opening header and closing footer for a given language.
    pub fn container_header_and_footer(
        lang_tag: &str,
        container_name: &str,
        trait_name: Option<&str>,
    ) -> (String, Option<String>) {
        match lang_tag {
            "rust" => {
                if let Some(tr) = trait_name {
                    (
                        format!("impl {} for {} {{", tr, container_name),
                        Some("}".to_string()),
                    )
                } else {
                    (format!("impl {} {{", container_name), Some("}".to_string()))
                }
            }
            "python" => (format!("class {}:", container_name), None),
            "typescript" | "tsx" | "javascript" => (
                format!("class {} {{", container_name),
                Some("}".to_string()),
            ),
            _ => (format!("{} {{", container_name), Some("}".to_string())),
        }
    }

    /// Infers the syntax highlighting language tag from a file path.
    pub fn language_tag_for_path(path: &std::path::Path) -> &'static str {
        match path.extension().and_then(|e| e.to_str()) {
            Some("rs") => "rust",
            Some("py") => "python",
            Some("ts") => "typescript",
            Some("tsx") => "tsx",
            Some("js" | "jsx" | "mjs" | "cjs") => "javascript",
            Some("go") => "go",
            _ => "text",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::symbol::{SymbolKind, TextSpan};

    fn make_test_symbol(
        id: u32,
        name: &str,
        kind: SymbolKind,
        span: TextSpan,
        doc: Option<&str>,
    ) -> SymbolNode {
        let sig = match kind {
            SymbolKind::Function | SymbolKind::Method => format!("pub fn {}()", name),
            SymbolKind::Struct => format!("pub struct {}", name),
            _ => format!("pub fn {}()", name),
        };
        SymbolNode {
            id: SymbolId(id),
            name: name.to_string(),
            kind,
            file_path: PathBuf::from("src/lib.rs"),
            span,
            signature: sig,
            docstring: doc.map(|s| s.to_string()),
            token_cost: 10,
            ast_hash: [0u8; 32],
            container_name: None,
            trait_name: None,
        }
    }

    #[test]
    fn test_render_symbol_all_lods() {
        let src = "/// Do something\npub fn calculate() {\n    let a = 1;\n    let b = 2;\n    let c = 3;\n    let d = 4;\n    let e = 5;\n    a + b + c + d + e\n}";
        let sym = make_test_symbol(
            1,
            "calculate",
            SymbolKind::Function,
            TextSpan::new(17, src.len(), 1, 9),
            Some("Do something"),
        );

        // LOD 0: SignatureOnly
        let sig_only = ContextFormatter::render_symbol(&sym, LodLevel::SignatureOnly, Some(src));
        assert_eq!(sig_only, "pub fn calculate();");

        // LOD 1: SignatureAndDoc
        let sig_doc = ContextFormatter::render_symbol(&sym, LodLevel::SignatureAndDoc, Some(src));
        assert!(sig_doc.contains("/// Do something"));
        assert!(sig_doc.contains("pub fn calculate();"));

        // LOD 2: SlicedBody
        let sliced = ContextFormatter::render_symbol(&sym, LodLevel::SlicedBody, Some(src));
        assert!(sliced.contains("// ... [implementation body sliced for budget] ..."));

        // LOD 3: FullBody
        let full = ContextFormatter::render_symbol(&sym, LodLevel::FullBody, Some(src));
        assert!(full.contains("let a = 1;"));
        assert!(full.contains("a + b + c + d + e"));
    }

    #[test]
    fn test_assign_lod_prioritization() {
        let sym1 = make_test_symbol(
            1,
            "seed_fn",
            SymbolKind::Function,
            TextSpan::new(0, 50, 0, 3),
            Some("Seed doc"),
        );
        let sym2 = make_test_symbol(
            2,
            "neighbor_fn",
            SymbolKind::Function,
            TextSpan::new(51, 100, 4, 7),
            Some("Neighbor doc"),
        );

        let symbols = vec![sym1, sym2];
        let mut ppr_scores = HashMap::new();
        ppr_scores.insert(SymbolId(1), 0.8);
        ppr_scores.insert(SymbolId(2), 0.2);

        let mut sources = HashMap::new();
        sources.insert(
            PathBuf::from("src/lib.rs"),
            "pub fn seed_fn() {\n}\npub fn neighbor_fn() {\n}".to_string(),
        );

        // Generous budget -> seed gets FullBody, neighbor gets SignatureAndDoc
        let lod_map =
            ContextFormatter::assign_lod(&symbols, &ppr_scores, &[SymbolId(1)], 1000, &sources);
        assert_eq!(lod_map.get(&SymbolId(1)), Some(&LodLevel::FullBody));
        assert_eq!(lod_map.get(&SymbolId(2)), Some(&LodLevel::SignatureAndDoc));

        // Very tight budget -> both stay at SignatureOnly
        let tight_map =
            ContextFormatter::assign_lod(&symbols, &ppr_scores, &[SymbolId(1)], 5, &sources);
        assert_eq!(tight_map.get(&SymbolId(1)), Some(&LodLevel::SignatureOnly));
        assert_eq!(tight_map.get(&SymbolId(2)), Some(&LodLevel::SignatureOnly));
    }

    #[test]
    fn test_format_markdown_grouped_by_file() {
        let sym1 = make_test_symbol(
            1,
            "foo",
            SymbolKind::Function,
            TextSpan::new(0, 20, 0, 2),
            None,
        );
        let sym2 = make_test_symbol(
            2,
            "bar",
            SymbolKind::Function,
            TextSpan::new(21, 40, 3, 5),
            None,
        );

        let symbols = vec![sym1, sym2];
        let mut lod_map = HashMap::new();
        lod_map.insert(SymbolId(1), LodLevel::SignatureOnly);
        lod_map.insert(SymbolId(2), LodLevel::SignatureOnly);

        let sources = HashMap::new();
        let md = ContextFormatter::format_markdown(&symbols, &lod_map, &sources);

        assert!(md.contains("### File: `src/lib.rs`"));
        assert!(md.contains("```rust"));
        assert!(md.contains("// Lines 1-3"));
        assert!(md.contains("pub fn foo();"));
        assert!(md.contains("// Lines 4-6"));
        assert!(md.contains("pub fn bar();"));
        assert!(md.contains("```"));
    }

    #[allow(clippy::too_many_arguments)]
    fn make_test_symbol_full(
        id: u32,
        name: &str,
        kind: SymbolKind,
        file_path: &str,
        span: TextSpan,
        sig: &str,
        doc: Option<&str>,
        container: Option<&str>,
        trait_name: Option<&str>,
    ) -> SymbolNode {
        SymbolNode {
            id: SymbolId(id),
            name: name.to_string(),
            kind,
            file_path: PathBuf::from(file_path),
            span,
            signature: sig.to_string(),
            docstring: doc.map(|s| s.to_string()),
            token_cost: 10,
            ast_hash: [0u8; 32],
            container_name: container.map(|s| s.to_string()),
            trait_name: trait_name.map(|s| s.to_string()),
        }
    }

    #[test]
    fn test_format_markdown_rust_container_scoping() {
        let struct_sym = make_test_symbol_full(
            1,
            "DirEntry",
            SymbolKind::Struct,
            "src/entry.rs",
            TextSpan::new(0, 30, 0, 2),
            "pub struct DirEntry { path: PathBuf }",
            None,
            None,
            None,
        );
        let method_new = make_test_symbol_full(
            2,
            "new",
            SymbolKind::Method,
            "src/entry.rs",
            TextSpan::new(40, 70, 4, 6),
            "pub fn new(path: PathBuf) -> Self",
            None,
            Some("DirEntry"),
            None,
        );
        let method_clone = make_test_symbol_full(
            3,
            "clone",
            SymbolKind::Method,
            "src/entry.rs",
            TextSpan::new(80, 110, 8, 10),
            "fn clone(&self) -> Self",
            None,
            Some("DirEntry"),
            Some("Clone"),
        );
        let fn_helper = make_test_symbol_full(
            4,
            "helper",
            SymbolKind::Function,
            "src/entry.rs",
            TextSpan::new(120, 140, 12, 14),
            "pub fn helper()",
            None,
            None,
            None,
        );

        let symbols = vec![struct_sym, method_new, method_clone, fn_helper];
        let mut lod_map = HashMap::new();
        for s in &symbols {
            lod_map.insert(s.id, LodLevel::SignatureOnly);
        }

        let sources = HashMap::new();
        let md = ContextFormatter::format_markdown(&symbols, &lod_map, &sources);

        assert!(md.contains("### File: `src/entry.rs`"));
        assert!(md.contains("```rust"));

        // Standalone struct
        assert!(md.contains("// Lines 1-3\npub struct DirEntry { path: PathBuf }"));

        // Inherent impl block
        assert!(md.contains(
            "impl DirEntry {\n    // Lines 5-7\n    pub fn new(path: PathBuf) -> Self;\n}"
        ));

        // Trait impl block
        assert!(md.contains(
            "impl Clone for DirEntry {\n    // Lines 9-11\n    fn clone(&self) -> Self;\n}"
        ));

        // Standalone function
        assert!(md.contains("// Lines 13-15\npub fn helper();"));
    }

    #[test]
    fn test_format_markdown_python_container_scoping() {
        let class_sym = make_test_symbol_full(
            1,
            "UserService",
            SymbolKind::Struct,
            "service.py",
            TextSpan::new(0, 25, 0, 2),
            "class UserService:",
            Some("Manages user accounts."),
            None,
            None,
        );
        let init_method = make_test_symbol_full(
            2,
            "__init__",
            SymbolKind::Method,
            "service.py",
            TextSpan::new(30, 60, 3, 5),
            "def __init__(self, db: Database):",
            None,
            Some("UserService"),
            None,
        );
        let get_user = make_test_symbol_full(
            3,
            "get_user",
            SymbolKind::Method,
            "service.py",
            TextSpan::new(70, 100, 6, 8),
            "def get_user(self, user_id: int) -> User:",
            None,
            Some("UserService"),
            None,
        );

        let symbols = vec![class_sym, init_method, get_user];
        let mut lod_map = HashMap::new();
        lod_map.insert(SymbolId(1), LodLevel::SignatureAndDoc);
        lod_map.insert(SymbolId(2), LodLevel::SignatureOnly);
        lod_map.insert(SymbolId(3), LodLevel::SignatureOnly);

        let sources = HashMap::new();
        let md = ContextFormatter::format_markdown(&symbols, &lod_map, &sources);

        assert!(md.contains("### File: `service.py`"));
        assert!(md.contains("```python"));

        // Class header with docstring, followed by indented methods
        assert!(md.contains("# Lines 1-3\n# Manages user accounts.\nclass UserService:\n    # Lines 4-6\n    def __init__(self, db: Database):\n\n    # Lines 7-9\n    def get_user(self, user_id: int) -> User:"));
    }

    #[test]
    fn test_format_markdown_typescript_container_scoping() {
        let method_sym = make_test_symbol_full(
            1,
            "getUser",
            SymbolKind::Method,
            "src/service.ts",
            TextSpan::new(20, 50, 2, 4),
            "public getUser(id: number): User",
            None,
            Some("UserService"),
            None,
        );

        let symbols = vec![method_sym];
        let mut lod_map = HashMap::new();
        lod_map.insert(SymbolId(1), LodLevel::SignatureOnly);

        let sources = HashMap::new();
        let md = ContextFormatter::format_markdown(&symbols, &lod_map, &sources);

        assert!(md.contains("### File: `src/service.ts`"));
        assert!(md.contains("```typescript"));
        assert!(md.contains(
            "class UserService {\n    // Lines 3-5\n    public getUser(id: number): User;\n}"
        ));
    }

    #[test]
    fn test_format_markdown_go_receiver_rendering() {
        let method_sym = make_test_symbol_full(
            1,
            "Start",
            SymbolKind::Method,
            "server.go",
            TextSpan::new(20, 50, 2, 4),
            "func (s *Server) Start() error",
            None,
            Some("Server"),
            None,
        );

        let symbols = vec![method_sym];
        let mut lod_map = HashMap::new();
        lod_map.insert(SymbolId(1), LodLevel::SignatureOnly);

        let sources = HashMap::new();
        let md = ContextFormatter::format_markdown(&symbols, &lod_map, &sources);

        assert!(md.contains("### File: `server.go`"));
        assert!(md.contains("```go"));
        // Top-level receiver method without synthetic class or impl
        assert!(md.contains("// Lines 3-5\nfunc (s *Server) Start() error"));
        assert!(!md.contains("class Server"));
        assert!(!md.contains("impl Server"));
    }
}
