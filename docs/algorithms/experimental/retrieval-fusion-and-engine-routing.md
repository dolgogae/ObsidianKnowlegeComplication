---
title: ALG-MEM-004/005 — Retrieval Fusion and Engine Routing
status: experimental
owners:
  - algorithms-ai-engineer
  - mcp-adapter-engineer
last_updated: 2026-08-16
decision_refs:
  - ADR-0002
  - ADR-0004
source_refs:
  - HIST-MCP-RESEARCH
  - HIST-KNOWLEDGE-PLATFORM
---

# ALG-MEM-004/005: Retrieval Fusion and Engine Routing

## Objective and non-goals

Combine lexical, semantic, graph, memory, evidence, and interference signals, then choose engine weights based on measured task performance. It does not let an LLM freely invoke tools, make search indexes canonical, or claim one weight vector works across topics.

## Inputs and outputs

Online inputs are a query, capability/privacy/budget requirements, versioned engine rankings, calibrated features, and routing policy; output is a bounded ordered canonical-ID candidate list with component/engine provenance. Offline router inputs are frozen benchmark cases and configurations; output is a signed cohort-specific routing policy plus evaluation report.

## Retrieval formula — ALG-MEM-004

```text
R_i = alpha L_i + beta S_i + gamma G_i + delta B_i + epsilon C_i - zeta I_i
```

All component scores MUST be transformed to comparable, versioned scales before linear fusion. Until calibrated, use deterministic Reciprocal Rank Fusion (RRF) for candidate generation:

```text
RRF(i) = sum_{e in engines containing i} v_e / (k_rrf + rank_e(i))
```

Initial pipeline: top 50 BM25 + top 50 semantic + top 30 graph → RRF → bounded reranker → top 5.

## Router objective — ALG-MEM-005

```text
w* = argmax_w [NDCG@10(w) + mu Recall@10(w)
               - lambda Tokens(w) - kappa Latency(w)]
```

Tokens and latency MUST be normalized to declared budgets so coefficients are dimensionless.

## Symbols

| Symbol | Meaning | Type/range/unit | Experimental default |
|---|---|---|---|
| `i` | retrieval candidate | canonical item ID | required |
| `L_i` | calibrated lexical/BM25 score | `[0,1]` | engine-derived |
| `S_i` | calibrated semantic cosine score | `[0,1]` | engine-derived |
| `G_i` | graph activation | `[0,1]` | ALG-MEM-002 |
| `B_i` | normalized memory activation | `[0,1]` | ALG-MEM-001 |
| `C_i` | calibrated evidence confidence | `[0,1]` | future ALG-CLM-001 |
| `I_i` | interference/redundancy penalty | `[0,1]` | measured |
| `alpha..zeta` | nonnegative fusion weights | real, usually sum positive weights to 1 | learned per cohort; no universal default |
| `v_e` | trusted engine contribution weight | nonnegative real | `1` before benchmarks |
| `rank_e(i)` | 1-based rank from engine `e` | positive integer | observed |
| `k_rrf` | RRF rank stabilizer | positive real | `60` |
| `w` | router engine/weight configuration | bounded search space | required |
| `mu` | Recall tradeoff | nonnegative | cohort-tuned |
| `lambda` | normalized token-cost penalty | nonnegative | cohort-tuned |
| `kappa` | normalized latency penalty | nonnegative | cohort-tuned |
| `NDCG@10` | normalized discounted cumulative gain | `[0,1]` | held-out metric |
| `Recall@10` | relevant-item recall | `[0,1]` | held-out metric |

## Pseudocode

```text
capabilities = classify_query_requirements(query)
eligible = engines satisfying capabilities, privacy, health, and benchmark floor
run bounded eligible engines under deadlines
canonicalize candidate IDs and retain engine/version/rank provenance
fused = RRF(candidate rankings, benchmark weights)
optional reranker reranks only bounded top-N and cannot introduce items
apply policy/evidence/interference features if calibrated
return top-k with component breakdown and fallbacks

offline:
  for each vault/topic/time split and bounded candidate weight vector w:
      evaluate NDCG, Recall, token cost, latency
  choose w* on validation; report held-out test once
```

## Complexity

Fusion is `O(total candidates + U log U)` for `U` unique candidates. Router search is `O(|W| × benchmark cases × retrieval cost)`; use bounded grid/Bayesian optimization offline, never unconstrained query-time learning.

## Edge and security cases

Engine timeout, stale/poisoned index, mismatched item identities, unavailable embeddings, provider data residency, rank manipulation, duplicated candidates, zero-result engine, correlated engines, benchmark leakage, and unbounded token context require explicit handling. Fallback is a declared deterministic engine set, normally local lexical retrieval.

## Worked examples

RRF: candidate `x` ranks 1 in lexical and 3 in graph, with both `v=1`, `k_rrf=60`:

```text
RRF(x) = 1/61 + 1/63 ≈ 0.032266
```

Linear fusion: `L=.9,S=.7,G=.4,B=.6,C=.8,I=.2` and weights `.30,.30,.10,.10,.20,.20` give `R=.27+.21+.04+.06+.16-.04=.70`.

Router: configuration A has `NDCG=.75`, `Recall=.80`, normalized tokens `.30`, latency `.20`; with `mu=.5,lambda=.1,kappa=.2`, objective is `.75+.40-.03-.04=1.08`.

## Golden vectors

| Case | Expected |
|---|---|
| candidate rank 1 and 3, `k=60`, unit weights | RRF approximately `0.032266` |
| candidate absent from an engine | that engine contributes zero |
| linear example above | `R=.70` |
| all advanced engines unavailable | declared lexical fallback, diagnostic |
| reranker returns unknown ID | reject unknown item; provider error |

## Calibration and rollback

Report Recall@5/10, MRR, nDCG, faithfulness, context precision/recall, latency, tokens, source diversity, and confidence intervals per topic/cohort. Benchmark each engine per Vault; do not present a universal global rank. If a learned route underperforms its deterministic fallback or violates budget/privacy, roll back to the last signed routing policy or rule-based router.
