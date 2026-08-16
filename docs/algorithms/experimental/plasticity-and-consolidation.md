---
title: ALG-MEM-003 — Plasticity and Consolidation
status: experimental
owners:
  - algorithms-ai-engineer
last_updated: 2026-08-16
decision_refs:
  - ADR-0004
source_refs:
  - HIST-KNOWLEDGE-PLATFORM
---

# ALG-MEM-003: Plasticity and Consolidation

## Objective and non-goals

Adapt associative edge weights from co-activation and rank items for possible promotion from episode-like records into generalized knowledge. This is an offline candidate generator, not automatic truth creation, deletion, or a literal synaptic model.

## Inputs and outputs

Plasticity inputs are versioned association weights and bounded, deduplicated co-activation events; output is a new normalized graph snapshot. Consolidation inputs are calibrated item features and evidence; output is an evidence-bound review proposal or no proposal, never a direct source/output mutation.

## Plasticity formula

```text
w_ij^(t+1) = (1-lambda_w) w_ij^t + eta a_i a_j
```

After each batch, enforce nonnegativity and normalize outgoing weights to prevent explosion. One concrete research policy is `w_ij ← min(w_max, max(0,w_ij))`, then `w_ij ← w_ij / sum_j w_ij` when the row sum is positive.

## Consolidation formula

```text
K_i = sigma(alpha B_i + beta U_i + gamma N_i + delta C_i - epsilon X_i)
promote i if K_i > tau
```

Promotion means “create a review proposal,” never direct mutation.

## Symbols

| Symbol | Meaning | Type/range/unit | Experimental value for examples |
|---|---|---|---|
| `w_ij^t` | association weight from item `i` to `j` at step `t` | nonnegative real | observed |
| `lambda_w` | per-update weight decay | `[0,1]` | `0.1` |
| `eta` | learning rate | nonnegative real | `0.2` |
| `a_i,a_j` | bounded activations | `[0,1]` | observed |
| `w_max` | pre-normalization cap | positive real | `1` |
| `K_i` | consolidation score | `(0,1)` | computed |
| `sigma` | logistic function | `1/(1+e^-x)` | defined |
| `B_i` | normalized memory activation feature | typically `[0,1]` after calibration | observed |
| `U_i` | demonstrated utility feature | `[0,1]` | observed |
| `N_i` | novelty/complementarity feature | `[0,1]` | observed |
| `C_i` | evidence confidence feature | `[0,1]` | observed |
| `X_i` | conflict/interference risk | `[0,1]` | observed |
| `alpha..epsilon` | feature coefficients | nonnegative real | no production default; example all `1` |
| `tau` | review-proposal threshold | `(0,1)` | example `0.7` |

The `epsilon` coefficient in consolidation is unrelated to the time stabilizer in ALG-MEM-001; implementations SHOULD name it `coef_conflict`.

## Pseudocode

```text
for bounded coactivation event in authenticated batch:
    w[i,j] = (1-lambda_w)*w[i,j] + eta*a[i]*a[j]
for affected source row i:
    clamp weights; normalize row deterministically

for candidate item i:
    features = versioned, calibrated feature extractors
    K = sigmoid(dot(positive_coefficients, [B,U,N,C]) - coef_conflict*X)
    if K > tau: emit evidence-bound consolidation proposal
human/policy validates and approves separately
```

## Complexity

Plasticity update is `O(number of observed co-activations)`; normalization is `O(affected edges)`. Consolidation scoring is `O(items × features)`. Dense all-pairs co-activation is forbidden at target scale.

## Edge and security cases

Co-activation spam, feedback loops, popularity bias, source duplication, correlated features, missing confidence, graph explosion, private activity histories, and threshold gaming must be evaluated. Require per-source/session contribution caps and provenance. Do not promote when evidence is missing even if the numeric score is high.

## Worked examples

Plasticity: `w=0.4`, `lambda_w=0.1`, `eta=0.2`, `a_i=0.5`, `a_j=0.8` gives `w'=0.9×0.4 + 0.2×0.4 = 0.44` before row normalization.

Consolidation: all coefficients `1`, features `B=.8,U=.7,N=.6,C=.9,X=.4` yield logit `2.6`, so `K=sigma(2.6)≈0.9309`; at `tau=.7` this emits a review proposal.

## Golden vectors

| Formula case | Expected |
|---|---|
| `w=.4, lambda=.1, eta=.2, ai=.5, aj=.8` | pre-normalization `0.44` |
| `eta=0` | decay only `(1-lambda)w` |
| all consolidation features/coefficients zero | `K=0.5` |
| logit `2.6` | `K≈0.930862` |
| high `K`, missing EvidenceRef | no valid proposal |

## Calibration and rollback

Evaluate held-out curator acceptance, downstream retrieval/synthesis gain, conflict creation, popularity/source bias, edge stability, and false-promotion cost. Compare static graph and no-consolidation baselines. Disable updates and revert to the last versioned edge snapshot/model if metrics drift; raw source snapshots remain unchanged.
