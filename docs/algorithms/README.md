---
title: Algorithm Registry and Status Policy
status: normative-v1
owners:
  - architect
  - algorithms-ai-engineer
  - core-rust-engineer
last_updated: 2026-09-05
decision_refs:
  - ADR-0003
  - ADR-0004
  - ADR-0015
  - ADR-0017
  - ADR-0022
  - ADR-0024
source_refs:
  - HIST-KNOWLEDGE-PLATFORM
  - HIST-COMPILER-PLAN
---

# Algorithm Registry and Status Policy

The word “brain” is an engineering analogy. Immutable snapshots resemble episodic records, generalized claims resemble semantic memory, graph edges support associative retrieval, and background consolidation resembles memory consolidation. These formulas are **neuroscience-inspired computational models**, not literal or validated models of a human brain.

## Status meanings

- `normative-v1`: first normative revision, required in the deterministic compiler path.
- `normative-future`: specified direction, not required or enabled in the current default compiler path.
- `experimental`: isolated opt-in research; cannot affect default outputs.
- `research-only`: exploratory; no product commitment.
- `historical`: context only.

## Registry

| ID | Algorithm | Status | Document |
|---|---|---|---|
| ALG-SNP-001 | snapshot identity and hashing | normative-v1 | [`stable/snapshot-identity-and-hashing.md`](stable/snapshot-identity-and-hashing.md) |
| ALG-NRM-001 | Markdown/Canvas/link normalization | normative-v1 | [`stable/markdown-canvas-and-link-normalization.md`](stable/markdown-canvas-and-link-normalization.md) |
| ALG-DED-001 | exact duplicate grouping | normative-v1 | [`stable/exact-and-near-deduplication.md`](stable/exact-and-near-deduplication.md) |
| ALG-DED-002 | MinHash/LSH near-duplicate candidates | normative-v1, review-only | [`stable/exact-and-near-deduplication.md`](stable/exact-and-near-deduplication.md) |
| ALG-CNF-001 | conflict resolution and output layout | normative-v1 | [`stable/conflict-resolution-and-output-layout.md`](stable/conflict-resolution-and-output-layout.md) |
| ALG-PRV-001 | provenance and evidence closure | normative-v1 | [`stable/provenance-and-evidence.md`](stable/provenance-and-evidence.md) |
| ALG-SEM-001 | semantic candidates and complete taxonomy | normative-v1 for V3 | [`stable/semantic-candidate-and-taxonomy.md`](stable/semantic-candidate-and-taxonomy.md) |
| ALG-INT-001 | evidence-complete synthesis and critic gate | normative-v1 for V3 | [`stable/evidence-complete-integration.md`](stable/evidence-complete-integration.md) |
| ALG-CLM-001 | Bayesian claim confidence | normative-future | [`stable/bayesian-claim-confidence.md`](stable/bayesian-claim-confidence.md) |
| ALG-MEM-001 | ACT-R-inspired activation and topic decay | experimental | [`experimental/memory-activation-and-topic-decay.md`](experimental/memory-activation-and-topic-decay.md) |
| ALG-MEM-002 | graph spreading activation | experimental | [`experimental/spreading-activation.md`](experimental/spreading-activation.md) |
| ALG-MEM-003 | plasticity and consolidation | experimental | [`experimental/plasticity-and-consolidation.md`](experimental/plasticity-and-consolidation.md) |
| ALG-MEM-004 | hybrid retrieval fusion | experimental | [`experimental/retrieval-fusion-and-engine-routing.md`](experimental/retrieval-fusion-and-engine-routing.md) |
| ALG-MEM-005 | benchmark-based engine routing | experimental | [`experimental/retrieval-fusion-and-engine-routing.md`](experimental/retrieval-fusion-and-engine-routing.md) |
| ALG-MEM-006 | Vault merge scoring | experimental | [`experimental/vault-merge-scoring.md`](experimental/vault-merge-scoring.md) |
| ALG-MEM-007 | Hopfield associative retrieval | research-only | [`experimental/hopfield-associative-retrieval.md`](experimental/hopfield-associative-retrieval.md) |

## Stable boundary

The stable V2 compiler includes snapshot identity, canonicalization for
comparison, source-local-first and cross-source link rewriting, exact
deduplication, review-only near-duplicate candidates, deterministic
conflict/output rules, typed ambiguity actions, immutable materialization,
provenance, approval, and atomic compilation. It MUST work without an
embedding model, LLM, graph server, or MCP server.

Bayesian claim confidence is housed with stable knowledge-model mathematics because its semantics must remain auditable, but its status is `normative-future`; V2 has no mandatory Claim layer and V3 does not use its uncalibrated score as an approval signal.

The V3 path additionally requires ALG-SEM-001 and ALG-INT-001. These algorithms
use provider output only as recorded proposals; deterministic local validation,
critic closure, and curator approval remain compiler gates.

## Experimental isolation

ACT-R activation, spreading activation, plasticity, consolidation, retrieval
fusion, engine routing, and merge scores belong in a future `okc-memory`
package or evaluation harness. Hopfield retrieval is research-only.
Experimental results may annotate reports but MUST NOT change frozen V1/V2
compatibility behavior or V3 default taxonomy, disposition, conflict
resolution, approval, provenance, or safe output operations.

## Promotion gate

Promotion requires all of the following:

1. fixed task definition and data contract;
2. non-neural/simple baselines and ablation studies;
3. held-out, topic-stratified evaluation with leakage controls;
4. confidence intervals, calibration error, and failure-slice reporting;
5. latency, memory, token, and privacy costs;
6. deterministic fallback and rollback switch;
7. security review and reproducible fixtures;
8. accepted ADR plus updates to specifications, traceability, and default policy.

See [`experimental/calibration-and-experiment-design.md`](experimental/calibration-and-experiment-design.md).

## Required algorithm-document template

Every algorithm specifies objective/non-goals, inputs/outputs, formulas, all symbols and units/ranges, defaults, pseudocode, complexity, edge/security cases, a worked example, golden vectors, calibration or correctness metrics, failure/rollback behavior, and references. Undefined coefficients are not implementable defaults.
