---
title: ADR-0006 — Compiled Output Excludes Raw Source Copies
status: normative-v1
owners:
  - architect
  - qa-security-engineer
last_updated: 2026-08-16
decision_refs:
  - ADR-0006
source_refs:
  - HIST-COMPILER-PLAN
---

# ADR-0006: Compiled Output Excludes Raw Source Copies

## Status

Accepted on 2026-08-15.

## Context

Embedding complete raw source Vaults in compiled output would inflate packs, leak excluded/private files, complicate licenses, and blur the difference between a snapshot archive and integrated knowledge.

## Decision

Compiled Vaults contain integrated selected/generated knowledge, attachments, views, and audit metadata, but no raw `_sources` directory or source archives. Provenance uses hashes, identities, spans, attribution, and optional external snapshot references.

## Consequences

Artifacts are smaller and safer. Full byte reconstruction requires access to the immutable source snapshot store; provenance explanations must remain useful when that store is offline.
