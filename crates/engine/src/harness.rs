//! Agent Harness Exploration Measurement & Trace Recording Engine.
//!
//! Provides [`AgentTraceRecorder`], [`AgentSessionTrace`], and [`HarnessBenchmarkRunner`]
//! for recording live and simulated AI coding agent exploration sessions (e.g. Claude Code,
//! Antigravity, SWE-bench style harnesses), tracking prompt token consumption, tool call counts,
//! and task completion latency with and without RepoTrim codebase intelligence.
//!
//! # Theoretical Foundations & Evaluation Metrics
//! - **Exploration Cost Reduction (ECR%)**:
//!   $$\text{ECR} = \left(1 - \frac{\text{Tokens}_{\text{RepoTrim}}}{\text{Tokens}_{\text{Baseline}}}\right) \times 100\%$$
//! - **Information Density (SPT - Symbols Per Thousand Tokens)**:
//!   $$\text{SPT} = \frac{|S|}{\text{Tokens Consumed}} \times 1000$$
//! - **Tool Call Reduction**:
//!   $$\text{TCR} = \left(1 - \frac{\text{Calls}_{\text{RepoTrim}}}{\text{Calls}_{\text{Baseline}}}\right) \times 100\%$$

use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::Path;
use std::time::Instant;

use crate::loader::LoadedRepository;
use crate::navigation::{AdaptiveNavigator, NavigatorConfig};
use crate::primitives::CodebaseIntelligence;
use crate::task::TaskContext;
use crate::tokens::{count_tokens, TokenizerModel};

/// Execution status of an individual agent tool call.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum InvocationStatus {
    /// Tool invocation executed cleanly.
    Success,
    /// Tool invocation failed with an error description.
    Error(String),
}

/// Record of an individual tool invocation performed during an agent exploration session.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentToolInvocation {
    /// Name of the invoked tool (e.g. `locate_entrypoints`, `read_file`, `trim_context`).
    pub tool_name: String,
    /// Human-readable or JSON summary of input arguments.
    pub arguments_summary: String,
    /// Prompt tokens consumed by this tool call.
    pub input_tokens: usize,
    /// Tokens yielded in the tool call output.
    pub output_tokens: usize,
    /// Realized execution latency in microseconds.
    pub execution_latency_us: u128,
    /// Distinct symbol identifiers or names discovered/inspected during this invocation.
    pub symbols_referenced: Vec<String>,
    /// Distinct file paths referenced or inspected during this invocation.
    pub files_referenced: Vec<String>,
    /// Relative timestamp offset in milliseconds from session start.
    pub timestamp_ms: u64,
    /// Status outcome of the invocation.
    pub status: InvocationStatus,
}

/// Complete trace of an agent exploration session / episode.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentSessionTrace {
    /// Unique identifier for the agent session or benchmark trial.
    pub session_id: String,
    /// Harness environment identifier (e.g. `"antigravity"`, `"claude_code"`, `"swe_bench"`).
    pub harness_type: String,
    /// High-level user goal, prompt, or issue description.
    pub task_goal: String,
    /// Flag indicating whether RepoTrim codebase intelligence was active.
    pub with_repotrim: bool,
    /// Chronological sequence of tool invocations.
    pub invocations: Vec<AgentToolInvocation>,
    /// Total input/prompt tokens consumed across all steps.
    pub total_input_tokens: usize,
    /// Total output tokens returned across all steps.
    pub total_output_tokens: usize,
    /// Total wall-clock elapsed duration in milliseconds.
    pub total_duration_ms: u64,
    /// Flag indicating whether the agent successfully gathered required task context.
    pub task_success: bool,
    /// Number of unique files accessed during exploration.
    pub unique_files_accessed: usize,
    /// Number of unique symbols discovered or inspected.
    pub unique_symbols_accessed: usize,
    /// Information density: Symbols Per Thousand Tokens (SPT).
    pub information_density_spt: f32,
}

impl AgentSessionTrace {
    /// Total tokens consumed (input + output) across the session.
    pub fn total_tokens(&self) -> usize {
        self.total_input_tokens + self.total_output_tokens
    }

    /// Total number of tool invocations performed.
    pub fn tool_call_count(&self) -> usize {
        self.invocations.len()
    }

    /// Exports the session trace as pretty-printed JSON.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    /// Parses an `AgentSessionTrace` from a JSON string.
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }
}

/// Live recorder tracking tool calls, token spend, and latency during an active agent session.
#[derive(Debug)]
pub struct AgentTraceRecorder {
    session_id: String,
    harness_type: String,
    task_goal: String,
    with_repotrim: bool,
    start_time: Instant,
    invocations: Vec<AgentToolInvocation>,
    seen_files: HashSet<String>,
    seen_symbols: HashSet<String>,
    total_input_tokens: usize,
    total_output_tokens: usize,
}

impl AgentTraceRecorder {
    /// Creates a new trace recorder for a given agent task episode.
    pub fn new(
        session_id: impl Into<String>,
        harness_type: impl Into<String>,
        task_goal: impl Into<String>,
        with_repotrim: bool,
    ) -> Self {
        Self {
            session_id: session_id.into(),
            harness_type: harness_type.into(),
            task_goal: task_goal.into(),
            with_repotrim,
            start_time: Instant::now(),
            invocations: Vec::new(),
            seen_files: HashSet::new(),
            seen_symbols: HashSet::new(),
            total_input_tokens: 0,
            total_output_tokens: 0,
        }
    }

    /// Records an arbitrary tool invocation event into the trace.
    pub fn record_invocation(&mut self, invocation: AgentToolInvocation) {
        self.total_input_tokens += invocation.input_tokens;
        self.total_output_tokens += invocation.output_tokens;
        for f in &invocation.files_referenced {
            self.seen_files.insert(f.clone());
        }
        for s in &invocation.symbols_referenced {
            self.seen_symbols.insert(s.clone());
        }
        self.invocations.push(invocation);
    }

    /// Convenience helper to record a tool call with specific metadata.
    #[allow(clippy::too_many_arguments)]
    pub fn record_tool_call(
        &mut self,
        tool_name: impl Into<String>,
        arguments_summary: impl Into<String>,
        input_tokens: usize,
        output_tokens: usize,
        latency_us: u128,
        symbols: &[String],
        files: &[String],
        status: InvocationStatus,
    ) {
        let timestamp_ms = self.start_time.elapsed().as_millis() as u64;
        self.record_invocation(AgentToolInvocation {
            tool_name: tool_name.into(),
            arguments_summary: arguments_summary.into(),
            input_tokens,
            output_tokens,
            execution_latency_us: latency_us,
            symbols_referenced: symbols.to_vec(),
            files_referenced: files.to_vec(),
            timestamp_ms,
            status,
        });
    }

    /// Finalizes the trace session, computing aggregate metrics and information density.
    pub fn finalize(self, task_success: bool) -> AgentSessionTrace {
        let total_duration_ms = self.start_time.elapsed().as_millis() as u64;
        let total_tokens = self.total_input_tokens + self.total_output_tokens;
        let unique_symbols = self.seen_symbols.len();
        let information_density_spt = if total_tokens > 0 {
            (unique_symbols as f32 / total_tokens as f32) * 1000.0
        } else {
            0.0
        };

        AgentSessionTrace {
            session_id: self.session_id,
            harness_type: self.harness_type,
            task_goal: self.task_goal,
            with_repotrim: self.with_repotrim,
            invocations: self.invocations,
            total_input_tokens: self.total_input_tokens,
            total_output_tokens: self.total_output_tokens,
            total_duration_ms,
            task_success,
            unique_files_accessed: self.seen_files.len(),
            unique_symbols_accessed: unique_symbols,
            information_density_spt,
        }
    }

    /// Saves the finalized trace to a JSON file on disk.
    pub fn save_to_file(&self, path: &Path, task_success: bool) -> std::io::Result<()> {
        let trace = self.clone_as_trace(task_success);
        let json = serde_json::to_string_pretty(&trace)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, json)
    }

    fn clone_as_trace(&self, task_success: bool) -> AgentSessionTrace {
        let total_duration_ms = self.start_time.elapsed().as_millis() as u64;
        let total_tokens = self.total_input_tokens + self.total_output_tokens;
        let unique_symbols = self.seen_symbols.len();
        let information_density_spt = if total_tokens > 0 {
            (unique_symbols as f32 / total_tokens as f32) * 1000.0
        } else {
            0.0
        };

        AgentSessionTrace {
            session_id: self.session_id.clone(),
            harness_type: self.harness_type.clone(),
            task_goal: self.task_goal.clone(),
            with_repotrim: self.with_repotrim,
            invocations: self.invocations.clone(),
            total_input_tokens: self.total_input_tokens,
            total_output_tokens: self.total_output_tokens,
            total_duration_ms,
            task_success,
            unique_files_accessed: self.seen_files.len(),
            unique_symbols_accessed: unique_symbols,
            information_density_spt,
        }
    }
}

/// Pairwise comparative evaluation between a Baseline agent session and a RepoTrim-assisted session.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HarnessComparison {
    /// Scenario name or identifier.
    pub scenario_name: String,
    /// Baseline total token spend.
    pub baseline_tokens: usize,
    /// RepoTrim total token spend.
    pub repotrim_tokens: usize,
    /// Token spend reduction percentage (ECR%).
    pub token_reduction_pct: f32,
    /// Baseline tool call count.
    pub baseline_tool_calls: usize,
    /// RepoTrim tool call count.
    pub repotrim_tool_calls: usize,
    /// Tool call count reduction percentage.
    pub tool_call_reduction_pct: f32,
    /// Baseline total duration in milliseconds.
    pub baseline_duration_ms: u64,
    /// RepoTrim total duration in milliseconds.
    pub repotrim_duration_ms: u64,
    /// Wall-clock latency speedup factor.
    pub latency_speedup: f32,
    /// Baseline information density (SPT).
    pub baseline_spt: f32,
    /// RepoTrim information density (SPT).
    pub repotrim_spt: f32,
    /// Information density multiplier ($\text{SPT}_{\text{repotrim}} / \text{SPT}_{\text{baseline}}$).
    pub spt_multiplier: f32,
    /// Baseline task completion outcome.
    pub baseline_success: bool,
    /// RepoTrim task completion outcome.
    pub repotrim_success: bool,
}

impl HarnessComparison {
    /// Constructs a pairwise comparison from baseline and RepoTrim session traces.
    pub fn compare(
        scenario_name: impl Into<String>,
        baseline: &AgentSessionTrace,
        repotrim: &AgentSessionTrace,
    ) -> Self {
        let baseline_tokens = baseline.total_tokens();
        let repotrim_tokens = repotrim.total_tokens();

        let token_reduction_pct = if baseline_tokens > 0 {
            (1.0 - (repotrim_tokens as f32 / baseline_tokens as f32)).max(0.0) * 100.0
        } else {
            0.0
        };

        let baseline_tool_calls = baseline.tool_call_count();
        let repotrim_tool_calls = repotrim.tool_call_count();

        let tool_call_reduction_pct = if baseline_tool_calls > 0 {
            (1.0 - (repotrim_tool_calls as f32 / baseline_tool_calls as f32)).max(0.0) * 100.0
        } else {
            0.0
        };

        let baseline_duration_ms = baseline.total_duration_ms;
        let repotrim_duration_ms = repotrim.total_duration_ms;

        let latency_speedup = if repotrim_duration_ms > 0 {
            baseline_duration_ms as f32 / repotrim_duration_ms as f32
        } else {
            1.0
        };

        let baseline_spt = baseline.information_density_spt;
        let repotrim_spt = repotrim.information_density_spt;

        let spt_multiplier = if baseline_spt > 0.0 {
            repotrim_spt / baseline_spt
        } else {
            1.0
        };

        Self {
            scenario_name: scenario_name.into(),
            baseline_tokens,
            repotrim_tokens,
            token_reduction_pct,
            baseline_tool_calls,
            repotrim_tool_calls,
            tool_call_reduction_pct,
            baseline_duration_ms,
            repotrim_duration_ms,
            latency_speedup,
            baseline_spt,
            repotrim_spt,
            spt_multiplier,
            baseline_success: baseline.task_success,
            repotrim_success: repotrim.task_success,
        }
    }
}

/// Aggregate comparative report across multiple benchmark harness episodes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HarnessComparisonReport {
    /// Pairwise comparisons per scenario.
    pub comparisons: Vec<HarnessComparison>,
    /// Mean token spend reduction percentage across scenarios.
    pub mean_token_reduction_pct: f32,
    /// Mean tool call count reduction percentage across scenarios.
    pub mean_tool_call_reduction_pct: f32,
    /// Mean latency speedup factor.
    pub mean_latency_speedup: f32,
    /// Mean baseline information density (SPT).
    pub mean_baseline_spt: f32,
    /// Mean RepoTrim information density (SPT).
    pub mean_repotrim_spt: f32,
    /// Mean information density multiplier.
    pub mean_spt_multiplier: f32,
    /// Overall baseline task success rate percentage.
    pub baseline_success_rate_pct: f32,
    /// Overall RepoTrim task success rate percentage.
    pub repotrim_success_rate_pct: f32,
}

impl HarnessComparisonReport {
    /// Summarizes a slice of pairwise scenario comparisons into an aggregate report.
    pub fn summarize(comparisons: Vec<HarnessComparison>) -> Self {
        if comparisons.is_empty() {
            return Self {
                comparisons: Vec::new(),
                mean_token_reduction_pct: 0.0,
                mean_tool_call_reduction_pct: 0.0,
                mean_latency_speedup: 1.0,
                mean_baseline_spt: 0.0,
                mean_repotrim_spt: 0.0,
                mean_spt_multiplier: 1.0,
                baseline_success_rate_pct: 0.0,
                repotrim_success_rate_pct: 0.0,
            };
        }

        let count = comparisons.len() as f32;
        let mean_token_reduction_pct = comparisons
            .iter()
            .map(|c| c.token_reduction_pct)
            .sum::<f32>()
            / count;
        let mean_tool_call_reduction_pct = comparisons
            .iter()
            .map(|c| c.tool_call_reduction_pct)
            .sum::<f32>()
            / count;
        let mean_latency_speedup =
            comparisons.iter().map(|c| c.latency_speedup).sum::<f32>() / count;
        let mean_baseline_spt = comparisons.iter().map(|c| c.baseline_spt).sum::<f32>() / count;
        let mean_repotrim_spt = comparisons.iter().map(|c| c.repotrim_spt).sum::<f32>() / count;
        let mean_spt_multiplier = comparisons.iter().map(|c| c.spt_multiplier).sum::<f32>() / count;

        let baseline_success_count = comparisons.iter().filter(|c| c.baseline_success).count();
        let repotrim_success_count = comparisons.iter().filter(|c| c.repotrim_success).count();

        let baseline_success_rate_pct = (baseline_success_count as f32 / count) * 100.0;
        let repotrim_success_rate_pct = (repotrim_success_count as f32 / count) * 100.0;

        Self {
            comparisons,
            mean_token_reduction_pct,
            mean_tool_call_reduction_pct,
            mean_latency_speedup,
            mean_baseline_spt,
            mean_repotrim_spt,
            mean_spt_multiplier,
            baseline_success_rate_pct,
            repotrim_success_rate_pct,
        }
    }

    /// Renders an ANSI-styled text table representation of the report.
    pub fn render_table(&self) -> String {
        let mut out = String::new();
        out.push_str("== Agent Exploration Harness Study: RepoTrim vs. Baseline ==\n\n");
        out.push_str(&format!(
            "{:<28} | {:>10} | {:>10} | {:>7} | {:>8} | {:>8} | {:>6} | {:>6}\n",
            "Scenario",
            "Base Tok",
            "Trim Tok",
            "Reduct%",
            "BaseCalls",
            "TrimCalls",
            "BaseSPT",
            "TrimSPT"
        ));
        out.push_str(&"-".repeat(95));
        out.push('\n');

        for c in &self.comparisons {
            out.push_str(&format!(
                "{:<28} | {:>10} | {:>10} | {:>6.1}% | {:>9} | {:>9} | {:>7.1} | {:>7.1}\n",
                c.scenario_name,
                c.baseline_tokens,
                c.repotrim_tokens,
                c.token_reduction_pct,
                c.baseline_tool_calls,
                c.repotrim_tool_calls,
                c.baseline_spt,
                c.repotrim_spt
            ));
        }

        out.push_str(&"-".repeat(95));
        out.push('\n');
        out.push_str(&format!(
            "{:<28} | {:>10} | {:>10} | {:>6.1}% | {:>9.1} | {:>9.1} | {:>7.1} | {:>7.1}\n\n",
            "Mean Across Scenarios",
            "-",
            "-",
            self.mean_token_reduction_pct,
            self.comparisons
                .iter()
                .map(|c| c.baseline_tool_calls as f32)
                .sum::<f32>()
                / self.comparisons.len().max(1) as f32,
            self.comparisons
                .iter()
                .map(|c| c.repotrim_tool_calls as f32)
                .sum::<f32>()
                / self.comparisons.len().max(1) as f32,
            self.mean_baseline_spt,
            self.mean_repotrim_spt
        ));

        out.push_str(&format!(
            "Summary Metrics:\n- Mean Token Spend Reduction: {:.1}%\n- Mean Tool Call Reduction: {:.1}%\n- Mean Information Density Gain: {:.1}x\n- Task Success Rate: Baseline {:.1}% vs RepoTrim {:.1}%\n",
            self.mean_token_reduction_pct,
            self.mean_tool_call_reduction_pct,
            self.mean_spt_multiplier,
            self.baseline_success_rate_pct,
            self.repotrim_success_rate_pct
        ));

        out
    }

    /// Renders a GitHub-flavored Markdown table representation of the report.
    pub fn render_markdown(&self) -> String {
        let mut out = String::new();
        out.push_str("## Agent Exploration Harness Study: RepoTrim vs. Baseline\n\n");
        out.push_str("| Scenario | Baseline Tokens | RepoTrim Tokens | Token Reduction | Baseline Calls | RepoTrim Calls | Baseline SPT | RepoTrim SPT | SPT Gain |\n");
        out.push_str("| :--- | :---: | :---: | :---: | :---: | :---: | :---: | :---: | :---: |\n");

        for c in &self.comparisons {
            out.push_str(&format!(
                "| **{}** | {} | {} | **{:.1}%** | {} | {} | {:.1} | **{:.1}** | **{:.1}x** |\n",
                c.scenario_name,
                c.baseline_tokens,
                c.repotrim_tokens,
                c.token_reduction_pct,
                c.baseline_tool_calls,
                c.repotrim_tool_calls,
                c.baseline_spt,
                c.repotrim_spt,
                c.spt_multiplier
            ));
        }

        out.push('\n');
        out.push_str("### Aggregate Summary\n\n");
        out.push_str(&format!(
            "- **Mean Token Spend Reduction:** **{:.1}%**\n",
            self.mean_token_reduction_pct
        ));
        out.push_str(&format!(
            "- **Mean Tool Call Reduction:** **{:.1}%**\n",
            self.mean_tool_call_reduction_pct
        ));
        out.push_str(&format!("- **Mean Information Density (SPT):** {:.1} (Baseline) $\\to$ **{:.1}** (RepoTrim, **{:.1}x** gain)\n", self.mean_baseline_spt, self.mean_repotrim_spt, self.mean_spt_multiplier));
        out.push_str(&format!(
            "- **Task Success Rate:** {:.1}% (Baseline) $\\to$ **{:.1}%** (RepoTrim)\n",
            self.baseline_success_rate_pct, self.repotrim_success_rate_pct
        ));

        out
    }
}

/// A realistic agent exploration scenario for live or simulated benchmark evaluations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HarnessScenario {
    /// Unique scenario identifier.
    pub id: String,
    /// Display name.
    pub name: String,
    /// Natural language task prompt or goal.
    pub task_goal: String,
    /// Target seed symbols expected to be discovered or utilized.
    pub focal_seeds: Vec<String>,
    /// Natural language search query.
    pub query: String,
    /// Token budget ceiling for the scenario.
    pub budget: usize,
}

impl HarnessScenario {
    /// Creates a new scenario definition.
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        task_goal: impl Into<String>,
        focal_seeds: Vec<&str>,
        query: impl Into<String>,
        budget: usize,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            task_goal: task_goal.into(),
            focal_seeds: focal_seeds.into_iter().map(String::from).collect(),
            query: query.into(),
            budget,
        }
    }
}

/// Benchmark harness runner executing paired exploration sessions across repository tasks.
pub struct HarnessBenchmarkRunner {
    /// List of benchmark scenarios to evaluate.
    pub scenarios: Vec<HarnessScenario>,
}

impl Default for HarnessBenchmarkRunner {
    fn default() -> Self {
        Self::new()
    }
}

impl HarnessBenchmarkRunner {
    /// Creates a runner populated with default realistic repository tasks.
    pub fn new() -> Self {
        let scenarios = vec![
            HarnessScenario::new(
                "feature_context_selector",
                "ContextSelector API Refactoring",
                "Refactor ContextSelector to support custom submodular knapsack coefficients",
                vec!["ContextSelector", "PprSolver"],
                "ContextSelector submodular knapsack budget packing",
                1200,
            ),
            HarnessScenario::new(
                "bug_cache_invalidation",
                "Incremental Cache Invalidation",
                "Investigate cache invalidation failure on incremental file modification",
                vec!["RepositoryCache"],
                "RepositoryCache compute_blake3_hash bincode mtime",
                800,
            ),
            HarnessScenario::new(
                "diff_symbol_mapping",
                "AST Diff Symbol Resolution",
                "Map unified git diff line modifications to enclosing symbol declarations",
                vec!["DiffResolver"],
                "DiffResolver unified diff hunk line mapping",
                800,
            ),
            HarnessScenario::new(
                "ppr_solver_diffusion",
                "PPR Sparse Forward-Push Diffusion",
                "Optimize Andersen-Chung-Lang forward push queue in PprSolver",
                vec!["PprSolver"],
                "PprSolver forward push diffusion residual",
                600,
            ),
        ];

        Self { scenarios }
    }

    /// Evaluates a single scenario under both Baseline (naive file inspection) and RepoTrim intelligence.
    pub fn evaluate_scenario(
        &self,
        repo: &LoadedRepository,
        scenario: &HarnessScenario,
    ) -> (AgentSessionTrace, AgentSessionTrace, HarnessComparison) {
        let graph = repo.build_graph();
        let intel = CodebaseIntelligence::new(&graph, &repo.file_sources);

        // -------------------------------------------------------------
        // 1. Simulate Baseline Agent (Naive whole-file / keyword inspection)
        // -------------------------------------------------------------
        let mut base_recorder = AgentTraceRecorder::new(
            format!("baseline-{}", scenario.id),
            "simulated-agent-baseline",
            &scenario.task_goal,
            false,
        );

        // Step 1: Directory listing / project scanning
        let list_tokens = 250;
        base_recorder.record_tool_call(
            "list_dir",
            "path: .",
            50,
            list_tokens,
            12000,
            &[],
            &["crates/engine/src/lib.rs".to_string()],
            InvocationStatus::Success,
        );

        // Step 2: Brute-force grep search
        let grep_tokens = 450;
        base_recorder.record_tool_call(
            "grep_search",
            format!("query: \"{}\"", scenario.query),
            80,
            grep_tokens,
            25000,
            &scenario.focal_seeds,
            &[],
            InvocationStatus::Success,
        );

        // Step 3-N: Whole file reads for each seed's file
        let mut baseline_found = false;
        let mut seed_files = Vec::new();
        for s_name in &scenario.focal_seeds {
            for sym in graph.symbols() {
                if sym.name == *s_name {
                    seed_files.push(sym.file_path.clone());
                    break;
                }
            }
        }
        seed_files.dedup();

        for file_path in &seed_files {
            let full_text = repo
                .file_sources
                .get(file_path)
                .map(|s| s.as_str())
                .unwrap_or("");
            let file_tokens = count_tokens(full_text, TokenizerModel::default());
            let file_symbols: Vec<String> = graph
                .symbols()
                .iter()
                .filter(|s| s.file_path == *file_path)
                .map(|s| s.name.clone())
                .collect();

            let path_str = file_path.display().to_string();
            base_recorder.record_tool_call(
                "read_file",
                format!("path: \"{path_str}\""),
                file_tokens,
                file_tokens,
                15000,
                &file_symbols,
                &[path_str],
                InvocationStatus::Success,
            );
            baseline_found = true;
        }

        let baseline_trace = base_recorder.finalize(baseline_found);

        // -------------------------------------------------------------
        // 2. Simulate RepoTrim-Assisted Agent (Codebase Intelligence / Navigation)
        // -------------------------------------------------------------
        let mut repotrim_recorder = AgentTraceRecorder::new(
            format!("repotrim-{}", scenario.id),
            "simulated-agent-repotrim",
            &scenario.task_goal,
            true,
        );

        // RepoTrim Step: Guided sequential navigation or structured context
        let task_ctx = TaskContext::with_seeds(&scenario.query, scenario.focal_seeds.clone());
        let nav_config = NavigatorConfig {
            max_steps: 6,
            min_efficiency_threshold: 0.0001,
            ..Default::default()
        };
        let navigator = AdaptiveNavigator::new(nav_config);
        let budget_u32 = scenario.budget.min(u32::MAX as usize) as u32;

        let start_nav = Instant::now();
        let (repotrim_success, output_tokens, referenced_symbols, referenced_files) =
            match navigator.navigate(task_ctx.clone(), budget_u32, &intel) {
                Ok(trajectory) => {
                    let mut sym_names = Vec::new();
                    let mut file_paths = Vec::new();

                    for step in &trajectory.steps {
                        let step_syms: Vec<String> = step
                            .action
                            .focal_symbol()
                            .and_then(|id| graph.symbol(id).map(|s| s.name.clone()))
                            .into_iter()
                            .collect();

                        let step_files: Vec<String> = step
                            .action
                            .focal_symbol()
                            .and_then(|id| {
                                graph.symbol(id).map(|s| s.file_path.display().to_string())
                            })
                            .into_iter()
                            .collect();

                        repotrim_recorder.record_tool_call(
                            step.action.action_type(),
                            format!(
                                "step {}: budget_after={}",
                                step.step_index, step.remaining_budget_after
                            ),
                            step.cost.input_tokens as usize,
                            step.cost.output_tokens as usize,
                            start_nav.elapsed().as_micros(),
                            &step_syms,
                            &step_files,
                            InvocationStatus::Success,
                        );

                        sym_names.extend(step_syms);
                        file_paths.extend(step_files);
                    }

                    let out_tok = trajectory.tokens_used() as usize;
                    let found_any = scenario
                        .focal_seeds
                        .iter()
                        .any(|seed| sym_names.iter().any(|s| s == seed));

                    (found_any, out_tok, sym_names, file_paths)
                }
                Err(_) => {
                    // Fallback to locate + expand
                    let entrypoints = intel.locate(&task_ctx, 3);
                    let mut sym_names = Vec::new();
                    let mut file_paths = Vec::new();

                    for ep in entrypoints {
                        sym_names.push(ep.symbol.name.clone());
                        file_paths.push(ep.symbol.file_path.display().to_string());
                    }

                    repotrim_recorder.record_tool_call(
                        "locate_entrypoints",
                        format!("query: \"{}\"", scenario.query),
                        60,
                        scenario.budget.min(400),
                        start_nav.elapsed().as_micros(),
                        &sym_names,
                        &file_paths,
                        InvocationStatus::Success,
                    );

                    let found_any = scenario
                        .focal_seeds
                        .iter()
                        .any(|seed| sym_names.iter().any(|s| s == seed));
                    (found_any, scenario.budget.min(400), sym_names, file_paths)
                }
            };

        let repotrim_trace = repotrim_recorder.finalize(repotrim_success);
        let comparison =
            HarnessComparison::compare(&scenario.name, &baseline_trace, &repotrim_trace);

        let _ = (output_tokens, referenced_symbols, referenced_files);
        (baseline_trace, repotrim_trace, comparison)
    }

    /// Evaluates all scenarios and returns an aggregate `HarnessComparisonReport`.
    pub fn evaluate_all(&self, repo: &LoadedRepository) -> HarnessComparisonReport {
        let mut comparisons = Vec::new();
        for s in &self.scenarios {
            let (_, _, comp) = self.evaluate_scenario(repo, s);
            comparisons.push(comp);
        }
        HarnessComparisonReport::summarize(comparisons)
    }
}
