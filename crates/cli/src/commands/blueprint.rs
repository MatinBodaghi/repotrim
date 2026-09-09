use clap::Args;
use colored::Colorize;
use repotrim_engine::IntentResolver;
use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;
use std::time::Instant;

use crate::loader::LoadedRepository;

#[derive(Args, Debug)]
pub struct BlueprintArgs {
    /// Task or feature description to scaffold (e.g. "Add user session authentication")
    pub task: String,

    /// Target codebase directory to scan
    #[arg(short = 'p', long = "path", default_value = ".")]
    pub path: PathBuf,

    /// Recommended token budget for context slicing, or 'auto' for Knee-Curve tuning
    #[arg(short = 'b', long = "budget", default_value = "3000")]
    pub budget: String,

    /// Target LLM architecture for auto-budgeting presets (e.g. 'claude', 'gpt-4o', 'deepseek', 'ollama')
    #[arg(short = 'm', long = "model")]
    pub model: Option<String>,

    /// Destination file to write blueprint (defaults to 'FEATURE_BLUEPRINT.md', or '-' for stdout)
    #[arg(short = 'o', long = "output", default_value = "FEATURE_BLUEPRINT.md")]
    pub output: String,

    /// Disable incremental AST caching and force full re-parsing
    #[arg(long = "no-cache")]
    pub no_cache: bool,
}

pub fn execute(args: BlueprintArgs) -> Result<(), Box<dyn std::error::Error>> {
    let start_time = Instant::now();

    eprintln!(
        "{} Analyzing codebase for task '{}'...",
        "⚙".cyan().bold(),
        args.task.bold()
    );

    let mut repo = LoadedRepository::load_with_options(&args.path, !args.no_cache)?;
    repo.load_all_sources()?;

    let top_seeds = IntentResolver::resolve_query(&repo.symbols, &args.task, 6);

    let mut seed_files = Vec::new();
    let mut seen_files = HashSet::new();
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

    let blueprint_content = generate_blueprint_text(
        &args.task,
        &seed_files,
        &symbol_targets,
        &args.budget,
        args.model.as_deref(),
    );

    if args.output == "-" {
        println!("{}", blueprint_content);
    } else {
        let out_path = PathBuf::from(&args.output);
        fs::write(&out_path, &blueprint_content)?;
        eprintln!(
            "{} Successfully generated feature blueprint in '{}' ({:?})",
            "✓".green().bold(),
            out_path.display().to_string().bold(),
            start_time.elapsed()
        );
        eprintln!(
            "   Inferred {} seed symbols across {} primary target files.",
            symbol_targets.len().to_string().bold(),
            seed_files.len().to_string().bold()
        );
    }

    Ok(())
}

/// Formats a complete, high-density Markdown feature blueprint.
pub fn generate_blueprint_text(
    task: &str,
    seed_files: &[String],
    symbol_targets: &[(repotrim_engine::SymbolNode, f32)],
    budget: &str,
    model: Option<&str>,
) -> String {
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
        for f in seed_files {
            doc.push_str(&format!("  - `{}`\n", f));
        }
    }
    doc.push_str("- **Expected Outcome:** Functional implementation passing all unit and integration tests with zero compiler or linter warnings.\n\n");

    doc.push_str("---\n\n");
    doc.push_str("## 2. Seed Anchors (for RepoTrim Context Slicing)\n");
    doc.push_str("RepoTrim uses these anchors to compute Forward-Push Personalized PageRank and pack the optimal dependency skeleton:\n\n");

    doc.push_str("### Key Symbol Targets\n");
    if symbol_targets.is_empty() {
        doc.push_str("- *(No direct symbol matches found; use `--query` for broad context)*\n");
    } else {
        for (sym, conf) in symbol_targets {
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

    let primary_seed_name = symbol_targets
        .first()
        .map(|(s, _)| s.name.as_str())
        .unwrap_or("");

    let model_flag = model.map(|m| format!(" --model {}", m)).unwrap_or_default();

    let cli_cmd = if !primary_seed_name.is_empty() {
        format!(
            "repotrim select --seed {} --budget {}{}\n# OR\nrepotrim select --query \"{}\" --budget {}{}",
            primary_seed_name, budget, model_flag, task, budget, model_flag
        )
    } else {
        format!(
            "repotrim select --query \"{}\" --budget {}{}",
            task, budget, model_flag
        )
    };

    doc.push_str(&format!("```bash\n{}\n```\n\n", cli_cmd));
    doc.push_str("Or via MCP tool:\n");

    let mcp_budget_val = if let Ok(b) = budget.parse::<usize>() {
        serde_json::json!(b)
    } else {
        serde_json::json!(budget)
    };
    let mut mcp_args = serde_json::json!({
        "query": task,
        "budget": mcp_budget_val
    });
    if let Some(m) = model {
        mcp_args["model"] = serde_json::json!(m);
    }
    doc.push_str(&format!(
        "```json\n{{\n  \"name\": \"trim_context\",\n  \"arguments\": {}\n}}\n```\n\n",
        serde_json::to_string_pretty(&mcp_args).unwrap_or_default()
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

    doc
}
