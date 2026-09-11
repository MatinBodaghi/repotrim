# RepoTrim: Mathematical Evolution & Codebase Intelligence Master Plan

> **Mission:** "Transition RepoTrim from a static context-selection engine into a task-aware codebase intelligence layer for AI harnesses through progressive 0.x releases (`v0.6.0` → `v0.11.0+`)—grounded in typed multiplex graph theory, monotone submodular coverage, adaptive sequential navigation, and verifiable empirical guarantees, preserving rapid iteration without premature `v1.0.0` API lock-in."

---

## 1. Executive Vision & Research Architecture

### 1.1 The Paradigm Shift
Today, RepoTrim maximizes a static set utility under a token budget:
$$\max_{S \subseteq V} U(S \mid q, G) \quad \text{s.t.} \quad C(S) \le B$$

This evolution broadens the engine from one-shot retrieval into **budgeted, task-aware codebase navigation**:
$$\max_{\pi} \Pr(\text{success} \mid q, G, \pi) \quad \text{s.t.} \quad \mathrm{Cost}(\pi) \le B$$
where exploration cost is modeled across tokens, tool calls, and latency:
$$\mathrm{Cost}(\pi) = \alpha T_{in}(\pi) + \beta T_{out}(\pi) + \gamma N_{tool}(\pi) + \delta N_{file}(\pi) + \eta L(\pi)$$

### 1.2 Mathematical Hierarchy
```text
┌─────────────────────────────────────────────────────────────────────────────┐
│ 1. Codebase Graph: Typed Multiplex G = (V, E, τ_V, τ_E, w)                  │
│    Relation matrices A = Σ ω_r(q) A_r with learned / calibrated weights      │
└──────────────────────────────────────┬──────────────────────────────────────┘
                                       ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│ 2. Task-Conditioned Structural Prior: Personalized PageRank                 │
│    p_q = (1 - α) s_q + α P_q^T p_q  (Inference prior, not final utility)   │
└──────────────────────────────────────┬──────────────────────────────────────┘
                                       ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│ 3. Information Selection: Monotone Submodular Coverage Utility              │
│    F(S; q, G) = α Rel(S,q) + β Cov(S,q,G) + γ Path(S,q,G) + δ Test - λ Red  │
│    Submodularity: f(A ∪ {x}) - f(A) ≥ f(B ∪ {x}) - f(B) for A ⊆ B            │
└──────────────────────────────────────┬──────────────────────────────────────┘
                                       ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│ 4. Budgeted Optimization: Dual-Mode Knapsack Solvers                        │
│    - Fast Production: Lazy-Greedy CELF acceleration                         │
│    - Research Oracle: Exact offline search computing approximation ratio     │
└──────────────────────────────────────┬──────────────────────────────────────┘
                                       ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│ 5. Adaptive Navigation: Sequential Evidence Acquisition                     │
│    Interactive policy choosing a* = argmax_a Δ(a | ψ) / c(a)               │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## 2. Git Branching & Engineering Discipline

### 2.1 Branch Taxonomy & Roles
All branches originate from and merge back into `dev`. Branches strictly follow this 3-tier taxonomy:

| Branch Prefix | Purpose | Scope & Activities |
| :--- | :--- | :--- |
| `research/*` | **Research / Mathematical Evolution** | Formal problem formulations, mathematical modeling, proofs, algorithmic invariants, and design specifications. |
| `feature/*` | **Isolated Implementation** | Concrete engine implementations, data structures, algorithms, trait boundaries, CLI commands, and MCP tools. |
| `experiment/*` | **Benchmark / Hypothesis Testing** | Empirical evaluation, ablation studies, oracle approximation gap measurement, and harness exploration metrics. |

```text
main (v0.5.0 stable) ───[v0.6.0 tag]───[v0.7.0 tag]───[v0.8.0 tag]───[v0.9.0 tag]───►
  │                           ▲              ▲              ▲              ▲
  └───► dev (Integration) ────┴──────────────┴──────────────┴──────────────┴────────►
          │
          ├── research/*    (Math formulation, theory, specification)
          ├── feature/*     (Isolated code implementation)
          └── experiment/*  (Ablations, oracle gaps, empirical benchmarks)
```

### 2.2 Quality & Commit Governance
1. **Commit Message Format:** Strict compliance with `AGENTS.md`:
   - Summary: `type: Short imperative description` ($\le 50$ chars, no trailing period).
   - Blank line.
   - Body: Hard-wrapped to 72 characters, explaining rationale.
   - Zero co-author lines.
2. **Quality Gates:**
   - `cargo test --workspace --all-targets` must pass 100%.
   - `cargo clippy --workspace --all-targets -- -D warnings` must produce zero warnings.
   - `cargo fmt --check` must be clean.
   - Benchmark regression checks via `repotrim benchmark`.
3. **Push Policy:** Commits remain local. Never run `git push` without explicit user instruction.

---

## 3. Master Milestone & Phase Registry

| Milestone | Target Version | Scope & Focus | Associated Branches | Status |
| :--- | :--- | :--- | :--- | :---: |
| **Milestone 1** | `v0.6.0` | Typed Multiplex Code Graph & Task Priors | `research/typed-multiplex-graph`<br>`feature/typed-multiplex-csr` | `[COMPLETED]` |
| **Milestone 2** | `v0.7.0` | Monotone Submodular Coverage & Dual-Mode Solvers | `research/submodular-coverage`<br>`feature/submodular-solver`<br>`experiment/knapsack-oracle-gap` | `[COMPLETED]` |
| **Milestone 3** | `v0.8.0` | Path Reasoning, Energy Scoring & Structured Context | `research/path-energy-model`<br>`feature/structured-context` | `[READY]` |
| **Milestone 4** | `v0.9.0` | Codebase Intelligence Primitives & MCP Contracts | `feature/intelligence-primitives`<br>`feature/intelligence-mcp` | `[PLANNED]` |
| **Milestone 5** | `v0.10.0` | Sequential Exploration & Adaptive Navigation | `research/adaptive-navigation`<br>`feature/adaptive-navigator` | `[PLANNED]` |
| **Milestone 6** | `v0.11.0` | 7-Tier Ablation Suite, Proofs & Engine Modularization | `experiment/ablation-suite`<br>`experiment/agent-harness-study`<br>`research/theory-proofs` | `[PLANNED]` |

> [!NOTE]
> Releases continue sequentially in `0.x` (`v0.6.0` through `v0.11.0`, and further to `v0.12.0+` if additional algorithms are developed). `v1.0.0` will NOT be released or targeted until extensive dogfooding, stability, and evaluation are established.

---

## 4. Milestone 1 (`v0.6.0`): Typed Multiplex Graph & Task Priors

### Phase 31: Formal Entity-Relation Vocabulary & Task Modeling `[COMPLETED]`
- **Branch:** `research/typed-multiplex-graph` (completed & merged into `dev`)
- **Objective:** Establish the formal mathematical types for code entities $\tau_V$, edge relations $\tau_E$, and structured task representations $q = (x, z, m)$.
- **Math Formulation:**
  - Node types: $\tau_V \in \{\text{Package}, \text{Module}, \text{File}, \text{Class}, \text{Struct}, \text{Interface}, \text{Function}, \text{Method}, \text{Test}, \text{Config}, \text{Endpoint}\}$.
  - Edge types: $\tau_E \in \{\text{Imports}, \text{Calls}, \text{References}, \text{Inherits}, \text{Implements}, \text{Contains}, \text{Reads}, \text{Writes}, \text{IsTestedBy}, \text{CoChangesWith}\}$.
  - Task representation: $q = (x, z, m)$ where $x$ is task prompt, $z \in \mathbb{R}^d$ concept embeddings/tokens, $m$ metadata.

#### Commit Breakdown
- **Commit 31.1 (`feat: Define typed entity and relation schema`)**:
  ```text
  feat: Define typed entity and relation schema

  Introduce NodeType and RelationType enums in repotrim-engine modeling
  first-class code entities (tests, configs, endpoints) and relations
  (calls, inheritance, test-linkage, co-edits) for typed graph analysis.
  ```
- **Commit 31.2 (`feat: Implement structured task representation`)**:
  ```text
  feat: Implement structured task representation

  Add TaskContext struct encapsulating raw query text, extracted intent
  concepts, seed hints, and operational metadata. Integrate concept
  extraction with existing intent resolver.
  ```
- **Commit 31.3 (`test: Add unit tests for typed entities and tasks`)**:
  ```text
  test: Add unit tests for typed entities and tasks

  Add comprehensive unit and serialization tests verifying entity and
  relation invariants, string conversions, and roundtrip JSON parsing.
  ```

---

### Phase 32: Multiplex CSR Graph Representation & Layered Transition Matrices `[COMPLETED]`
- **Branch:** `feature/typed-multiplex-csr` (completed & merged into `dev`)
- **Objective:** Upgrade the graph storage to a typed multiplex Compressed Sparse Row (CSR) representation supporting layered adjacency matrices $A = \sum_{r \in \mathcal{R}} \omega_r(q) A_r$.
- **Math Formulation:**
  $$P_q = \text{Normalize}\left(\sum_{r \in \mathcal{R}} \omega_r(q) A_r\right), \quad \mathbf{p}_q = (1 - \alpha)\mathbf{s}_q + \alpha P_q^T \mathbf{p}_q$$
  - Task-conditioned relation weights $\omega_r(q)$ (e.g., test-heavy tasks upweight $\omega_{\text{is\_tested\_by}}$).
  - Sparse forward-push ACL diffusion preserved with $O(1/\epsilon)$ local running time.

#### Commit Breakdown
- **Commit 32.1 (`feat: Implement typed multiplex CSR code graph`)**:
  ```text
  feat: Implement typed multiplex CSR code graph

  Add MultiplexCsrGraph storing per-relation sparse edge slices. Provide
  zero-allocation typed neighbor iterators and degree lookups across
  arbitrary relation subsets.
  ```
- **Commit 32.2 (`feat: Add task-conditioned transition matrix builder`)**:
  ```text
  feat: Add task-conditioned transition matrix builder

  Implement dynamic edge-weight scaling based on TaskContext hints,
  yielding normalized transition probabilities P_q conditioned on task
  intent (e.g. bugfix vs refactor vs test creation).
  ```
- **Commit 32.3 (`refactor: Adapt ACL forward-push to multiplex graph`)**:
  ```text
  refactor: Adapt ACL forward-push to multiplex graph

  Update PprSolver to operate over task-conditioned multiplex CSR
  graphs while preserving local forward-push O(1/eps) performance.
  ```
- **Commit 32.4 (`test: Validate multiplex PPR convergence and sparsity`)**:
  ```text
  test: Validate multiplex PPR convergence and sparsity

  Add property tests verifying probability conservation, teleportation
  invariants, and task-conditioned diffusion divergence against static
  uniform baselines.
  ```

---

## 5. Milestone 2 (`v0.7.0`): Monotone Submodular Coverage & Dual-Mode Solvers

### Phase 33: Probabilistic Evidence Coverage & Submodular Kernel Engine `[COMPLETED]`
- **Branch:** `research/submodular-coverage` (completed & merged into `dev`)
- **Objective:** Replace ad-hoc relevance scoring with a mathematically rigorous probabilistic coverage function exhibiting provable diminishing returns.
- **Math Formulation:**
  - Evidence distribution kernel $K(v, u; q) \in [0, 1]$ measuring information $v$ provides regarding $u$.
  - Probabilistic coverage:
    $$\mathrm{Cov}(S; q, G) = \sum_{u \in V} \omega(u, q) \left[ 1 - \prod_{v \in S} (1 - K(v, u; q)) \right]$$
  - Theorem: $\mathrm{Cov}(S)$ is normalized, non-negative, monotone increasing ($S \subseteq T \implies \mathrm{Cov}(S) \le \mathrm{Cov}(T)$), and submodular ($\Delta(x \mid A) \ge \Delta(x \mid B)$ for $A \subseteq B$).

#### Commit Breakdown
- **Commit 33.1 (`feat: Implement evidence distribution kernel`)**:
  ```text
  feat: Implement evidence distribution kernel

  Add EvidenceKernel computing pairwise entity mutual information K(v,u)
  combining graph topological distance, containment, and type linkage.
  ```
- **Commit 33.2 (`feat: Implement monotone probabilistic coverage`)**:
  ```text
  feat: Implement monotone probabilistic coverage

  Add ProbabilisticCoverage evaluator with efficient incremental delta
  computation for candidate addition exploiting log-space product sums.
  ```
- **Commit 33.3 (`test: Prove submodularity and diminishing returns`)**:
  ```text
  test: Prove submodularity and diminishing returns

  Add proptest suite verifying marginal gain monotonicity and submodular
  inequality invariants across random graph topologies.
  ```

---

### Phase 34: Multi-Objective Utility Function & Realistic Cost Accounting `[COMPLETED]`
- **Branch:** `feature/submodular-solver` (completed & merged into `dev`)
- **Objective:** Formulate the unified objective $F(S; q, G)$ and multi-factor token cost model $c(v)$.
- **Math Formulation:**
  - Utility:
    $$F(S; q, G) = \alpha \mathrm{Rel}(S, q) + \beta \mathrm{Cov}(S, q, G) + \delta \mathrm{Test}(S, q, G) - \lambda \mathrm{Red}(S)$$
  - Realistic token cost:
    $$c(v) = c_{\text{text}}(v) + c_{\text{meta}}(v) + c_{\text{rel}}(v) + c_{\text{format}}(v)$$

#### Commit Breakdown
- **Commit 34.1 (`feat: Implement multi-factor token cost estimator`)**:
  ```text
  feat: Implement multi-factor token cost estimator

  Add RealisticCostModel accurately pricing raw syntax tokens, container
  scaffolding, relation annotations, and Markdown framing overhead.
  ```
- **Commit 34.2 (`feat: Implement unified submodular utility model`)**:
  ```text
  feat: Implement unified submodular utility model

  Add SubmodularUtility combining task-conditioned relevance, coverage,
  and test evidence with configurable ablation weights.
  ```
- **Commit 34.3 (`test: Validate cost and utility monotonicity`)**:
  ```text
  test: Validate cost and utility monotonicity

  Add unit tests verifying exact budget bounding and utility scoring
  behavior across diverse code entities.
  ```

---

### Phase 35: Dual-Mode Knapsack Solvers & Research Oracle Gap Analysis `[COMPLETED]`
- **Branch:** `experiment/knapsack-oracle-gap` (completed & merged into `dev`)
- **Objective:** Introduce a dual-mode solver architecture: fast lazy CELF for production, and an exact branch-and-bound / IP oracle for small candidate sets to measure empirical approximation gaps.
- **Math Formulation:**
  - Knapsack formulation: $\max_{S \subseteq V} F(S) \text{ s.t. } \sum_{v \in S} c(v) \le B$.
  - Approximation ratio metric: $\mathrm{Ratio} = \frac{F(S_{\text{prod}})}{F(S^*)}$.

#### Commit Breakdown
- **Commit 35.1 (`feat: Add exact branch-and-bound knapsack oracle`)**:
  ```text
  feat: Add exact branch-and-bound knapsack oracle

  Implement ExactKnapsackOracle for instances with |V_cand| <= 32 to
  compute provably optimal objective values F(S*) for scientific bounds.
  ```
- **Commit 35.2 (`feat: Adapt CELF solver to submodular coverage`)**:
  ```text
  feat: Adapt CELF solver to submodular coverage

  Upgrade CelfOptimizer to maximize SubmodularUtility with lazy marginal
  gain updates and best-singleton correction guarantees.
  ```
- **Commit 35.3 (`test: Benchmark approximation ratio on test fixtures`)**:
  ```text
  test: Benchmark approximation ratio on test fixtures

  Add comparative test evaluating F(S_celf) / F(S_exact) across test
  codebases, proving empirical adherence to theoretical bounds.
  ```

---

## 6. Milestone 3 (`v0.8.0`): Path Reasoning & Structured Context

### Phase 36: Task-Relevant Path Inference & Probabilistic Energy Scoring
- **Branch:** `research/path-energy-model`
- **Objective:** Infer coherent execution and dependency paths $\mathcal{P}_q$ between task entrypoints and candidate implementations.
- **Math Formulation:**
  - Path score:
    $$\mathrm{Score}(p \mid q) = \sum_{v_i \in p} r(v_i \mid q) + \sum_{e_i \in p} \phi(e_i, q) - \kappa \cdot \mathrm{Length}(p)$$
  - Boltzmann path distribution:
    $$P(p \mid q, G) = \frac{\exp(\mathrm{Score}(p \mid q))}{\sum_{p' \in \mathcal{P}_q} \exp(\mathrm{Score}(p' \mid q))}$$
  - Soft path coverage added to utility: $\mathrm{Path}(S; q, G) = \sum_{p} w_p [1 - \prod_{v \in S \cap p} (1 - k_v)]$.

#### Commit Breakdown
- **Commit 36.1 (`feat: Implement path search and candidate generation`)**:
  ```text
  feat: Implement path search and candidate generation

  Add PathFinder discovering top-k constrained shortest and most probable
  dependency paths connecting entrypoints to relevant implementations.
  ```
- **Commit 36.2 (`feat: Implement probabilistic path energy scoring`)**:
  ```text
  feat: Implement probabilistic path energy scoring

  Add PathScorer computing Boltzmann probabilities over candidate paths
  penalizing structural distance and rewarding task relevance.
  ```
- **Commit 36.3 (`feat: Integrate path coverage into submodular utility`)**:
  ```text
  feat: Integrate path coverage into submodular utility

  Extend SubmodularUtility with soft path coverage reward ensuring
  solution pathways are preserved continuously without isolated gaps.
  ```
- **Commit 36.4 (`test: Validate path inference and probability scores`)**:
  ```text
  test: Validate path inference and probability scores

  Add unit tests validating path extraction across multi-hop call graphs
  and verifying probability distribution normalization.
  ```

---

### Phase 37: Structured Context Object & Diagnostic Explanation Metadata
- **Branch:** `feature/structured-context`
- **Objective:** Transition output from raw text dumps to a structured context object $\mathcal{C} = (V_C, E_C, M_C)$ with explicit rationale, confidence, and omission diagnostics.
- **Math Formulation:**
  - Context object schema:
    $$\mathcal{C} = \{\text{task}, \text{entrypoints}, \text{paths}, \text{evidence}, \text{omissions}, \text{confidence}, \text{budget\_used}\}$$

#### Commit Breakdown
- **Commit 37.1 (`feat: Define structured context graph object`)**:
  ```text
  feat: Define structured context graph object

  Add StructuredContext, EvidenceItem, and PathTrace models carrying
  symbol data, relation edges, token costs, and attribution rationale.
  ```
- **Commit 37.2 (`feat: Implement omission tracking and diagnostics`)**:
  ```text
  feat: Implement omission tracking and diagnostics

  Record candidate pruning reasons (budget exhaustion, redundancy, low
  marginal gain) to provide transparency to AI coding harnesses.
  ```
- **Commit 37.3 (`feat: Add structured JSON and Markdown serializers`)**:
  ```text
  feat: Add structured JSON and Markdown serializers

  Implement dual formatting for StructuredContext: machine-readable JSON
  for autonomous harnesses and optimized hierarchical Markdown for LLMs.
  ```
- **Commit 37.4 (`test: Validate context serialization and fidelity`)**:
  ```text
  test: Validate context serialization and fidelity

  Add integration tests verifying structured context formatting, token
  budget adherence, and deserialization roundtrips.
  ```

---

## 7. Milestone 4 (`v0.9.0`): Codebase Intelligence Primitives & MCP

### Phase 38: Six Core Codebase Intelligence Primitives
- **Branch:** `feature/intelligence-primitives`
- **Objective:** Expose 6 mathematically meaningful operations turning RepoTrim into a live codebase service:
  1. `locate(task)`: Entrypoint discovery via seed priors.
  2. `neighbors(symbol)`: High-value local evidence neighborhood.
  3. `trace(source, target, task)`: Most probable causal paths.
  4. `expand(symbol, budget)`: Budgeted local submodular expansion.
  5. `impact(symbol)`: Downstream blast radius and affected tests.
  6. `context(task, budget)`: End-to-end budgeted evidence package.

#### Commit Breakdown
- **Commit 38.1 (`feat: Implement locate and neighbors engine primitives`)**:
  ```text
  feat: Implement locate and neighbors engine primitives

  Add locate() returning ranked task entrypoints and neighbors()
  providing weighted typed relation subgraphs for given symbols.
  ```
- **Commit 38.2 (`feat: Implement trace and expand engine primitives`)**:
  ```text
  feat: Implement trace and expand engine primitives

  Add trace() resolving ranked paths between endpoints and expand()
  performing localized knapsack packing around a focal symbol.
  ```
- **Commit 38.3 (`feat: Implement unified context engine primitive`)**:
  ```text
  feat: Implement unified context engine primitive

  Unify end-to-end context generation under context() returning
  StructuredContext with full path and omission diagnostics.
  ```
- **Commit 38.4 (`test: Unit test core engine intelligence primitives`)**:
  ```text
  test: Unit test core engine intelligence primitives

  Add unit tests validating each primitive against real repository
  fixtures and asserting expected output schemas.
  ```

---

### Phase 39: CLI Command Suite & Native MCP JSON-RPC Contracts
- **Branch:** `feature/intelligence-mcp`
- **Objective:** Expose the 6 primitives across CLI commands and native Model Context Protocol tools with structured JSON schemas.

#### Commit Breakdown
- **Commit 39.1 (`feat: Add locate, trace, and expand CLI commands`)**:
  ```text
  feat: Add locate, trace, and expand CLI commands

  Expose new primitives via CLI commands repotrim locate, repotrim trace,
  and repotrim expand with pretty ANSI table and JSON outputs.
  ```
- **Commit 39.2 (`feat: Update MCP server tool catalog with primitives`)**:
  ```text
  feat: Update MCP server tool catalog with primitives

  Add locate_entrypoints, trace_paths, and expand_symbol tools to MCP
  server, updating trim_context schema to return StructuredContext.
  ```
- **Commit 39.3 (`test: Test MCP tool invocation and CLI compatibility`)**:
  ```text
  test: Test MCP tool invocation and CLI compatibility

  Add integration tests verifying MCP tool registration, JSON-RPC schema
  validation, and stdio execution.
  ```

---

## 8. Milestone 5 (`v0.10.0`): Sequential & Adaptive Navigation

### Phase 40: State-Aware Sequential Exploration & Action Engine
- **Branch:** `research/adaptive-navigation`
- **Objective:** Model the interaction between an AI harness and RepoTrim as a stateful, sequential observation process.
- **Math Formulation:**
  - State: $s_t = (G, q, H_t, B_t, o_t)$ where $H_t$ is history, $B_t$ remaining budget, $o_t$ observations.
  - Action space: $\mathcal{A} = \{\text{inspect}(v), \text{expand}(v), \text{trace}(u, v), \text{test\_link}(v), \text{stop}\}$.
  - Action cost: $c(s_t, a_t) > 0$.

#### Commit Breakdown
- **Commit 40.1 (`feat: Implement sequential navigation state model`)**:
  ```text
  feat: Implement sequential navigation state model

  Add NavigationState, Observation, and NavigationAction types tracking
  agent exploration history, remaining budget, and discovered evidence.
  ```
- **Commit 40.2 (`feat: Implement action candidate generator`)**:
  ```text
  feat: Implement action candidate generator

  Add ActionGenerator proposing feasible navigation actions (symbol
  inspection, neighbor expansion, path tracing) from current frontier.
  ```
- **Commit 40.3 (`test: Test navigation state transitions and cost tracking`)**:
  ```text
  test: Test navigation state transitions and cost tracking

  Add unit tests validating state updates, observation recording, and
  strict remaining budget decrementation.
  ```

---

### Phase 41: Adaptive Submodular Greedy Policy & Dynamic Gain Updates
- **Branch:** `feature/adaptive-navigator`
- **Objective:** Implement the adaptive greedy policy selecting the action with highest conditional expected marginal gain per cost:
  $$a^* = \arg\max_{a \in \mathcal{A}} \frac{\Delta(a \mid \psi)}{c(a)}$$
- **Math Formulation:**
  - Conditional marginal gain: $\Delta(a \mid \psi) = \mathbb{E}[U(\text{updated evidence}) - U(\psi) \mid \psi, a]$.
  - Exploit adaptive submodularity (Golovin & Krause, 2011) to guarantee near-optimal sequential information gathering under budget $B$.

#### Commit Breakdown
- **Commit 41.1 (`feat: Implement adaptive marginal gain estimator`)**:
  ```text
  feat: Implement adaptive marginal gain estimator

  Add AdaptiveGainEstimator computing expected utility improvements of
  actions given current partial observation history psi.
  ```
- **Commit 41.2 (`feat: Implement adaptive greedy navigation policy`)**:
  ```text
  feat: Implement adaptive greedy navigation policy

  Add AdaptiveNavigator orchestrating multi-step exploration loops,
  updating priors dynamically upon observing new symbols.
  ```
- **Commit 41.3 (`feat: Expose navigate command in CLI and MCP`)**:
  ```text
  feat: Expose navigate command in CLI and MCP

  Add repotrim navigate command and navigate_codebase MCP tool enabling
  autonomous harnesses to execute guided exploration sessions.
  ```
- **Commit 41.4 (`test: Validate adaptive exploration efficiency`)**:
  ```text
  test: Validate adaptive exploration efficiency

  Add integration tests proving adaptive navigation discovers required
  dependencies with fewer actions than static whole-graph selection.
  ```

---

## 9. Milestone 6 (`v0.11.0`): Empirical Validation, Proofs & Engine Modularization

### Phase 42: Comprehensive 7-Tier Ablation Benchmark Harness
- **Branch:** `experiment/ablation-suite`
- **Objective:** Expand `crates/engine/src/eval.rs` to benchmark all 7 ablation variants on identical repository tasks:
  1. Full context
  2. Lexical (BM25)
  3. Graph-only
  4. PPR-only
  5. Static submodular utility (RepoTrim v0.7)
  6. Path-aware context (RepoTrim v0.8)
  7. Adaptive navigation (RepoTrim v0.10)

#### Commit Breakdown
- **Commit 42.1 (`feat: Implement 7-tier ablation benchmark runner`)**:
  ```text
  feat: Implement 7-tier ablation benchmark runner

  Upgrade BenchmarkRunner in eval.rs to execute all 7 ablation strategies
  under identical token and action budgets across polyglot fixtures.
  ```
- **Commit 42.2 (`feat: Add exploration cost metrics to evaluation`)**:
  ```text
  feat: Add exploration cost metrics to evaluation

  Record input tokens, output tokens, tool call counts, inspected files,
  and calculate ECR (Exploration Cost Reduction) and SPT ratios.
  ```
- **Commit 42.3 (`test: Run full ablation suite and generate study report`)**:
  ```text
  test: Run full ablation suite and generate study report

  Execute comprehensive evaluation across small, medium, and monorepo
  fixtures, exporting study results to docs/benchmarks/ablation_study.md.
  ```

---

### Phase 43: Agent Harness Exploration Measurement & Real-World Validation
- **Branch:** `experiment/agent-harness-study`
- **Objective:** Measure real agent performance (e.g. Claude Code / Antigravity / SWE-bench task harnesses) comparing task success rates and token spend with and without RepoTrim codebase intelligence.

#### Commit Breakdown
- **Commit 43.1 (`feat: Add agent harness trace recorder`)**:
  ```text
  feat: Add agent harness trace recorder

  Implement AgentTraceRecorder logging tool invocations, prompt token
  consumption, and task completion latency during live agent sessions.
  ```
- **Commit 43.2 (`docs: Document empirical findings and research paper`)**:
  ```text
  docs: Document empirical findings and research paper

  Author docs/benchmarks/agent_exploration_study.md documenting empirical
  findings, token reduction curves, and academic evaluation metrics.
  ```

---

### Phase 44: Formal Theoretical Proofs, Citations & Engine Modularization
- **Branch:** `research/theory-proofs`
- **Objective:** Document formal mathematical proofs (monotonicity, submodularity, knapsack bounds), add all citations per `AGENTS.md`, and refine engine module organization for `v0.11.0`.

#### Commit Breakdown
- **Commit 44.1 (`docs: Document formal mathematical proofs and theorems`)**:
  ```text
  docs: Document formal mathematical proofs and theorems

  Add in-code docstrings and docs/THEORY.md proving non-negativity,
  monotonicity, and submodularity of the probabilistic coverage kernel.
  ```
- **Commit 44.2 (`docs: Add academic citations to README and docs`)**:
  ```text
  docs: Add academic citations to README and docs

  Add formal citations for Golovin & Krause (2011), Sviridenko (2004),
  Nemhauser et al. (1978), and Haveliwala (2003) across documentation.
  ```
- **Commit 44.3 (`refactor: Modularize public engine exports for v0.11.0`)**:
  ```text
  refactor: Modularize public engine exports for v0.11.0

  Clean up repotrim-engine exports, ensuring modular trait boundaries
  and clean separation between production and oracle modules.
  ```
- **Commit 44.4 (`chore: Prepare v0.11.0 release and documentation`)**:
  ```text
  chore: Prepare v0.11.0 release and documentation

  Update Cargo.toml workspace versions to 0.11.0, compile master
  changelog, and ensure all CI verification checks pass cleanly.
  ```

---

## 10. Execution Summary & Next Immediate Action
 
| Step | Action | Command / Target | Status |
| :--- | :--- | :--- | :---: |
| **1** | Complete Phase 31 on `research/typed-multiplex-graph` | Commits 31.1, 31.2, 31.3 | `[DONE]` |
| **2** | Complete Phase 32 on `feature/typed-multiplex-csr` | Commits 32.1, 32.2, 32.3, 32.4 | `[DONE]` |
| **3** | Milestone 1 (`v0.6.0`) Complete & Merged to `dev` | Typed Multiplex CSR & Task Priors | `[DONE]` |
| **4** | Create research branch for Phase 33 | `git checkout -b research/submodular-coverage` | `[READY]` |
| **5** | Begin Phase 33 (Commit 33.1) | Implement `EvidenceKernel` in `crates/engine` | `[NEXT]` |
