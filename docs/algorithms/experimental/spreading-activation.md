---
title: ALG-MEM-002 — Spreading Activation
status: experimental
owners:
  - algorithms-ai-engineer
last_updated: 2026-08-16
decision_refs:
  - ADR-0004
source_refs:
  - HIST-KNOWLEDGE-PLATFORM
---

# ALG-MEM-002: Spreading Activation

## Objective and non-goals

Diffuse query seed relevance through a knowledge graph to retrieve useful neighbors/backlinks. This PageRank-like association heuristic does not establish causal or factual relationships and must not promote graph popularity over evidence by default.

## Inputs and outputs

Inputs are a normalized query seed vector, a versioned weighted canonical-ID graph, damping/convergence parameters, and resource bounds. Output is an ordered node activation vector with graph version, component provenance, convergence status, and truncation diagnostics.

## Formula

```text
a^(k+1) = (1-rho) s_q + rho P^T a^k
```

For `0≤rho<1` and a row-stochastic transition matrix, iteration converges to a personalized stationary vector under ordinary conditions.

## Symbols

| Symbol | Meaning | Type/range/unit | Experimental default |
|---|---|---|---|
| `q` | query | text/structured request | required |
| `s_q` | normalized query seed vector | nonnegative `R^N`, sum `1` | candidate scores normalized |
| `a^k` | activation after iteration `k` | nonnegative `R^N`, sum approximately `1` | `a^0=s_q` |
| `P` | directed transition matrix | `N×N`, row-stochastic | edge-weight normalized |
| `P^T` | transpose of `P` | `N×N` | defined |
| `rho` | propagation/damping factor | `[0,1)` | `0.5` initial experiment |
| `k` | iteration | nonnegative integer | until tolerance or max |
| `N` | graph node count | positive integer | observed |
| tolerance | L1 convergence threshold | positive | `1e-6` |
| max iterations | computation bound | positive integer | `50` |

Dangling rows redistribute according to `s_q` (personalized) rather than host-dependent behavior. Edge-type weights are versioned configuration.

## Pseudocode

```text
seed = normalize_nonnegative(seed_candidates)
P = build_sparse_transition(graph, edge_type_weights)
activation = seed
repeat up to max_iterations:
    propagated = transpose(P) * activation
    add dangling_mass * seed
    next = (1-rho) * seed + rho * propagated
    if l1(next - activation) < tolerance: break
    activation = next
return deterministic_sort(nodes by -activation, NodeId)
```

## Complexity

Sparse iteration costs `O(k(|V|+|E|))` time and `O(|V|+|E|)` memory. Query-local neighborhood bounds can reduce cost but must report truncation.

## Edge and security cases

Handle dangling nodes, disconnected components, self-loops, duplicate edges, adversarial link farms, one source generating many edges, stale graph versions, negative/raw invalid weights, and cross-tenant leakage. Provenance and source diversity may cap edge influence. Graph scores never replace evidence confidence.

## Worked example

Two nodes swap activation: `P=[[0,1],[1,0]]`, seed `s=[1,0]`, `rho=0.5`, `a^0=s`.

```text
a^1 = 0.5[1,0] + 0.5[0,1] = [0.5,0.5]
a^2 = 0.5[1,0] + 0.5[0.5,0.5] = [0.75,0.25]
```

The fixed point is `[2/3,1/3]`.

## Golden vectors

| Graph/seed | Parameters | Expected |
|---|---|---|
| identity `P`, any normalized seed | any `rho<1` | `a^k=s` |
| two-node swap, `s=[1,0]` | `rho=0.5` | fixed point `[2/3,1/3]` |
| no outgoing edge | dangling→seed | probability mass preserved |
| `rho=0` | any graph | exactly `s_q` |

## Calibration and rollback

Measure nDCG/Recall improvements over lexical+semantic baselines, source diversity, link-farm sensitivity, latency, and per-topic results. Ablate each edge type. If graph contribution degrades a slice or exceeds cost, set its retrieval coefficient to zero or disable propagation; seed retrieval remains intact.
