---
title: Architect Role Guide
status: normative-v1
owners:
  - architect
last_updated: 2026-08-16
decision_refs:
  - ADR-0001
  - ADR-0002
source_refs:
  - HIST-CURRENT-PLAN
---

# Architect Role Guide

## Responsibilities

Own product boundaries, component dependency direction, canonical IR authority, schema evolution, cross-component invariants, ADR quality, and reconciliation of normative documents. Ensure future registry/platform work does not leak into the local compiler core.

## Non-responsibilities

Do not prescribe vendor models as core dependencies, implement UI merge policy in adapters, treat experimental scores as truth, or bypass specialist review for security/format changes.

## Mandatory reading

Read [`../../AGENTS.md`](../../AGENTS.md), [`../../PROJECT_CONTEXT.md`](../../PROJECT_CONTEXT.md), [`../CURRENT_STATE.md`](../CURRENT_STATE.md), [`../specs/product-and-scope.md`](../specs/product-and-scope.md), [`../specs/framework-architecture.md`](../specs/framework-architecture.md), [`../specs/canonical-knowledge-ir.md`](../specs/canonical-knowledge-ir.md), [`../algorithms/README.md`](../algorithms/README.md), and all accepted [`../adr/`](../adr/) records affected by the work.

## Owned interfaces and invariants

- boundaries among `vaultc`, protocols, adapters, and experimental packages;
- IR and format version ownership;
- immutable snapshot, canonical-source, provider-neutrality, approval, and provenance invariants;
- precedence and requirement-ID consistency.

## Required review/tests

Review dependency graphs, schema compatibility tests, traceability, threat-model changes, and ADR consequences. Architectural changes require counterexamples, migration/rollback, and affected quality gates.

## Handoff

Provide implementers with accepted ADR, changed requirement IDs, explicit invariants/non-goals, migration path, and required verification. Update current state and decision log after acceptance.
