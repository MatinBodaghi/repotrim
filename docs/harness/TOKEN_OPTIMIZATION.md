# Token Optimization Guide for Agent Harnesses

This guide provides practical strategies for AI agent harnesses to achieve maximum reasoning accuracy while minimizing prompt token consumption.

---

## 1. The 3-Tier Context Funnel

To achieve 70–85% token reduction across multi-turn agent sessions, structure the prompt context in three tiers:

```text
┌────────────────────────────────────────────────────────┐
│  Tier 1: Feature Blueprint (~300 tokens)               │  <-- docs/harness/FEATURE_BLUEPRINT_TEMPLATE.md
├────────────────────────────────────────────────────────┤
│  Tier 2: RepoTrim Sliced Skeleton (~2,000–4,000 tokens) │  <-- Output from repotrim select
├────────────────────────────────────────────────────────┤
│  Tier 3: Active Working File (~1,000 tokens)            │  <-- Exact file being edited
└────────────────────────────────────────────────────────┘
```

Total context payload: **~3,500 – 5,500 tokens** vs. **30,000 – 80,000 tokens** for naive multi-file dumps.

---

## 2. Seed Selection Best Practices

Personalized PageRank is only as effective as the personalization seed $\mathbf{p}_0$. Choose seeds based on the agent's task phase:

| Agent Phase | Recommended Seed | Why |
| :--- | :--- | :--- |
| **Bug Fixing / Triage** | Stack trace symbols or failing test file | Concentrates probability mass directly on the bug site and its immediate callers. |
| **Feature Addition** | Target interface/trait and relevant feature blueprint | Traverses type signatures and imports to surface all integration touchpoints. |
| **Refactoring** | The struct or module being refactored | Explores AST containment ($E_{\text{AST}}$) and call sites ($E_{\text{Call}}$) across all consumers. |
| **Code Review / PR** | Git diff files (`git diff --name-only`) | Diffuses relevance strictly along modified symbols and dependent call graphs. |

---

## 3. Dynamic Budget Allocation

Instead of using a static 8,000-token budget for every request, scale the budget dynamically:

- **Quick Verification / Type Checks:** `1,500` tokens (mostly $\text{LOD}_0$ signatures).
- **Standard Feature Implementation:** `3,500 – 4,500` tokens (mix of $\text{LOD}_2$ slices and $\text{LOD}_0$ signatures).
- **Complex Multi-Module Architecture:** `8,000` tokens (elevated diversity penalty to pull symbols across distant modules).

---

## 4. Avoiding Common Token Traps

1. **Do not re-dump unchanged dependencies:** Once a dependency's signature is in context, subsequent turns should only query symbols touched in the latest turn.
2. **Prefer $\text{LOD}_2$ (control-flow slices) over raw files:** Functions with 200 lines of internal loop calculations or logging can be expressed in 20 tokens of control structure and return statements.
3. **Use Markdown fences:** RepoTrim formats context with clear file and line demarcation, helping LLMs generate accurate diffs without hallucinating line offsets.
