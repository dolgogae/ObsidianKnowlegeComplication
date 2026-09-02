---
title: ADR-0015 — OKC V2 Format Family and Identities
status: normative-v1
owners:
  - architect
  - release-maintainer
last_updated: 2026-09-02
decision_refs:
  - ADR-0015
source_refs:
  - HIST-CURRENT-PLAN
---

# ADR-0015: OKC V2 Format Family and Identities

## Status

Accepted on 2026-09-02.

## Context

The unpublished `vaultc` schema 1 names, domains, audit paths, and Pack suffix
cannot describe the OKC product without creating ambiguous mixed-version
artifacts. The V1 bytes are nevertheless useful compatibility fixtures and
must not be silently reinterpreted.

## Decision

The product and only executable are named `okc`, version `0.2.0`. Every newly
written public serialization uses `format_family: "okc"`, `schema_version: 2`,
domain-separated identities beginning `okc:*:v2\0`, the `.okc/` audit
directory, `.okcpack` suffix, and Pack profile
`okc-tar-zstd-deterministic-v2`.

Schema 1 domains and literals remain frozen in internal read-only compatibility
packages. V2 writers MUST NOT emit `.vaultc`, `.vaultpack`, or
`vaultc:*:v1\0`. A parser MUST determine the family/version before interpreting
an identity or control file and MUST reject unknown versions rather than guess.

## Consequences

- V1 and V2 identities cannot collide or be confused by string replacement.
- New literal ID, canonical JSON, provenance, and complete-Pack goldens are
  required.
- Compatibility behavior is governed by ADR-0018.
