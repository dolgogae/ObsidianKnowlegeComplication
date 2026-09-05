---
title: ADR-0024 — Evidence-Complete Integration and Critic Gate
status: normative-v1
owners:
  - architect
  - algorithms-ai-engineer
  - core-rust-engineer
  - qa-security-engineer
last_updated: 2026-09-03
decision_refs:
  - ADR-0003
  - ADR-0006
  - ADR-0010
  - ADR-0017
  - ADR-0024
source_refs:
  - HIST-COMPILER-PLAN
---

# ADR-0024: Evidence-Complete Integration and Critic Gate

## Status

Accepted on 2026-09-03.

## Context

A free-form generated Markdown document cannot prove that every source block
and frontmatter value was handled, that contradictions survived synthesis, or
that unsupported claims were rejected. “Perfect integration” also cannot mean
that a model establishes objective truth.

## Decision

V3 defines completeness mechanically: every Markdown document belongs to one
approved cluster; every source block and frontmatter value has exactly one
typed disposition; every generated section has evidence; every contradiction
retains all source claims with time/context; and a separate critic plus curator
approval closes the plan.

`ALG-SEM-001` performs deterministic chunking and one-model-space embedding,
validates vector dimensions/order/finite values, and unions fixed-seed semantic
neighbors with deterministic MinHash, title, alias, and link candidates. The
candidate list is recorded. An organizer proposes a complete one-cluster-per-
document taxonomy. A curator may merge, split, rename, or move clusters and
must approve the hash of the complete taxonomy before synthesis approval.

`ALG-INT-001` requires synthesis for singleton and multi-document clusters.
Large clusters may use evidence-extraction and reduce tasks under the same
synthesis role. A proposal contains stable sections, section evidence, typed
related links, block and metadata dispositions, and contradiction sets. A
disposition is exactly one of `integrated`, `preserved_verbatim`, or
`omission_proposed`. An omission takes effect only when its exact target hash
and rationale are covered by the cluster approval.

The critic independently compares the proposal with source evidence and emits
typed findings for unsupported claims, omission, hidden contradiction,
misattribution, link/metadata loss, and prompt-injection influence. `critical`
or `major` findings block approval. A `minor` finding requires a curator-bound
waiver and rationale. Manual section amendment preserves section IDs and
evidence maps, appends a new proposal revision, reruns the critic, and makes
older approvals stale.

Materialization writes canonical notes beneath
`knowledge/<approved-taxonomy>/<slug>.md` and one source-specific redirect stub
beneath `legacy/<source-id>/<original-path>.md`. Markdown and Canvas references
are rewritten through the sealed canonical map. Attachments retain hash
deduplication, Canvas retains safe typed rewriting, and Base remains opaque
with a warning. Every canonical section and stub closes provenance through its
source block, proposal, critic, and approval.

“Perfect” therefore means disposition completeness, evidence closure,
contradiction preservation, critic success, and explicit human approval. It is
not a claim of truth, optimal taxonomy, or calibrated confidence. `ALG-MEM-006`
and uncalibrated `ALG-CLM-001` scores remain outside the default path.

## Invalidation and rollback

Any source, policy, taxonomy, proposal, critic, route, schema, prompt, or manual
section change appends a new revision and makes dependent approvals stale.
Compilation never repairs or auto-approves stale state. Rollback selects an
older complete run; history is not rewritten.

## Consequences

- Completeness and omission authority are testable properties.
- Contradictory evidence remains visible instead of being voted away.
- Review workload increases because auto-approval is deliberately absent.
- Canonical notes replace semantic duplicates while legacy stubs preserve
  source-path navigation and provenance.
