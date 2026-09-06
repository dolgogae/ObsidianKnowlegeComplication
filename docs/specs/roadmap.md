---
title: Roadmap
status: normative-future
owners:
  - architect
  - release-maintainer
last_updated: 2026-09-06
decision_refs:
  - ADR-0001
  - ADR-0002
  - ADR-0022
  - ADR-0023
  - ADR-0024
  - ADR-0025
  - ADR-0026
  - ADR-0027
source_refs:
  - HIST-KNOWLEDGE-PLATFORM
  - HIST-COMPILER-PLAN
---

# Roadmap

## Progress marker: 2026-09-06

The repository has one current Schema 3 development slice at version `0.3.0`:
safe corpus construction, projects and append-only journals, HTTP provider
profiles, sensitive routing, organizer/synthesis/critic review, approval
closure, offline Markdown-directory compile/verify/explain, CLI/TUI, and typed
Python/Node.js packages using interop schema 2.

Retired implementations are available only from the annotated archive tags in
ADR-0027. They are not a compatibility milestone and must not return to the
current dependency graph or CLI.

The current product is not stable. Open stabilization work includes:

- deterministic block chunking/batching, fixed-seed HNSW, and complete
  candidate-union parity for `ALG-SEM-001`;
- hierarchical evidence synthesis and manual section amendment;
- persisted sensitive-finding exceptions and provider conformance;
- a supervised current-schema command adapter, if still required;
- attachment, Canvas, Base, and complete link materialization;
- a deterministic current-schema OKCPack writer and verifier;
- provider-backed TUI PTY, crash, cancellation, and supported-platform tests;
- fuzz/property campaigns, descriptor-relative traversal hardening, and the
  100,000-note/20 GB performance and cost report;
- the remote Python/Node native matrix, reproducibility, signing,
  notarization, SBOM, attestation, and publication gates.

`CURRENT_STATE.md` and `TRACEABILITY.md` are authoritative for evidence.

## Phase 0 — documentation and current source boundary

Keep product/spec/algorithm/ADR/traceability contracts sufficient for a fresh
agent. Maintain one current implementation. Preserve prior generations in Git
history/archive tags instead of source compatibility layers.

## Phase 1 — current compiler stabilization

Complete the current semantic algorithms, non-Markdown materialization,
portable safe I/O, Pack format, hostile-input/fuzz suite, deterministic host
matrix, and scale benchmark. Maintain offline approved-plan compilation and
closed provenance.

## Phase 2 — adapters and packaging

Qualify Python and Node.js packages, a generic Obsidian review/install plugin,
and a thin MCP adapter without cloning compiler policy. After API-v1 usage is
stable, evaluate Go, then JVM and .NET bindings.

## Phase 3 — semantic knowledge and evaluation

Evaluate versioned Entity/Claim/Relationship/Topic models, evidence-aware
conflict views, retrieval benchmarks, agent utility, and calibrated confidence.
Promotion requires baselines, ablations, held-out evaluation, calibration,
security review, deterministic fallback, and an ADR.

## Phase 4 — experimental memory and routing

Keep ACT-R-inspired activation, topic decay, graph spreading, consolidation,
retrieval fusion, engine routing, merge scoring, and Hopfield retrieval in an
isolated evaluation package. They cannot affect safe file materialization by
default.

## Phase 5 — optional on-premise registry

Only after the local compiler and package format are stable, consider an
on-premise immutable registry with search, benchmarking, compatibility,
signing, consent, moderation, takedown/revocation, and backup/DR. Begin with
ordinary Linux VMs and measured requirements; do not introduce an orchestrator
without an ADR and operational evidence.

## Milestone rule

Every phase preserves immutable sources, deterministic approved compilation,
provider neutrality, evidence closure, explicit approval, adapter thinness,
and fail-closed publication. Future work cannot bypass earlier gates.
