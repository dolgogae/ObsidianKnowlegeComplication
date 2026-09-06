---
title: ADR-0003 — Immutable Inputs and New Output
status: normative
owners:
  - architect
  - qa-security-engineer
last_updated: 2026-08-16
decision_refs:
  - ADR-0003
source_refs:
  - HIST-KNOWLEDGE-PLATFORM
---

# ADR-0003: Immutable Inputs and New Output

## Status

Accepted on 2026-08-15.

## Context

Directly merging source folders risks data loss, partial writes, irreproducible decisions, and broken provenance. Source Vaults may also be hostile.

## Decision

Treat each source as an immutable snapshot, parse into canonical IR, create a reviewable plan, and materialize a new sibling-staged output. Never modify sources. Reject an existing destination by default.

## Consequences

Compilation is auditable and retryable. It requires additional storage and a deliberate future update/uninstall workflow. Snapshot verification becomes mandatory immediately before materialization.
