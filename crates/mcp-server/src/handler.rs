use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock, RwLockWriteGuard};

use repotrim_engine::{
    ArchitectureReport, ContextSelector, DiffResolver, ImpactAnalyzer, IntentResolver,
    LoadedRepository, ModelProfile, PprSolver, RepositoryWatcher, SymbolId, SymbolKind,
};

use crate::protocol::{
    InitializeResult, JsonRpcRequest, JsonRpcResponse, ServerCapabilities, ServerInfo,
    ToolCallResult, ToolDefinition, ToolsCapability, ToolsListResult, INVALID_PARAMS,
    MCP_PROTOCOL_VERSION, METHOD_NOT_FOUND,
};

/// Handles incoming MCP requests and manages workspace repository caching with live watcher sync.
pub struct McpHandler {
    default_root: PathBuf,
    cached_repo: Option<(
        PathBuf,
        Arc<RwLock<LoadedRepository>>,
        Option<RepositoryWatcher>,
    )>,
}

impl Default for McpHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl McpHandler {
    pub fn new() -> Self {
        Self {
            default_root: PathBuf::from("."),
            cached_repo: None,
        }
    }

    pub fn with_root<P: Into<PathBuf>>(root: P) -> Self {
        Self {
            default_root: root.into(),
            cached_repo: None,
        }
    }

    /// Dispatches a JSON-RPC request to the appropriate handler method.
    pub fn handle_request(&mut self, req: JsonRpcRequest) -> Option<JsonRpcResponse> {
        // Notifications do not specify an id and do not expect a response
        let is_notification = req.id.is_none();
        let id = req.id.unwrap_or(serde_json::Value::Null);

        match req.method.as_str() {
            "initialize" => {
                let result = InitializeResult {
                    protocol_version: MCP_PROTOCOL_VERSION.to_string(),
                    capabilities: ServerCapabilities {
                        tools: ToolsCapability {
                            list_changed: Some(false),
                        },
                    },
                    server_info: ServerInfo {
                        name: "repotrim-mcp".to_string(),
                        version: env!("CARGO_PKG_VERSION").to_string(),
                    },
                };
                Some(JsonRpcResponse::success(
                    id,
                    serde_json::to_value(result).unwrap(),
                ))
            }
            "notifications/initialized" => {
                eprintln!("repotrim-mcp: Client handshake completed (notifications/initialized)");
                None
            }
            "ping" => Some(JsonRpcResponse::success(id, serde_json::json!({}))),
            "tools/list" => {
                let list = ToolsListResult {
                    tools: self.declared_tools(),
                };
                Some(JsonRpcResponse::success(
                    id,
                    serde_json::to_value(list).unwrap(),
                ))
            }
            "tools/call" => {
                let params = match req.params {
                    Some(p) => p,
                    None => {
                        return Some(JsonRpcResponse::error(
                            id,
                            INVALID_PARAMS,
                            "Missing 'params' in tools/call request",
                        ))
                    }
                };

                let tool_name = match params.get("name").and_then(|n| n.as_str()) {
                    Some(n) => n,
                    None => {
                        return Some(JsonRpcResponse::error(
                            id,
                            INVALID_PARAMS,
                            "Missing 'name' field in tools/call params",
                        ))
                    }
                };

                let arguments = params
                    .get("arguments")
                    .cloned()
                    .unwrap_or(serde_json::json!({}));
                let tool_result = self.execute_tool(tool_name, arguments);
                Some(JsonRpcResponse::success(
                    id,
                    serde_json::to_value(tool_result).unwrap(),
                ))
            }
            _ => {
                if is_notification {
                    None
                } else {
                    Some(JsonRpcResponse::error(
                        id,
                        METHOD_NOT_FOUND,
                        format!("Unknown method: '{}'", req.method),
                    ))
                }
            }
        }
    }

    /// Declares the tool specifications exposed to AI agents via the MCP protocol.
    pub fn declared_tools(&self) -> Vec<ToolDefinition> {
        vec![
            ToolDefinition {
                name: "trim_context".to_string(),
                description: "Extract mathematically optimal prompt context within a token budget using Personalized PageRank and CELF knapsack optimization. Seeds can be explicit symbol names, a natural language query, or inferred from current git diff.".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "seeds": {
                            "type": "array",
                            "items": { "type": "string" },
                            "description": "Optional list of seed symbol identifiers to anchor context around (e.g. ['ContextSelector', 'select_context'])"
                        },
                        "query": {
                            "type": "string",
                            "description": "Optional natural language intent, search terms, or error message to automatically infer seeds"
                        },
                        "fromDiff": {
                            "type": "boolean",
                            "description": "Optional flag to automatically infer seeds from current uncommitted git changes"
                        },
                        "budget": {
                            "description": "Maximum token budget for selected context, or 'auto' for Knee-Curve tuning (default: 1000)"
                        },
                        "model": {
                            "type": "string",
                            "description": "Target LLM architecture for auto-budgeting presets (e.g. 'claude', 'gpt-4o', 'deepseek', 'ollama')"
                        },
                        "path": {
                            "type": "string",
                            "description": "Target codebase directory to scan (default: '.')"
                        },
                        "format": {
                            "type": "string",
                            "enum": ["markdown", "json"],
                            "description": "Output serialization format (default: 'markdown')"
                        }
                    }
                }),
            },
            ToolDefinition {
                name: "query_graph_stats".to_string(),
                description: "Retrieve repository code graph connectivity metrics, syntax entity counts, and global PageRank architectural hubs.".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "Target codebase directory to scan (default: '.')"
                        }
                    }
                }),
            },
            ToolDefinition {
                name: "inspect_symbol".to_string(),
                description: "Inspect a symbol's declaration details, token cost, outgoing dependencies with transition weights, and incoming callers across the workspace.".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "symbol": {
                            "type": "string",
                            "description": "Name of the symbol to inspect"
                        },
                        "path": {
                            "type": "string",
                            "description": "Target codebase directory to scan (default: '.')"
                        }
                    },
                    "required": ["symbol"]
                }),
            },
            ToolDefinition {
                name: "clean_cache".to_string(),
                description: "Clear the incremental AST Merkle cache (.repotrim directory) to force a fresh re-scan.".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "Target codebase directory containing .repotrim cache (default: '.')"
                        }
                    }
                }),
            },
            ToolDefinition {
                name: "generate_blueprint".to_string(),
                description: "Generate a structured feature blueprint (FEATURE_BLUEPRINT.md) with inferred seed anchors, target files, and recommended token budgets for a given task description.".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "task": {
                            "type": "string",
                            "description": "Task or feature description to scaffold (e.g. 'Add user session authentication')"
                        },
                        "budget": {
                            "description": "Recommended token budget for context slicing, or 'auto' for Knee-Curve tuning (default: 3000)"
                        },
                        "model": {
                            "type": "string",
                            "description": "Target LLM architecture for auto-budgeting presets (e.g. 'claude', 'gpt-4o', 'deepseek', 'ollama')"
                        },
                        "path": {
                            "type": "string",
                            "description": "Target codebase directory to scan (default: '.')"
                        }
                    },
                    "required": ["task"]
                }),
            },
            ToolDefinition {
                name: "generate_architecture_docs".to_string(),
                description: "Generate comprehensive, durable repository architecture documentation (ARCHITECTURE.md) including subsystem topology, architectural layers, central hubs, Mermaid diagrams, and public API inventories.".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "Target codebase directory to scan (default: '.')"
                        },
                        "output": {
                            "type": "string",
                            "description": "Optional destination file path to write generated documentation to (e.g. 'docs/ARCHITECTURE.md')"
                        }
                    }
                }),
            },
            ToolDefinition {
                name: "analyze_impact".to_string(),
                description: "Analyze the semantic blast radius and ripple effects of code changes (from git diff or a target symbol). Classifies direct mutations, 1st-order callers, transitive dependencies, affected test candidates, and computes an architectural risk score.".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "symbol": {
                            "type": "string",
                            "description": "Optional target symbol name to evaluate blast radius for"
                        },
                        "fromDiff": {
                            "type": "boolean",
                            "description": "Optional flag to infer modified symbols from git diff (default: true if no symbol provided)"
                        },
                        "diffAgainst": {
                            "type": "string",
                            "description": "Optional git revision or branch to diff against (e.g. 'origin/main', 'HEAD~1')"
                        },
                        "budget": {
                            "description": "Maximum token budget for blast radius context outline (default: 1000)"
                        },
                        "format": {
                            "type": "string",
                            "enum": ["markdown", "json"],
                            "description": "Output serialization format (default: 'markdown')"
                        },
                        "path": {
                            "type": "string",
                            "description": "Target codebase directory to scan (default: '.')"
                        }
                    }
                }),
            },
        ]
    }

    /// Dispatches tool execution to the appropriate internal tool logic.
    fn execute_tool(&mut self, name: &str, arguments: serde_json::Value) -> ToolCallResult {
        match name {
            "trim_context" => self.tool_trim_context(arguments),
            "query_graph_stats" => self.tool_query_graph_stats(arguments),
            "inspect_symbol" => self.tool_inspect_symbol(arguments),
            "clean_cache" => self.tool_clean_cache(arguments),
            "generate_blueprint" => self.tool_generate_blueprint(arguments),
            "generate_architecture_docs" => self.tool_generate_architecture_docs(arguments),
            "analyze_impact" => self.tool_analyze_impact(arguments),
            _ => ToolCallResult::error(format!("Unsupported tool '{}'", name)),
        }
    }

    /// Resolves target directory from optional argument or default root.
    fn resolve_path(&self, args: &serde_json::Value) -> PathBuf {
        args.get("path")
            .and_then(|p| p.as_str())
            .map(PathBuf::from)
            .unwrap_or_else(|| self.default_root.clone())
    }

    /// Retrieves or loads the repository with incremental caching and live watcher synchronization.
    fn get_or_load_repo(
        &mut self,
        target_path: &Path,
    ) -> Result<RwLockWriteGuard<'_, LoadedRepository>, String> {
        let needs_reload = match &self.cached_repo {
            Some((cached_path, _, _)) => cached_path != target_path,
            None => true,
        };

        if needs_reload {
            let loaded = LoadedRepository::load(target_path).map_err(|e| {
                format!(
                    "Failed to load repository at '{}': {}",
                    target_path.display(),
                    e
                )
            })?;
            let repo_arc = Arc::new(RwLock::new(loaded));
            let watcher = RepositoryWatcher::start(Arc::clone(&repo_arc), None, None).ok();
            self.cached_repo = Some((target_path.to_path_buf(), repo_arc, watcher));
        }

        let (_, repo_arc, _) = self.cached_repo.as_ref().unwrap();
        repo_arc
            .write()
            .map_err(|_| "Repository lock poisoned".to_string())
    }

    fn tool_trim_context(&mut self, args: serde_json::Value) -> ToolCallResult {
        let seeds_opt: Option<Vec<String>> =
            args.get("seeds").and_then(|s| s.as_array()).map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            });
        let query_opt = args
            .get("query")
            .and_then(|q| q.as_str())
            .map(|s| s.to_string());
        let from_diff = args
            .get("fromDiff")
            .and_then(|d| d.as_bool())
            .unwrap_or(false);

        let seeds = seeds_opt.unwrap_or_default();
        if seeds.is_empty() && query_opt.is_none() && !from_diff {
            return ToolCallResult::error(
                "At least one seed source must be provided: 'seeds', 'query', or 'fromDiff: true'",
            );
        }

        let is_auto = match args.get("budget") {
            Some(serde_json::Value::String(s)) => s.eq_ignore_ascii_case("auto"),
            _ => false,
        };

        let model_str = args.get("model").and_then(|m| m.as_str());
        let model_profile = if is_auto {
            Some(ModelProfile::parse(model_str.unwrap_or("claude")))
        } else {
            None
        };

        let explicit_budget: usize = match args.get("budget") {
            Some(serde_json::Value::Number(n)) => n.as_u64().unwrap_or(1000) as usize,
            Some(serde_json::Value::String(s)) => s.parse::<usize>().unwrap_or(1000),
            _ => 1000,
        };

        let format_str = args
            .get("format")
            .and_then(|f| f.as_str())
            .unwrap_or("markdown");
        let target_path = self.resolve_path(&args);

        let mut repo = match self.get_or_load_repo(&target_path) {
            Ok(r) => r,
            Err(e) => return ToolCallResult::error(e),
        };

        let mut weighted_seeds: std::collections::HashMap<SymbolId, f32> =
            std::collections::HashMap::new();
        let mut missing_seeds: Vec<String> = Vec::new();

        // 1. Explicit seeds
        for seed_name in &seeds {
            let matches: Vec<SymbolId> = repo
                .symbols
                .iter()
                .filter(|s| s.name == *seed_name || s.name.eq_ignore_ascii_case(seed_name))
                .map(|s| s.id)
                .collect();

            if !matches.is_empty() {
                for id in matches {
                    *weighted_seeds.entry(id).or_default() += 2.0;
                }
            } else {
                missing_seeds.push(seed_name.clone());
            }
        }

        // 2. Natural language query seeds
        if let Some(query) = &query_opt {
            let query_seeds = IntentResolver::resolve_query(&repo.symbols, query, 5);
            for (id, weight) in query_seeds {
                *weighted_seeds.entry(id).or_default() += weight;
            }
        }

        // 3. Git diff seeds
        if from_diff {
            if let Ok(diff_text) = DiffResolver::get_git_diff(&target_path) {
                let modified = DiffResolver::parse_unified_diff(&diff_text);
                let diff_seeds = DiffResolver::resolve_modified_symbols(&repo.symbols, &modified);
                for (id, weight) in diff_seeds {
                    *weighted_seeds.entry(id).or_default() += weight;
                }
            }
        }

        if weighted_seeds.is_empty() {
            return ToolCallResult::error(format!(
                "No valid seed symbols could be identified from the provided parameters in {}",
                target_path.display()
            ));
        }

        if let Err(e) = repo.load_all_sources() {
            return ToolCallResult::error(format!("Failed to load sources: {}", e));
        }

        let seed_pairs: Vec<(SymbolId, f32)> = weighted_seeds.into_iter().collect();
        let graph = repo.build_graph();
        let selector = ContextSelector::default();
        let (selected, markdown, auto_report_opt) = if let Some(ref model) = model_profile {
            let (sel, md, rep) = selector.select_and_format_context_auto_weighted(
                &graph,
                &seed_pairs,
                *model,
                &repo.file_sources,
            );
            (sel, md, Some(rep))
        } else {
            let (sel, md) = selector.select_and_format_context_weighted(
                &graph,
                &seed_pairs,
                explicit_budget,
                &repo.file_sources,
            );
            (sel, md, None)
        };

        let total_tokens: usize = selected.iter().map(|s| s.token_cost).sum();

        if format_str == "json" {
            let budget_val = if let Some(ref rep) = auto_report_opt {
                serde_json::json!(rep.knee_tokens)
            } else {
                serde_json::json!(explicit_budget)
            };
            let mut json_val = serde_json::json!({
                "budget": budget_val,
                "tokens_used": total_tokens,
                "symbols_count": selected.len(),
                "symbols": selected.iter().map(|s| {
                    serde_json::json!({
                        "id": s.id.0,
                        "name": s.name,
                        "kind": format!("{:?}", s.kind),
                        "file": s.file_path.display().to_string(),
                        "lines": [s.span.start_row + 1, s.span.end_row + 1],
                        "token_cost": s.token_cost,
                        "signature": s.signature,
                    })
                }).collect::<Vec<_>>(),
                "markdown": markdown,
            });
            if let Some(ref rep) = auto_report_opt {
                json_val["auto_budget"] = serde_json::json!({
                    "model": rep.model_name,
                    "optimal_budget": rep.optimal_budget,
                    "knee_tokens": rep.knee_tokens,
                    "knee_utility_ratio": rep.knee_utility_ratio,
                    "candidate_count": rep.candidate_count,
                    "selected_count": rep.selected_count,
                });
            }
            ToolCallResult::success(serde_json::to_string_pretty(&json_val).unwrap())
        } else {
            let mut prefix = String::new();
            if let Some(ref rep) = auto_report_opt {
                prefix.push_str(&format!(
                    "<!-- Auto-budget tuned to {} tokens via Knee-Curve (knee utility: {:.1}%, ceiling: {}, model: {}) -->\n\n",
                    rep.knee_tokens, rep.knee_utility_ratio * 100.0, rep.optimal_budget, rep.model_name
                ));
            }
            if !missing_seeds.is_empty() {
                prefix.push_str(&format!(
                    "<!-- Warning: Unresolved explicit seeds: {} -->\n\n",
                    missing_seeds.join(", ")
                ));
            }
            if let Some(query) = &query_opt {
                prefix.push_str(&format!(
                    "<!-- Inferred context from query: \"{}\" -->\n\n",
                    query
                ));
            }
            if from_diff {
                prefix.push_str("<!-- Inferred context from git diff changes -->\n\n");
            }
            ToolCallResult::success(format!("{}{}", prefix, markdown))
        }
    }

    fn tool_query_graph_stats(&mut self, args: serde_json::Value) -> ToolCallResult {
        let target_path = self.resolve_path(&args);
        let repo = match self.get_or_load_repo(&target_path) {
            Ok(r) => r,
            Err(e) => return ToolCallResult::error(e),
        };

        let graph = repo.build_graph();
        let num_symbols = graph.num_symbols();
        let num_edges = graph.num_edges();

        let mut num_fns = 0;
        let mut num_methods = 0;
        let mut num_structs = 0;
        let mut num_enums = 0;
        let mut num_traits = 0;
        let mut total_tokens = 0;

        for s in graph.symbols() {
            total_tokens += s.token_cost;
            match s.kind {
                SymbolKind::Function => num_fns += 1,
                SymbolKind::Method => num_methods += 1,
                SymbolKind::Struct => num_structs += 1,
                SymbolKind::Enum => num_enums += 1,
                SymbolKind::Trait => num_traits += 1,
                _ => {}
            }
        }

        let ppr = PprSolver::default();
        let uniform_seeds: Vec<(SymbolId, f32)> = (0..num_symbols as u32)
            .map(|id| (SymbolId(id), 1.0 / num_symbols.max(1) as f32))
            .collect();

        let ppr_scores = ppr.compute(&graph, &uniform_seeds);
        let mut hub_scores: Vec<(SymbolId, f32)> = ppr_scores.into_iter().collect();
        hub_scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        let mut out = String::new();
        out.push_str("### RepoTrim Codebase Graph Statistics\n\n");
        out.push_str(&format!(
            "- **Directory:** `{}`\n",
            repo.root_path.display()
        ));
        out.push_str(&format!(
            "- **Files:** {} (Cache: {}/{} warm hits, {:.1}%)\n",
            repo.cache_report.total_files,
            repo.cache_report.cached_files,
            repo.cache_report.total_files,
            repo.cache_report.hit_ratio * 100.0,
        ));
        out.push_str(&format!("- **Total Code Tokens:** {}\n", total_tokens));
        out.push_str(&format!("- **Symbols (Nodes V):** {} (Functions: {}, Methods: {}, Structs: {}, Enums: {}, Traits: {})\n",
            num_symbols, num_fns, num_methods, num_structs, num_enums, num_traits
        ));
        out.push_str(&format!(
            "- **Resolved Edges (E):** {} (Avg Out-Degree: {:.2})\n\n",
            num_edges,
            if num_symbols > 0 {
                num_edges as f32 / num_symbols as f32
            } else {
                0.0
            }
        ));

        out.push_str("#### Top Architectural Hubs (Global PageRank Centrality):\n");
        for (rank, (sym_id, score)) in hub_scores.iter().take(8).enumerate() {
            if let Some(sym) = graph.symbol(*sym_id) {
                out.push_str(&format!(
                    "{}. `{}` ({:?}) - {:.2}% PR | {}:L{}\n",
                    rank + 1,
                    sym.name,
                    sym.kind,
                    score * 100.0,
                    sym.file_path.display(),
                    sym.span.start_row + 1
                ));
            }
        }

        ToolCallResult::success(out)
    }

    fn tool_inspect_symbol(&mut self, args: serde_json::Value) -> ToolCallResult {
        let symbol_name = match args.get("symbol").and_then(|s| s.as_str()) {
            Some(s) => s,
            None => return ToolCallResult::error("Missing required parameter 'symbol'"),
        };

        let target_path = self.resolve_path(&args);
        let repo = match self.get_or_load_repo(&target_path) {
            Ok(r) => r,
            Err(e) => return ToolCallResult::error(e),
        };

        let graph = repo.build_graph();
        let matching_symbols: Vec<_> = graph
            .symbols()
            .iter()
            .filter(|s| s.name.eq_ignore_ascii_case(symbol_name))
            .collect();

        if matching_symbols.is_empty() {
            return ToolCallResult::error(format!(
                "Symbol '{}' was not found in parsed graph",
                symbol_name
            ));
        }

        let num_symbols = graph.num_symbols();
        let mut out = String::new();

        for sym in matching_symbols {
            let sym_id = sym.id;
            let neighbors = graph.neighbors(sym_id);
            let weights = graph.transition_probabilities(sym_id);

            let mut incoming_callers = Vec::new();
            for other_id in 0..num_symbols as u32 {
                if other_id == sym_id.0 {
                    continue;
                }
                let other_neighbors = graph.neighbors(SymbolId(other_id));
                if other_neighbors.contains(&sym_id.0) {
                    if let Some(other_sym) = graph.symbol(SymbolId(other_id)) {
                        incoming_callers.push(other_sym);
                    }
                }
            }

            out.push_str(&format!("### Symbol: `{}` ({:?})\n", sym.name, sym.kind));
            out.push_str(&format!(
                "- **Location:** `{}:L{}-L{}`\n",
                sym.file_path.display(),
                sym.span.start_row + 1,
                sym.span.end_row + 1
            ));
            out.push_str(&format!("- **Token Cost:** ~{} tokens\n", sym.token_cost));
            out.push_str(&format!("- **Signature:** `{}`\n", sym.signature));
            if let Some(doc) = &sym.docstring {
                out.push_str(&format!("- **Docstring:** {}\n", doc));
            }

            out.push_str(&format!(
                "\n**Outgoing Dependencies ({}):**\n",
                neighbors.len()
            ));
            if neighbors.is_empty() {
                out.push_str("- None (leaf node)\n");
            } else {
                for (idx, &dst_id) in neighbors.iter().enumerate() {
                    let prob = weights.get(idx).copied().unwrap_or(0.0);
                    if let Some(dst) = graph.symbol(SymbolId(dst_id)) {
                        out.push_str(&format!(
                            "- `->` `{}` ({:?}) | {:.1}% weight | {}:L{}\n",
                            dst.name,
                            dst.kind,
                            prob * 100.0,
                            dst.file_path.display(),
                            dst.span.start_row + 1
                        ));
                    }
                }
            }

            out.push_str(&format!(
                "\n**Incoming Callers / References ({}):**\n",
                incoming_callers.len()
            ));
            if incoming_callers.is_empty() {
                out.push_str("- None in workspace\n");
            } else {
                for caller in incoming_callers {
                    out.push_str(&format!(
                        "- `<-` `{}` ({:?}) | {}:L{}\n",
                        caller.name,
                        caller.kind,
                        caller.file_path.display(),
                        caller.span.start_row + 1
                    ));
                }
            }
            out.push_str("\n---\n\n");
        }

        ToolCallResult::success(out)
    }

    fn tool_clean_cache(&mut self, args: serde_json::Value) -> ToolCallResult {
        let target_path = self.resolve_path(&args);
        let cache_dir = target_path.join(".repotrim");
        self.cached_repo = None;

        if cache_dir.exists() {
            match fs::remove_dir_all(&cache_dir) {
                Ok(_) => ToolCallResult::success(format!(
                    "Successfully removed cache at '{}'",
                    cache_dir.display()
                )),
                Err(e) => ToolCallResult::error(format!("Failed to remove cache: {}", e)),
            }
        } else {
            ToolCallResult::success(format!(
                "No cache directory found at '{}'",
                cache_dir.display()
            ))
        }
    }

    fn tool_generate_blueprint(&mut self, args: serde_json::Value) -> ToolCallResult {
        let task = match args.get("task").and_then(|t| t.as_str()) {
            Some(t) => t,
            None => return ToolCallResult::error("Missing required parameter 'task'"),
        };

        let budget_val = match args.get("budget") {
            Some(serde_json::Value::Number(n)) => serde_json::json!(n.as_u64().unwrap_or(3000)),
            Some(serde_json::Value::String(s)) => {
                if let Ok(n) = s.parse::<usize>() {
                    serde_json::json!(n)
                } else {
                    serde_json::json!(s)
                }
            }
            _ => serde_json::json!(3000),
        };
        let model_str = args.get("model").and_then(|m| m.as_str());
        let target_path = self.resolve_path(&args);

        let repo = match self.get_or_load_repo(&target_path) {
            Ok(r) => r,
            Err(e) => return ToolCallResult::error(e),
        };

        let top_seeds = IntentResolver::resolve_query(&repo.symbols, task, 6);

        let mut seed_files = Vec::new();
        let mut seen_files = std::collections::HashSet::new();
        let mut symbol_targets = Vec::new();

        for (sym_id, confidence) in &top_seeds {
            if let Some(sym) = repo.symbols.iter().find(|s| s.id == *sym_id) {
                let file_str = sym.file_path.display().to_string();
                if seen_files.insert(file_str.clone()) {
                    seed_files.push(file_str);
                }
                symbol_targets.push((sym.clone(), *confidence));
            }
        }

        let mut doc = String::new();
        doc.push_str(&format!("# Feature Blueprint: {}\n\n", task));
        doc.push_str("> **Purpose:** High-density, low-token specification for AI agent harnesses to execute this task without exploratory whole-file dumping.\n\n");
        doc.push_str("---\n\n");

        doc.push_str("## 1. Objective & Scope\n");
        doc.push_str(&format!("- **Task:** {}\n", task));
        doc.push_str("- **Target Subsystems:**\n");
        if seed_files.is_empty() {
            doc.push_str("  - *(No existing files matched; new module creation)*\n");
        } else {
            for f in &seed_files {
                doc.push_str(&format!("  - `{}`\n", f));
            }
        }
        doc.push_str("- **Expected Outcome:** Functional implementation passing all unit and integration tests with zero compiler or linter warnings.\n\n");

        doc.push_str("---\n\n");
        doc.push_str("## 2. Seed Anchors (for RepoTrim Context Slicing)\n");
        doc.push_str("RepoTrim uses these anchors to compute Forward-Push Personalized PageRank and pack the optimal dependency skeleton:\n\n");

        doc.push_str("### Key Symbol Targets\n");
        if symbol_targets.is_empty() {
            doc.push_str("- *(No direct symbol matches found; use `query` for broad context)*\n");
        } else {
            for (sym, conf) in &symbol_targets {
                doc.push_str(&format!(
                    "- `{}` ({:?}) - `{}:L{}` | Inferred relevance: {:.0}%\n",
                    sym.name,
                    sym.kind,
                    sym.file_path.display(),
                    sym.span.start_row + 1,
                    conf * 100.0
                ));
            }
        }

        doc.push_str("\n### Recommended RepoTrim Invocation\n");
        doc.push_str("Execute before reading any full files to load the complete caller/callee context skeleton:\n\n");
        let mut invocation_args = serde_json::json!({
            "query": task,
            "budget": budget_val
        });
        if let Some(m) = model_str {
            invocation_args["model"] = serde_json::json!(m);
        }
        doc.push_str(&format!(
            "```json\n{{\n  \"name\": \"trim_context\",\n  \"arguments\": {}\n}}\n```\n\n",
            serde_json::to_string_pretty(&invocation_args).unwrap_or_default()
        ));

        doc.push_str("---\n\n");
        doc.push_str("## 3. Technical Constraints & Invariants\n");
        doc.push_str("- **Context Economy:** Follow the 3-Tier Context Funnel (Blueprint -> RepoTrim Skeleton -> Target File only).\n");
        doc.push_str("- **LOD Interpretation:** Do not attempt to re-implement or fix sliced code marked with `pass` or `// ... [sliced] ...`.\n");
        doc.push_str("- **Code Quality:** Zero clippy warnings (`cargo clippy --workspace --all-targets -- -D warnings`).\n");
        doc.push_str("- **Git Commit Style:** Atomic commits, imperative summary under 50 characters, 72-char body wrap, no co-author tags.\n\n");

        doc.push_str("---\n\n");
        doc.push_str("## 4. Acceptance Criteria & Test Plan\n");
        doc.push_str("- [ ] Implementation fulfills the core objective without regressions.\n");
        doc.push_str("- [ ] Unit tests added verifying positive and edge cases.\n");
        doc.push_str("- [ ] Workspace tests pass: `cargo test --workspace --all-targets`\n");
        doc.push_str(
            "- [ ] Linter checks clean: `cargo clippy --workspace --all-targets -- -D warnings`\n",
        );
        doc.push_str("- [ ] Code formatted cleanly: `cargo fmt --all -- --check`\n");

        ToolCallResult::success(doc)
    }

    fn tool_generate_architecture_docs(&mut self, args: serde_json::Value) -> ToolCallResult {
        let target_path = self.resolve_path(&args);

        let repo = match self.get_or_load_repo(&target_path) {
            Ok(r) => r,
            Err(e) => return ToolCallResult::error(e),
        };

        let graph = repo.build_graph();
        let report = ArchitectureReport::analyze(&graph, &repo.root_path);
        let markdown = report.to_markdown();

        if let Some(output_file) = args.get("output").and_then(|o| o.as_str()) {
            let out_path = target_path.join(output_file);
            if let Some(parent) = out_path.parent() {
                let _ = fs::create_dir_all(parent);
            }
            if let Err(e) = fs::write(&out_path, &markdown) {
                return ToolCallResult::error(format!(
                    "Failed to write architecture specification to '{}': {}",
                    out_path.display(),
                    e
                ));
            }
        }

        ToolCallResult::success(markdown)
    }

    fn tool_analyze_impact(&mut self, args: serde_json::Value) -> ToolCallResult {
        let target_path = self.resolve_path(&args);

        let mut repo = match self.get_or_load_repo(&target_path) {
            Ok(r) => r,
            Err(e) => return ToolCallResult::error(e),
        };

        if let Err(e) = repo.load_all_sources() {
            return ToolCallResult::error(format!("Failed to load file sources: {}", e));
        }

        let graph = repo.build_graph();

        let budget: usize = args
            .get("budget")
            .and_then(|b| {
                if let Some(s) = b.as_str() {
                    if s == "auto" {
                        Some(1500)
                    } else {
                        s.parse().ok()
                    }
                } else {
                    b.as_u64().map(|u| u as usize)
                }
            })
            .unwrap_or(1000);

        let format = args
            .get("format")
            .and_then(|f| f.as_str())
            .unwrap_or("markdown");

        let report = if let Some(sym_name) = args.get("symbol").and_then(|s| s.as_str()) {
            ImpactAnalyzer::analyze_symbol(&graph, sym_name, budget, &repo.file_sources)
        } else if let Some(rev) = args.get("diffAgainst").and_then(|d| d.as_str()) {
            match DiffResolver::get_git_diff_against(&target_path, rev) {
                Ok(diff_text) => {
                    ImpactAnalyzer::analyze_diff(&graph, &diff_text, budget, &repo.file_sources)
                }
                Err(e) => {
                    return ToolCallResult::error(format!(
                        "Failed to extract git diff against '{}': {}",
                        rev, e
                    ))
                }
            }
        } else {
            match DiffResolver::get_git_diff(&target_path) {
                Ok(diff_text) => {
                    ImpactAnalyzer::analyze_diff(&graph, &diff_text, budget, &repo.file_sources)
                }
                Err(e) => {
                    return ToolCallResult::error(format!("Failed to extract git diff: {}", e))
                }
            }
        };

        if format == "json" {
            match serde_json::to_string_pretty(&report) {
                Ok(json_str) => ToolCallResult::success(json_str),
                Err(e) => {
                    ToolCallResult::error(format!("Failed to serialize report to JSON: {}", e))
                }
            }
        } else {
            ToolCallResult::success(report.context_markdown)
        }
    }
}
