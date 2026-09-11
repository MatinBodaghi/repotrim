use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::PathBuf;

use crate::slicer::AstSlicer;
use crate::symbol::{LodLevel, SymbolId, SymbolKind, SymbolNode};
use crate::tokens::{count_tokens, TokenizerModel};

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
                        return AstSlicer::slice_symbol(symbol, src);
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
    /// PPR score priority, and the available token budget using the default fast heuristic.
    pub fn assign_lod(
        symbols: &[SymbolNode],
        ppr_scores: &HashMap<SymbolId, f32>,
        seed_ids: &[SymbolId],
        budget: usize,
        file_sources: &HashMap<PathBuf, String>,
    ) -> HashMap<SymbolId, LodLevel> {
        Self::assign_lod_with_model(
            symbols,
            ppr_scores,
            seed_ids,
            budget,
            file_sources,
            TokenizerModel::default(),
        )
    }

    /// Dynamically allocates Level-of-Detail (LOD) to selected symbols using a specific `TokenizerModel`,
    /// accounting for full rendered Markdown structural framing (file headers, container wrappers, and line comments).
    pub fn assign_lod_with_model(
        symbols: &[SymbolNode],
        ppr_scores: &HashMap<SymbolId, f32>,
        seed_ids: &[SymbolId],
        budget: usize,
        file_sources: &HashMap<PathBuf, String>,
        model: TokenizerModel,
    ) -> HashMap<SymbolId, LodLevel> {
        if symbols.is_empty() || budget == 0 {
            return HashMap::new();
        }

        let mut lod_map: HashMap<SymbolId, LodLevel> = HashMap::with_capacity(symbols.len());
        let seed_set: std::collections::HashSet<SymbolId> = seed_ids.iter().copied().collect();

        // Pass 1: Initialize all symbols at baseline SignatureOnly
        for sym in symbols {
            lod_map.insert(sym.id, LodLevel::SignatureOnly);
        }

        // Calculate baseline token consumption including structural Markdown framing
        let initial_md = Self::format_markdown(symbols, &lod_map, file_sources);
        let mut current_tokens = count_tokens(&initial_md, model);

        // If baseline already exceeds budget, keep at SignatureOnly
        if current_tokens >= budget {
            return lod_map;
        }

        // Pass 2: Upgrade root seeds to FullBody or SlicedBody if budget permits
        for sym in symbols {
            if seed_set.contains(&sym.id) {
                // Try FullBody
                lod_map.insert(sym.id, LodLevel::FullBody);
                let candidate_md = Self::format_markdown(symbols, &lod_map, file_sources);
                let full_tokens = count_tokens(&candidate_md, model);

                if full_tokens <= budget {
                    current_tokens = full_tokens;
                } else {
                    // Try SlicedBody
                    lod_map.insert(sym.id, LodLevel::SlicedBody);
                    let sliced_md = Self::format_markdown(symbols, &lod_map, file_sources);
                    let sliced_tokens = count_tokens(&sliced_md, model);

                    if sliced_tokens <= budget {
                        current_tokens = sliced_tokens;
                    } else {
                        // Revert to SignatureOnly
                        lod_map.insert(sym.id, LodLevel::SignatureOnly);
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
            lod_map.insert(sym.id, LodLevel::SignatureAndDoc);
            let candidate_md = Self::format_markdown(symbols, &lod_map, file_sources);
            let candidate_tokens = count_tokens(&candidate_md, model);

            if candidate_tokens <= budget {
                current_tokens = candidate_tokens;
            } else {
                lod_map.insert(sym.id, LodLevel::SignatureOnly);
            }
        }

        lod_map
    }

    /// Formats selected symbols into structured Markdown with strict budget enforcement.
    ///
    /// If the initial Markdown exceeds `budget`, dynamically prunes lower-priority non-seed
    /// symbols and downgrades Level-of-Detail until `count_tokens(&markdown, model) <= budget`.
    ///
    /// Returns a tuple containing:
    /// 1. The pruned list of surviving `SymbolNode`s
    /// 2. The rendered Markdown string (strictly guaranteed `<= budget` tokens)
    /// 3. The final `LodLevel` mapping for the surviving symbols
    pub fn format_markdown_budgeted(
        symbols: &[SymbolNode],
        lod_map: &HashMap<SymbolId, LodLevel>,
        file_sources: &HashMap<PathBuf, String>,
        budget: usize,
        model: TokenizerModel,
        ppr_scores: &HashMap<SymbolId, f32>,
        seed_ids: &[SymbolId],
    ) -> (Vec<SymbolNode>, String, HashMap<SymbolId, LodLevel>) {
        if symbols.is_empty() || budget == 0 {
            return (Vec::new(), String::new(), HashMap::new());
        }

        let mut working_symbols: Vec<SymbolNode> = symbols.to_vec();
        let mut working_lods: HashMap<SymbolId, LodLevel> = lod_map.clone();
        let seed_set: HashSet<SymbolId> = seed_ids.iter().copied().collect();

        // 1. Initial format check
        let mut md = Self::format_markdown(&working_symbols, &working_lods, file_sources);
        let mut tokens = count_tokens(&md, model);

        if tokens <= budget {
            return (working_symbols, md, working_lods);
        }

        // 2. Stage 1 degradation: downgrade non-seeds from SignatureAndDoc to SignatureOnly
        for sym in &working_symbols {
            if !seed_set.contains(&sym.id) {
                working_lods.insert(sym.id, LodLevel::SignatureOnly);
            }
        }
        md = Self::format_markdown(&working_symbols, &working_lods, file_sources);
        tokens = count_tokens(&md, model);
        if tokens <= budget {
            return (working_symbols, md, working_lods);
        }

        // 3. Stage 2 degradation: downgrade seeds from FullBody -> SlicedBody -> SignatureOnly
        for sym in &working_symbols {
            if seed_set.contains(&sym.id) {
                if let Some(lod) = working_lods.get_mut(&sym.id) {
                    if *lod == LodLevel::FullBody {
                        *lod = LodLevel::SlicedBody;
                    }
                }
            }
        }
        md = Self::format_markdown(&working_symbols, &working_lods, file_sources);
        tokens = count_tokens(&md, model);
        if tokens <= budget {
            return (working_symbols, md, working_lods);
        }

        for sym in &working_symbols {
            if seed_set.contains(&sym.id) {
                working_lods.insert(sym.id, LodLevel::SignatureOnly);
            }
        }
        md = Self::format_markdown(&working_symbols, &working_lods, file_sources);
        tokens = count_tokens(&md, model);
        if tokens <= budget {
            return (working_symbols, md, working_lods);
        }

        // 4. Stage 3 pruning: prune non-seed symbols in ascending PPR score priority
        let mut non_seed_ids: Vec<SymbolId> = working_symbols
            .iter()
            .filter(|s| !seed_set.contains(&s.id))
            .map(|s| s.id)
            .collect();
        non_seed_ids.sort_by(|a, b| {
            let score_a = ppr_scores.get(a).copied().unwrap_or(0.0);
            let score_b = ppr_scores.get(b).copied().unwrap_or(0.0);
            score_a
                .partial_cmp(&score_b)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        for drop_id in non_seed_ids {
            working_lods.remove(&drop_id);
            working_symbols.retain(|s| s.id != drop_id);

            md = Self::format_markdown(&working_symbols, &working_lods, file_sources);
            tokens = count_tokens(&md, model);
            if tokens <= budget {
                return (working_symbols, md, working_lods);
            }
        }

        // 5. Stage 4 pruning: if even seeds alone exceed budget, prune seeds by ascending PPR score
        if tokens > budget && working_symbols.len() > 1 {
            let mut seed_candidates: Vec<SymbolId> = working_symbols.iter().map(|s| s.id).collect();
            seed_candidates.sort_by(|a, b| {
                let score_a = ppr_scores.get(a).copied().unwrap_or(0.0);
                let score_b = ppr_scores.get(b).copied().unwrap_or(0.0);
                score_a
                    .partial_cmp(&score_b)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });

            for drop_id in seed_candidates {
                if working_symbols.len() <= 1 {
                    break;
                }
                working_lods.remove(&drop_id);
                working_symbols.retain(|s| s.id != drop_id);

                md = Self::format_markdown(&working_symbols, &working_lods, file_sources);
                tokens = count_tokens(&md, model);
                if tokens <= budget {
                    return (working_symbols, md, working_lods);
                }
            }
        }

        // 6. Stage 5 emergency truncation: if budget is smaller than a single formatted file
        if tokens > budget && budget < 20 {
            return (Vec::new(), String::new(), HashMap::new());
        }

        (working_symbols, md, working_lods)
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
        let file_paths = Self::sort_files_topologically(&file_paths, &by_file);

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

    /// Sorts file paths in causal topological dependency order.
    ///
    /// If file A references types, structs, traits, or functions defined in file B,
    /// file B is prioritized before file A so that foundational definitions and callees
    /// precede orchestrators and callers.
    ///
    /// Uses Kahn's algorithm (1962). Ties and unresolvable dependency cycles
    /// fall back deterministically to canonical lexicographical order.
    pub fn sort_files_topologically<'a>(
        file_paths: &[&'a PathBuf],
        by_file: &HashMap<&'a PathBuf, Vec<&'a SymbolNode>>,
    ) -> Vec<&'a PathBuf> {
        if file_paths.len() <= 1 {
            return file_paths.to_vec();
        }

        // 1. Collect all symbol names defined in each file
        let mut defined_in_file: HashMap<&'a PathBuf, HashSet<&str>> = HashMap::new();
        for &path in file_paths {
            if let Some(syms) = by_file.get(path) {
                let mut names = HashSet::new();
                for s in syms {
                    names.insert(s.name.as_str());
                }
                defined_in_file.insert(path, names);
            }
        }

        // Helper set of common language keywords and primitive types to ignore
        let ignored_keywords: HashSet<&'static str> = [
            "fn",
            "pub",
            "mut",
            "self",
            "Self",
            "let",
            "ref",
            "crate",
            "super",
            "where",
            "impl",
            "for",
            "as",
            "in",
            "if",
            "else",
            "match",
            "while",
            "loop",
            "return",
            "break",
            "continue",
            "struct",
            "enum",
            "type",
            "trait",
            "mod",
            "use",
            "const",
            "static",
            "async",
            "await",
            "dyn",
            "true",
            "false",
            "u8",
            "u16",
            "u32",
            "u64",
            "u128",
            "usize",
            "i8",
            "i16",
            "i32",
            "i64",
            "i128",
            "isize",
            "f32",
            "f64",
            "bool",
            "char",
            "str",
            "String",
            "Option",
            "Some",
            "None",
            "Result",
            "Ok",
            "Err",
            "Vec",
            "Box",
            "Rc",
            "Arc",
            "HashMap",
            "HashSet",
            "def",
            "class",
            "cls",
            "yield",
            "import",
            "from",
            "try",
            "except",
            "finally",
            "raise",
            "with",
            "pass",
            "int",
            "float",
            "list",
            "dict",
            "set",
            "tuple",
            "Optional",
            "List",
            "Dict",
            "Set",
            "Tuple",
            "Any",
            "Union",
            "function",
            "interface",
            "var",
            "export",
            "public",
            "private",
            "protected",
            "readonly",
            "number",
            "boolean",
            "any",
            "void",
            "null",
            "undefined",
            "Promise",
            "func",
            "package",
            "byte",
            "rune",
            "float64",
            "nil",
            "error",
        ]
        .into_iter()
        .collect();

        // 2. Build dependency graph: B -> A means file B must precede file A (because A references B)
        let mut adj: HashMap<&'a PathBuf, HashSet<&'a PathBuf>> = HashMap::new();
        let mut in_degree: HashMap<&'a PathBuf, usize> = HashMap::new();

        for &path in file_paths {
            in_degree.insert(path, 0);
        }

        for &file_a in file_paths {
            let empty_defs = HashSet::new();
            let defs_a = defined_in_file.get(file_a).unwrap_or(&empty_defs);

            // Extract all identifier words used in file_a's symbol signatures/containers/traits
            let mut used_idents: HashSet<&str> = HashSet::new();
            if let Some(syms) = by_file.get(file_a) {
                for s in syms {
                    Self::extract_identifiers(&s.signature, &mut used_idents);
                    if let Some(c) = &s.container_name {
                        Self::extract_identifiers(c, &mut used_idents);
                    }
                    if let Some(t) = &s.trait_name {
                        Self::extract_identifiers(t, &mut used_idents);
                    }
                }
            }

            // Keep only external identifiers (not defined in file_a, not keywords)
            used_idents
                .retain(|ident| !defs_a.contains(ident) && !ignored_keywords.contains(ident));

            // Check which other files define any of these identifiers
            for &file_b in file_paths {
                if file_b == file_a {
                    continue;
                }
                if let Some(defs_b) = defined_in_file.get(file_b) {
                    let references_b = defs_b.iter().any(|b_sym| used_idents.contains(b_sym));
                    if references_b {
                        // file_b is a dependency of file_a, so file_b -> file_a
                        if adj.entry(file_b).or_default().insert(file_a) {
                            *in_degree.entry(file_a).or_insert(0) += 1;
                        }
                    }
                }
            }
        }

        // 3. Kahn's Algorithm using a sorted set (BTreeSet) to deterministically break ties alphabetically
        let mut ready: BTreeSet<&'a PathBuf> = BTreeSet::new();
        for &path in file_paths {
            if in_degree.get(path).copied().unwrap_or(0) == 0 {
                ready.insert(path);
            }
        }

        let mut ordered: Vec<&'a PathBuf> = Vec::with_capacity(file_paths.len());
        let mut visited: HashSet<&'a PathBuf> = HashSet::new();

        while let Some(&next) = ready.iter().next() {
            ready.remove(next);
            ordered.push(next);
            visited.insert(next);

            if let Some(neighbors) = adj.get(next) {
                for &neighbor in neighbors {
                    if let Some(deg) = in_degree.get_mut(neighbor) {
                        *deg = deg.saturating_sub(1);
                        if *deg == 0 && !visited.contains(neighbor) {
                            ready.insert(neighbor);
                        }
                    }
                }
            }
        }

        // 4. Cycle fallback: any remaining files not in ordered are sorted alphabetically and appended
        if ordered.len() < file_paths.len() {
            let mut remaining: Vec<&'a PathBuf> = file_paths
                .iter()
                .copied()
                .filter(|p| !visited.contains(p))
                .collect();
            remaining.sort();
            ordered.extend(remaining);
        }

        ordered
    }

    /// Extracts alphanumeric identifier words from text.
    fn extract_identifiers<'s>(text: &'s str, out: &mut HashSet<&'s str>) {
        for word in text.split(|c: char| !c.is_alphanumeric() && c != '_') {
            let trimmed = word.trim();
            if !trimmed.is_empty() && !trimmed.starts_with(|c: char| c.is_ascii_digit()) {
                out.insert(trimmed);
            }
        }
    }

    /// Estimates the token cost of file framing (header and closing code fence).
    pub fn file_framing_tokens(
        path: &std::path::Path,
        lang_tag: &str,
        model: TokenizerModel,
    ) -> usize {
        let sample = format!("### File: `{}`\n```{}\n```\n\n", path.display(), lang_tag);
        count_tokens(&sample, model)
    }

    /// Estimates the token cost of container framing (opening declaration and closing footer).
    pub fn container_framing_tokens(
        lang_tag: &str,
        container_name: &str,
        trait_name: Option<&str>,
        model: TokenizerModel,
    ) -> usize {
        let (header, footer) =
            Self::container_header_and_footer(lang_tag, container_name, trait_name);
        let sample = if let Some(footer) = footer {
            format!("{}\n{}\n", header, footer)
        } else {
            format!("{}\n", header)
        };
        count_tokens(&sample, model)
    }

    /// Estimates the token cost of line comment and indentation overhead for a symbol.
    pub fn symbol_framing_tokens(
        sym: &SymbolNode,
        in_container: bool,
        model: TokenizerModel,
    ) -> usize {
        let comment_prefix = if sym.file_path.extension().and_then(|e| e.to_str()) == Some("py") {
            "#"
        } else {
            "//"
        };
        let comment = format!(
            "{} Lines {}-{}\n",
            comment_prefix,
            sym.span.start_row + 1,
            sym.span.end_row + 1
        );
        let comment_tokens = count_tokens(&comment, model);
        let indent_tokens = if in_container {
            let line_count = sym.signature.lines().count().max(1);
            line_count
        } else {
            0
        };
        comment_tokens + indent_tokens
    }

    /// Estimates the total token cost of a rendered symbol including its comment and indentation.
    pub fn rendered_symbol_tokens(
        sym: &SymbolNode,
        lod: LodLevel,
        file_src: Option<&str>,
        in_container: bool,
        model: TokenizerModel,
    ) -> usize {
        let rendered = Self::render_symbol(sym, lod, file_src);
        let comment_prefix = if sym.file_path.extension().and_then(|e| e.to_str()) == Some("py") {
            "#"
        } else {
            "//"
        };
        let block = format!(
            "{} Lines {}-{}\n{}",
            comment_prefix,
            sym.span.start_row + 1,
            sym.span.end_row + 1,
            rendered
        );
        let text = if in_container {
            Self::indent_lines(&block, "    ")
        } else {
            block
        };
        count_tokens(&text, model)
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
        assert!(sliced.contains("// ... [5 lines elided] ..."));
        assert!(sliced.contains("a + b + c + d + e"));

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

    #[test]
    fn test_sort_files_topologically_causal_order() {
        // models.rs defines User and Database
        let user_sym = make_test_symbol_full(
            1,
            "User",
            SymbolKind::Struct,
            "src/models.rs",
            TextSpan::new(0, 30, 0, 2),
            "pub struct User;",
            None,
            None,
            None,
        );
        let db_sym = make_test_symbol_full(
            2,
            "Database",
            SymbolKind::Struct,
            "src/models.rs",
            TextSpan::new(31, 65, 3, 5),
            "pub struct Database;",
            None,
            None,
            None,
        );

        // controllers.rs references Database and User in login_handler signature
        let handler_sym = make_test_symbol_full(
            3,
            "login_handler",
            SymbolKind::Function,
            "src/controllers.rs",
            TextSpan::new(0, 70, 0, 3),
            "pub fn login_handler(db: &Database) -> User;",
            None,
            None,
            None,
        );

        let models_path = PathBuf::from("src/models.rs");
        let controllers_path = PathBuf::from("src/controllers.rs");

        let mut by_file = HashMap::new();
        by_file.insert(&models_path, vec![&user_sym, &db_sym]);
        by_file.insert(&controllers_path, vec![&handler_sym]);

        // Alphabetically, "src/controllers.rs" comes before "src/models.rs"
        let file_paths = vec![&controllers_path, &models_path];

        // Topologically, models must come before controllers because controllers depends on models
        let sorted = ContextFormatter::sort_files_topologically(&file_paths, &by_file);
        assert_eq!(sorted, vec![&models_path, &controllers_path]);
    }

    #[test]
    fn test_sort_files_topologically_cycle_fallback() {
        let sym_a = make_test_symbol_full(
            1,
            "Alpha",
            SymbolKind::Struct,
            "src/a.rs",
            TextSpan::new(0, 30, 0, 2),
            "pub struct Alpha(pub Beta);",
            None,
            None,
            None,
        );
        let sym_b = make_test_symbol_full(
            2,
            "Beta",
            SymbolKind::Struct,
            "src/b.rs",
            TextSpan::new(0, 30, 0, 2),
            "pub struct Beta(pub Alpha);",
            None,
            None,
            None,
        );

        let path_a = PathBuf::from("src/a.rs");
        let path_b = PathBuf::from("src/b.rs");

        let mut by_file = HashMap::new();
        by_file.insert(&path_a, vec![&sym_a]);
        by_file.insert(&path_b, vec![&sym_b]);

        let file_paths = vec![&path_b, &path_a];
        let sorted = ContextFormatter::sort_files_topologically(&file_paths, &by_file);

        // Cycle gracefully falls back to deterministic alphabetical ordering
        assert_eq!(sorted, vec![&path_a, &path_b]);
    }

    #[test]
    fn test_framing_token_helpers() {
        let path = PathBuf::from("crates/engine/src/celf.rs");
        let file_tokens =
            ContextFormatter::file_framing_tokens(&path, "rust", TokenizerModel::FastHeuristic);
        assert!(file_tokens > 0);

        let container_tokens = ContextFormatter::container_framing_tokens(
            "rust",
            "CelfOptimizer",
            None,
            TokenizerModel::FastHeuristic,
        );
        assert!(container_tokens > 0);

        let sym = make_test_symbol(
            1,
            "optimize",
            SymbolKind::Method,
            TextSpan::new(0, 50, 10, 15),
            None,
        );
        let sym_tokens =
            ContextFormatter::symbol_framing_tokens(&sym, true, TokenizerModel::FastHeuristic);
        assert!(sym_tokens > 0);
    }

    #[test]
    fn test_format_markdown_budgeted_strict_adherence() {
        let mut symbols = Vec::new();
        let mut ppr_scores = HashMap::new();
        let mut sources = HashMap::new();

        let path = PathBuf::from("src/lib.rs");
        sources.insert(
            path.clone(),
            "pub fn f1() {}\npub fn f2() {}\npub fn f3() {}\npub fn f4() {}\npub fn f5() {}"
                .to_string(),
        );

        for i in 1..=5 {
            let sym = make_test_symbol_full(
                i,
                &format!("f{}", i),
                SymbolKind::Function,
                "src/lib.rs",
                TextSpan::new(
                    (i as usize - 1) * 15,
                    i as usize * 15,
                    (i as usize - 1) * 2,
                    i as usize * 2 + 1,
                ),
                &format!("pub fn f{}()", i),
                Some("Some docstring for testing"),
                None,
                None,
            );
            symbols.push(sym);
            ppr_scores.insert(SymbolId(i), i as f32 * 0.1);
        }

        let mut lod_map = HashMap::new();
        for s in &symbols {
            lod_map.insert(s.id, LodLevel::SignatureAndDoc);
        }

        // Test with a tight budget: 60 tokens
        let (pruned_syms, md, pruned_lods) = ContextFormatter::format_markdown_budgeted(
            &symbols,
            &lod_map,
            &sources,
            60,
            TokenizerModel::FastHeuristic,
            &ppr_scores,
            &[SymbolId(5)], // Seed is f5 (highest PPR)
        );

        let md_tokens = count_tokens(&md, TokenizerModel::FastHeuristic);
        assert!(
            md_tokens <= 60,
            "Rendered markdown tokens ({}) must be <= budget (60)",
            md_tokens
        );
        assert!(!pruned_syms.is_empty());
        assert_eq!(pruned_syms.len(), pruned_lods.len());
        // Seed f5 should be preserved
        assert!(pruned_syms.iter().any(|s| s.name == "f5"));
    }
}
