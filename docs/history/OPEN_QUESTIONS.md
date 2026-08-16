---
title: Open Questions
status: normative-future
owners:
  - architect
last_updated: 2026-08-16
decision_refs:
  - ADR-0002
  - ADR-0004
source_refs:
  - HIST-KNOWLEDGE-PLATFORM
  - HIST-COMPILER-PLAN
---

# Open Questions

These questions are not permission to improvise defaults. Resolve a behavior-changing answer through an ADR and specification update.

## V1 implementation decisions

- Exact canonical AST encoding and pinned Comrak/Unicode versions.
- Filename raw-byte policy on Unix and invalid UTF-8 treatment.
- Complete portable filename constraints and maximum component/path limits.
- Short-document near-duplicate behavior and final pinned MinHash seed table.
- Final public Rust type names, CLI flags, numeric exit codes, and diagnostic catalog.
- SQLite schema, migration policy, resume/cleanup behavior, and workspace encryption needs.
- Exact deterministic tar/zstd compatibility contract across compressor versions.
- Signature profile and key lifecycle; likely post-V1 but required before marketplace distribution.

## AI and knowledge semantics

- Which proposal kinds are enabled in the first augmentation release.
- Whether remote hosted LLM APIs are allowed under the “no public cloud” constraint, and how users declare data residency.
- Canonical ontology/entity resolution, claim negation, temporal/context semantics, and human conflict adjudication.
- Authority/reliability calibration and dependency clustering for evidence.
- Provider transcript secret handling, retention, and reproducibility versus privacy.

## Benchmark and experimental models

- Topic taxonomy, assignment governance, cohort minimum size, and fair percentile policy.
- Ground-truth construction and bias/leakage control for LLM-generated cases.
- Initial coefficients/normalization for activation, graph diffusion, consolidation, retrieval, routing, and merge scoring; current numeric values are experiment examples only.
- Promotion thresholds and minimum practical effects by task.

## Pack/plugin/registry governance

- Pack dependencies, updates, uninstall after user edits, namespace collision, and reproducible rebuild compatibility.
- Publisher identity, licensing/consent, moderation, takedown/deletion, revocation, and already-derived pack policy.
- Backup/HA/DR and scheduling of isolated jobs for the on-premise registry.
- Scale thresholds and the orchestrator choice after single/multi-host Compose becomes insufficient.
- OSS MCP compatibility, licensing, version pinning, and fallback maintenance.
