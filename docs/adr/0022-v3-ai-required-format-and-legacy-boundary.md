---
title: ADR-0022 — V3 AI-Required Format and Legacy Boundary
status: normative-v1
owners:
  - architect
  - release-maintainer
last_updated: 2026-09-03
decision_refs:
  - ADR-0015
  - ADR-0018
  - ADR-0019
  - ADR-0022
source_refs:
  - HIST-CURRENT-PLAN
---

# ADR-0022: V3 AI-Required Format and Legacy Boundary

## Status

Accepted on 2026-09-03.

## Context

The V2 format permits a complete AI-free build and treats near-duplicate
semantic consolidation as review-only. V3 instead defines a Compiled Vault as
the result of AI-assisted classification and evidence-complete synthesis of
every Markdown document. Reusing schema 2 or its identity domains would make
one identifier describe two incompatible approval and materialization models.

V1 and V2 artifacts must remain auditable. Their mutable projects, proposals,
and approvals cannot be translated into V3 authority because they did not bind
taxonomy, block dispositions, metadata dispositions, critic findings, or the
V3 disclosure route.

## Decision

The workspace product version is `0.3.0`. New V3 project, plan, provider
recording, protocol, artifact, and pack structures use schema version 3 and
domain-separated identities beginning `okc:*:v3\0`. The deterministic pack
profile is `okc-tar-zstd-deterministic-v3`.

A successful V3 compile MUST consume one `ApprovedIntegrationPlan` that binds:

- the exact source snapshot/corpus and semantic configuration hashes;
- one approved taxonomy covering every Markdown document exactly once;
- one valid synthesis proposal and passing critic report for every cluster,
  including singleton clusters;
- one explicit current curator approval per cluster;
- every omission and minor-finding waiver;
- every provider request/response recording used by the run; and
- the exact materialization recipe.

Compile is offline and MUST NOT call a provider. Missing, rejected, stale, or
incomplete integration state deterministically refuses publication.

V1 and V2 are read-only compatibility families. The public compatibility
surface is `verify` and `explain`; V3 writers do not emit schema 1 or 2. A V2
project upgrade requires `project upgrade --out`: it creates an absent V3
project, copies only source bindings and curator display configuration, leaves
the original project untouched, and creates a new run. Plans, recordings,
decisions, and approvals are never carried forward.

The V3 application state is an append-only journal of runs, tasks, exchanges,
clusters, and approvals backed by content-addressed objects. Source or policy
changes append a new run and mark dependent approvals stale; they do not delete
or rewrite history.

## Compatibility and rollback

Schema family is detected before decoding. Unknown or mixed versions fail
closed. V3 can be disabled by using a V2 binary solely to inspect existing V2
artifacts, but no V3 object is downgraded or reinterpreted. The frozen V1/V2
goldens remain release regressions.

## Consequences

- V3 has an intentionally stronger AI and approval precondition than V2.
- AI-free reproducibility now means provider-free replay and compilation from
  sealed recordings, not creation of a new V3 integration without AI.
- The format break is explicit and V2 audit evidence remains meaningful.
- A stable `0.3.0` release remains blocked until all V3 and existing quality
  gates have evidence.
