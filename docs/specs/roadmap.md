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
source_refs:
  - HIST-KNOWLEDGE-PLATFORM
  - HIST-COMPILER-PLAN
---

# Roadmap

## Progress marker: 2026-09-06

- Phase 0 is complete.
- Phases 1 and 2 have an implemented `0.2.0` vertical slice with local macOS
  arm64 test evidence, but their full cross-platform, parser, provenance,
  compatibility, safety, and performance exit criteria remain open.
- Phase 3 has deterministic unsigned V2 OKCPack creation/verification and a
  locally implemented typed Python and Node.js API-v1 package slice. Its remote
  native/version matrix is pending. The signing profile, MCP adapter, and
  Obsidian plugin have not started.
- Phases 4–6 remain documentation/research direction only.
- V3 now has an accepted AI-required format/provider/integration contract and a
  development vertical slice: schema-3 projects and journals, HTTP providers,
  sensitive routing, organizer/synthesis/critic review, approval closure, and
  provider-free directory compile/verify/explain. It is not a stable milestone:
  `ALG-SEM-001` HNSW/chunking parity, manual amendment, V3 Pack and
  non-Markdown carry-through, command adapter, PTY qualification, scale, and hosted
  cross-platform evidence are open.
- The runtime-neutral `okc-interop` facade, CPython 3.11+ `abi3` package, and
  Node.js 22.13+ Node-API package now cover the current V3 approval flow and
  frozen V1/V2/V3 verify/explain. They remain development packages until the
  four-host package matrix and every shared V3 gate pass.

## V3 stabilization milestone

Complete `ALG-SEM-001` and `ALG-INT-001` end to end, including hierarchical
synthesis, manual section amendments, persisted remote
disclosure exceptions, deterministic V3 Pack, attachment/Canvas/Base
materialization and link rewriting, and
all V3-specific quality gates. Stable release language is prohibited until
the 100k-note performance/cost report and existing QG-001–008 blockers close.

This marker is a navigation aid; [`../CURRENT_STATE.md`](../CURRENT_STATE.md)
and [`../TRACEABILITY.md`](../TRACEABILITY.md) are authoritative for current
implementation and verification status.

## Phase 0 — documentation baseline

Freeze product boundary, stable algorithms, provider-neutral contracts, security invariants, ADRs, role guides, and recoverable history. Exit: a cold-start coding agent can begin implementation from Markdown alone.

## Phase 1 — deterministic Rust compiler

Deliver the Rust workspace, snapshot/identity pipeline, Markdown/Canvas IR, exact deduplication, conflict planning, safe output layout, provenance, atomic compile, verification, CLI, fixture corpus, and cross-platform CI. No AI dependency.

## Phase 2 — provider protocol and review workflow

Deliver `okc-protocol`, universal subprocess provider, proposal schema, evidence validation, approvals, record/replay, and example provider packages. Add a non-AI manual decision workflow first.

## Phase 3 — adapters and packaging

Deliver deterministic `.okcpack`, language packages, signing profile, generic
Obsidian review/install plugin, and thin MCP server for coding agents. Maintain
one compiler implementation. Python and Node.js are the first API-v1 language
packages; after their usage experience stabilizes the contract, evaluate Go,
then JVM and .NET bindings rather than cloning compiler policy.

## Phase 4 — semantic knowledge and evaluation

Introduce versioned Entity/Claim/Relationship/Topic models, evidence-aware conflict views, vault health, retrieval benchmarks, agent utility, topic-relative percentiles, and calibrated Bayesian confidence. Promotion requires evidence and ADRs.

## Phase 5 — experimental memory and routing

Evaluate ACT-R-inspired activation, topic decay, graph spreading, consolidation, retrieval fusion, engine routing, merge scoring, and research-only Hopfield retrieval in isolated `okc-memory`. Do not couple these experiments to safe file materialization.

## Phase 6 — on-premise Knowledge Package Registry

Potential four-VM baseline: app (Nginx, Next.js, Spring Boot, Keycloak), data (PostgreSQL, OpenSearch, SeaweedFS, NATS), worker (FastAPI, compiler/MCP gateway, scanners), and GPU (vLLM, embedding, reranker), deployed on ordinary Linux VMs with Docker Compose and Ansible. This topology is historical guidance, not a V2 framework dependency.

Registry work includes immutable source/pack versions, search, benchmarks, compatibility, signing, moderation, consent, takedown, revocation, backup/DR, and later marketplace features. Do not start with Kubernetes; adopt an orchestrator only after measured operational need and an ADR.

## Milestone rule

Each phase must preserve source immutability, deterministic core behavior, provider neutrality, provenance closure, explicit approval, and adapter thinness. Later features do not bypass earlier gates.
