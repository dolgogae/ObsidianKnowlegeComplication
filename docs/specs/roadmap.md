---
title: Roadmap
status: normative-future
owners:
  - architect
  - release-maintainer
last_updated: 2026-08-16
decision_refs:
  - ADR-0001
  - ADR-0002
source_refs:
  - HIST-KNOWLEDGE-PLATFORM
  - HIST-COMPILER-PLAN
---

# Roadmap

## Progress marker: 2026-08-16

- Phase 0 is complete.
- Phases 1 and 2 have an implemented `0.2.0` vertical slice with local macOS
  arm64 test evidence, but their full cross-platform, parser, provenance,
  compatibility, safety, and performance exit criteria remain open.
- Phase 3 has deterministic unsigned OKCPack creation/verification only. The
  signing profile, MCP adapter, and Obsidian plugin have not started.
- Phases 4–6 remain documentation/research direction only.

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

Deliver deterministic `.okcpack`, signing profile, generic Obsidian review/install plugin, and thin MCP server for coding agents. Maintain one compiler implementation.

## Phase 4 — semantic knowledge and evaluation

Introduce versioned Entity/Claim/Relationship/Topic models, evidence-aware conflict views, vault health, retrieval benchmarks, agent utility, topic-relative percentiles, and calibrated Bayesian confidence. Promotion requires evidence and ADRs.

## Phase 5 — experimental memory and routing

Evaluate ACT-R-inspired activation, topic decay, graph spreading, consolidation, retrieval fusion, engine routing, merge scoring, and research-only Hopfield retrieval in isolated `okc-memory`. Do not couple these experiments to safe file materialization.

## Phase 6 — on-premise Knowledge Package Registry

Potential four-VM baseline: app (Nginx, Next.js, Spring Boot, Keycloak), data (PostgreSQL, OpenSearch, SeaweedFS, NATS), worker (FastAPI, compiler/MCP gateway, scanners), and GPU (vLLM, embedding, reranker), deployed on ordinary Linux VMs with Docker Compose and Ansible. This topology is historical guidance, not a V2 framework dependency.

Registry work includes immutable source/pack versions, search, benchmarks, compatibility, signing, moderation, consent, takedown, revocation, backup/DR, and later marketplace features. Do not start with Kubernetes; adopt an orchestrator only after measured operational need and an ADR.

## Milestone rule

Each phase must preserve source immutability, deterministic core behavior, provider neutrality, provenance closure, explicit approval, and adapter thinness. Later features do not bypass earlier gates.
