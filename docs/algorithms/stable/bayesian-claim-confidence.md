---
title: ALG-CLM-001 — Bayesian Claim Confidence
status: normative-future
owners:
  - algorithms-ai-engineer
  - architect
last_updated: 2026-08-16
decision_refs:
  - ADR-0004
source_refs:
  - HIST-KNOWLEDGE-PLATFORM
---

# ALG-CLM-001: Bayesian Claim Confidence

## Objective and non-goals

Combine auditable evidence likelihood ratios into a calibrated support probability for a context- and time-scoped claim. This score does not establish objective truth, replace human adjudication, infer causality, or permit copied sources to masquerade as independent evidence. It is not part of V2 file compilation.

## Inputs and outputs

Input is one normalized context/time-scoped Claim, evidence references with dependency clusters, and a versioned calibrated prior/likelihood model. Output is a posterior support probability, uncertainty interval, evidence contribution breakdown, cohort, and calibration-model version.

## Formula

```text
logit P(c | E) = logit P(c)
                 + sum_{k=1..m} omega_k ln(P(E_k | c) / P(E_k | not c))

logit(p) = ln(p / (1-p))
sigma(x) = 1 / (1 + exp(-x))
P(c | E) = sigma(logit P(c | E))
```

Compute in log-odds space and clamp only for numerical stability, not to hide uncertainty.

## Symbols

| Symbol | Meaning | Type/range/unit | Default |
|---|---|---|---|
| `c` | one normalized claim in a declared context/time | Claim ID | required |
| `E` | evidence set | `{E_1..E_m}` | required, `m≥1` |
| `E_k` | evidence item or dependency cluster | EvidenceRef/cluster | required |
| `P(c)` | prior probability/support rate | `(0,1)` | cohort-calibrated; no universal default |
| `P(E_k|c)` | likelihood under claim | `(0,1]` | learned/elicited per evidence model |
| `P(E_k|not c)` | likelihood under negated claim | `(0,1]` | learned/elicited per evidence model |
| `omega_k` | reliability/independence discount | `[0,1]` | 1 only for validated independent source |
| `m` | number of evidence clusters | positive integer | observed |
| `ln` | natural logarithm | real | defined |
| `logit` | probability-to-log-odds transform | `(0,1)→R` | defined |
| `sigma` | logistic inverse | `R→(0,1)` | defined |

## Dependency handling

Evidence copied from a common upstream source must be clustered. Either use one cluster likelihood or assign discounts whose total effective weight reflects the single origin. Source authority, directness, freshness, consistency, and independent confirmation may inform calibrated likelihoods/weights, but multiplying arbitrary UI scores is not Bayesian evidence.

## Pseudocode

```text
assert claim has context, time semantics, and evidence
clusters = dependency_cluster(evidence by citation/content/provenance graph)
odds = logit(calibrated_prior(claim_cohort))
for cluster in clusters:
    lr = calibrated_likelihood_ratio(cluster, claim)
    weight = calibrated_independence_reliability(cluster)
    odds += weight * ln(lr)
posterior = sigmoid(odds)
return posterior with cohort/model version, interval, evidence breakdown
```

## Complexity

Given dependency clusters, scoring is `O(m)`. Building clusters is `O(m + e)` for a provenance/citation graph, excluding entity/claim resolution costs.

## Edge and security cases

Reject zero likelihoods without smoothing policy, missing provenance, ambiguous claim negation, context/version mismatch, future evidence leakage, and uncalibrated priors presented as probability. Defend against source spam/Sybil evidence by dependency clustering and publisher identity signals. Confidence is displayed with cohort, timestamp, model version, and uncertainty interval.

## Worked example

Prior `P(c)=0.5`, so prior log-odds is `0`. One independent evidence cluster has likelihood ratio `4` and `omega=1`; two articles copied from one second origin are one cluster with likelihood ratio `3` and `omega=0.5`.

```text
posterior log-odds = ln(4) + 0.5 ln(3) = 1.9356
P(c|E) = sigma(1.9356) ≈ 0.8739
```

Treating the copied pair as two independent items would overstate confidence and is forbidden.

## Golden vectors

| Prior | Evidence `(LR, omega)` | Expected posterior |
|---|---|---|
| 0.5 | none | 0.5 |
| 0.5 | `(4,1)` | 0.8 |
| 0.2 | `(4,1)` | 0.5 |
| 0.5 | `(4,1),(3,0.5)` | approximately 0.8739 |
| 0.5 | `(1,1)` | 0.5 |

Implementations compare within declared floating-point tolerance and also test stable log-space behavior for extreme likelihood ratios.

## Calibration and rollback

Required metrics include Brier score, log loss, expected calibration error, reliability diagrams, coverage of confidence intervals, and topic/time/source slices against held-out adjudicated claims. If calibration drifts beyond cohort thresholds, suppress numeric confidence and show evidence qualitatively. Never fall back to majority vote.
