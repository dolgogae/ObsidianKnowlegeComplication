---
title: ALG-MEM-006 — Vault Merge Scoring
status: experimental
owners:
  - algorithms-ai-engineer
  - architect
last_updated: 2026-08-16
decision_refs:
  - ADR-0004
source_refs:
  - HIST-KNOWLEDGE-PLATFORM
---

# ALG-MEM-006: Vault Merge Scoring

## Objective and non-goals

Estimate whether compiling Vault `A` with Vault `B` improves a specified topic/task cohort. It is not a universal Vault quality score, a truth score, or permission to merge without review.

## Inputs and outputs

Inputs are two immutable snapshot IDs, a frozen deterministic/approved compile result, topic/task cohort, benchmark cases, feature definitions, and declared utility weights. Output is the complete feature vector, uncertainty intervals, costs/conflicts, and an optional aggregate score when calibration is valid.

## Formula

```text
M(A,B) = theta_1 DeltaCoverage
       + theta_2 DeltaRetrieval
       + theta_3 Novelty
       + theta_4 Evidence
       - theta_5 Redundancy
       - theta_6 Conflict
       - theta_7 Dilution
```

Every feature is computed against the same frozen benchmark and normalized to `[-1,1]` for deltas or `[0,1]` for rates. Report the feature vector beside the aggregate.

## Symbols

| Symbol | Meaning | Type/range/unit | Experimental default |
|---|---|---|---|
| `A,B` | immutable Vault snapshots | Snapshot IDs | required |
| `M(A,B)` | topic/task-conditioned merge utility | real, commonly `[-1,1]` after normalized weights | computed |
| `DeltaCoverage` | change in benchmark topic/claim coverage | `[-1,1]` | measured |
| `DeltaRetrieval` | change in retrieval metric | `[-1,1]` | measured |
| `Novelty` | useful nonredundant information gain | `[0,1]` | measured |
| `Evidence` | improvement in evidence/provenance quality | `[-1,1]` | measured |
| `Redundancy` | duplicate/context-waste rate | `[0,1]` | measured |
| `Conflict` | unresolved/weighted contradiction rate | `[0,1]` | measured |
| `Dilution` | loss from added distractors/context | `[0,1]` | measured |
| `theta_1..theta_7` | nonnegative cohort utility weights | real; normalize sum to `1` for display | learned/declared, no universal default |

Compatibility views MAY expose similarity, complementarity, redundancy, and conflict separately. “Knowledge DNA” MAY describe topic distribution, depth, breadth, freshness, connectivity, evidence, and originality, but it must name its cohort/model version.

## Pseudocode

```text
freeze snapshots, topic cohort, benchmark cases, compiler/routing policy
evaluate baseline A (and optionally B) without data leakage
compile candidate A+B with deterministic plan and fixed approved transcript
evaluate same cases on candidate
derive normalized feature vector with uncertainty
score using predeclared theta weights
return score, features, confidence intervals, cases, costs, conflicts
```

## Complexity

Dominated by compilation and benchmark execution: approximately `O(compile(A+B) + cases × retrieval/agent cost)`. Pairwise marketplace scoring is `O(V²)` and must use candidate prefiltering, cached fingerprints, and explicit freshness.

## Edge and security cases

Prevent benchmark leakage, topic-size instability, source/license incompatibility, duplicated-source inflation, majority-vote conflict suppression, adversarial filler, cohort drift, and Simpson's paradox. Small cohorts do not receive confident percentiles. Negative component values remain visible.

## Worked example

With equal normalized weights `theta_j=1/7` and features `ΔCoverage=.4`, `ΔRetrieval=.2`, `Novelty=.5`, `Evidence=.3`, `Redundancy=.2`, `Conflict=.1`, `Dilution=.1`:

```text
M = (0.4+0.2+0.5+0.3-0.2-0.1-0.1)/7
  = 1.0/7 ≈ 0.142857
```

The modest positive score does not hide the `.1` conflict rate.

## Golden vectors

| Feature condition | Expected |
|---|---|
| all features zero | `M=0` |
| example above with equal normalized weights | `M≈0.142857` |
| only `Conflict=1`, `theta_6=.25`, others zero | `M=-.25` |
| weights or feature schema missing | no aggregate score |

## Calibration and rollback

Validate against blinded curator preference and downstream task utility with topic-stratified intervals. Report rank stability under reasonable weight perturbations and each component's ablation. If aggregate scores mislead or cohorts are too small, remove the aggregate and show multidimensional facets only. The platform explicitly rejects a single absolute ranking across unrelated topics.
