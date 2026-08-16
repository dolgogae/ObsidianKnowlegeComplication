---
title: ADR-0004 — Deterministic Core and Provider-Neutral AI
status: normative-v1
owners:
  - architect
  - algorithms-ai-engineer
last_updated: 2026-08-16
decision_refs:
  - ADR-0004
source_refs:
  - HIST-COMPILER-PLAN
---

# ADR-0004: Deterministic Core and Provider-Neutral AI

## Status

Accepted on 2026-08-15.

## Context

Knowledge synthesis benefits from models, but vendor lock-in, prompt nondeterminism, hallucination, privacy, and direct filesystem authority would undermine the compiler contract.

## Decision

The non-AI path is complete and deterministic. AI connects through capability traits or versioned NDJSON and returns evidence-bound proposals only. Proposals require deterministic validation and explicit approval. Request/response transcripts support offline replay. Core ships no mandatory vendor SDK.

## Consequences

Any model can integrate without changing core, and outputs remain reviewable/reproducible. Applications must implement approval UX, model capabilities vary, and generated quality is not guaranteed by protocol conformance.
