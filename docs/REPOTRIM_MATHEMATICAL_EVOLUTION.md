# RepoTrim — Mathematical Evolution & Codebase Intelligence Roadmap

**Status:** Design proposal / research roadmap  
**Target:** Turn RepoTrim from a repository context-selection engine into a task-aware codebase intelligence layer for AI harnesses.  
**Primary objective:** Reduce the exploration cost (especially tokens and tool calls) required by an AI coding harness to find a viable path through a repository and complete a task.  
**Secondary objective:** Establish a mathematically coherent formulation that can support a small, defensible research paper.

---

## 1. Executive Summary

RepoTrim already contains a strong mathematical foundation:

- a graph representation of a codebase;
- task/seed-aware Personalized PageRank (PPR) for relevance propagation;
- token-aware selection;
- a greedy/CELF-style submodular selection layer;
- context formatting and retrieval infrastructure;
- additional signals such as structural relationships, slicing, history, and related metadata.

The main issue is **not that the current mathematics is wrong**. The issue is that the mathematical objective is narrower than the intended product vision.

Today, the mathematical center can be understood approximately as:

\[
\max_{S \subseteq V} U(S\mid q,G)
\quad\text{s.t.}\quad C(S)\le B
\]

where a set of relevant code elements is selected under a token budget.

The intended product is broader:

> Given a task, a codebase graph, and a resource budget, provide the AI harness with the smallest/cheapest amount of structured evidence needed to navigate toward a successful task solution.

That is closer to **budgeted, task-aware, sequential codebase navigation** than plain context retrieval.

The recommended evolution is therefore:

```text
Current

Codebase → Graph → PPR relevance → Submodular selection → Context

Target

Codebase
   ↓
Codebase Graph + evidence model
   ↓
Task-conditioned prior
   ↓
Relevant-node / relevant-path inference
   ↓
Information-gain / redundancy-aware utility
   ↓
Budgeted navigation policy
   ↓
Minimal sufficient evidence
   ↓
AI Harness
```

The key architectural principle is:

> **Do not remove PPR or CELF. Demote them from “the theory” to principled components inside a larger mathematical framework.**

PPR can estimate structural/task relevance. Submodular optimization can select non-redundant evidence. A sequential/adaptive layer can model what to inspect next and how observations change the next decision.

---

# 2. Product Goal

## 2.1 The actual problem

An AI coding harness commonly spends tokens while discovering a codebase:

```text
Task
 ↓
search
 ↓
open file
 ↓
inspect symbol
 ↓
search caller
 ↓
open dependency
 ↓
inspect tests
 ↓
search configuration
 ↓
repeat...
```

The model is repeatedly paying for **codebase discovery** before it can spend its context on planning and implementation.

RepoTrim should act as a codebase intelligence layer between the harness and the repository:

```text
User task
   ↓
AI Harness
   ↓
RepoTrim
   ├── codebase graph
   ├── symbol index
   ├── dependency relations
   ├── structural relations
   ├── tests and evidence
   ├── likely entry points
   ├── likely paths
   └── uncertainty/confidence
   ↓
AI Model / Harness
```

The expected benefit is not merely fewer raw tokens. It is **less exploratory work**.

---

## 2.2 Product-level objective

Define exploration cost as a vector:

\[
\mathrm{Cost}(\pi)=
\alpha T_{in}(\pi)
+\beta T_{out}(\pi)
+\gamma N_{tool}(\pi)
+\delta N_{file}(\pi)
+\eta L(\pi)
\]

where:

- \(T_{in}\): input tokens consumed;
- \(T_{out}\): output tokens generated;
- \(N_{tool}\): tool calls;
- \(N_{file}\): files/symbols inspected;
- \(L\): latency or equivalent runtime cost;
- \(\alpha,\beta,\gamma,\delta,\eta\): user/system-dependent weights.

For a simpler first paper, use token cost as the primary constraint and report the other terms as secondary measurements.

The product objective becomes:

\[
\boxed{
\min_{\pi} \mathrm{Cost}(\pi)
\quad\text{s.t.}\quad
\Pr(\mathrm{success}\mid q,G,\pi)\ge \tau
}
\]

An equivalent maximization form is:

\[
\boxed{
\max_{\pi} \Pr(\mathrm{success}\mid q,G,\pi)
\quad\text{s.t.}\quad
\mathrm{Cost}(\pi)\le B
}
\]

This captures the central idea much better than “retrieve top-k chunks.”

---

# 3. Mathematical Objects

Use the following objects as the formal vocabulary of the project.

## 3.1 Codebase graph

Represent the repository as a typed, directed, weighted graph:

\[
G=(V,E,\tau_V,\tau_E,w)
\]

where:

- \(V\) = code entities;
- \(E\) = relationships;
- \(\tau_V(v)\) = node type;
- \(\tau_E(e)\) = edge type;
- \(w(e)\) = edge weight.

Possible node types:

```text
Repository
Package
Module
File
Class
Struct
Interface
Function
Method
Type
Test
Endpoint
Config
Schema / migration
```

Possible edge types:

```text
imports
calls
references
inherits
implements
contains
belongs_to
returns
accepts
reads
writes
configures
is_tested_by
co_changes_with
```

The graph should be typed rather than treated as an unstructured adjacency matrix.

### Why this matters

A call edge and a co-change edge do not have the same semantics. A test edge should not necessarily carry the same propagation strength as an import edge.

Therefore, instead of one adjacency matrix, consider a multiplex graph:

\[
A = \sum_{r\in\mathcal R} \omega_r A_r
\]

where:

- \(A_r\) is the adjacency matrix for relation type \(r\);
- \(\omega_r\ge0\) is the learned or calibrated importance of relation \(r\).

This preserves the current graph-based philosophy while making it mathematically more expressive.

---

# 4. Task Representation

The task should not be treated only as a string used for search.

Represent a task as:

\[
q=(x,z,m)
\]

where:

- \(x\) = natural-language task text;
- \(z\) = extracted task concepts / entities / symbols;
- \(m\) = task metadata if available.

Examples of extracted concepts:

```text
“Fix refresh-token failure in OAuth middleware”

Concepts:
- authentication
- refresh token
- OAuth
- middleware
- failure/bug
- token rotation
```

The concepts can seed the graph via lexical matching, symbol resolution, embeddings, tests, history, or other signals.

---

# 5. Task-Conditioned Relevance

## 5.1 Current PPR role

RepoTrim's current Personalized PageRank is a good primitive for propagating relevance from task-derived seeds through the code graph.

The general PPR form is:

\[
\mathbf p
=
(1-\alpha)\mathbf s
+
\alpha P^T\mathbf p
\]

where:

- \(P\) is a normalized transition matrix;
- \(\mathbf s\) is the task-conditioned seed distribution;
- \(\alpha\) controls propagation;
- \(\mathbf p\) is the resulting relevance vector.

The important change should be conceptual:

> PPR should be treated as a **prior over where task-relevant information is likely to live**, not as the final definition of usefulness.

Personalized / topic-sensitive PageRank is a well-established way to bias graph ranking toward a query or context rather than using one global importance score. See Haveliwala (2003).  
Reference: https://doi.org/10.1109/TKDE.2003.1208999

## 5.2 Multiplex/task-conditioned PPR

For typed code graphs:

\[
P_q
=
\mathrm{Normalize}
\left(
\sum_{r\in\mathcal R}
\omega_r(q)A_r
\right)
\]

Then:

\[
\mathbf p_q
=
(1-\alpha)\mathbf s_q
+
\alpha P_q^T\mathbf p_q
\]

This allows, for example, a test-fixing task to increase the weight of `is_tested_by` edges, while a refactoring task can give more weight to structural and dependency edges.

### Important design rule

Do **not** add arbitrary relation weights until they can be measured or calibrated.

Start with fixed priors, then evaluate learned/optimized weights experimentally.

---

# 6. The Core Utility Function

This is the most important mathematical improvement.

Instead of letting one relevance score define selection, define the value of a selected context set explicitly.

Let \(S\subseteq V\) be the current selected set.

A useful starting model is:

\[
\boxed{
U(S\mid q,G)
=
\alpha I(S;q)
+\beta C(S;q,G)
+\gamma P(S;q,G)
+\delta H(S;G)
-\lambda R(S)
-\rho E(S;q,G)
}
\]

where:

- \(I\): task/information relevance;
- \(C\): structural/dependency coverage;
- \(P\): path coverage;
- \(H\): historical/evidence signal;
- \(R\): redundancy;
- \(E\): unresolved uncertainty or exploration risk;
- coefficients are non-negative weights.

Do not assume this exact weighted sum is automatically the correct final function. It is the initial research hypothesis.

The research task is to establish which components are necessary, what properties the resulting function has, and which terms should be omitted.

---

# 7. Information Relevance

A simple first definition:

\[
I(S;q)=\sum_{v\in V}\mathbf 1[v\in S]r(v\mid q,G)
\]

where \(r(v\mid q,G)\) can combine:

\[
r(v\mid q,G)
=
\theta_1r_{lex}
+\theta_2r_{graph}
+\theta_3r_{semantic}
+\theta_4r_{history}
+\theta_5r_{test}
\]

Potential signals:

- lexical match between task and symbol/file names;
- PPR score;
- embedding similarity;
- graph distance;
- test linkage;
- recent/co-change history;
- API/entry-point priors.

The weights \(\theta_i\) should eventually be calibrated using held-out tasks rather than manually tuned forever.

---

# 8. Coverage

A selected node can cover more than itself.

Define an evidence neighborhood:

\[
N_q(v)\subseteq V
\]

or more generally an evidence distribution:

\[
K(v,u;q)
\in[0,1]
\]

meaning how much selecting \(v\) gives information about entity \(u\).

Then:

\[
C(S;q,G)
=
\sum_{u\in V}
\omega(u,q)
\left[
1-
\prod_{v\in S}(1-K(v,u;q))
\right]
\]

This is a probabilistic coverage-style function.

Its useful property is diminishing returns: once the important evidence around \(u\) is already covered, selecting another nearby node adds less new value.

This form is a natural candidate for a monotone submodular component.

---

# 9. Redundancy

Redundancy is critical for token efficiency.

If two functions expose nearly the same information, returning both may waste context.

A simple pairwise redundancy term is:

\[
R(S)=
\sum_{u,v\in S, u<v}
\mathrm{sim}(u,v)
\]

where `sim` can combine:

- textual similarity;
- same-file proximity;
- same neighborhood;
- similar signatures;
- overlapping descendants/callers;
- embedding similarity.

However, pairwise redundancy is not automatically compatible with the monotone-submodular assumptions needed for standard guarantees.

### Recommendation

Prefer a coverage construction that naturally exhibits diminishing returns over an arbitrary “subtract pairwise similarity” expression if theoretical guarantees are a priority.

In other words:

```text
Better for theory:
coverage / facility-location / probabilistic coverage

Riskier for theory:
large hand-built weighted sum with arbitrary pairwise penalties
```

This distinction should be explicit in the paper.

---

# 10. Submodularity

A set function \(f:2^V\rightarrow\mathbb R\) is submodular if:

\[
f(A)+f(B)
\ge
f(A\cup B)+f(A\cap B)
\]

for all \(A,B\subseteq V\).

An equivalent diminishing-returns form is:

\[
A\subseteq B,
\quad x\notin B
\Rightarrow
f(x\mid A)\ge f(x\mid B)
\]

where:

\[
f(x\mid S)=f(S\cup\{x\})-f(S)
\]

Interpretation for RepoTrim:

> The first relevant function/module you add may be extremely informative; after you already have several closely related functions, another similar function should usually add less new information.

This is exactly the behavior desired for context selection.

Classic results establish strong approximation guarantees for greedy maximization of monotone submodular functions, including the \((1-1/e)\) result under a cardinality constraint.  
Reference: Nemhauser, Wolsey, Fisher (1978), “An Analysis of the Approximations for Maximizing Submodular Set Functions.”

Useful survey/reference entry: https://people.csail.mit.edu/stefje/fall15/notes_lecture15.pdf

---

# 11. Token Budget as a Knapsack Constraint

Let each candidate entity have a token cost:

\[
c(v)>0
\]

and let the total context budget be \(B\).

Then:

\[
C(S)=\sum_{v\in S}c(v)
\]

and the selection problem becomes:

\[
\boxed{
\max_{S\subseteq V} f(S)
\quad
\text{s.t.}
\quad
\sum_{v\in S}c(v)\le B
}
\]

This is the classic monotone submodular maximization problem under a knapsack constraint.

Sviridenko (2004) gave a \((1-1/e)\)-approximation algorithm for monotone submodular maximization with one knapsack constraint using partial enumeration, at higher computational cost.  
Reference: https://doi.org/10.1016/S0167-6377(03)00062-2

This is important because it gives RepoTrim a path toward **honest mathematical guarantees** instead of merely calling a greedy heuristic “optimal.”

---

# 12. What to do with CELF

CELF is highly relevant because it is a lazy-greedy acceleration technique for submodular selection. The classic work by Leskovec et al. demonstrates that exploiting diminishing returns can greatly reduce the number of expensive objective evaluations while retaining the greedy choice sequence.  
Reference: https://doi.org/10.1145/1281192.1281239

RepoTrim should keep this optimization layer.

However:

> **CELF is an acceleration mechanism for a greedy process; it is not itself a proof that the resulting solution is globally optimal.**

Therefore documentation should distinguish:

```text
Mathematical objective
        ↓
Submodularity assumptions
        ↓
Greedy / approximate optimization
        ↓
CELF = lazy evaluation optimization
```

This terminology will make the paper much more defensible.

---

# 13. Why the Current “PPR → CELF” Pipeline Is Not Yet the Full Theory

The current pipeline is strong for selecting a static context set.

But an AI harness does not behave like a static selector.

The harness may do this:

```text
Ask RepoTrim for entry point
 ↓
Inspect result
 ↓
Update hypothesis
 ↓
Ask for callers
 ↓
Inspect tests
 ↓
Reject one branch
 ↓
Follow another branch
```

This is **sequential and adaptive**.

The information revealed by earlier actions changes what should be selected next.

Therefore the mathematical model should eventually support a policy:

\[
\pi=(a_1,a_2,\dots,a_T)
\]

where each action depends on the observations received so far.

---

# 14. Sequential Navigation Model

Represent the harness/RepoTrim interaction as a stateful process.

A state can contain:

\[
s_t=(G,q,H_t,B_t,o_t)
\]

where:

- \(G\): codebase graph;
- \(q\): task;
- \(H_t\): currently known evidence/history;
- \(B_t\): remaining budget;
- \(o_t\): observations gathered so far.

Possible actions:

```text
inspect(node)
expand(node)
follow(edge)
get_callers(node)
get_callees(node)
get_tests(node)
get_related_history(node)
get_path(a,b)
select_context(subgraph)
```

Each action has a cost:

\[
c(s,a)>0
\]

and an information gain / task utility:

\[
g(s,a)
\]

The goal becomes:

\[
\boxed{
\max_{\pi}
\mathbb E[U(H_T,q,G)]
\quad
\text{s.t.}
\quad
\mathbb E\left[\sum_{t=1}^{T}c(s_t,a_t)\right]\le B
}
\]

This is a budgeted sequential decision problem.

A general Markov decision process (MDP) is a standard mathematical framework for states, actions, transitions and rewards in sequential decision-making. Sutton & Barto provide the canonical treatment.  
Reference: https://mitpress.ublish.com/book/reinforcement-learning-an-introduction-2

### Important warning

Do **not** immediately turn RepoTrim into a reinforcement-learning project.

That would likely make the system unnecessarily complex and would create a data/learning requirement that is not necessary for the first paper.

Use the MDP framing mainly as a clean mathematical vocabulary for the sequential problem.

---

# 15. The More Promising Theory: Adaptive Submodularity

For RepoTrim, adaptive submodularity may be more directly useful than general reinforcement learning.

Adaptive submodularity extends ordinary submodular diminishing returns to situations where decisions are made sequentially and observations are revealed along the way.

Golovin and Krause (2011) formalize adaptive submodularity and show that under suitable conditions adaptive greedy policies can have strong approximation guarantees.  
Reference: https://doi.org/10.1613/JAIR.3278

This is highly relevant to RepoTrim because:

```text
Action 1:
inspect AuthService

Observation:
it calls TokenManager

Action 2:
inspect TokenManager

Observation:
tests are in RefreshTokenTest

Action 3:
inspect RefreshTokenTest
```

The value of the next action depends on what the previous action revealed.

That is precisely the type of setting in which adaptive decision theory becomes relevant.

---

# 16. Recommended Mathematical Architecture

Use three mathematical layers.

## Layer A — Structural prior

Use the graph to estimate likely relevance:

\[
\mathbf p_q
=
\mathrm{PPR}(G,q)
\]

This is cheap, interpretable, and pre-computable/cachable.

---

## Layer B — Information selection

Use a set utility:

\[
f(S\mid q,G)
\]

with coverage and diminishing returns.

Solve:

\[
\max_S f(S)
\quad
\text{s.t.}
\quad
c(S)\le B
\]

Use greedy/CELF in production; compare against stronger offline solvers on small instances to estimate the approximation gap.

---

## Layer C — Adaptive navigation

Model interaction as a policy:

\[
\pi^*(q,G,B)
\]

that chooses the next evidence request according to current knowledge.

In an initial version:

```text
PPR prior
 → choose next candidate by gain/cost
 → observe evidence
 → recompute gains
 → repeat
```

This may already produce a strong practical system without ML training.

---

# 17. A Concrete Utility for the First Research Version

Start simple.

Define:

\[
F(S;q,G)
=
\alpha\,Rel(S,q)
+
\beta\,Cov(S,q,G)
+
\gamma\,Path(S,q,G)
+
\delta\,Test(S,q,G)
-
\lambda\,Red(S)
\]

Do not include every possible feature immediately.

### 17.1 Relevance

\[
Rel(S,q)
=
\sum_{v\in S}r(v\mid q,G)
\]

where \(r\) initially comes from normalized combinations of lexical relevance and PPR.

### 17.2 Coverage

\[
Cov(S,q,G)
=
\sum_{u\in V}\omega_u
\left[1-\prod_{v\in S}(1-K(v,u))\right]
\]

### 17.3 Path coverage

Let \(\mathcal P_q\) be plausible task-relevant paths.

\[
Path(S,q,G)
=
\sum_{p\in\mathcal P_q}w_p\,\mathbf 1[S\cap p\neq\varnothing]
\]

A soft version is preferable in practice:

\[
Path(S,q,G)
=
\sum_{p\in\mathcal P_q}
 w_p
\left[
1-\prod_{v\in S\cap p}(1-k_v)
\right]
\]

This creates a notion of “coverage of likely solution routes.”

### 17.4 Tests

Tests should not simply be “more relevant code.” They are evidence about correctness.

Define:

\[
Test(S,q,G)=
\sum_{t\in Tests(q)}
\omega_t
\left[1-\prod_{v\in S}(1-K(v,t))\right]
\]

### 17.5 Redundancy

Prefer a coverage-based diminishing-return construction where possible. Keep explicit pairwise redundancy only as a controlled ablation.

---

# 18. The Token Model Should Also Improve

The current project already reasons about token cost. The next step is to make cost more realistic.

Do not define cost only as:

\[
c(v)=\text{tokens of node body}
\]

Instead use:

\[
c(v)=
 c_{text}(v)
+c_{metadata}(v)
+c_{relations}(v)
+c_{format}(v)
\]

For a specific harness, you can measure actual serialized token cost.

At first use an exact tokenizer when available.

For cross-model evaluation, report both:

1. exact cost under the target tokenizer;
2. normalized structural cost independent of a particular tokenizer.

This prevents a paper from accidentally becoming tied to one model vendor.

---

# 19. Context Is Not Only a Set — It Can Be a Structured Object

The output given to the harness should not always be a flat list of chunks.

Define a context object:

\[
\mathcal C=
(V_C,E_C,M_C)
\]

where:

- \(V_C\): selected code entities;
- \(E_C\): selected relationships/paths;
- \(M_C\): metadata such as confidence, role, source, and token cost.

Example conceptual output:

```json
{
  "task": "fix refresh token rotation bug",
  "entrypoints": ["AuthMiddleware"],
  "likely_paths": [
    [
      "AuthMiddleware",
      "TokenValidator",
      "RefreshTokenService",
      "TokenStore"
    ]
  ],
  "related_tests": ["RefreshTokenTest"],
  "candidates": [
    {
      "symbol": "RefreshTokenService.rotate",
      "relevance": 0.93,
      "confidence": 0.88,
      "token_cost": 143
    }
  ],
  "omitted": {
    "count": 182,
    "reason": "low expected marginal information"
  }
}
```

This allows the harness to reason over a map rather than blindly reading text.

---

# 20. Path Inference

The system should explicitly support task paths.

A path is:

\[
p=(v_1,v_2,\dots,v_k)
\]

with:

\[
(v_i,v_{i+1})\in E
\]

Define path score:

\[
Score(p\mid q)
=
\sum_i r(v_i\mid q)
+
\sum_i \phi(e_i,q)
-
\kappa\,Length(p)
\]

where \(\phi(e_i,q)\) scores the usefulness of each relationship.

A stronger probabilistic form is:

\[
P(p\mid q,G)
\propto
\exp(Score(p\mid q,G))
\]

Normalize only if needed.

### Why probability is useful

The system can then say:

```text
Path A: 0.61
Path B: 0.23
Path C: 0.09
```

rather than making a brittle binary claim:

```text
This is the path.
```

This uncertainty can guide further exploration.

---

# 21. Navigation as Gain / Cost

At each step, given current evidence \(S\), choose the next action/node:

\[
a^*
=
\arg\max_{a}
\frac{
\Delta U(a\mid S)
}{c(a)}
\]

where:

\[
\Delta U(a\mid S)
=
U(S\cup\{a\})-U(S)
\]

This is the natural bridge between current CELF-style selection and future task navigation.

The crucial improvement is that after observing new evidence, recompute the gains.

---

# 22. Adaptive Version

Let \(\psi\) represent the current partial observation state.

Define conditional expected marginal gain:

\[
\Delta(a\mid\psi)
=
\mathbb E
\left[
U(\mathrm{updated\ evidence})-U(\psi)
\mid\psi,a
\right]
\]

Then choose:

\[
a^*
=
\arg\max_a
\frac{\Delta(a\mid\psi)}{c(a)}
\]

and update \(\psi\).

If the resulting objective can be shown to satisfy adaptive submodularity and adaptive monotonicity, adaptive greedy may admit approximation guarantees. This is exactly why adaptive submodularity is worth investigating instead of immediately resorting to reinforcement learning.  
Reference: https://doi.org/10.1613/JAIR.3278

---

# 23. Formal Research Questions

The paper/project should answer a small number of clear questions.

## RQ1 — Can codebase structure reduce agent exploration cost?

Compare a harness with and without RepoTrim.

Metrics:

- input tokens;
- output tokens;
- tool calls;
- files inspected;
- successful task completion;
- latency.

## RQ2 — Does graph-aware retrieval outperform lexical retrieval under the same budget?

Compare:

```text
BM25 / lexical
embedding-only
PPR-only
PPR + greedy selection
full proposed method
```

## RQ3 — Does redundancy-aware selection improve information per token?

Ablate redundancy/coverage terms.

## RQ4 — Does sequential/adaptive navigation reduce exploration further?

Compare:

```text
static top-k
static budgeted context
adaptive navigation
```

## RQ5 — Does the method generalize across repository sizes?

Evaluate:

```text
small repositories
medium repositories
large repositories / monorepos
```

This is crucial because the project is intentionally not only for giant repositories.

---

# 24. Baselines

Do not compare only against “all context.” Use several increasingly strong baselines.

### Baseline A — Full context

Give the harness all available context within the allowed maximum.

### Baseline B — Lexical retrieval

BM25 or repository grep/search-based retrieval.

### Baseline C — Semantic retrieval

Embedding similarity over files/symbols/chunks.

### Baseline D — Graph-only

Use graph proximity without submodular selection.

### Baseline E — PPR-only

Rank by PPR, take a fixed budget.

### Baseline F — PPR + budgeted greedy

The current style of architecture.

### Baseline G — Proposed adaptive system

Task-aware relevance + coverage/redundancy + path reasoning + adaptive budgeted navigation.

This ablation ladder will make it much easier to show which mathematical addition actually matters.

---

# 25. Experimental Protocol

Use a fixed set of repositories and tasks.

For each task:

1. initialize a clean harness session;
2. provide the same natural-language task;
3. constrain the token/tool budget;
4. run one baseline;
5. record all exploration actions;
6. evaluate task success;
7. reset the environment;
8. repeat for the next method.

Use multiple budgets, for example:

```text
2k
4k
8k
16k
32k
```

Do not only report one budget.

---

# 26. Core Metrics

## 26.1 Task success

Primary correctness metric:

\[
Success(q)\in\{0,1\}
\]

or graded success if the benchmark allows partial credit.

## 26.2 Token reduction

\[
Reduction
=
1-
\frac{Tokens_{method}}{Tokens_{baseline}}
\]

## 26.3 Exploration cost reduction

\[
ECR
=
1-
\frac{Cost_{method}}{Cost_{baseline}}
\]

## 26.4 Success per token

\[
SPT
=
\frac{SuccessRate}{MeanTokens}
\]

## 26.5 Utility per token

For internal algorithm evaluation:

\[
UPT
=
\frac{U(S)}{C(S)}
\]

## 26.6 Navigation efficiency

A useful measure:

\[
NE
=
\frac{\text{useful task-relevant discoveries}}{\text{exploration actions}}
\]

The precise numerator should be defined before evaluation and ideally validated with task traces.

---

# 27. Approximation and Theoretical Guarantees

The project should only claim a theorem if its assumptions are actually satisfied.

A clean target theorem would look like:

> Assume \(F(S)\) is non-negative, monotone, and submodular, and costs are non-negative. Under a single knapsack constraint, algorithm A returns a feasible set with approximation ratio \(\rho\) relative to the optimum.

Do not state a guarantee for the real implementation until:

1. the exact objective is formalized;
2. monotonicity has been proved;
3. submodularity has been proved;
4. the algorithm matches the theorem assumptions.

Sviridenko (2004) provides a classical \((1-1/e)\) result for the monotone single-knapsack case, but it is not the same algorithm as every practical greedy/CELF implementation.  
Reference: https://doi.org/10.1016/S0167-6377(03)00062-2

Recent literature continues to study stronger guarantees and practical algorithms for constrained submodular optimization; this should be treated as an active technical area rather than a solved detail.  
See, for example: https://doi.org/10.1007/s10878-024-01214-x

---

# 28. A Safer Production/Research Split

There should be two algorithmic modes.

## Fast production mode

```text
PPR prior
→ approximate utility
→ CELF/lazy greedy
→ compact graph/context
→ harness
```

Goals:

- low latency;
- low memory;
- predictable behavior;
- easy integration.

## Research oracle mode

For small candidate graphs only:

```text
exact / stronger solver
→ compare against greedy
→ estimate approximation gap
```

This is extremely useful scientifically.

For example, if candidate set size is small enough to enumerate, compute:

\[
F(S^*)
\]

exactly and compare production output:

\[
Ratio
=
\frac{F(S_{prod})}{F(S^*)}
\]

This turns “our greedy method seems good” into an actual measured approximation gap on controlled instances.

---

# 29. Complexity Goals

Do not optimize only the objective quality.

The system exists to save the harness tokens; it must not consume more compute than the savings justify.

Track:

\[
T_{index}
,
T_{query}
,
T_{selection}
,
T_{serialization}
\]

and memory:

\[
M_{graph}, M_{cache}
\]

A useful operational condition is:

\[
Cost_{RepoTrim}
<
Expected\ Exploration\ Cost\ Saved
\]

For interactive use, query-time selection should ideally be small relative to the cost of one or several unnecessary model calls.

---

# 30. Cache Strategy

Because the graph changes much more slowly than individual tasks, separate repository-time work from query-time work.

## Repository-time

Compute:

- AST/symbol graph;
- relation matrices;
- token estimates;
- embeddings if used;
- history features;
- indexes;
- cached graph operators.

## Query-time

Compute:

- task parsing;
- seed distribution;
- PPR / local diffusion;
- candidate expansion;
- budgeted selection;
- output formatting.

This is important for making the mathematical model practical.

---

# 31. Do Not Overfit the Theory to One Language

The mathematical model should operate on abstract graph entities.

Language-specific parsers should only provide the evidence used to build the graph.

Therefore:

```text
Language parser
   ↓
Typed entities/relations
   ↓
Language-agnostic graph math
```

This is what makes the approach applicable to:

```text
Rust
Python
TypeScript
JavaScript
Go
Java
C/C++
...
```

without redefining the optimization problem for every language.

---

# 32. The “Codebase Management” Layer

The longer-term system should expose a small set of mathematically meaningful primitives.

## Primitive 1 — Locate

```text
locate(task)
```

Returns likely entry points.

## Primitive 2 — Explain relations

```text
neighbors(symbol)
```

Returns high-value neighboring evidence.

## Primitive 3 — Trace

```text
trace(source, target, task)
```

Returns likely paths.

## Primitive 4 — Expand

```text
expand(symbol, budget)
```

Returns the highest-information evidence under a local token budget.

## Primitive 5 — Impact

```text
impact(symbol)
```

Returns affected nodes.

## Primitive 6 — Context

```text
context(task, budget)
```

Returns a compact evidence package.

The mathematical layer should support all these operations rather than being tied only to one “select” command.

---

# 33. Harness Integration Goal

The harness should be able to ask RepoTrim questions without forcing the model to search the whole repository.

Conceptually:

```text
Model
  ↓
RepoTrim: locate(task)
  ↓
RepoTrim: trace(...)
  ↓
RepoTrim: expand(...)
  ↓
Model receives structured evidence
  ↓
Model acts
```

This changes RepoTrim from a “preprocessor” into a **codebase intelligence service**.

That distinction should be reflected in the documentation and research framing.

---

# 34. Recommended API Contract

A useful first JSON contract:

```json
{
  "task": "...",
  "budget": {
    "tokens": 8000,
    "tool_calls": 4
  },
  "entrypoints": [],
  "paths": [],
  "evidence": [],
  "omissions": [],
  "confidence": 0.0,
  "remaining_budget": 0
}
```

The harness should never be forced to infer why a node appeared.

Every evidence item should ideally carry:

```text
why_selected
relevance
confidence
cost
relations
```

This also makes debugging the math much easier.

---

# 35. What NOT to Do Yet

## Do not immediately add an LLM reranker

It can improve results but can hide mathematical weaknesses and make evaluation expensive.

First establish whether graph + principled utility already works.

## Do not immediately train a neural policy

Training requires data, task traces, and stable evaluation.

First establish a deterministic/approximately deterministic mathematical baseline.

## Do not add unlimited heuristics

Every new score should have a semantic meaning and an ablation plan.

## Do not claim “optimal” without a theorem

Use:

```text
approximate
near-optimal
budgeted greedy
heuristic
empirically strong
```

only when those descriptions match the implementation/evidence.

---

# 36. Recommended Development Phases

## Phase 0 — Freeze the research question

Write down:

> Given a task, graph, and budget, how can the system minimize exploration cost while preserving task-relevant information?

This becomes the north-star question.

---

## Phase 1 — Formalize the current system

Document exactly what the current code already does.

Produce:

```text
G definition
seed definition
PPR equation
candidate set
utility definition
cost definition
selection algorithm
```

No algorithmic change yet.

---

## Phase 2 — Replace heuristic utility with explicit utility

Introduce:

```text
relevance
coverage
path coverage
redundancy
cost
```

Start with a minimal model.

Then test each term independently.

---

## Phase 3 — Prove properties

For the chosen objective establish:

- non-negativity;
- monotonicity if applicable;
- submodularity if applicable;
- cost feasibility;
- any approximation bound actually supported by the algorithm.

This is the most important phase for the paper.

---

## Phase 4 — Keep CELF as production accelerator

Use the mathematically justified greedy objective with lazy evaluation.

Compare against:

- exact search on small instances;
- stronger offline approximations;
- current implementation.

---

## Phase 5 — Add path reasoning

Introduce task-relevant paths and path coverage.

Start with deterministic path scores.

Do not start with learned policy optimization.

---

## Phase 6 — Add adaptive navigation

Only after the static objective is stable.

Use:

\[
\text{expected information gain}/\text{cost}
\]

for next-step decisions.

Investigate whether adaptive submodularity can be established.

---

## Phase 7 — Harness evaluation

Run real agent tasks and measure:

```text
success
input tokens
output tokens
tool calls
files inspected
latency
```

This demonstrates actual product value.

---

# 37. Ablation Matrix

A strong first paper can be built around the following variants:

| Variant | Graph | PPR | Coverage | Redundancy | Path | Adaptive |
|---|---:|---:|---:|---:|---:|---:|
| Full context | No | No | No | No | No | No |
| Lexical | No | No | No | No | No | No |
| Graph-only | Yes | No | No | No | No | No |
| PPR | Yes | Yes | No | No | No | No |
| Static utility | Yes | Yes | Yes | Yes | No | No |
| Path-aware | Yes | Yes | Yes | Yes | Yes | No |
| Adaptive | Yes | Yes | Yes | Yes | Yes | Yes |

This lets the paper show exactly where the gains come from.

---

# 38. A Compact Paper Structure

A small paper can be structured as:

## 1. Introduction

AI coding agents spend a significant fraction of their budget exploring repositories before acting.

## 2. Problem

Define budget-constrained codebase navigation.

## 3. Representation

Define the typed codebase graph.

## 4. Method

Explain:

- task-conditioned PPR;
- coverage utility;
- submodular selection;
- budgeted optimization;
- path/navigation extension.

## 5. Algorithm

Give pseudocode.

## 6. Theoretical properties

Prove the properties that actually hold.

## 7. Experiments

Compare against retrieval baselines and measure agent exploration cost.

## 8. Discussion

Explain failures, limitations, and scalability.

## 9. Conclusion

State that the system converts repository structure into a compact navigation prior for AI coding harnesses.

---

# 39. Suggested Pseudocode

## 39.1 Static budgeted context

```text
INPUT:
  code graph G
  task q
  token budget B

1. Build task seed distribution s_q
2. Compute task-conditioned relevance p_q using PPR
3. Build candidate set V_q
4. For each candidate v:
      estimate cost c(v)
      estimate relevance r(v)
      estimate evidence coverage K(v, ·)
5. Define utility F(S | q, G)
6. Run budgeted greedy / CELF:
      while budget remains:
          select argmax marginal_gain / cost
7. Return structured context graph C
```

## 39.2 Adaptive navigation

```text
INPUT:
  G, q, budget B

state ← empty evidence

while budget remains:
    generate candidate actions
    estimate expected marginal information of each action
    choose action with highest expected gain / cost
    execute action
    update evidence state
    recompute candidate gains

return evidence graph + likely paths + context
```

---

# 40. Recommended Mathematical Notation for the Paper

Keep notation small.

| Symbol | Meaning |
|---|---|
| \(G=(V,E)\) | Codebase graph |
| \(q\) | Task/query |
| \(S\) | Selected evidence set |
| \(\pi\) | Navigation policy/path sequence |
| \(B\) | Token/resource budget |
| \(c(v)\) | Cost of evidence node/action |
| \(r(v\mid q,G)\) | Task-conditioned relevance |
| \(F(S\mid q,G)\) | Static context utility |
| \(U(\psi,q,G)\) | Utility given partial observations |
| \(\Delta(v\mid S)\) | Marginal utility |
| \(\Delta(a\mid\psi)\) | Conditional/adaptive marginal utility |
| \(P_q\) | Task-conditioned graph transition matrix |
| \(\mathbf p_q\) | PPR relevance vector |

Do not introduce dozens of symbols in the first paper.

---

# 41. Theoretical Positioning

The project should be positioned as the intersection of:

```text
Program / code graph analysis
        +
Graph-based relevance propagation
        +
Submodular information selection
        +
Budgeted optimization
        +
Sequential/adaptive information acquisition
        +
AI coding-agent exploration
```

The novel contribution should **not** be claimed as inventing PPR or submodularity.

The contribution should be the **specific formulation, representation, objective, algorithmic composition, and empirical validation for codebase navigation under an AI-agent budget**.

That is a much more credible research claim.

---

# 42. Recommended Claims vs. Claims to Avoid

## Good claims

- “RepoTrim models codebase exploration as a budgeted information acquisition problem.”
- “We combine task-conditioned graph relevance with redundancy-aware evidence selection.”
- “We formulate codebase context selection as monotone submodular maximization under a token budget, under stated assumptions.”
- “We evaluate exploration cost and task success jointly.”
- “We investigate an adaptive extension for sequential codebase navigation.”

## Avoid until proven

- “Mathematically optimal.”
- “Globally optimal retrieval.”
- “Guaranteed best context.”
- “Lossless token compression.”
- “Works for every codebase.”

Instead use:

- “language-agnostic graph formulation”;
- “broad repository evaluation”;
- “robust across evaluated repository sizes/languages.”

---

# 43. How the Current Repo Should Evolve

The existing modules can be mapped conceptually into the new architecture.

```text
Existing parser.rs
    ↓
Graph/entity construction

Existing graph.rs
    ↓
Typed codebase graph

Existing ppr.rs
    ↓
Task-conditioned relevance prior

Existing celf.rs
    ↓
Budgeted submodular optimizer / lazy greedy

Existing selector.rs
    ↓
Static context selection orchestration

Existing retrieval/search modules
    ↓
Candidate generation + task interpretation

Existing impact/slicing/history features
    ↓
Evidence and path features

New modules to add
    ├── utility.rs
    ├── coverage.rs
    ├── path_model.rs
    ├── navigation.rs
    ├── policy.rs (optional later)
    ├── confidence.rs
    └── evaluation.rs
```

This is an evolution rather than a rewrite.

---

# 44. Proposed New Core Interfaces

Conceptually:

```rust
trait RelevanceModel {
    fn relevance(&self, task: &Task, graph: &Graph) -> Scores;
}

trait UtilityModel {
    fn utility(&self, evidence: &EvidenceSet, task: &Task) -> f64;
    fn marginal_gain(&self, candidate: &Candidate, evidence: &EvidenceSet, task: &Task) -> f64;
}

trait BudgetModel {
    fn cost(&self, item: &EvidenceItem) -> u32;
}

trait Navigator {
    fn next_action(&self, state: &NavigationState) -> Action;
}
```

The exact API is an implementation decision; the important part is keeping the mathematical responsibilities separate.

---

# 45. Recommended First Implementation

Do **not** implement the entire adaptive model at once.

The first strong milestone should be:

```text
Typed graph
+
Task-conditioned PPR
+
Coverage-based monotone submodular utility
+
Real token cost
+
Budgeted greedy/CELF
+
Path-aware output
```

This already represents a coherent mathematical contribution.

Then add:

```text
Adaptive next-action selection
```

as the second-stage research extension.

---

# 46. What “Success” Should Look Like

The desired behavior is not:

> “RepoTrim returns fewer tokens.”

It should be:

> “For the same task and resource budget, the harness reaches the relevant code faster and with fewer exploratory tokens/tool calls, while preserving task success.”

A strong result would look like:

```text
Same repository
Same task
Same model
Same budget

Baseline:
  24 tool calls
  38k exploration tokens
  success = 82%

RepoTrim:
   8 tool calls
  12k exploration tokens
  success = 81–85%
```

Even if the exact numbers differ, this is the kind of evaluation that validates the product thesis.

---

# 47. Research Milestone Checklist

## Mathematical

- [ ] Define typed graph \(G\).
- [ ] Define task representation \(q\).
- [ ] Define relevance \(r(v\mid q,G)\).
- [ ] Define evidence coverage.
- [ ] Define path coverage.
- [ ] Define token/resource cost.
- [ ] Define utility \(F\).
- [ ] Prove or disprove monotonicity.
- [ ] Prove or disprove submodularity.
- [ ] Match algorithm to theorem assumptions.
- [ ] State the actual approximation guarantee.

## Engineering

- [ ] Separate indexing-time and query-time computation.
- [ ] Add structured context output.
- [ ] Add path inference.
- [ ] Add confidence/uncertainty.
- [ ] Keep CELF as a production acceleration.
- [ ] Add exact/offline oracle for small graphs.

## Evaluation

- [ ] Real coding tasks.
- [ ] Small repositories.
- [ ] Medium repositories.
- [ ] Large repositories.
- [ ] Multiple languages.
- [ ] Same token budget across methods.
- [ ] Full-context baseline.
- [ ] Lexical baseline.
- [ ] Semantic baseline.
- [ ] PPR baseline.
- [ ] Proposed method.
- [ ] Ablation study.
- [ ] Statistical confidence intervals.

## Paper

- [ ] Problem definition.
- [ ] Mathematical formulation.
- [ ] Algorithm.
- [ ] Theoretical result(s).
- [ ] Reproducible experiments.
- [ ] Limitations.
- [ ] Avoid unsupported optimality claims.

---

# 48. References

## PageRank / Personalized Ranking

1. **Brin, S. & Page, L.** “The Anatomy of a Large-Scale Hypertextual Web Search Engine.” 1998.  
   https://doi.org/10.1016/S0169-7552(98)00110-X

2. **Haveliwala, T. H.** “Topic-Sensitive PageRank: A Context-Sensitive Ranking Algorithm for Web Search.” IEEE TKDE, 2003.  
   https://doi.org/10.1109/TKDE.2003.1208999

## Submodularity

3. **Nemhauser, G. L., Wolsey, L. A., Fisher, M. L.** “An Analysis of the Approximations for Maximizing Submodular Set Functions.” Mathematical Programming, 1978.  
   General reference and lecture exposition: https://people.csail.mit.edu/stefje/fall15/notes_lecture15.pdf

4. **Sviridenko, M.** “A Note on Maximizing a Submodular Set Function Subject to a Knapsack Constraint.” Operations Research Letters, 2004.  
   https://doi.org/10.1016/S0167-6377(03)00062-2

5. **Leskovec, J. et al.** “Cost-effective Outbreak Detection in Networks.” KDD, 2007. This is the classic CELF-related reference and demonstrates lazy evaluation for submodular selection.  
   https://doi.org/10.1145/1281192.1281239

6. **Du, H. W., Li, X., Wang, G.** “New approximations for monotone submodular maximization with knapsack constraint.” Journal of Combinatorial Optimization, 2024.  
   https://doi.org/10.1007/s10878-024-01214-x

## Adaptive / Sequential Optimization

7. **Golovin, D. & Krause, A.** “Adaptive Submodularity: Theory and Applications in Active Learning and Stochastic Optimization.” Journal of Artificial Intelligence Research, 2011.  
   https://doi.org/10.1613/JAIR.3278

8. **Sutton, R. S. & Barto, A. G.** “Reinforcement Learning: An Introduction.” 2nd edition. MIT Press, 2018.  
   https://mitpress.ublish.com/book/reinforcement-learning-an-introduction-2

## Code Graph / Representation

9. **Code Property Graph concept:** a code representation combining multiple program-analysis relations into a graph. For the research phase, RepoTrim should use this concept as inspiration but define its own task-aware typed graph explicitly rather than relying on a generic CPG definition.  
   https://en.wikipedia.org/wiki/Code_property_graph

---

# 49. Final Architectural Thesis

The strongest long-term form of RepoTrim is not:

```text
“a smarter code search tool.”
```

It is:

```text
A mathematical codebase intelligence layer
that models repository structure as a graph,
conditions relevance on the task,
selects information under a resource budget,
and progressively navigates toward sufficient evidence.
```

The mathematical hierarchy should therefore be:

\[
\boxed{
\text{Codebase Graph}
\rightarrow
\text{Task-conditioned Prior}
\rightarrow
\text{Information Utility}
\rightarrow
\text{Budgeted Selection}
\rightarrow
\text{Adaptive Navigation}
}
\]

with:

\[
\boxed{
\text{PPR} = \text{relevance prior}
}
\]

\[
\boxed{
\text{Submodularity} = \text{information/diminishing-return model}
}
\]

\[
\boxed{
\text{CELF} = \text{efficient greedy evaluation}
}
\]

\[
\boxed{
\text{Adaptive Submodularity} = \text{candidate theory for sequential navigation}
}
\]

and:

\[
\boxed{
\text{Token / tool budget} = \text{resource constraint}
}
\]

This is the cleanest path for preserving the strengths of the current RepoTrim implementation while giving the project a substantially stronger mathematical identity.

---

# 50. One-Sentence Research Definition

> **RepoTrim studies budget-constrained task-aware navigation over a typed codebase graph, with the goal of maximizing task-relevant information available to an AI coding harness while minimizing the tokens and exploration actions required to obtain it.**

That sentence should be the north star for the next architectural iteration and for the eventual paper.
