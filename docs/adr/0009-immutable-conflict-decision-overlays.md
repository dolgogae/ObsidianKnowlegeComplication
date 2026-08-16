---
title: ADR-0009 — Immutable Conflict Decision Overlays
status: normative-v1
owners:
  - architect
  - core-rust-engineer
  - qa-security-engineer
last_updated: 2026-08-16
decision_refs:
  - ADR-0009
source_refs:
  - HIST-CURRENT-PLAN
---

# ADR-0009: Immutable Conflict Decision Overlays

## Status

Accepted on 2026-08-16.

## Context

`DraftPlan` is a sealed, content-addressed value. Mutating a conflict's
`resolution` after planning while retaining the original `plan_id` would make
the identifier lie about the authorized build. Recomputing the plan would in
turn invalidate proposal and approval bindings.

A generic `user_resolved` label is also insufficient for required conflicts
such as an ambiguous link. Without a typed selected target or rewrite payload,
the compiler cannot prove that the materialized operation actually implements
the claimed resolution.

## Decision

The framework keeps `DraftPlan` immutable. Each conflict contains a
`content_hash` derived from its stable facts and excluding its resolution
state. Decisions are stored separately on `ApprovedPlan` as a canonical
`ConflictDecisionLog`.

Every conflict decision MUST bind:

- the sealed `plan_id`;
- the `conflict_id` and `conflict_content_hash`;
- a resolution kind;
- a non-empty resolver and policy version;
- an optional bounded rationale.

Unknown, duplicate, wrong-plan, stale-hash, or already-resolved decisions are
rejected. Required conflicts must have complete decision coverage before
compilation. Compilation and independent verification revalidate the sealed
plan and the overlay rather than trusting serialized control files.

V1 accepts only `waived_by_policy` in an external conflict decision. It records
that the unresolved source representation is intentionally retained under an
explicit policy; it does not claim that the ambiguity was fixed.
`user_resolved` and `provider_suggested` remain reserved until a versioned,
typed action payload can name and validate the selected target/rewrite.

## Consequences

- A plan ID continues to identify exactly one immutable planning result.
- Proposal approvals and conflict decisions can be reviewed and invalidated
  independently while remaining bound to the same plan.
- Serialized-plan tampering and stale conflict approvals fail closed.
- Curators may explicitly waive a required conflict in V1, but cannot yet
  express a target selection or rewrite through the public decision schema.
- Adding typed conflict actions requires a schema/version update, tests, and an
  ADR; it must not reinterpret existing V1 waivers.
