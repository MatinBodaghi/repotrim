use std::fmt::Write as FmtWrite;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock, RwLockWriteGuard};

use repotrim_engine::{
    count_tokens, ArchitectureReport, BenchmarkMetrics, BenchmarkRunner, BenchmarkSummary,
    CoeditCache, CoeditConfig, CommunityConfig, CommunityDetector, ContextSelector,
    ContextStrategy, DiffResolver, EdgeWeightLearner, GitCommitMiner, HybridRetriever,
    ImpactAnalyzer, IntentResolver, LayerWeights, LoadedRepository, LodLevel, ModelProfile,
    PprSolver, RepositoryWatcher, RetrievalConfig, SearchMode, SymbolId, SymbolKind, TaskContext,
    TokenizerModel,
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
                        "tokenizer": {
                            "type": "string",
                            "enum": ["fast", "calibrated", "exact", "cl100k", "o200k"],
                            "description": "Tokenizer model for exact or heuristic token counting ('fast', 'calibrated', 'exact' / 'cl100k', 'o200k'; default: 'fast')"
                        },
                        "path": {
                            "type": "string",
                            "description": "Target codebase directory to scan (default: '.')"
                        },
                        "format": {
                            "type": "string",
                            "enum": ["markdown", "json"],
                            "description": "Output serialization format (default: 'markdown')"
                        },
                        "diagnostics": {
                            "type": "boolean",
                            "description": "Optional flag to include knapsack numerical stability and sensitivity diagnostics"
                        },
                        "jointLod": {
                            "type": "boolean",
                            "description": "Optional flag to enable Multiple-Choice Knapsack (MCKP) joint symbol selection and Level-of-Detail (LOD) optimization"
                        },
                        "useCoedits": {
                            "type": "boolean",
                            "description": "Optional flag to include historical Git commit co-edit edges in the multiplex graph"
                        },
                        "learnWeights": {
                            "type": "boolean",
                            "description": "Optional flag to dynamically calibrate multiplex layer weights using empirical Git commit history"
                        },
                        "communityBoost": {
                            "type": "number",
                            "description": "Optional intra-community cohesion boost multiplier (e.g. 0.35) to focus context selection on seeds' topological communities"
                        },
                        "retrievalMode": {
                            "type": "string",
                            "enum": ["hybrid", "lexical", "dense"],
                            "description": "Query seed retrieval scoring mode ('hybrid' RRF, 'lexical' BM25+, 'dense' cosine; default: 'hybrid')"
                        },
                        "queryExpand": {
                            "type": "boolean",
                            "description": "Optional flag to enable Rocchio Pseudo-Relevance Feedback (PRF) query expansion for natural language queries"
                        },
                        "structured": {
                            "type": "boolean",
                            "description": "Optional flag to return a fully structured context graph with typed edges, causal paths, cost breakdown, and omission diagnostics"
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
                description: "Generate an agent-ready high-density feature specification blueprint with inferred seed anchors, target implementation files, and relevant interface definitions.".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "task": {
                            "type": "string",
                            "description": "Feature specification or bug fix description"
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
                        },
                        "resolution": {
                            "type": "number",
                            "description": "Optional modularity resolution parameter gamma (default: 1.0, <1.0 for macro, >1.0 for micro)"
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
                        "tokenizer": {
                            "type": "string",
                            "enum": ["fast", "calibrated", "exact", "cl100k", "o200k"],
                            "description": "Tokenizer model for exact or heuristic token counting ('fast', 'calibrated', 'exact' / 'cl100k', 'o200k'; default: 'fast')"
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
            ToolDefinition {
                name: "mine_coedits".to_string(),
                description: "Mine historical Git commit co-edits and logical couplings, analyze layer empirical co-change rates, and learn principled layer weights.".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "Target codebase directory to scan (default: '.')"
                        },
                        "maxCommits": {
                            "type": "number",
                            "description": "Maximum historical commits to analyze (default: 200)"
                        },
                        "minSupport": {
                            "type": "number",
                            "description": "Minimum co-edit occurrences required (default: 2)"
                        },
                        "format": {
                            "type": "string",
                            "enum": ["markdown", "json"],
                            "description": "Output format ('markdown' or 'json', default: 'markdown')"
                        }
                    }
                }),
            },
            ToolDefinition {
                name: "detect_communities".to_string(),
                description: "Detect multi-resolution topological communities, analyze hierarchical modularity (Macro/Meso/Micro), and spotlight architectural drift / misplaced symbols.".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "Target codebase directory to scan (default: '.')"
                        },
                        "resolution": {
                            "type": "number",
                            "description": "Modularity resolution parameter gamma (default: 1.0, <1.0 for macro, >1.0 for micro)"
                        },
                        "hierarchy": {
                            "type": "boolean",
                            "description": "Whether to return multi-scale hierarchy (Macro, Meso, Micro; default: false)"
                        },
                        "drift": {
                            "type": "boolean",
                            "description": "Whether to analyze architectural drift and misplaced symbols (default: false)"
                        },
                        "format": {
                            "type": "string",
                            "enum": ["markdown", "json"],
                            "description": "Output format ('markdown' or 'json', default: 'markdown')"
                        }
                    }
                }),
            },
            ToolDefinition {
                name: "search_symbols".to_string(),
                description: "Perform hybrid lexical (BM25+) and dense semantic (subword feature hashing) symbol retrieval across the repository.".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "query": {
                            "type": "string",
                            "description": "Search query or natural language description (e.g. 'PPR solver', 'select context', 'knapsack')"
                        },
                        "limit": {
                            "type": "number",
                            "description": "Maximum number of symbols to retrieve (default: 10)"
                        },
                        "mode": {
                            "type": "string",
                            "enum": ["hybrid", "lexical", "dense"],
                            "description": "Retrieval scoring mode ('hybrid' RRF, 'lexical' BM25+, 'dense' cosine; default: 'hybrid')"
                        },
                        "expand": {
                            "type": "boolean",
                            "description": "Enable Rocchio Pseudo-Relevance Feedback (PRF) query expansion (default: false)"
                        },
                        "format": {
                            "type": "string",
                            "enum": ["markdown", "json"],
                            "description": "Output serialization format ('markdown' or 'json', default: 'markdown')"
                        },
                        "path": {
                            "type": "string",
                            "description": "Target codebase directory to scan (default: '.')"
                        }
                    },
                    "required": ["query"]
                }),
            },
            ToolDefinition {
                name: "run_benchmark".to_string(),
                description: "Execute rigorous empirical evaluation harness and Aider comparative benchmark. Evaluates context selection strategies (Whole-File Dump, Naive Keyword/Grep, Aider Repo Map with Global PageRank, RepoTrim Vanilla, and RepoTrim Full with multiplex CPG, learned layer weights, co-edit edges, forward-push diffusion, CELF knapsack, community cohesion, and MCKP joint LOD) across token budget adherence, token reduction %, 1st-order direct dependency recall, 2nd-order transitive recall, context precision, community cohesion, orphan symbol rate, and execution latency.".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "scenario": {
                            "type": "string",
                            "description": "Specific benchmark scenario name or ID filter (e.g. 'context_selector', 'ppr_solver', 'diff_resolver', 'multi_seed_subsystem', 'query_intent_random_walk')"
                        },
                        "budget": {
                            "type": "integer",
                            "description": "Override token budget for the benchmark scenarios"
                        },
                        "strategies": {
                            "type": "array",
                            "items": {
                                "type": "string",
                                "enum": ["whole_file", "naive_grep", "aider_repo_map", "repo_trim_vanilla", "repo_trim_full"]
                            },
                            "description": "List of context strategies to benchmark (default: all 5 strategies)"
                        },
                        "format": {
                            "type": "string",
                            "enum": ["markdown", "json"],
                            "description": "Output serialization format ('markdown' or 'json', default: 'markdown')"
                        },
                        "path": {
                            "type": "string",
                            "description": "Target codebase directory to scan (default: '.')"
                        }
                    }
                }),
            },
            ToolDefinition {
                name: "locate_entrypoints".to_string(),
                description: "Discovers top-ranked codebase entrypoint symbols for an agent task or issue prompt. Evaluates seed hints, concept keywords, BM25+ lexical and trigram fuzzy matching, and target file constraints.".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "query": {
                            "type": "string",
                            "description": "Natural language task prompt, issue description, or symbol hint"
                        },
                        "targetFiles": {
                            "type": "array",
                            "items": { "type": "string" },
                            "description": "Optional list of target files or globs to prioritize during search"
                        },
                        "limit": {
                            "type": "integer",
                            "description": "Maximum number of ranked candidate entrypoints to return (default: 10)"
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
                    },
                    "required": ["query"]
                }),
            },
            ToolDefinition {
                name: "trace_paths".to_string(),
                description: "Discovers and scores constrained multi-hop causal execution paths between source and target symbols using Boltzmann path energy scoring, and renders Mermaid sequence diagrams.".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "source": {
                            "type": "string",
                            "description": "Source entrypoint symbol name"
                        },
                        "target": {
                            "type": "string",
                            "description": "Target destination symbol name"
                        },
                        "task": {
                            "type": "string",
                            "description": "Optional natural language task description to condition Boltzmann path scoring"
                        },
                        "includeMermaid": {
                            "type": "boolean",
                            "description": "Whether to render a Mermaid sequence diagram in the output (default: true)"
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
                    },
                    "required": ["source", "target"]
                }),
            },
            ToolDefinition {
                name: "expand_symbol".to_string(),
                description: "Expands a localized submodular knapsack context cluster around a focal symbol within a token budget ceiling, extracting relevant syntax definitions and causal connection paths.".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "symbol": {
                            "type": "string",
                            "description": "Focal symbol name to expand context around"
                        },
                        "budget": {
                            "description": "Maximum token budget ceiling for expansion cluster (default: 1000)"
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
                    },
                    "required": ["symbol"]
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
            "mine_coedits" => self.tool_mine_coedits(arguments),
            "detect_communities" => self.tool_detect_communities(arguments),
            "search_symbols" => self.tool_search_symbols(arguments),
            "run_benchmark" => self.tool_run_benchmark(arguments),
            "locate_entrypoints" => self.tool_locate_entrypoints(arguments),
            "trace_paths" => self.tool_trace_paths(arguments),
            "expand_symbol" => self.tool_expand_symbol(arguments),
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
        let tokenizer_str = args
            .get("tokenizer")
            .and_then(|t| t.as_str())
            .unwrap_or("fast");
        let tokenizer_model =
            TokenizerModel::from_str_name(tokenizer_str).unwrap_or(TokenizerModel::FastHeuristic);
        let diagnostics = args
            .get("diagnostics")
            .and_then(|d| d.as_bool())
            .unwrap_or(false);
        let joint_lod = args
            .get("jointLod")
            .and_then(|d| d.as_bool())
            .unwrap_or(false);
        let use_coedits = args
            .get("useCoedits")
            .and_then(|d| d.as_bool())
            .unwrap_or(false);
        let learn_weights = args
            .get("learnWeights")
            .and_then(|d| d.as_bool())
            .unwrap_or(false);
        let community_boost = args
            .get("communityBoost")
            .and_then(|d| d.as_f64())
            .unwrap_or(0.0) as f32;
        let retrieval_mode_str = args
            .get("retrievalMode")
            .and_then(|m| m.as_str())
            .unwrap_or("hybrid");
        let query_expand = args
            .get("queryExpand")
            .and_then(|e| e.as_bool())
            .unwrap_or(false);
        let structured = args
            .get("structured")
            .and_then(|s| s.as_bool())
            .unwrap_or(false);
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
                .filter(|s| s.name == *seed_name)
                .map(|s| s.id)
                .collect();
            if matches.is_empty() {
                missing_seeds.push(seed_name.clone());
            } else {
                for id in matches {
                    *weighted_seeds.entry(id).or_default() += 1.0;
                }
            }
        }

        // 2. Query seeds via BM25+ and dense semantic intent resolution
        if let Some(ref q) = query_opt {
            let search_mode = match retrieval_mode_str.to_lowercase().as_str() {
                "lexical" | "bm25" => SearchMode::LexicalOnly,
                "dense" | "semantic" => SearchMode::DenseOnly,
                _ => SearchMode::Hybrid,
            };
            let ret_config = RetrievalConfig {
                mode: search_mode,
                top_k: 5,
                query_expansion: query_expand,
                ..Default::default()
            };
            let query_seeds =
                IntentResolver::resolve_query_with_config(&repo.symbols, q, &ret_config);
            for r in query_seeds {
                *weighted_seeds.entry(r.symbol_id).or_default() += r.score;
            }
        }

        // 3. Git diff seeds
        if from_diff {
            match DiffResolver::get_git_diff(&target_path) {
                Ok(diff_text) => {
                    let modified_lines = DiffResolver::parse_unified_diff(&diff_text);
                    let diff_seeds =
                        DiffResolver::resolve_modified_symbols(&repo.symbols, &modified_lines);
                    for (id, weight) in diff_seeds {
                        *weighted_seeds.entry(id).or_default() += weight;
                    }
                }
                Err(e) => {
                    eprintln!("repotrim-mcp: Failed to read git diff: {}", e);
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

        let mut coedit_edges: Vec<(SymbolId, SymbolId, f32)> = Vec::new();
        let mut layer_weights = LayerWeights::default();

        if use_coedits || learn_weights {
            if let Ok(head_hash) = GitCommitMiner::get_head_hash(&repo.root_path) {
                let cache_file = repo.root_path.join(".repotrim").join("coedit.bin");
                let coedit_graph = if let Some(cached) =
                    CoeditCache::load_from_file(&cache_file, &head_hash)
                {
                    cached
                } else {
                    let config = CoeditConfig::default();
                    let mined =
                        GitCommitMiner::mine_repository(&repo.root_path, &repo.symbols, &config)
                            .unwrap_or_default();
                    let _ = CoeditCache::save_to_file(&cache_file, &mined);
                    mined
                };

                if use_coedits {
                    coedit_edges = coedit_graph.to_directed_edges(0.15);
                }

                if learn_weights {
                    let (learned, _) = EdgeWeightLearner::learn_weights(
                        &repo.symbols,
                        &repo.edges,
                        &repo.imports,
                        &coedit_graph,
                        LayerWeights::default(),
                    );
                    layer_weights = learned;
                }
            }
        }

        let graph = repo.build_graph_with_coedits(&coedit_edges, layer_weights);
        let selector = ContextSelector::default().with_tokenizer(tokenizer_model);
        let (selected, markdown, auto_report_opt, sensitivity_opt, mckp_opt) = if joint_lod {
            if let Some(ref model) = model_profile {
                let (sel, md, rep, mckp) = selector
                    .select_and_format_context_auto_weighted_joint_lod(
                        &graph,
                        &seed_pairs,
                        *model,
                        &repo.file_sources,
                    );
                (sel, md, Some(rep), None, Some(mckp))
            } else {
                let (sel, md, mckp) = selector.select_and_format_context_weighted_joint_lod(
                    &graph,
                    &seed_pairs,
                    explicit_budget,
                    &repo.file_sources,
                );
                (sel, md, None, None, Some(mckp))
            }
        } else if let Some(ref model) = model_profile {
            let (sel, md, rep) = selector.select_and_format_context_auto_weighted(
                &graph,
                &seed_pairs,
                *model,
                &repo.file_sources,
            );
            let sens = if diagnostics {
                let (_, _, s) = selector.select_and_format_context_weighted_with_sensitivity(
                    &graph,
                    &seed_pairs,
                    rep.knee_tokens,
                    &repo.file_sources,
                );
                Some(s)
            } else {
                None
            };
            (sel, md, Some(rep), sens, None)
        } else if diagnostics {
            let (sel, md, s) = selector.select_and_format_context_weighted_with_sensitivity(
                &graph,
                &seed_pairs,
                explicit_budget,
                &repo.file_sources,
            );
            (sel, md, None, Some(s), None)
        } else {
            let (sel, md) = if community_boost > 0.0 {
                selector.select_and_format_context_with_community(
                    &graph,
                    &seed_pairs,
                    explicit_budget,
                    community_boost,
                    &repo.file_sources,
                )
            } else {
                selector.select_and_format_context_weighted(
                    &graph,
                    &seed_pairs,
                    explicit_budget,
                    &repo.file_sources,
                )
            };
            (sel, md, None, None, None)
        };

        let total_tokens: usize = count_tokens(&markdown, tokenizer_model);

        if format_str == "json" {
            let budget_val = if let Some(ref rep) = auto_report_opt {
                serde_json::json!(rep.knee_tokens)
            } else {
                serde_json::json!(explicit_budget)
            };
            let mut json_val = serde_json::json!({
                "budget": budget_val,
                "tokens_used": total_tokens,
                "tokenizer": tokenizer_model.name(),
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
            if let Some(ref sens) = sensitivity_opt {
                json_val["sensitivity"] = serde_json::to_value(sens).unwrap();
            }
            if let Some(ref mckp) = mckp_opt {
                json_val["joint_lod"] = serde_json::to_value(mckp).unwrap();
            }
            if structured || format_str == "json" {
                let task_ctx = query_opt.as_deref().map(TaskContext::from_query);
                let target_budget = if let Some(ref rep) = auto_report_opt {
                    rep.knee_tokens
                } else {
                    explicit_budget
                };
                let structured_ctx = selector.select_structured_context_weighted(
                    &graph,
                    &seed_pairs,
                    target_budget,
                    &repo.file_sources,
                    task_ctx,
                );
                json_val["structured_context"] = serde_json::to_value(&structured_ctx).unwrap();
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
            if let Some(ref sens) = sensitivity_opt {
                prefix.push_str(&format!(
                    "<!-- Knapsack Sensitivity: Stability Index: {:.1}% ({} of {} stable), Epsilon: {:e}, Max Error Bound: {:.5} -->\n\n",
                    sens.stability_index * 100.0,
                    sens.stable_count,
                    sens.selected_count,
                    sens.epsilon,
                    sens.max_error_bound,
                ));
            }
            if let Some(ref mckp) = mckp_opt {
                let mut sig_c = 0;
                let mut doc_c = 0;
                let mut slice_c = 0;
                let mut full_c = 0;
                for &lod in mckp.selected_lods.values() {
                    match lod {
                        LodLevel::SignatureOnly => sig_c += 1,
                        LodLevel::SignatureAndDoc => doc_c += 1,
                        LodLevel::SlicedBody => slice_c += 1,
                        LodLevel::FullBody => full_c += 1,
                    }
                }
                prefix.push_str(&format!(
                    "<!-- Joint LOD (MCKP): Total Tokens: {}, Cumulative Utility: {:.3}, Levels: {} Signature, {} Sig+Doc, {} Sliced, {} Full -->\n\n",
                    mckp.total_tokens, mckp.cumulative_utility, sig_c, doc_c, slice_c, full_c
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

        let top_seeds = IntentResolver::resolve_query(&repo.symbols, task, 12);

        let mut seed_files = Vec::new();
        let mut seen_files = std::collections::HashSet::new();
        let mut symbol_targets = Vec::new();
        let mut seen_symbols = std::collections::HashSet::new();

        for (sym_id, confidence) in &top_seeds {
            if let Some(sym) = repo.symbols.iter().find(|s| s.id == *sym_id) {
                let file_str = sym.file_path.display().to_string().replace('\\', "/");
                if seen_files.insert(file_str.clone()) {
                    seed_files.push(file_str);
                }
                if seen_symbols.insert(sym.id) {
                    symbol_targets.push((sym.clone(), *confidence));
                }
            }
        }

        // Direct task keyword matching: ensure symbols and files directly matching task terms
        // (such as "tokens.rs", "estimate_tokens", "count_tokens", "TokenizerModel" for "token estimation")
        // are prioritized in the blueprint anchors.
        let task_lower = task.to_lowercase();
        let task_words: Vec<&str> = task_lower
            .split(|c: char| !c.is_alphanumeric() && c != '_')
            .filter(|w| w.len() >= 3)
            .collect();

        for sym in &repo.symbols {
            let file_str_norm = sym.file_path.display().to_string().replace('\\', "/");
            let file_lower = file_str_norm.to_lowercase();
            let name_lower = sym.name.to_lowercase();

            if file_lower.contains("/tests/")
                || file_lower.contains("/test_")
                || name_lower.starts_with("test_")
            {
                continue;
            }

            let matches_word = task_words
                .iter()
                .any(|&w| name_lower.contains(w) || file_lower.contains(w));
            if matches_word && !seen_symbols.contains(&sym.id) {
                let is_core = matches!(
                    sym.kind,
                    SymbolKind::Function
                        | SymbolKind::Struct
                        | SymbolKind::Enum
                        | SymbolKind::Trait
                        | SymbolKind::Method
                );
                if is_core {
                    if seen_files.insert(file_str_norm.clone()) {
                        seed_files.push(file_str_norm);
                    }
                    seen_symbols.insert(sym.id);
                    symbol_targets.push((sym.clone(), 0.90));
                    if symbol_targets.len() >= 20 {
                        break;
                    }
                }
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
                    sym.file_path.display().to_string().replace('\\', "/"),
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
        let resolution = args
            .get("resolution")
            .and_then(|r| r.as_f64())
            .unwrap_or(1.0);
        let report =
            ArchitectureReport::analyze_with_resolution(&graph, &repo.root_path, resolution);
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
        let tokenizer_str = args
            .get("tokenizer")
            .and_then(|t| t.as_str())
            .unwrap_or("fast");
        let tokenizer_model =
            TokenizerModel::from_str_name(tokenizer_str).unwrap_or(TokenizerModel::FastHeuristic);

        let report = if let Some(sym_name) = args.get("symbol").and_then(|s| s.as_str()) {
            ImpactAnalyzer::analyze_symbol_with_model(
                &graph,
                sym_name,
                budget,
                &repo.file_sources,
                tokenizer_model,
            )
        } else if let Some(rev) = args.get("diffAgainst").and_then(|d| d.as_str()) {
            match DiffResolver::get_git_diff_against(&target_path, rev) {
                Ok(diff_text) => ImpactAnalyzer::analyze_diff_with_model(
                    &graph,
                    &diff_text,
                    budget,
                    &repo.file_sources,
                    tokenizer_model,
                ),
                Err(e) => {
                    return ToolCallResult::error(format!(
                        "Failed to extract git diff against '{}': {}",
                        rev, e
                    ))
                }
            }
        } else {
            match DiffResolver::get_git_diff(&target_path) {
                Ok(diff_text) => ImpactAnalyzer::analyze_diff_with_model(
                    &graph,
                    &diff_text,
                    budget,
                    &repo.file_sources,
                    tokenizer_model,
                ),
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

    fn tool_mine_coedits(&mut self, args: serde_json::Value) -> ToolCallResult {
        let max_commits = args
            .get("maxCommits")
            .and_then(|v| v.as_u64())
            .unwrap_or(200) as usize;
        let min_support = args.get("minSupport").and_then(|v| v.as_u64()).unwrap_or(2) as usize;
        let format_str = args
            .get("format")
            .and_then(|f| f.as_str())
            .unwrap_or("markdown");

        let target_path = self.resolve_path(&args);
        let repo = match self.get_or_load_repo(&target_path) {
            Ok(r) => r,
            Err(e) => return ToolCallResult::error(e),
        };

        let head_hash = match GitCommitMiner::get_head_hash(&repo.root_path) {
            Ok(h) => h,
            Err(e) => return ToolCallResult::error(format!("Git error: {}", e)),
        };

        let cache_file = repo.root_path.join(".repotrim").join("coedit.bin");
        let config = CoeditConfig {
            max_commits,
            min_support,
            ..Default::default()
        };

        let coedit_graph = if let Some(cached) =
            CoeditCache::load_from_file(&cache_file, &head_hash)
        {
            cached
        } else {
            let mined =
                match GitCommitMiner::mine_repository(&repo.root_path, &repo.symbols, &config) {
                    Ok(m) => m,
                    Err(e) => return ToolCallResult::error(format!("Mining error: {}", e)),
                };
            let _ = CoeditCache::save_to_file(&cache_file, &mined);
            mined
        };

        let (learned_weights, report) = EdgeWeightLearner::learn_weights(
            &repo.symbols,
            &repo.edges,
            &repo.imports,
            &coedit_graph,
            LayerWeights::default(),
        );

        if format_str == "json" {
            let top_pairs: Vec<serde_json::Value> = coedit_graph
                .top_pairs(30)
                .into_iter()
                .map(|p| {
                    let s_sym = repo.symbols.iter().find(|s| s.id == p.source);
                    let t_sym = repo.symbols.iter().find(|s| s.id == p.target);
                    serde_json::json!({
                        "source": s_sym.map(|s| s.name.as_str()).unwrap_or("?"),
                        "source_file": s_sym.map(|s| s.file_path.to_string_lossy().to_string()).unwrap_or_default(),
                        "target": t_sym.map(|s| s.name.as_str()).unwrap_or("?"),
                        "target_file": t_sym.map(|s| s.file_path.to_string_lossy().to_string()).unwrap_or_default(),
                        "raw_count": p.raw_count,
                        "support": p.support,
                        "confidence": p.confidence,
                        "jaccard": p.jaccard,
                    })
                })
                .collect();

            let res = serde_json::json!({
                "head_hash": coedit_graph.head_hash,
                "commits_analyzed": coedit_graph.total_commits_analyzed,
                "valid_commits": coedit_graph.valid_commits,
                "megacommits_filtered": coedit_graph.megacommits_filtered,
                "total_coedit_pairs": coedit_graph.pairs.len(),
                "top_couplings": top_pairs,
                "layer_stats": report.layer_stats,
                "default_weights": report.default_weights,
                "learned_weights": learned_weights,
                "baseline_mrr": report.baseline_mrr,
                "learned_mrr": report.learned_mrr,
                "mrr_improvement_pct": report.mrr_improvement_pct,
            });
            ToolCallResult::success(serde_json::to_string_pretty(&res).unwrap())
        } else {
            let md = report.to_markdown();
            ToolCallResult::success(md)
        }
    }

    fn tool_detect_communities(&mut self, args: serde_json::Value) -> ToolCallResult {
        let target_path = self.resolve_path(&args);
        let resolution = args
            .get("resolution")
            .and_then(|r| r.as_f64())
            .unwrap_or(1.0);
        let hierarchy = args
            .get("hierarchy")
            .and_then(|h| h.as_bool())
            .unwrap_or(false);
        let drift = args.get("drift").and_then(|d| d.as_bool()).unwrap_or(false);
        let format_str = args
            .get("format")
            .and_then(|f| f.as_str())
            .unwrap_or("markdown");

        let repo = match self.get_or_load_repo(&target_path) {
            Ok(r) => r,
            Err(e) => return ToolCallResult::error(e),
        };

        let graph = repo.build_graph();

        if hierarchy {
            let hier = CommunityDetector::detect_hierarchy(&graph);
            if format_str == "json" {
                match serde_json::to_string_pretty(&hier) {
                    Ok(json_str) => ToolCallResult::success(json_str),
                    Err(e) => ToolCallResult::error(format!("JSON serialization error: {}", e)),
                }
            } else {
                let mut md = String::new();
                let _ = writeln!(md, "# Multi-Scale Community Hierarchy\n");
                let _ = writeln!(
                    md,
                    "## 1. Macro Subsystems (γ = 0.5)\n- Modularity: **{:.4}** | Communities: **{}**\n",
                    hier.macro_modularity,
                    hier.macro_communities.len()
                );
                let _ = writeln!(
                    md,
                    "## 2. Meso Modules (γ = 1.0)\n- Modularity: **{:.4}** | Communities: **{}**\n",
                    hier.meso_modularity,
                    hier.meso_communities.len()
                );
                let _ = writeln!(
                    md,
                    "## 3. Micro Components (γ = 2.5)\n- Modularity: **{:.4}** | Communities: **{}**\n",
                    hier.micro_modularity,
                    hier.micro_communities.len()
                );
                ToolCallResult::success(md)
            }
        } else {
            let config = CommunityConfig::with_resolution(resolution);
            let result = CommunityDetector::detect(&graph, &config);
            let drifts = if drift {
                Some(CommunityDetector::analyze_drift(
                    &graph,
                    &result.communities,
                    &repo.root_path,
                ))
            } else {
                None
            };

            if format_str == "json" {
                let json_val = serde_json::json!({
                    "resolution": result.resolution,
                    "modularity": result.modularity,
                    "total_symbols": graph.num_symbols(),
                    "community_count": result.communities.len(),
                    "communities": result.communities,
                    "drift": drifts,
                });
                match serde_json::to_string_pretty(&json_val) {
                    Ok(json_str) => ToolCallResult::success(json_str),
                    Err(e) => ToolCallResult::error(format!("JSON serialization error: {}", e)),
                }
            } else {
                let mut md = String::new();
                let _ = writeln!(
                    md,
                    "# Topological Community Catalog (γ = {:.2})\n",
                    result.resolution
                );
                let _ = writeln!(md, "- **Total Symbols:** {}", graph.num_symbols());
                let _ = writeln!(md, "- **Communities Found:** {}", result.communities.len());
                let _ = writeln!(md, "- **Modularity Q:** {:.4}\n", result.modularity);
                let _ = writeln!(
                    md,
                    "| ID | Name | Symbols | Tokens | Density | Dominant Dir | Purity | Key Symbols |"
                );
                let _ = writeln!(
                    md,
                    "| :---: | :--- | :---: | :---: | :---: | :--- | :---: | :--- |"
                );
                for c in &result.communities {
                    let _ = writeln!(
                        md,
                        "| {} | `{}` | {} | {} | {:.2} | `{}` | {:.0}% | {} |",
                        c.id,
                        c.name,
                        c.symbol_count,
                        c.total_tokens,
                        c.density,
                        c.dominant_directory
                            .display()
                            .to_string()
                            .replace('\\', "/"),
                        c.directory_purity * 100.0,
                        c.key_symbols.join(", ")
                    );
                }

                if let Some(ref d_list) = drifts {
                    let _ = writeln!(md, "\n---\n\n## Architectural Drift & Leaky Abstractions\n");
                    if d_list.is_empty() {
                        let _ = writeln!(md, "✓ No significant architectural drift detected.");
                    } else {
                        let _ = writeln!(
                            md,
                            "| Symbol | Kind | Declared Directory | Coupled Community | Drift Score |"
                        );
                        let _ = writeln!(md, "| :--- | :---: | :--- | :--- | :---: |");
                        for d in d_list {
                            let _ = writeln!(
                                md,
                                "| `{}` | `{:?}` | `{}` | `{}` | {:.1}% |",
                                d.symbol_name,
                                d.symbol_kind,
                                d.declared_directory
                                    .display()
                                    .to_string()
                                    .replace('\\', "/"),
                                d.community_name,
                                d.drift_score * 100.0
                            );
                        }
                    }
                }
                ToolCallResult::success(md)
            }
        }
    }

    fn tool_search_symbols(&mut self, args: serde_json::Value) -> ToolCallResult {
        let query = match args.get("query").and_then(|q| q.as_str()) {
            Some(q) if !q.trim().is_empty() => q.trim().to_string(),
            _ => return ToolCallResult::error("Missing or empty 'query' parameter"),
        };

        let target_path = self.resolve_path(&args);
        let repo = match self.get_or_load_repo(&target_path) {
            Ok(r) => r,
            Err(e) => return ToolCallResult::error(e),
        };

        let limit = args.get("limit").and_then(|l| l.as_u64()).unwrap_or(10) as usize;
        let mode_str = args
            .get("mode")
            .and_then(|m| m.as_str())
            .unwrap_or("hybrid");
        let search_mode = match mode_str.to_lowercase().as_str() {
            "lexical" | "bm25" => SearchMode::LexicalOnly,
            "dense" | "semantic" => SearchMode::DenseOnly,
            _ => SearchMode::Hybrid,
        };
        let expand = args
            .get("expand")
            .and_then(|e| e.as_bool())
            .unwrap_or(false);
        let format_str = args
            .get("format")
            .and_then(|f| f.as_str())
            .unwrap_or("markdown");

        let ret_config = RetrievalConfig {
            mode: search_mode,
            top_k: limit,
            query_expansion: expand,
            ..Default::default()
        };

        let retriever = HybridRetriever::with_config(ret_config);
        let results = retriever.search(&repo.symbols, &query);

        if format_str == "json" {
            let json_val = serde_json::json!({
                "query": query,
                "mode": format!("{:?}", search_mode).to_lowercase(),
                "expanded": expand,
                "total_results": results.len(),
                "results": results.iter().map(|r| {
                    let sym = &repo.symbols[r.symbol_id.0 as usize];
                    serde_json::json!({
                        "id": r.symbol_id.0,
                        "name": sym.name,
                        "kind": format!("{:?}", sym.kind),
                        "file": sym.file_path.display().to_string().replace('\\', "/"),
                        "lines": [sym.span.start_row + 1, sym.span.end_row + 1],
                        "score": r.score,
                        "bm25_score": r.bm25_score,
                        "dense_score": r.dense_score,
                        "rrf_score": r.rrf_score,
                        "matched_terms": r.matched_terms,
                    })
                }).collect::<Vec<_>>(),
            });
            match serde_json::to_string_pretty(&json_val) {
                Ok(json_str) => ToolCallResult::success(json_str),
                Err(e) => ToolCallResult::error(format!("JSON serialization error: {}", e)),
            }
        } else {
            let mut md = String::new();
            let _ = writeln!(md, "# Symbol Retrieval Results for `{}`\n", query);
            let _ = writeln!(md, "- **Mode:** {:?}", search_mode);
            let _ = writeln!(md, "- **Pseudo-Relevance Expansion:** {}", expand);
            let _ = writeln!(md, "- **Matches:** {}\n", results.len());

            if results.is_empty() {
                let _ = writeln!(md, "No matching symbols found.");
            } else {
                let _ = writeln!(
                    md,
                    "| Rank | Symbol | Kind | File | Score | BM25+ | Dense | RRF | Matches |"
                );
                let _ = writeln!(
                    md,
                    "| :---: | :--- | :---: | :--- | :---: | :---: | :---: | :---: | :--- |"
                );
                for (i, r) in results.iter().enumerate() {
                    let sym = &repo.symbols[r.symbol_id.0 as usize];
                    let file_str = sym.file_path.display().to_string().replace('\\', "/");
                    let matches_str = if r.matched_terms.is_empty() {
                        "-".to_string()
                    } else {
                        r.matched_terms.join(", ")
                    };
                    let _ = writeln!(
                        md,
                        "| {} | `{}` | `{:?}` | `{}:{}` | {:.4} | {:.2} | {:.4} | {:.4} | {} |",
                        i + 1,
                        sym.name,
                        sym.kind,
                        file_str,
                        sym.span.start_row + 1,
                        r.score,
                        r.bm25_score,
                        r.dense_score,
                        r.rrf_score,
                        matches_str,
                    );
                }
            }
            ToolCallResult::success(md)
        }
    }

    fn tool_run_benchmark(&mut self, args: serde_json::Value) -> ToolCallResult {
        let path = self.resolve_path(&args);
        let mut repo = match self.get_or_load_repo(&path) {
            Ok(r) => r,
            Err(e) => return ToolCallResult::error(format!("Failed to load repository: {}", e)),
        };
        let _ = repo.load_all_sources();
        let mut runner = BenchmarkRunner::new();

        if let Some(scenario_filter) = args.get("scenario").and_then(|s| s.as_str()) {
            let filter_lower = scenario_filter.to_lowercase();
            runner.scenarios.retain(|s| {
                s.id.to_lowercase().contains(&filter_lower)
                    || s.name.to_lowercase().contains(&filter_lower)
            });
            if runner.scenarios.is_empty() {
                return ToolCallResult::error(format!(
                    "No benchmark scenarios matched filter: '{}'",
                    scenario_filter
                ));
            }
        }

        if let Some(budget) = args.get("budget").and_then(|b| b.as_u64()) {
            for s in &mut runner.scenarios {
                s.budget = budget as usize;
            }
        }

        let strategies = if let Some(strat_arr) = args.get("strategies").and_then(|s| s.as_array())
        {
            let mut list = Vec::new();
            for item in strat_arr {
                if let Some(name) = item.as_str() {
                    match name.to_lowercase().as_str() {
                        "whole_file" => list.push(ContextStrategy::WholeFile),
                        "naive_grep" => list.push(ContextStrategy::NaiveGrep),
                        "aider_repo_map" => list.push(ContextStrategy::AiderRepoMap),
                        "repo_trim_vanilla" => list.push(ContextStrategy::RepoTrimVanilla),
                        "repo_trim_full" => list.push(ContextStrategy::RepoTrimFull),
                        other => {
                            return ToolCallResult::error(format!("Unknown strategy: '{}'", other))
                        }
                    }
                }
            }
            if list.is_empty() {
                ContextStrategy::all().to_vec()
            } else {
                list.dedup();
                list
            }
        } else {
            ContextStrategy::all().to_vec()
        };

        let mut all_metrics = Vec::new();
        for scenario in &runner.scenarios {
            let metrics = runner.evaluate_scenario(&repo, scenario, &strategies);
            all_metrics.extend(metrics);
        }

        let summary = BenchmarkSummary::summarize(&all_metrics);
        let format = args
            .get("format")
            .and_then(|f| f.as_str())
            .unwrap_or("markdown");

        if format.eq_ignore_ascii_case("json") {
            let json_val = serde_json::json!({
                "repository": path.display().to_string(),
                "scenarios_evaluated": runner.scenarios.len(),
                "strategies_evaluated": strategies,
                "metrics": all_metrics,
                "summary": summary,
            });
            match serde_json::to_string_pretty(&json_val) {
                Ok(s) => ToolCallResult::success(s),
                Err(e) => ToolCallResult::error(format!("JSON serialization error: {}", e)),
            }
        } else {
            let mut md = String::new();
            let _ = writeln!(md, "# RepoTrim Empirical Benchmark Report\n");
            let _ = writeln!(md, "- **Repository:** `{}`", path.display());
            let _ = writeln!(md, "- **Scenarios Evaluated:** {}", runner.scenarios.len());
            let _ = writeln!(md, "- **Strategies Evaluated:** {}\n", strategies.len());

            for scenario in &runner.scenarios {
                let scenario_metrics: Vec<&BenchmarkMetrics> = all_metrics
                    .iter()
                    .filter(|m| m.scenario == scenario.name)
                    .collect();

                let _ = writeln!(md, "## Scenario: {}", scenario.name);
                let _ = writeln!(md, "- **Budget:** {} tokens", scenario.budget);
                if !scenario.seeds.is_empty() {
                    let _ = writeln!(md, "- **Seeds:** `{}`", scenario.seeds.join("`, `"));
                }
                if let Some(ref q) = scenario.query {
                    let _ = writeln!(md, "- **Query:** \"{}\"", q);
                }
                let _ = writeln!(md, "- **Description:** {}\n", scenario.description);

                let _ = writeln!(
                    md,
                    "| Strategy | Tokens | Reduction | Direct Recall | Transitive Recall | Precision | Cohesion | Orphans | Symbols | Latency |"
                );
                let _ = writeln!(
                    md,
                    "| :--- | :---: | :---: | :---: | :---: | :---: | :---: | :---: | :---: | :---: |"
                );
                for m in &scenario_metrics {
                    let latency_display = if m.execution_latency_us >= 1000 {
                        format!("{:.2}ms", m.execution_latency_us as f64 / 1000.0)
                    } else {
                        format!("{}µs", m.execution_latency_us)
                    };
                    let strategy_str = if m.strategy == ContextStrategy::RepoTrimFull {
                        format!("**{}**", m.strategy_name)
                    } else {
                        m.strategy_name.clone()
                    };
                    let _ = writeln!(
                        md,
                        "| {} | {} | {:.1}% | {:.1}% | {:.1}% | {:.1}% | {:.1}% | {:.1}% | {} | {} |",
                        strategy_str,
                        m.tokens_used,
                        m.token_reduction_pct,
                        m.direct_dep_recall_pct,
                        m.transitive_dep_recall_pct,
                        m.context_precision_pct,
                        m.community_cohesion_pct,
                        m.orphan_rate_pct,
                        m.symbol_count,
                        latency_display
                    );
                }
                let _ = writeln!(md);
            }

            let _ = writeln!(md, "## Aggregate Benchmark Summary\n");
            let _ = writeln!(
                md,
                "| Strategy | Mean Tokens | Token Reduction | Direct Recall | Transitive Recall | Precision | Cohesion | Orphans | Mean Latency |"
            );
            let _ = writeln!(
                md,
                "| :--- | :---: | :---: | :---: | :---: | :---: | :---: | :---: | :---: |"
            );
            for s in &summary {
                let latency_display = if s.mean_latency_us >= 1000 {
                    format!("{:.2}ms", s.mean_latency_us as f64 / 1000.0)
                } else {
                    format!("{}µs", s.mean_latency_us)
                };
                let strategy_str = if s.strategy == ContextStrategy::RepoTrimFull {
                    format!("**{}**", s.strategy_name)
                } else {
                    s.strategy_name.clone()
                };
                let _ = writeln!(
                    md,
                    "| {} | {} | {:.1}% | {:.1}% | {:.1}% | {:.1}% | {:.1}% | {:.1}% | {} |",
                    strategy_str,
                    s.mean_tokens,
                    s.mean_token_reduction_pct,
                    s.mean_direct_recall_pct,
                    s.mean_transitive_recall_pct,
                    s.mean_precision_pct,
                    s.mean_cohesion_pct,
                    s.mean_orphan_rate_pct,
                    latency_display
                );
            }

            ToolCallResult::success(md)
        }
    }

    fn tool_locate_entrypoints(&mut self, args: serde_json::Value) -> ToolCallResult {
        let query = match args.get("query").and_then(|q| q.as_str()) {
            Some(q) => q,
            None => return ToolCallResult::error("Missing required parameter 'query'"),
        };

        let target_path = self.resolve_path(&args);
        let repo = match self.get_or_load_repo(&target_path) {
            Ok(r) => r,
            Err(e) => return ToolCallResult::error(e),
        };

        let limit = args.get("limit").and_then(|l| l.as_u64()).unwrap_or(10) as usize;
        let format_str = args
            .get("format")
            .and_then(|f| f.as_str())
            .unwrap_or("markdown");

        let graph = repo.build_graph();
        let intel = repo.intelligence(&graph);

        let mut task = TaskContext::from_query(query);
        if let Some(target_files) = args.get("targetFiles").and_then(|t| t.as_array()) {
            let files: Vec<PathBuf> = target_files
                .iter()
                .filter_map(|f| f.as_str().map(PathBuf::from))
                .collect();
            if !files.is_empty() {
                task.metadata.target_files = files;
            }
        }

        let entrypoints = intel.locate(&task, limit);

        if format_str == "json" {
            ToolCallResult::success(serde_json::to_string_pretty(&entrypoints).unwrap())
        } else {
            let mut out = String::new();
            let _ = writeln!(out, "### Task Entrypoints for: `{}`\n", query);
            if entrypoints.is_empty() {
                out.push_str("No entrypoint candidates discovered.\n");
            } else {
                for (idx, entry) in entrypoints.iter().enumerate() {
                    let _ = writeln!(
                        out,
                        "{}. **`{}`** ({:?}) — Score: `{:.2}`",
                        idx + 1,
                        entry.symbol.name,
                        entry.symbol.kind,
                        entry.score
                    );
                    let _ = writeln!(
                        out,
                        "   - Location: `{}:L{}-L{}`",
                        entry.symbol.file_path.display(),
                        entry.symbol.span.start_row + 1,
                        entry.symbol.span.end_row + 1
                    );
                    let _ = writeln!(out, "   - Attribution: {}", entry.reason);
                    if !entry.symbol.signature.trim().is_empty() {
                        let _ =
                            writeln!(out, "   - Signature: `{}`", entry.symbol.signature.trim());
                    }
                    out.push('\n');
                }
            }
            ToolCallResult::success(out)
        }
    }

    fn tool_trace_paths(&mut self, args: serde_json::Value) -> ToolCallResult {
        let source = match args.get("source").and_then(|s| s.as_str()) {
            Some(s) => s,
            None => return ToolCallResult::error("Missing required parameter 'source'"),
        };
        let target = match args.get("target").and_then(|t| t.as_str()) {
            Some(t) => t,
            None => return ToolCallResult::error("Missing required parameter 'target'"),
        };

        let target_path = self.resolve_path(&args);
        let repo = match self.get_or_load_repo(&target_path) {
            Ok(r) => r,
            Err(e) => return ToolCallResult::error(e),
        };

        let task_query = args.get("task").and_then(|t| t.as_str());
        let include_mermaid = args
            .get("includeMermaid")
            .and_then(|m| m.as_bool())
            .unwrap_or(true);
        let format_str = args
            .get("format")
            .and_then(|f| f.as_str())
            .unwrap_or("markdown");

        let graph = repo.build_graph();
        let symbols = graph.symbols();

        let src_sym = match symbols.iter().find(|s| s.name.eq_ignore_ascii_case(source)) {
            Some(s) => s,
            None => {
                return ToolCallResult::error(format!(
                    "Source symbol '{}' was not found in codebase",
                    source
                ))
            }
        };

        let tgt_sym = match symbols.iter().find(|s| s.name.eq_ignore_ascii_case(target)) {
            Some(s) => s,
            None => {
                return ToolCallResult::error(format!(
                    "Target symbol '{}' was not found in codebase",
                    target
                ))
            }
        };

        let intel = repo.intelligence(&graph);
        let task_ctx = task_query.map(TaskContext::from_query);

        let trace_result = match intel.trace(src_sym.id, tgt_sym.id, task_ctx.as_ref()) {
            Ok(r) => r,
            Err(e) => return ToolCallResult::error(format!("Trace failed: {}", e)),
        };

        if format_str == "json" {
            ToolCallResult::success(serde_json::to_string_pretty(&trace_result).unwrap())
        } else {
            let mut out = String::new();
            let _ = writeln!(
                out,
                "### Causal Path Trace: `{}` -> `{}`\n",
                trace_result.source.name, trace_result.target.name
            );
            let _ = writeln!(
                out,
                "- **Source:** `{}` ({}:L{})",
                trace_result.source.name,
                trace_result.source.file_path.display(),
                trace_result.source.span.start_row + 1
            );
            let _ = writeln!(
                out,
                "- **Target:** `{}` ({}:L{})",
                trace_result.target.name,
                trace_result.target.file_path.display(),
                trace_result.target.span.start_row + 1
            );
            let _ = writeln!(
                out,
                "- **Discovered Paths:** {}\n",
                trace_result.paths.len()
            );

            if let Some(ref best) = trace_result.best_path {
                let _ = writeln!(
                    out,
                    "#### Best Causal Path (Probability: {:.1}%, Energy Score: {:.2})\n",
                    best.probability * 100.0,
                    best.score
                );

                let mut step_strs = Vec::new();
                for &node_id in &best.nodes {
                    let name = symbols
                        .get(node_id.0 as usize)
                        .map(|s| s.name.as_str())
                        .unwrap_or("Unknown");
                    step_strs.push(name);
                }

                let mut chain = String::new();
                for (i, name) in step_strs.iter().enumerate() {
                    if i > 0 {
                        let rel = best
                            .relations
                            .get(i - 1)
                            .copied()
                            .unwrap_or(repotrim_engine::RelationType::Calls);
                        chain.push_str(&format!(" --[{:?}]--> ", rel));
                    }
                    chain.push_str(&format!("`{}`", name));
                }
                let _ = writeln!(out, "{}\n", chain);
            }

            if include_mermaid && !trace_result.mermaid_diagram.is_empty() {
                out.push_str("#### Sequence Diagram\n\n");
                out.push_str(&trace_result.mermaid_diagram);
                out.push('\n');
            }

            ToolCallResult::success(out)
        }
    }

    fn tool_expand_symbol(&mut self, args: serde_json::Value) -> ToolCallResult {
        let symbol = match args.get("symbol").and_then(|s| s.as_str()) {
            Some(s) => s,
            None => return ToolCallResult::error("Missing required parameter 'symbol'"),
        };

        let target_path = self.resolve_path(&args);
        let repo = match self.get_or_load_repo(&target_path) {
            Ok(r) => r,
            Err(e) => return ToolCallResult::error(e),
        };

        let budget: usize = match args.get("budget") {
            Some(serde_json::Value::Number(n)) => n.as_u64().unwrap_or(1000) as usize,
            Some(serde_json::Value::String(s)) => s.parse::<usize>().unwrap_or(1000),
            _ => 1000,
        };

        let format_str = args
            .get("format")
            .and_then(|f| f.as_str())
            .unwrap_or("markdown");

        let graph = repo.build_graph();
        let symbols = graph.symbols();

        let focal_sym = match symbols.iter().find(|s| s.name.eq_ignore_ascii_case(symbol)) {
            Some(s) => s,
            None => {
                return ToolCallResult::error(format!(
                    "Focal symbol '{}' was not found in codebase",
                    symbol
                ))
            }
        };

        let intel = repo.intelligence(&graph);
        let expansion = match intel.expand(focal_sym.id, budget, None) {
            Ok(e) => e,
            Err(err) => return ToolCallResult::error(format!("Expansion failed: {}", err)),
        };

        if format_str == "json" {
            ToolCallResult::success(serde_json::to_string_pretty(&expansion).unwrap())
        } else {
            let mut out = String::new();
            let _ = writeln!(
                out,
                "### Local Submodular Context Expansion: `{}`\n",
                expansion.focal_symbol.name
            );
            let _ = writeln!(
                out,
                "- **Focal Symbol:** `{}` ({}:L{})",
                expansion.focal_symbol.name,
                expansion.focal_symbol.file_path.display(),
                expansion.focal_symbol.span.start_row + 1
            );
            let _ = writeln!(
                out,
                "- **Budget:** {} tokens | **Tokens Used:** {} ({:.1}%) | **Cluster:** {} symbols\n",
                expansion.budget,
                expansion.tokens_used,
                (expansion.tokens_used as f64 / expansion.budget.max(1) as f64) * 100.0,
                expansion.symbols.len()
            );

            if !expansion.paths.is_empty() {
                out.push_str("#### Causal Paths to Cluster Members\n\n");
                for (i, path) in expansion.paths.iter().take(5).enumerate() {
                    let mut step_strs = Vec::new();
                    for &node_id in &path.nodes {
                        let name = symbols
                            .get(node_id.0 as usize)
                            .map(|s| s.name.as_str())
                            .unwrap_or("Unknown");
                        step_strs.push(name);
                    }
                    let _ = writeln!(
                        out,
                        "{}. {} (Prob: {:.1}%)",
                        i + 1,
                        step_strs.join(" -> "),
                        path.probability * 100.0
                    );
                }
                out.push('\n');
            }

            out.push_str("#### Source Context Code\n\n");
            out.push_str(&expansion.formatted_code);
            out.push('\n');

            ToolCallResult::success(out)
        }
    }
}
