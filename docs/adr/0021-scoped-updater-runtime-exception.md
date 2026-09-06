---
title: ADR-0021 — Scoped Updater Runtime Exception
status: normative
owners:
  - architect
  - release-maintainer
last_updated: 2026-09-02
decision_refs:
  - ADR-0019
  - ADR-0020
source_refs:
  - HIST-CURRENT-PLAN
---

# ADR-0021: Scoped Updater Runtime Exception

## Status

Accepted on 2026-09-02 to resolve a normative dependency conflict discovered
during implementation.

## Context

ADR-0019 says that the application does not introduce Tokio, while ADR-0020
requires the blocking API of the pinned `axoupdater 0.10.0`. That blocking API
creates a private current-thread Tokio runtime for each synchronous update
check or installation. The two unqualified requirements cannot both hold.

## Decision

OKC MUST NOT use Tokio for the TUI event loop, core/provider workers, project
services, cancellation, or publication. Those paths remain synchronous and
thread-based.

The exact `axoupdater 0.10.0` blocking adapter MAY create its library-owned,
current-thread runtime only while `okc update` or a receipt-aware update check
is executing. OKC does not expose that runtime, keep it alive between calls,
or schedule compiler/provider work on it. `tokio` remains transitive rather
than a direct OKC dependency.

If a later axoupdater release offers a supported blocking implementation
without this runtime, the exception SHOULD be removed in a new ADR.

## Consequences

- The reducer and long-running application architecture still satisfy the
  thread-based design boundary.
- A literal “no Tokio anywhere in the dependency graph” claim is forbidden.
- Dependency and runtime behavior must be disclosed in release review.
