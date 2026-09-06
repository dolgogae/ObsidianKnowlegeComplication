---
title: ADR-0017 — Typed Conflict Actions and Immutable Materialization
status: normative
owners:
  - architect
  - core-rust-engineer
  - qa-security-engineer
last_updated: 2026-09-02
decision_refs:
  - ADR-0009
  - ADR-0017
source_refs:
  - HIST-CURRENT-PLAN
---

# ADR-0017: Typed Conflict Actions and Immutable Materialization

## Status

Accepted on 2026-09-02. This ADR supersedes only ADR-0009's V1
waiver-only action restriction; ADR-0009's immutable-overlay rule remains.

## Context

A generic `user_resolved` label cannot prove which sealed link target was
selected or which exact bytes are authorized. Mutating a `DraftPlan` would
invalidate its identity and all proposal bindings.

## Decision

V2 conflict overlays bind the plan ID, conflict ID and content hash, curator
ID, policy version, optional rationale, and one typed action:

- preserve the original ambiguous representation under an explicit waiver;
- select one sealed Markdown `DocumentId`; or
- select one sealed typed Canvas reference target.

Selected targets MUST be exact members of the conflict's sealed candidate set.
Arbitrary replacement text and paths are forbidden. Markdown display text,
embed state, headings, and block suffixes are preserved. Multiple selections
in one source file are composed in source-span order into one effective
operation. Canvas rewrites preserve unknown JSON fields.

`DraftPlan` remains immutable. A canonical `MaterializationPlan` binds the
base plan ID, action-set hash, proposal-set hash, effective operations, and
materialization ID. Approval, compile, independent verification, and
provenance MUST call the same deterministic derivation and reject any stale or
forged serialized materialization.

Only `LINK_AMBIGUITY` permits a manual output-changing action in V2. Path,
case, and Unicode collisions remain deterministic rules; title, alias,
frontmatter, and near-duplicate items are review-only. Conflict actions and AI
proposals require individual decisions; bulk approval is not a public action.

## Consequences

- Human choices are machine-checkable capabilities over a sealed candidate
  set, not free-form text edits.
- One immutable plan may have multiple separately identified materializations.
- V1 waivers keep their original interpretation and are never upgraded in
  place.
