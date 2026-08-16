---
title: Experimental Algorithm Calibration and Study Design
status: experimental
owners:
  - algorithms-ai-engineer
  - qa-security-engineer
last_updated: 2026-08-16
decision_refs:
  - ADR-0004
source_refs:
  - HIST-KNOWLEDGE-PLATFORM
---

# Experimental Algorithm Calibration and Study Design

## Purpose

Define a reproducible gate for neuroscience-inspired retrieval/consolidation and future claim scoring. Formula elegance is not evidence of product utility.

## Experimental unit

Freeze and identify:

- immutable Vault snapshots and license/privacy permission;
- topic taxonomy and cohort assignment;
- query/task cases, evidence ground truth, and adjudication rubric;
- time split so future events cannot train past predictions;
- compiler, index, model, prompt, provider, and routing versions;
- hardware/resource budget and random seeds;
- primary metric, guardrails, minimum practical effect, and stopping rule.

## Baselines

Every study includes the simplest viable baseline: lexical BM25, recency, frequency, deterministic RRF, static graph, no consolidation, or nearest-neighbor retrieval as appropriate. Run ablations for each added signal. Compare equal candidate budgets.

## Metrics

- Retrieval: Recall@5/10, MRR, nDCG, context recall/precision, source diversity.
- Agent tasks: correctness, faithfulness, troubleshooting success, historical recall.
- Confidence: Brier score, log loss, expected calibration error, interval coverage.
- Merge: coverage gain, retrieval dilution, novelty, redundancy, conflict, provenance coverage.
- Cost: p50/p95 latency, CPU/GPU memory, tokens, energy proxy, index size.
- Safety/fairness: source dominance, topic/language slices, secret leakage, prompt-injection success, adversarial manipulation.

Report bootstrap or analytically justified confidence intervals and cohort sample sizes. Do not publish percentiles for cohorts below a declared minimum.

## Auto-generated query safeguards

Queries generated from a Vault must retain originating evidence as ground truth and be deduplicated across train/test. The same LLM/prompt cannot be the sole generator and sole judge. Human-audited samples and non-LLM structural cases are mandatory. Generated phrasing must not leak exact answer strings or paths.

## Promotion checklist

An experiment is eligible for an ADR only when it reproducibly beats the named baseline on the primary held-out metric, meets all guardrails, demonstrates stable topic slices, has bounded resource cost, exposes component explanations, supports a deterministic fallback, and passes security/privacy review. Promotion changes status and defaults in a separate reviewed change.

## Rollback

Every runtime experiment has a kill switch or coefficient-zero path, a last-known-good signed configuration, and a way to rebuild derived indexes. Rollback never alters immutable snapshots or provenance. Numeric scores are suppressed when calibration version/data freshness is invalid.

## Study record template

```text
Experiment ID / hypothesis / owner / date
Snapshot IDs / cohorts / permissions
Baseline / candidate / ablations
Frozen parameters and seeds
Primary metric and minimum effect
Guardrails and resource budgets
Results with intervals and slices
Failures and qualitative review
Decision: reject / iterate / ADR candidate
Artifacts and reproducibility commands
```
