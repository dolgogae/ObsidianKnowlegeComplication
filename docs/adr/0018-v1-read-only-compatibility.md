---
title: ADR-0018 — V1 Read-Only Compatibility and Migration
status: normative
owners:
  - architect
  - release-maintainer
last_updated: 2026-09-06
decision_refs:
  - ADR-0015
  - ADR-0018
  - ADR-0027
source_refs:
  - HIST-CURRENT-PLAN
---

# ADR-0018: V1 Read-Only Compatibility and Migration

## Status

Accepted on 2026-09-02. Superseded by ADR-0027 on 2026-09-06; the historical
decision below is preserved, but current packages provide no V1 reader or
migration surface.

## Context

Existing V1 artifacts need audit continuity, but carrying mutable V1 plan and
approval state into V2 would reinterpret old identities and conflict actions.

## Decision

`okc verify` and `okc explain` may dispatch to a frozen internal V1 reader for
`.vaultpack` and `.vaultc/manifest.json`. No public executable writes V1.
Frozen V1 source, ID, Pack, and artifact goldens remain in unpublished legacy
packages and must continue to pass.

Project migration is reconstruction, not schema translation. The user relinks
every source; OKC validates its V1 snapshot identity; then V2 inspect and plan
run from the immutable bytes. V1 approvals, AI proposals, recordings, and
conflict decisions are not carried forward.

Unknown or mixed versions fail closed. The deprecated `vaultc` Rust facade has
no executable and exists for one minor release only; it reexports the V2 core
API and does not authorize V1 writing.

## Consequences

- Old published evidence can be inspected without contaminating V2 state.
- Migration requires explicit source possession and new review decisions.
- The compatibility surface is intentionally smaller than the V2 compiler.
