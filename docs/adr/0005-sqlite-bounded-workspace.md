---
title: ADR-0005 — SQLite as the Bounded V1 Workspace
status: normative
owners:
  - core-rust-engineer
last_updated: 2026-08-16
decision_refs:
  - ADR-0005
source_refs:
  - HIST-COMPILER-PLAN
---

# ADR-0005: SQLite as the Bounded V1 Workspace

## Status

Accepted on 2026-08-15.

## Context

The 100,000-note target can exceed comfortable all-in-memory processing, but requiring PostgreSQL/OpenSearch/graph services would make a local framework difficult to adopt.

## Decision

Provide an abstract workspace with in-memory support for small tests and SQLite for V1 production use. The CLI enables `rusqlite` with bundled SQLite. Store large blobs in source/output streams rather than duplicating them in the database.

## Consequences

V1 stays local and self-contained. SQLite is not the future registry database, and schemas/migrations still require strict versioning and deterministic query ordering.
