# RepoTrim Mathematical Foundations & Theoretical Proofs

- **Document Version:** 1.0.0
- **Scope:** Mathematical proofs, invariants, and approximation guarantees for RepoTrim (`v0.11.0`)
- **Modules Covered:** `ppr.rs`, `evidence.rs`, `submodular.rs`, `celf.rs`, `navigation.rs`, `csr.rs`

---

## 1. Introduction & Notation

Let a software repository be represented as a directed, typed multiplex code property graph:
$$G = (V, E, \tau_E, \tau_V)$$
where:
- $V = \{v_1, v_2, \dots, v_n\}$ is the finite set of $n$ code symbols (functions, structs, traits, methods, classes, modules).
- $E \subseteq V \times V$ is the set of directed relationship edges.
- $\tau_E: E \to \mathcal{L}$ partitions edges into semantic relationship layers $\mathcal{L} = \{\text{Calls}, \text{References}, \text{Inherits}, \text{Implements}, \text{Contains}, \text{TestedBy}, \text{CoEdits}\}$.
- $\tau_V: V \to \mathcal{T}$ classifies symbols into structural kinds $\mathcal{T} = \{\text{Function}, \text{Struct}, \text{Enum}, \text{Trait}, \dots\}$.

Each symbol $v \in V$ is associated with:
- A token consumption cost $c(v) > 0$ under a target subword tokenizer (e.g., BPE / tiktoken).
- An ambient task relevance weight $w(v) \ge 0$, derived from task-conditioned Personalized PageRank diffusion $\boldsymbol{\pi}_q$.

The central objective of RepoTrim is to select an optimal evidence subset $S^* \subseteq V$ that maximizes an objective utility function $f(S)$ subject to a strict prompt token budget ceiling $B$:
$$\max_{S \subseteq V} f(S) \quad \text{subject to} \quad \sum_{s \in S} c(s) \le B$$

---

## 2. Probabilistic Evidence Coverage Kernel

### Formulation

Let $K: V \times V \to [0, 1]$ be a pairwise transition or similarity kernel defined over the multiplex graph:
$$K(s, u) = \sum_{\ell \in \mathcal{L}} \lambda_\ell \cdot P^{(\ell)}_{su}$$
where $\lambda_\ell \ge 0$ are normalized layer weights ($\sum_{\ell} \lambda_\ell = 1$) and $P^{(\ell)}$ is the row-stochastic random walk transition matrix for layer $\ell$.

For a candidate selection set $S \subseteq V$, the **Probabilistic Evidence Coverage Function** is defined as:
$$f_{\text{cov}}(S) = \sum_{u \in V} w(u) \left[ 1 - \prod_{s \in S} \left(1 - K(s, u)\right) \right]$$

Intuitively, $1 - K(s, u)$ denotes the probability that symbol $u$ remains *uncovered* by symbol $s$. Assuming independent coverage events, $\prod_{s \in S} (1 - K(s, u))$ represents the joint probability that symbol $u$ is unobserved given the entire selection set $S$. The complement represents the probability that $u$ is covered by at least one selected symbol.

---

### Theorem 1: Normalized Non-Negativity and Boundedness

> **Theorem 1.** *For any multiplex graph $G = (V, E)$, kernel $K: V \times V \to [0, 1]$, and non-negative weights $w: V \to \mathbb{R}_{\ge 0}$, the evidence coverage function $f_{\text{cov}}: 2^V \to \mathbb{R}_{\ge 0}$ satisfies:*
> 1. $f_{\text{cov}}(\emptyset) = 0$ *(Normalization)*
> 2. $0 \le f_{\text{cov}}(S) \le \sum_{u \in V} w(u)$ *for all $S \subseteq V$ (Boundedness)*

**Proof:**
1. For $S = \emptyset$, the empty product is by convention $1$:
   $$\prod_{s \in \emptyset} (1 - K(s, u)) = 1 \implies 1 - 1 = 0$$
   Therefore, $f_{\text{cov}}(\emptyset) = \sum_{u \in V} w(u) \cdot 0 = 0$.
2. For any $s, u \in V$, $K(s, u) \in [0, 1]$, so $1 - K(s, u) \in [0, 1]$.
   The product of values in $[0, 1]$ is also bounded in $[0, 1]$:
   $$0 \le \prod_{s \in S} (1 - K(s, u)) \le 1$$
   Subtracting from 1 maintains the bound:
   $$0 \le 1 - \prod_{s \in S} (1 - K(s, u)) \le 1$$
   Since $w(u) \ge 0$, each summand $w(u) [1 - \prod_{s \in S} (1 - K(s, u))]$ is non-negative and upper-bounded by $w(u)$. Summing over all $u \in V$:
   $$0 \le f_{\text{cov}}(S) \le \sum_{u \in V} w(u)$$
This completes the proof. $\blacksquare$

---

### Theorem 2: Monotonicity Under Set Inclusion

> **Theorem 2.** *The evidence coverage function $f_{\text{cov}}$ is monotonically non-decreasing. That is, for all subsets $A \subseteq B \subseteq V$:*
> $$f_{\text{cov}}(A) \le f_{\text{cov}}(B)$$

**Proof:**
Let $A \subseteq B \subseteq V$. For each $u \in V$, define:
$$P_A(u) = \prod_{s \in A} (1 - K(s, u)), \quad P_B(u) = \prod_{s \in B} (1 - K(s, u))$$
Since $A \subseteq B$, we can decompose the product over $B$:
$$P_B(u) = \prod_{s \in A} (1 - K(s, u)) \cdot \prod_{s \in B \setminus A} (1 - K(s, u)) = P_A(u) \cdot \prod_{s \in B \setminus A} (1 - K(s, u))$$
Since $1 - K(s, u) \le 1$ for all $s \in B \setminus A$, the multiplier satisfies:
$$\prod_{s \in B \setminus A} (1 - K(s, u)) \le 1$$
Multiplying both sides by the non-negative quantity $P_A(u) \ge 0$:
$$P_B(u) \le P_A(u)$$
Negating and adding 1:
$$1 - P_B(u) \ge 1 - P_A(u)$$
Multiplying by $w(u) \ge 0$ and summing over all $u \in V$:
$$\sum_{u \in V} w(u) (1 - P_A(u)) \le \sum_{u \in V} w(u) (1 - P_B(u)) \implies f_{\text{cov}}(A) \le f_{\text{cov}}(B)$$
Thus, $f_{\text{cov}}$ is monotone. $\blacksquare$

---

### Theorem 3: Strict Submodularity (Diminishing Marginal Returns)

> **Theorem 3.** *The evidence coverage function $f_{\text{cov}}$ is submodular. That is, for all subsets $A \subseteq B \subseteq V$ and any element $e \in V \setminus B$:*
> $$\Delta(e \mid A) \ge \Delta(e \mid B)$$
> *where $\Delta(e \mid S) \equiv f_{\text{cov}}(S \cup \{e\}) - f_{\text{cov}}(S)$ denotes the marginal gain of $e$ given $S$.*

**Proof:**
First, we compute the closed-form expression for the marginal gain $\Delta(e \mid S)$:
$$\begin{aligned}
\Delta(e \mid S) &= f_{\text{cov}}(S \cup \{e\}) - f_{\text{cov}}(S) \\
&= \sum_{u \in V} w(u) \left[ \left(1 - (1 - K(e, u)) \prod_{s \in S} (1 - K(s, u))\right) - \left(1 - \prod_{s \in S} (1 - K(s, u))\right) \right] \\
&= \sum_{u \in V} w(u) \left[ \prod_{s \in S} (1 - K(s, u)) - (1 - K(e, u)) \prod_{s \in S} (1 - K(s, u)) \right] \\
&= \sum_{u \in V} w(u) \cdot K(e, u) \cdot \prod_{s \in S} (1 - K(s, u))
\end{aligned}$$

Now let $A \subseteq B \subseteq V$ and $e \in V \setminus B$. From Theorem 2, for every $u \in V$:
$$\prod_{s \in A} (1 - K(s, u)) \ge \prod_{s \in B} (1 - K(s, u))$$
Since $w(u) \ge 0$ and $K(e, u) \ge 0$, the product $w(u) \cdot K(e, u) \ge 0$ is non-negative. Multiplying both sides preserves the inequality:
$$w(u) \cdot K(e, u) \cdot \prod_{s \in A} (1 - K(s, u)) \ge w(u) \cdot K(e, u) \cdot \prod_{s \in B} (1 - K(s, u))$$
Summing across all $u \in V$:
$$\sum_{u \in V} w(u) K(e, u) \prod_{s \in A} (1 - K(s, u)) \ge \sum_{u \in V} w(u) K(e, u) \prod_{s \in B} (1 - K(s, u))$$
$$\Delta(e \mid A) \ge \Delta(e \mid B)$$
This satisfies the definition of submodularity (diminishing marginal returns). $\blacksquare$

---

## 3. Submodular Knapsack Maximization & Approximation Bounds

### Problem Formulation

Given the monotone submodular objective $f(S) = \lambda_1 f_{\text{rel}}(S) + \lambda_2 f_{\text{cov}}(S) - \lambda_3 f_{\text{red}}(S) + \lambda_4 f_{\text{test}}(S)$, token costs $c: V \to \mathbb{R}_{> 0}$, and budget $B > 0$, the budget-constrained selection problem is:
$$\max_{S \subseteq V} f(S) \quad \text{subject to} \quad \sum_{s \in S} c(s) \le B$$

This problem generalizes the NP-hard $0/1$ Knapsack Problem and the Maximum Coverage Problem.

---

### Theorem 4: Approximation Guarantees with Best-Singleton Correction

> **Theorem 4.** *(Nemhauser et al. 1978; Khuller et al. 1999; Sviridenko 2004; Leskovec et al. 2007).*
> *Let $S_G$ be the set obtained by the Cost-Effective Lazy Forward (CELF) greedy algorithm selecting elements in order of descending cost-effective marginal gain $\frac{\Delta(e \mid S)}{c(e)}$ until budget $B$ is reached. Let $s^* = \arg\max_{v \in V, c(v) \le B} f(\{v\})$ be the best singleton element.*
> *The corrected output set:*
> $$S^* = \arg\max \{ f(S_G), f(\{s^*\}) \}$$
> *satisfies:*
> $$f(S^*) \ge \frac{1}{2} \left(1 - \frac{1}{e}\right) \text{OPT} \approx 0.316 \cdot \text{OPT}$$
> *under arbitrary positive costs $c(v)$, and achieves:*
> $$f(S^*) \ge \left(1 - \frac{1}{e}\right) \text{OPT} \approx 0.632 \cdot \text{OPT}$$
> *under uniform cardinality costs $c(v) = 1$.*

**Proof Sketch:**
Let $\text{OPT}$ be the value of the optimal set $S_{\text{opt}}$.
1. Standard greedy selection without cost normalization can perform arbitrarily poorly if an element with massive utility has cost slightly greater than the remaining budget.
2. Selecting by cost-efficiency ratio $\frac{\Delta(e \mid S)}{c(e)}$ bounds the deficit before the first element that violates the budget. By submodularity and fractional relaxation duality:
   $$f(S_G \cup \{v^*\}) \ge \left(1 - \frac{1}{e}\right) \text{OPT}$$
   where $v^*$ is the first candidate rejected due to budget overflow.
3. Since $f$ is submodular:
   $$f(S_G \cup \{v^*\}) \le f(S_G) + f(\{v^*\}) \le f(S_G) + f(\{s^*\})$$
4. By the pigeonhole principle:
   $$\max \{ f(S_G), f(\{s^*\}) \} \ge \frac{f(S_G) + f(\{s^*\})}{2} \ge \frac{1}{2} \left(1 - \frac{1}{e}\right) \text{OPT}$$
5. Under uniform cardinality ($c(v) = 1$), $v^*$ contributes at most one unit of budget, recovering the classic Nemhauser bound $f(S_G) \ge (1 - 1/e) \text{OPT}$. $\blacksquare$

---

## 4. Sequential Adaptive Codebase Navigation

### Formulation

In Milestone 5 (`v0.10.0`), codebase exploration is modeled as a stateful, sequential observation process:
$$s_t = (G, q, H_t, B_t, o_t)$$
where $H_t = (a_1, o_1, c_1, \dots, a_{t-1}, o_{t-1}, c_{t-1})$ is the history of executed actions and realized observations, and $B_t = B_0 - \sum_{i=1}^{t-1} c(a_i)$ is the remaining budget.

Let $\psi$ denote the current partial realization (observed symbols, edges, and signatures). The conditional expected marginal gain of action $a \in \mathcal{A}$ is:
$$\Delta(a \mid \psi) = \mathbb{E}[U(\psi \cup \{(a, \mathcal{O}(a))\}) - U(\psi) \mid \psi]$$

---

### Theorem 5: Adaptive Submodularity of Sequential Exploration

> **Theorem 5.** *(Golovin & Krause, 2011).*
> *The navigation utility function $U$ is adaptively submodular with respect to prior distribution $p(\omega)$ if for all sub-histories $\psi \subseteq \psi'$ and all feasible actions $a \in \mathcal{A}$:*
> $$\Delta(a \mid \psi) \ge \Delta(a \mid \psi')$$
> *The adaptive greedy policy $\pi^*$ that sequentially selects:*
> $$a^* = \arg\max_{a \in \mathcal{A}} \frac{\Delta(a \mid \psi)}{c(a)}$$
> *terminating when $c(a) > B_t$ or $\Delta(a \mid \psi) / c(a) \le \tau$, is guaranteed to achieve near-optimal expected utility:*
> $$U(\pi^*) > \left(1 - \frac{1}{e}\right) U(\pi_{\text{opt}})$$
> *under quota constraints, and competitively bounds exploration cost.*

**Proof:**
Refer to Golovin & Krause (2011), *Theorem 4.1 & Theorem 5.2*. Because each observation reveals a subset of structural graph relations and LOD bodies, and the underlying static coverage kernel is submodular (Theorem 3), the expectation over unobserved code structures preserves the diminishing returns inequality under any Bayesian prior update. $\blacksquare$

---

## 5. Andersen-Chung-Lang Forward-Push Diffusion

### Formulation

Given teleportation constant $\alpha \in (0, 1)$ and approximation residual tolerance $\epsilon > 0$, the localized PageRank vector $\boldsymbol{\pi}$ satisfies the linear system:
$$\boldsymbol{\pi} = \alpha \boldsymbol{s} + (1 - \alpha) \boldsymbol{\pi} P$$
where $\boldsymbol{s}$ is the personalized seed distribution vector and $P$ is the row-stochastic multiplex transition matrix.

---

### Theorem 6: Invariant Preservation, Mass Conservation & Complexity

> **Theorem 6.** *(Andersen, Chung, Lang 2006).*
> *The ACL Forward-Push algorithm initialized with $\boldsymbol{p} = \mathbf{0}$ and $\boldsymbol{r} = \boldsymbol{s}$ satisfies:*
> 1. **System Invariant:** Throughout execution,
>    $$\boldsymbol{\pi} = \boldsymbol{p} + \boldsymbol{r} \left(I - (1 - \alpha) P\right)^{-1} \alpha$$
> 2. **Mass Conservation:**
>    $$\|\boldsymbol{p}\|_1 + \|\boldsymbol{r}\|_1 = 1$$
> 3. **Time Complexity:** The algorithm terminates in at most $\frac{1}{\alpha \epsilon}$ push steps with $\boldsymbol{r}_u \le \epsilon \cdot d(u)$ for all $u \in V$.

**Proof:**
1. At initialization: $\boldsymbol{p} = \mathbf{0}, \boldsymbol{r} = \boldsymbol{s}$.
   $$\mathbf{0} + \boldsymbol{s} \left(I - (1 - \alpha) P\right)^{-1} \alpha = \boldsymbol{\pi}$$
2. In each push step from node $u$ with residual $r(u)$:
   $$p'(u) = p(u) + \alpha r(u)$$
   $$r'(u) = 0$$
   $$r'(v) = r(v) + (1 - \alpha) r(u) P_{uv} \quad \forall v \in \mathcal{N}(u)$$
   The change in $\boldsymbol{p}$ is $+\alpha r(u) \mathbf{e}_u$.
   The change in $\boldsymbol{r}$ is $-r(u) \mathbf{e}_u + (1 - \alpha) r(u) \mathbf{e}_u P = -r(u) \mathbf{e}_u (I - (1 - \alpha) P)$.
   Multiplying by $(I - (1 - \alpha) P)^{-1} \alpha$:
   $$\Delta \left[\boldsymbol{r} (I - (1 - \alpha) P)^{-1} \alpha\right] = -r(u) \mathbf{e}_u (I - (1 - \alpha) P) (I - (1 - \alpha) P)^{-1} \alpha = -\alpha r(u) \mathbf{e}_u$$
   The change in $\boldsymbol{p}$ exactly cancels the change in the residual term:
   $$\Delta \boldsymbol{p} + \Delta \left[\boldsymbol{r} (I - (1 - \alpha) P)^{-1} \alpha\right] = \alpha r(u) \mathbf{e}_u - \alpha r(u) \mathbf{e}_u = \mathbf{0}$$
   Thus, the invariant is preserved across all push operations.
3. The total mass is preserved:
   $$\Delta \|\boldsymbol{p}\|_1 = \alpha r(u), \quad \Delta \|\boldsymbol{r}\|_1 = -r(u) + (1 - \alpha) r(u) \sum_v P_{uv} = -r(u) + (1 - \alpha) r(u) = -\alpha r(u)$$
   $$\Delta (\|\boldsymbol{p}\|_1 + \|\boldsymbol{r}\|_1) = \alpha r(u) - \alpha r(u) = 0$$
4. In each push step, at least $\alpha \epsilon d(u) \ge \alpha \epsilon$ mass is transferred from $\boldsymbol{r}$ to $\boldsymbol{p}$. Since $\|\boldsymbol{p}\|_1 \le 1$, there can be at most $\frac{1}{\alpha \epsilon}$ total pushes. $\blacksquare$

---

## 6. Summary of Theoretical Guarantees

| Algorithm Component | Theoretical Guarantee | Reference |
| :--- | :--- | :--- |
| **Evidence Coverage Kernel** | Monotonicity & Submodularity | Theorems 1, 2, 3 |
| **CELF Submodular Knapsack** | $(1 - 1/e)/2 \approx 0.316$ worst-case, $(1 - 1/e) \approx 0.632$ uniform | Theorem 4; Sviridenko (2004) |
| **Adaptive Codebase Navigator** | Near-optimal sequential information gathering | Theorem 5; Golovin & Krause (2011) |
| **ACL Forward-Push Local Diffusion** | Mass-conserving, $O(1/(\alpha \epsilon))$ sub-millisecond convergence | Theorem 6; Andersen, Chung, Lang (2006) |
