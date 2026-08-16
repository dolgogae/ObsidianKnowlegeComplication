---
title: Algorithms and AI Engineer Role Guide
status: normative-v1
owners:
  - algorithms-ai-engineer
last_updated: 2026-08-16
decision_refs:
  - ADR-0004
source_refs:
  - HIST-KNOWLEDGE-PLATFORM
  - HIST-COMPILER-PLAN
---

# Algorithms and AI Engineer Role Guide

## Responsibilities

Own provider capability contracts, proposal/evidence schemas, transcript replay semantics, claim-confidence research, retrieval/benchmark methodology, experimental memory algorithms, calibration, ablation, and promotion evidence.

## Non-responsibilities

Do not let models write files or approve themselves, claim a brain-equivalent system, present uncalibrated scores as probabilities, use an LLM as sole benchmark generator/judge, or introduce model dependencies into deterministic core.

## Mandatory reading

Read [`../../AGENTS.md`](../../AGENTS.md), [`../../PROJECT_CONTEXT.md`](../../PROJECT_CONTEXT.md), [`../specs/ai-provider-and-augmentation.md`](../specs/ai-provider-and-augmentation.md), [`../specs/provenance-and-conflicts.md`](../specs/provenance-and-conflicts.md), [`../specs/security-and-trust-boundaries.md`](../specs/security-and-trust-boundaries.md), [`../specs/testing-and-quality-gates.md`](../specs/testing-and-quality-gates.md), [`../algorithms/README.md`](../algorithms/README.md), ALG-PRV-001, and every algorithm being changed.

## Owned interfaces and invariants

- `TextGenerator`, `EmbeddingProvider`, `RerankProvider`, `KnowledgeAugmentor` semantics;
- NDJSON capability/request/response/proposal/transcript schemas with core engineer;
- evidence-bound proposals and calibrated score metadata;
- experimental isolation and deterministic fallback.

## Required tests

Provider conformance, malformed/oversized/stale/malicious proposal cases, replay equality, cancellation/timeouts, calibration metrics, held-out time/topic splits, ablations, cost/safety slices, and baseline comparisons.

## Handoff

Give core/MCP engineers frozen schemas, capability requirements, resource bounds, model-independent fixtures, metric reports, failure behavior, and kill switch. Promotion needs an ADR; research results alone do not change defaults.
