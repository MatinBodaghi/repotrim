//! Sequential Navigation State Model and Action Space (Phase 40).
//!
//! # Theoretical Formulation & Architecture
//! Models the interaction between an autonomous AI agent harness and RepoTrim as a
//! stateful, sequential observation process under budget constraints:
//! $$\max_{\pi} \Pr(\text{success} \mid q, G, \pi) \quad \text{s.t.} \quad \mathrm{Cost}(\pi) \le B$$
//!
//! Grounded in:
//! - **Markov Decision & Observation Processes** (Bellman, 1957): State formulation
//!   $s_t = (G, q, H_t, B_t, o_t)$ where $H_t$ is the interaction trajectory,
//!   $B_t$ is remaining budget, and $o_t$ is the latest evidence observation.
//! - **Adaptive Submodular Optimization** (Golovin & Krause, 2011): Sequential evidence
//!   acquisition where marginal utility gains depend strictly on partial realization
//!   histories $\psi \subseteq V \times \mathcal{O}$.
//! - **Submodular Knapsack Selection** (Nemhauser et al., 1978): Strict cost decrementation
//!   ensuring total exploration expenditure does not violate budget $B$.

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

use crate::context_object::{PathTrace, StructuredContext, StructuredEdge, StructuredSymbol};
use crate::cost::CostBreakdown;
use crate::error::EngineError;
use crate::path::ExecutionPath;
use crate::primitives::{CodebaseIntelligence, NeighborEdge};
use crate::symbol::{LodLevel, NodeType, RelationType, SymbolId, SymbolNode};
use crate::task::TaskContext;
use crate::tokens::estimate_tokens;

/// Default fixed penalty in tokens incurred per action execution.
pub const DEFAULT_ACTION_INVOCATION_COST: u32 = 4;

/// Permissible discrete action primitives in sequential codebase exploration ($\mathcal{A}$).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum NavigationAction {
    /// Retrieve full signature, docstring, AST slice, and immediate 1-hop typed neighbors.
    Inspect {
        /// Target symbol identifier to inspect.
        symbol_id: SymbolId,
    },
    /// Localized submodular expansion centered on a focal symbol within a sub-budget.
    Expand {
        /// Focal symbol identifier to expand outward from.
        symbol_id: SymbolId,
        /// Maximum token budget allocated for this localized cluster expansion.
        budget: u32,
    },
    /// Trace constrained multi-hop causal execution path between source and target symbols.
    Trace {
        /// Execution origin symbol identifier.
        source_id: SymbolId,
        /// Execution destination symbol identifier.
        target_id: SymbolId,
    },
    /// Inspect test linkage connecting implementation symbols to regression test suites.
    TestLink {
        /// Focal implementation symbol identifier.
        symbol_id: SymbolId,
    },
    /// Terminate exploration session and finalize evidence package.
    Stop {
        /// Justification or termination rationale.
        reason: String,
    },
}

impl NavigationAction {
    /// Returns a human-readable identifier for the action type.
    pub fn action_type(&self) -> &'static str {
        match self {
            Self::Inspect { .. } => "INSPECT",
            Self::Expand { .. } => "EXPAND",
            Self::Trace { .. } => "TRACE",
            Self::TestLink { .. } => "TEST_LINK",
            Self::Stop { .. } => "STOP",
        }
    }

    /// Returns the primary focal symbol identifier if applicable.
    pub fn focal_symbol(&self) -> Option<SymbolId> {
        match self {
            Self::Inspect { symbol_id } => Some(*symbol_id),
            Self::Expand { symbol_id, .. } => Some(*symbol_id),
            Self::Trace { source_id, .. } => Some(*source_id),
            Self::TestLink { symbol_id } => Some(*symbol_id),
            Self::Stop { .. } => None,
        }
    }
}

/// Multi-factor cost accounting incurred by executing an action ($c(s_t, a_t)$).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActionCost {
    /// Input prompt tokens consumed by action arguments and parameters.
    pub input_tokens: u32,
    /// Output tokens yielded by the returned observation payload.
    pub output_tokens: u32,
    /// Fixed tool invocation penalty ($N_{\text{tool}}$).
    pub invocation_cost: u32,
    /// Total cost charged against the remaining budget:
    /// $\mathrm{Cost}(a) = T_{in} + T_{out} + N_{\text{tool}}$.
    pub total_tokens: u32,
}

impl ActionCost {
    /// Constructs a new cost breakdown.
    pub fn new(input_tokens: u32, output_tokens: u32, invocation_cost: u32) -> Self {
        Self {
            input_tokens,
            output_tokens,
            invocation_cost,
            total_tokens: input_tokens + output_tokens + invocation_cost,
        }
    }

    /// Constructs a zero-cost instance.
    pub fn zero() -> Self {
        Self {
            input_tokens: 0,
            output_tokens: 0,
            invocation_cost: 0,
            total_tokens: 0,
        }
    }
}

/// Detailed inspection observation payload.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InspectionObservation {
    /// Inspected symbol node metadata.
    pub symbol: SymbolNode,
    /// Formatted code snippet or declaration slice.
    pub code_snippet: String,
    /// Direct incoming typed neighbor edges ($u \to v$).
    pub incoming: Vec<NeighborEdge>,
    /// Direct outgoing typed neighbor edges ($v \to u$).
    pub outgoing: Vec<NeighborEdge>,
}

/// Localized submodular expansion observation payload.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExpansionObservation {
    /// Central focal symbol node.
    pub focal_symbol: SymbolNode,
    /// Co-selected context symbols within allocated micro-budget.
    pub selected_symbols: Vec<SymbolNode>,
    /// Total tokens consumed by the formatted cluster.
    pub tokens_used: u32,
    /// Synthesized markdown formatted source blocks.
    pub markdown_source: String,
}

/// Causal execution path trace observation payload.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PathTraceObservation {
    /// Source origin symbol node.
    pub source: SymbolNode,
    /// Destination target symbol node.
    pub target: SymbolNode,
    /// Whether an unbroken causal execution path was resolved.
    pub path_found: bool,
    /// Hop count of the shortest loopless causal path.
    pub hops: usize,
    /// Rendered Mermaid sequence diagram.
    pub mermaid_diagram: String,
    /// Underlying execution path sequence if discovered.
    pub path: Option<ExecutionPath>,
}

/// Test verification linkage observation payload.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TestLinkageObservation {
    /// Target implementation symbol node.
    pub target_symbol: SymbolNode,
    /// Discovered test symbols directly verifying the target.
    pub tests: Vec<SymbolNode>,
    /// Flag indicating whether the symbol lacks test coverage in the graph.
    pub untested: bool,
}

/// The discrete observation payload yielded by executing a navigation action ($o_t$).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Observation {
    /// Detailed AST metadata, source slice, and 1-hop typed neighborhood.
    Inspection(Box<InspectionObservation>),
    /// Localized submodular knapsack context expansion cluster.
    Expansion(Box<ExpansionObservation>),
    /// Causal execution path trace connecting two symbols.
    PathTrace(Box<PathTraceObservation>),
    /// Test verification linkage connecting symbols to unit or integration tests.
    TestLinkage(Box<TestLinkageObservation>),
    /// Finalized exploration session termination.
    Terminated {
        /// Rationale for termination.
        reason: String,
        /// Cumulative action steps executed.
        total_steps: usize,
        /// Cumulative tokens consumed across the trajectory.
        total_cost: u32,
    },
}

impl Observation {
    /// Generates a brief human-readable summary of the observation.
    pub fn summary(&self) -> String {
        match self {
            Self::Inspection(obs) => format!(
                "Inspected symbol '{}' ({:?}): {} incoming, {} outgoing edges",
                obs.symbol.name,
                obs.symbol.node_type(),
                obs.incoming.len(),
                obs.outgoing.len()
            ),
            Self::Expansion(obs) => format!(
                "Expanded around '{}': {} symbols selected using {} tokens",
                obs.focal_symbol.name,
                obs.selected_symbols.len(),
                obs.tokens_used
            ),
            Self::PathTrace(obs) => {
                if obs.path_found {
                    format!(
                        "Path trace '{}' -> '{}': {} hops resolved",
                        obs.source.name, obs.target.name, obs.hops
                    )
                } else {
                    format!(
                        "Path trace '{}' -> '{}': no path found",
                        obs.source.name, obs.target.name
                    )
                }
            }
            Self::TestLinkage(obs) => {
                if obs.untested {
                    format!(
                        "Test linkage for '{}': untested (0 tests found)",
                        obs.target_symbol.name
                    )
                } else {
                    format!(
                        "Test linkage for '{}': {} test cases linked",
                        obs.target_symbol.name,
                        obs.tests.len()
                    )
                }
            }
            Self::Terminated {
                reason,
                total_steps,
                total_cost,
            } => format!(
                "Exploration terminated ({}): {} steps, {} tokens expended",
                reason, total_steps, total_cost
            ),
        }
    }

    /// Extracts all symbol IDs observed or referenced in this observation.
    pub fn referenced_symbols(&self) -> Vec<SymbolId> {
        let mut ids = Vec::new();
        match self {
            Self::Inspection(obs) => {
                ids.push(obs.symbol.id);
                for edge in obs.incoming.iter().chain(obs.outgoing.iter()) {
                    ids.push(edge.target.id);
                }
            }
            Self::Expansion(obs) => {
                ids.push(obs.focal_symbol.id);
                for sym in &obs.selected_symbols {
                    ids.push(sym.id);
                }
            }
            Self::PathTrace(obs) => {
                ids.push(obs.source.id);
                ids.push(obs.target.id);
                if let Some(ref p) = obs.path {
                    for node_id in &p.nodes {
                        ids.push(*node_id);
                    }
                }
            }
            Self::TestLinkage(obs) => {
                ids.push(obs.target_symbol.id);
                for test in &obs.tests {
                    ids.push(test.id);
                }
            }
            Self::Terminated { .. } => {}
        }
        ids
    }
}

/// An individual state transition step along the interaction trajectory ($H_t$).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NavigationStep {
    /// Zero-based sequential step index.
    pub step_index: usize,
    /// Executed action primitive.
    pub action: NavigationAction,
    /// Yielded observation payload.
    pub observation: Observation,
    /// Realized multi-factor execution cost.
    pub cost: ActionCost,
    /// Remaining token budget immediately following step execution.
    pub remaining_budget_after: u32,
}

/// The stateful sequential exploration environment ($s_t = (G, q, H_t, B_t, o_t)$).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NavigationState {
    /// Active task query context ($q$).
    pub task: TaskContext,
    /// Initial budget limit ($B_0$).
    pub initial_budget: u32,
    /// Current remaining budget ($B_t \ge 0$).
    pub remaining_budget: u32,
    /// Chronological interaction trajectory ($H_t$).
    pub history: Vec<NavigationStep>,
    /// Accumulated set of observed symbols with their highest observed LOD.
    pub observed_symbols: HashMap<SymbolId, LodLevel>,
    /// Accumulated set of observed directed typed relationships.
    pub observed_edges: HashSet<(SymbolId, SymbolId, RelationType)>,
    /// Verified causal execution paths discovered during navigation.
    pub observed_paths: Vec<ExecutionPath>,
    /// Active exploration frontier ($\mathcal{F}_t$) containing adjacent discovered candidates.
    pub frontier: HashSet<SymbolId>,
    /// Flag indicating whether the exploration session is terminated.
    pub is_terminal: bool,
}

impl NavigationState {
    /// Initializes a new sequential navigation state with an initial token budget.
    pub fn new(task: TaskContext, initial_budget: u32) -> Self {
        Self {
            task,
            initial_budget,
            remaining_budget: initial_budget,
            history: Vec::new(),
            observed_symbols: HashMap::new(),
            observed_edges: HashSet::new(),
            observed_paths: Vec::new(),
            frontier: HashSet::new(),
            is_terminal: false,
        }
    }

    /// Initializes a new navigation state seeded with initial entrypoint symbols on the frontier.
    pub fn with_entrypoints(
        task: TaskContext,
        initial_budget: u32,
        entrypoints: &[SymbolId],
    ) -> Self {
        let mut state = Self::new(task, initial_budget);
        for &id in entrypoints {
            state.frontier.insert(id);
        }
        state
    }

    /// Returns the number of exploration steps executed so far.
    pub fn step_count(&self) -> usize {
        self.history.len()
    }

    /// Returns the cumulative token cost incurred across all previous steps.
    pub fn total_cost_incurred(&self) -> u32 {
        self.initial_budget.saturating_sub(self.remaining_budget)
    }

    /// Evaluates whether a proposed cost can be afforded under the remaining budget.
    pub fn can_afford(&self, cost: u32) -> bool {
        !self.is_terminal && self.remaining_budget >= cost
    }

    /// Records a realized step into the interaction trajectory and updates state invariants.
    pub fn record_step(
        &mut self,
        action: NavigationAction,
        observation: Observation,
        cost: ActionCost,
    ) {
        let total_cost = cost.total_tokens;
        self.remaining_budget = self.remaining_budget.saturating_sub(total_cost);

        let step = NavigationStep {
            step_index: self.history.len(),
            action,
            observation,
            cost,
            remaining_budget_after: self.remaining_budget,
        };

        if let NavigationAction::Stop { .. } = &step.action {
            self.is_terminal = true;
        }

        if self.remaining_budget == 0 {
            self.is_terminal = true;
        }

        self.history.push(step);
    }

    /// Executes an action primitive against the codebase intelligence service,
    /// recording observations and strictly updating the sequential state.
    pub fn apply_action(
        &mut self,
        action: NavigationAction,
        intel: &CodebaseIntelligence,
    ) -> Result<&NavigationStep, EngineError> {
        if self.is_terminal {
            return Err(EngineError::InvalidInput(
                "Cannot apply action to a terminated NavigationState".into(),
            ));
        }

        match action {
            NavigationAction::Stop { ref reason } => {
                let cost = ActionCost::new(0, 0, 0);
                let observation = Observation::Terminated {
                    reason: reason.clone(),
                    total_steps: self.history.len() + 1,
                    total_cost: self.total_cost_incurred(),
                };
                self.record_step(action, observation, cost);
            }

            NavigationAction::Inspect { symbol_id } => {
                let node = intel
                    .graph()
                    .symbol(symbol_id)
                    .ok_or(EngineError::SymbolNotFound(symbol_id.0))?
                    .clone();

                let neighborhood = intel.neighbors(symbol_id)?;

                let snippet = intel
                    .file_sources()
                    .get(&node.file_path)
                    .map(|content| {
                        let lines: Vec<&str> = content.lines().collect();
                        let start = node.span.start_row;
                        let end = (node.span.end_row + 1).min(lines.len());
                        if start < end {
                            lines[start..end].join("\n")
                        } else {
                            node.signature.clone()
                        }
                    })
                    .unwrap_or_else(|| node.signature.clone());

                let output_tokens = estimate_tokens(&snippet).max(1) as u32;
                let cost = ActionCost::new(2, output_tokens, DEFAULT_ACTION_INVOCATION_COST);

                if !self.can_afford(cost.total_tokens) {
                    return Err(EngineError::BudgetExceeded(
                        cost.total_tokens as usize,
                        self.remaining_budget as usize,
                    ));
                }

                // Update state representations
                self.observed_symbols.insert(symbol_id, LodLevel::FullBody);
                self.frontier.remove(&symbol_id);

                for edge in &neighborhood.outgoing {
                    self.observed_edges
                        .insert((symbol_id, edge.target.id, edge.relation));
                    if !self.observed_symbols.contains_key(&edge.target.id) {
                        self.frontier.insert(edge.target.id);
                    }
                }

                for edge in &neighborhood.incoming {
                    self.observed_edges
                        .insert((edge.target.id, symbol_id, edge.relation));
                    if !self.observed_symbols.contains_key(&edge.target.id) {
                        self.frontier.insert(edge.target.id);
                    }
                }

                let observation = Observation::Inspection(Box::new(InspectionObservation {
                    symbol: node,
                    code_snippet: snippet,
                    incoming: neighborhood.incoming,
                    outgoing: neighborhood.outgoing,
                }));

                self.record_step(action, observation, cost);
            }

            NavigationAction::Expand { symbol_id, budget } => {
                let node = intel
                    .graph()
                    .symbol(symbol_id)
                    .ok_or(EngineError::SymbolNotFound(symbol_id.0))?
                    .clone();

                let effective_budget = budget.min(self.remaining_budget);
                if effective_budget == 0 {
                    return Err(EngineError::BudgetExceeded(
                        budget as usize,
                        self.remaining_budget as usize,
                    ));
                }

                let expansion =
                    intel.expand(symbol_id, effective_budget as usize, Some(&self.task))?;
                let cost = ActionCost::new(
                    4,
                    expansion.tokens_used as u32,
                    DEFAULT_ACTION_INVOCATION_COST,
                );

                if !self.can_afford(cost.total_tokens) {
                    return Err(EngineError::BudgetExceeded(
                        cost.total_tokens as usize,
                        self.remaining_budget as usize,
                    ));
                }

                self.observed_symbols.insert(symbol_id, LodLevel::FullBody);
                self.frontier.remove(&symbol_id);

                for sym in &expansion.symbols {
                    self.observed_symbols.insert(sym.id, LodLevel::FullBody);
                    self.frontier.remove(&sym.id);

                    // Add immediate unobserved neighbors to frontier
                    if let Ok(neighbors) = intel.neighbors(sym.id) {
                        for edge in neighbors.outgoing.iter().chain(neighbors.incoming.iter()) {
                            if !self.observed_symbols.contains_key(&edge.target.id) {
                                self.frontier.insert(edge.target.id);
                            }
                        }
                    }
                }

                let observation = Observation::Expansion(Box::new(ExpansionObservation {
                    focal_symbol: node,
                    selected_symbols: expansion.symbols,
                    tokens_used: expansion.tokens_used as u32,
                    markdown_source: expansion.formatted_code,
                }));

                self.record_step(action, observation, cost);
            }

            NavigationAction::Trace {
                source_id,
                target_id,
            } => {
                let source = intel
                    .graph()
                    .symbol(source_id)
                    .ok_or(EngineError::SymbolNotFound(source_id.0))?
                    .clone();
                let target = intel
                    .graph()
                    .symbol(target_id)
                    .ok_or(EngineError::SymbolNotFound(target_id.0))?
                    .clone();

                let trace_result = intel.trace(source_id, target_id, Some(&self.task))?;
                let output_tokens =
                    (estimate_tokens(&trace_result.mermaid_diagram).max(1) as u32) + 10;
                let cost = ActionCost::new(4, output_tokens, DEFAULT_ACTION_INVOCATION_COST);

                if !self.can_afford(cost.total_tokens) {
                    return Err(EngineError::BudgetExceeded(
                        cost.total_tokens as usize,
                        self.remaining_budget as usize,
                    ));
                }

                let hops = trace_result
                    .best_path
                    .as_ref()
                    .map(|p| p.relations.len())
                    .unwrap_or(0);

                if let Some(ref path) = trace_result.best_path {
                    self.observed_paths.push(path.clone());
                    for &node_id in &path.nodes {
                        self.observed_symbols
                            .entry(node_id)
                            .or_insert(LodLevel::SignatureOnly);
                    }
                    for i in 0..path.nodes.len().saturating_sub(1) {
                        self.observed_edges.insert((
                            path.nodes[i],
                            path.nodes[i + 1],
                            RelationType::Calls,
                        ));
                    }
                }

                let observation = Observation::PathTrace(Box::new(PathTraceObservation {
                    source,
                    target,
                    path_found: trace_result.best_path.is_some(),
                    hops,
                    mermaid_diagram: trace_result.mermaid_diagram,
                    path: trace_result.best_path,
                }));

                self.record_step(action, observation, cost);
            }

            NavigationAction::TestLink { symbol_id } => {
                let node = intel
                    .graph()
                    .symbol(symbol_id)
                    .ok_or(EngineError::SymbolNotFound(symbol_id.0))?
                    .clone();

                let neighborhood = intel.neighbors(symbol_id)?;
                let mut tests = Vec::new();

                for edge in neighborhood
                    .outgoing
                    .iter()
                    .chain(neighborhood.incoming.iter())
                {
                    if edge.relation == RelationType::IsTestedBy
                        || edge.target.node_type() == NodeType::Test
                    {
                        tests.push(edge.target.clone());
                        if !self.observed_symbols.contains_key(&edge.target.id) {
                            self.frontier.insert(edge.target.id);
                        }
                    }
                }

                let untested = tests.is_empty();
                let output_tokens = (tests.len() as u32 * 12).max(4);
                let cost = ActionCost::new(2, output_tokens, DEFAULT_ACTION_INVOCATION_COST);

                if !self.can_afford(cost.total_tokens) {
                    return Err(EngineError::BudgetExceeded(
                        cost.total_tokens as usize,
                        self.remaining_budget as usize,
                    ));
                }

                let observation = Observation::TestLinkage(Box::new(TestLinkageObservation {
                    target_symbol: node,
                    tests,
                    untested,
                }));

                self.record_step(action, observation, cost);
            }
        }

        Ok(self.history.last().expect("History must contain last step"))
    }

    /// Synthesizes a unified `StructuredContext` from the evidence accumulated during sequential exploration.
    pub fn synthesize_context(
        &self,
        intel: &CodebaseIntelligence,
    ) -> Result<StructuredContext, EngineError> {
        let mut structured_symbols = Vec::new();
        let mut structured_edges = Vec::new();

        for (&sym_id, &lod) in &self.observed_symbols {
            if let Some(node) = intel.graph().symbol(sym_id) {
                let snippet = intel
                    .file_sources()
                    .get(&node.file_path)
                    .map(|c| {
                        let lines: Vec<&str> = c.lines().collect();
                        let start = node.span.start_row;
                        let end = (node.span.end_row + 1).min(lines.len());
                        if start < end {
                            lines[start..end].join("\n")
                        } else {
                            node.signature.clone()
                        }
                    })
                    .unwrap_or_else(|| node.signature.clone());

                let token_count = estimate_tokens(&snippet).max(1);
                structured_symbols.push(StructuredSymbol {
                    id: sym_id,
                    name: node.name.clone(),
                    file_path: node.file_path.clone(),
                    span: node.span,
                    kind: node.kind,
                    node_type: node.node_type(),
                    lod,
                    token_cost: token_count,
                    score: 1.0,
                    container_name: node.container_name.clone(),
                    trait_name: node.trait_name.clone(),
                    code: snippet,
                });
            }
        }

        for &(src, dst, rel) in &self.observed_edges {
            structured_edges.push(StructuredEdge {
                source: src,
                target: dst,
                relation: rel,
                weight: 1.0,
            });
        }

        let mut path_traces = Vec::new();
        for path in &self.observed_paths {
            let mut steps = Vec::new();
            for &id in &path.nodes {
                if let Some(node) = intel.graph().symbol(id) {
                    steps.push(node.name.clone());
                }
            }
            path_traces.push(PathTrace {
                nodes: path.nodes.clone(),
                relations: path.relations.clone(),
                trace: steps.join(" -> "),
                probability: path.probability,
                rationale: format!("Causal path of length {}", path.relations.len()),
            });
        }

        let total_tokens = self.total_cost_incurred() as usize;
        let cost_breakdown = CostBreakdown::new(
            total_tokens,
            self.history.len() * 10,
            self.observed_edges.len() * 5,
            20,
        );

        Ok(StructuredContext {
            task: Some(self.task.clone()),
            symbols: structured_symbols,
            edges: structured_edges,
            paths: path_traces,
            omissions: Vec::new(),
            confidence_score: 0.95,
            budget_limit: self.initial_budget as usize,
            tokens_used: total_tokens,
            cost_breakdown,
        })
    }
}

/// A candidate navigation action annotated with estimated cost, priority, and rationale.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CandidateAction {
    /// Proposed concrete action primitive.
    pub action: NavigationAction,
    /// Estimated conservative token cost before execution.
    pub estimated_cost: u32,
    /// Heuristic priority or expected utility score in `[0.0, 1.0]`.
    pub priority: f32,
    /// Attribution rationale explaining why this action was proposed.
    pub rationale: String,
}

/// Configuration parameters governing action candidate generation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActionGeneratorConfig {
    /// Maximum candidate actions to propose per step.
    pub max_candidates: usize,
    /// Default micro-budget for localized expansion candidates.
    pub default_expand_budget: u32,
    /// Whether to propose causal path traces between pairs of frontier nodes.
    pub enable_trace_candidates: bool,
    /// Whether to propose test verification linkage.
    pub enable_test_candidates: bool,
}

impl Default for ActionGeneratorConfig {
    fn default() -> Self {
        Self {
            max_candidates: 10,
            default_expand_budget: 300,
            enable_trace_candidates: true,
            enable_test_candidates: true,
        }
    }
}

/// Generator of feasible navigation action candidates from the current exploration state.
#[derive(Debug, Clone)]
pub struct ActionGenerator {
    config: ActionGeneratorConfig,
}

impl Default for ActionGenerator {
    fn default() -> Self {
        Self::new(ActionGeneratorConfig::default())
    }
}

impl ActionGenerator {
    /// Constructs a new `ActionGenerator` with custom configuration.
    pub fn new(config: ActionGeneratorConfig) -> Self {
        Self { config }
    }

    /// Estimates the conservative token cost for executing a given action.
    pub fn estimate_action_cost(
        &self,
        action: &NavigationAction,
        intel: &CodebaseIntelligence,
    ) -> u32 {
        match action {
            NavigationAction::Stop { .. } => 0,
            NavigationAction::Inspect { symbol_id } => {
                let symbol_cost = intel
                    .graph()
                    .symbol(*symbol_id)
                    .map(|s| s.token_cost as u32)
                    .unwrap_or(50);
                symbol_cost + DEFAULT_ACTION_INVOCATION_COST + 6
            }
            NavigationAction::Expand { budget, .. } => *budget + DEFAULT_ACTION_INVOCATION_COST + 4,
            NavigationAction::Trace { .. } => 60 + DEFAULT_ACTION_INVOCATION_COST,
            NavigationAction::TestLink { .. } => 30 + DEFAULT_ACTION_INVOCATION_COST,
        }
    }

    /// Proposes a set of feasible, prioritized candidate actions given the current state.
    pub fn propose_actions(
        &self,
        state: &NavigationState,
        intel: &CodebaseIntelligence,
    ) -> Result<Vec<CandidateAction>, EngineError> {
        if state.is_terminal {
            return Ok(Vec::new());
        }

        let mut candidates = Vec::new();
        let mut seen_actions = HashSet::new();

        // 1. Initial Entrypoint Localization (Bootstrap phase if frontier and observed are empty)
        if state.frontier.is_empty() && state.observed_symbols.is_empty() {
            let entrypoints = intel.locate(&state.task, self.config.max_candidates);
            for ep in entrypoints {
                let action = NavigationAction::Inspect {
                    symbol_id: ep.symbol.id,
                };
                let estimated_cost = self.estimate_action_cost(&action, intel);
                if state.can_afford(estimated_cost) && seen_actions.insert(action.clone()) {
                    candidates.push(CandidateAction {
                        action,
                        estimated_cost,
                        priority: ep.score.clamp(0.1, 1.0),
                        rationale: ep.reason,
                    });
                }
            }
        }

        // 2. Frontier Exploration (Inspect & Expand unobserved candidate symbols)
        for &sym_id in &state.frontier {
            if state.observed_symbols.contains_key(&sym_id) {
                continue;
            }

            if let Some(node) = intel.graph().symbol(sym_id) {
                // A. Propose Inspect
                let inspect_action = NavigationAction::Inspect { symbol_id: sym_id };
                let inspect_cost = self.estimate_action_cost(&inspect_action, intel);
                if state.can_afford(inspect_cost) && seen_actions.insert(inspect_action.clone()) {
                    let priority = match node.node_type() {
                        NodeType::Function | NodeType::Method => 0.85,
                        NodeType::Struct | NodeType::Class | NodeType::Interface => 0.80,
                        NodeType::Module | NodeType::Package => 0.70,
                        _ => 0.60,
                    };

                    candidates.push(CandidateAction {
                        action: inspect_action,
                        estimated_cost: inspect_cost,
                        priority,
                        rationale: format!(
                            "Frontier symbol '{}' ({:?}): inspect declaration and typed neighborhood",
                            node.name,
                            node.node_type()
                        ),
                    });
                }

                // B. Propose Expand for central entities with available budget
                let expand_budget = self
                    .config
                    .default_expand_budget
                    .min(state.remaining_budget);
                if expand_budget >= 100 {
                    let expand_action = NavigationAction::Expand {
                        symbol_id: sym_id,
                        budget: expand_budget,
                    };
                    let expand_cost = self.estimate_action_cost(&expand_action, intel);
                    if state.can_afford(expand_cost) && seen_actions.insert(expand_action.clone()) {
                        candidates.push(CandidateAction {
                            action: expand_action,
                            estimated_cost: expand_cost,
                            priority: 0.75,
                            rationale: format!(
                                "Frontier symbol '{}': pack cohesive submodular context cluster ({} tokens)",
                                node.name, expand_budget
                            ),
                        });
                    }
                }
            }
        }

        // 3. Causal Path Tracing candidates between observed symbols and frontier
        if self.config.enable_trace_candidates {
            let observed_ids: Vec<SymbolId> = state.observed_symbols.keys().copied().collect();
            let frontier_ids: Vec<SymbolId> = state.frontier.iter().copied().collect();

            for &src in &observed_ids {
                for &dst in &frontier_ids {
                    if src == dst {
                        continue;
                    }

                    let trace_action = NavigationAction::Trace {
                        source_id: src,
                        target_id: dst,
                    };
                    let trace_cost = self.estimate_action_cost(&trace_action, intel);
                    if state.can_afford(trace_cost) && seen_actions.insert(trace_action.clone()) {
                        let src_name = intel
                            .graph()
                            .symbol(src)
                            .map(|s| s.name.as_str())
                            .unwrap_or("?");
                        let dst_name = intel
                            .graph()
                            .symbol(dst)
                            .map(|s| s.name.as_str())
                            .unwrap_or("?");

                        candidates.push(CandidateAction {
                            action: trace_action,
                            estimated_cost: trace_cost,
                            priority: 0.65,
                            rationale: format!(
                                "Trace causal dependency flow from '{}' to frontier symbol '{}'",
                                src_name, dst_name
                            ),
                        });

                        if candidates.len() >= self.config.max_candidates * 2 {
                            break;
                        }
                    }
                }
                if candidates.len() >= self.config.max_candidates * 2 {
                    break;
                }
            }
        }

        // 4. Test Verification candidates for observed implementation symbols
        if self.config.enable_test_candidates {
            for (&sym_id, &lod) in &state.observed_symbols {
                if lod >= LodLevel::FullBody {
                    if let Some(node) = intel.graph().symbol(sym_id) {
                        if matches!(
                            node.node_type(),
                            NodeType::Function | NodeType::Method | NodeType::Struct
                        ) {
                            let test_action = NavigationAction::TestLink { symbol_id: sym_id };
                            let test_cost = self.estimate_action_cost(&test_action, intel);
                            if state.can_afford(test_cost)
                                && seen_actions.insert(test_action.clone())
                            {
                                candidates.push(CandidateAction {
                                    action: test_action,
                                    estimated_cost: test_cost,
                                    priority: 0.70,
                                    rationale: format!(
                                        "Discover test verification suites for '{}'",
                                        node.name
                                    ),
                                });
                            }
                        }
                    }
                }
            }
        }

        // 5. Termination Action (Stop)
        if state.step_count() > 0 {
            let (stop_priority, stop_reason) = if state.remaining_budget < 50 {
                (
                    0.95,
                    "Remaining budget is nearly exhausted; finalize evidence context".to_string(),
                )
            } else if state.observed_symbols.len() >= 5 {
                (
                    0.50,
                    format!(
                        "Sufficient evidence collected ({} symbols observed)",
                        state.observed_symbols.len()
                    ),
                )
            } else {
                (
                    0.10,
                    "Optional termination of exploratory navigation".to_string(),
                )
            };

            let stop_action = NavigationAction::Stop {
                reason: stop_reason.clone(),
            };
            candidates.push(CandidateAction {
                action: stop_action,
                estimated_cost: 0,
                priority: stop_priority,
                rationale: stop_reason,
            });
        }

        // Sort descending by priority
        candidates.sort_by(|a, b| {
            b.priority
                .partial_cmp(&a.priority)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        candidates.truncate(self.config.max_candidates);
        Ok(candidates)
    }
}
