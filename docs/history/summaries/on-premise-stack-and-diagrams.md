---
title: On-Premise Platform Stack and Diagram Summary
status: historical
owners:
  - architect
  - qa-security-engineer
last_updated: 2026-08-16
decision_refs:
  - ADR-0002
source_refs:
  - HIST-SHARED-CHAT
---

# On-Premise Platform Stack and Diagram Summary

Historical source material. Not normative. Use current specifications and accepted ADRs for implementation.

## Deployment constraint and baseline

The user prohibited public cloud. The platform design moved to Ubuntu 24.04 LTS VMs, Docker Engine/Compose, and Ansible, explicitly avoiding initial Kubernetes.

The later four-VM baseline was:

| VM | Services |
|---|---|
| App | Nginx, Next.js/TypeScript, Java 21 + Spring Boot 3.5.x, Keycloak |
| Data | PostgreSQL 17, OpenSearch, SeaweedFS, NATS JetStream |
| Worker | Python 3.12, FastAPI/Pydantic, parsers/compiler/MCP gateway, ClamAV/Gitleaks |
| GPU | vLLM, embeddings, reranker |

Supporting choices included OpenBao for secrets, Harbor for images, and Prometheus/Grafana/Loki/OpenTelemetry for observability. Earlier MVP advice was PostgreSQL-centered and warned against operating PostgreSQL FTS, OpenSearch, a separate vector database, and a graph database simultaneously without measured need.

## Diagram set captured in the public session

The final historical response described system context, container/component boundaries, trust zones, ingestion/transactional-outbox sequence, compile/merge sequence, deployment, canonical ERD, event schema, state machines, and invariants. The crucial invariants were immutable raw snapshots, PostgreSQL as platform canonical truth, rebuildable indexes, evidence on generated claims, snapshot-pinned merges, idempotent consumers, no direct LLM-to-database truth, and a benchmark gate before publishing.

For the present repository, this platform stack is future deployment context. The V1 framework has no Spring Boot, Next.js, OpenSearch, NATS, SeaweedFS, Keycloak, vLLM, or Docker runtime dependency.

## Isolation carried forward

Untrusted jobs remain non-root, source read-only, no host path/privilege, default-deny outbound network, resource-capped, seccomp/AppArmor constrained, and ephemeral. Docker Compose must express equivalent controls if/when the service is built.
