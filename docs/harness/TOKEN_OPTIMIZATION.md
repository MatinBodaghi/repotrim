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

## 2. Automated Intent & Seed Inference

Personalized PageRank is seeded with initial mass $\mathbf{p}_0$. RepoTrim provides automated inference so agents and developers do not need to manually guess exact symbol names:

| Agent Task Type | Recommended Invocation | How RepoTrim Resolves Context |
| :--- | :--- | :--- |
| **Bug Fixing / Triage** | `repotrim select --query "TypeError in auth token validation"` | BM25 + trigram matching identifies culprit symbols (`validate_token`, `AuthHeader`) and seeds PPR. |
| **Feature Addition** | `repotrim blueprint "implement rate limiter middleware"` | Generates `FEATURE_BLUEPRINT.md` with identified seed anchors and target files. |
| **Code Review / PR** | `repotrim select --from-diff` | Maps git diff line ranges to enclosing AST symbols, packing affected callers and type dependencies. |
| **Precise Refactoring** | `repotrim select --seed ContextSelector --budget 3000` | Explores AST containment ($E_{\text{AST}}$) and call sites ($E_{\text{Call}}$) from exact symbols. |

---

## 3. Dynamic Budget Allocation

Instead of using a static 8,000-token budget for every request, scale the budget dynamically:

- **Quick Verification / Type Checks:** `1,000 – 1,500` tokens (mostly $\text{LOD}_0$ signatures).
- **Standard Feature Implementation:** `3,000 – 4,500` tokens (mix of $\text{LOD}_2$ slices and $\text{LOD}_1$ signatures).
- **Complex Multi-Module Architecture:** `6,000 – 8,000` tokens (elevated diversity penalty to pull symbols across distant modules).

---

## 4. Avoiding Common Token Traps

1. **Leverage the 3-Tier Context Funnel ([`skills/repotrim/SKILL.md`](../../skills/repotrim/SKILL.md)):** Never dump whole modules when a Tier 2 skeleton (~1k–3k tokens) supplies exact type interfaces.
2. **Do not re-dump unchanged dependencies:** Once a dependency's signature is in context, subsequent turns should only query symbols touched in the latest turn.
3. **Prefer $\text{LOD}_2$ (control-flow slices) over raw files:** Functions with 200 lines of internal loop calculations or logging can be expressed in 20 tokens of control structure and return statements.
4. **Use Markdown fences:** RepoTrim formats context with clear file and line demarcation, helping LLMs generate accurate diffs without hallucinating line offsets.
