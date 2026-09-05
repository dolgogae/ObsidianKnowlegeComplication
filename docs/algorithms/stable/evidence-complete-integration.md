---
title: ALG-INT-001 — Evidence-Complete Integration
status: normative-v1
owners:
  - algorithms-ai-engineer
  - core-rust-engineer
  - qa-security-engineer
last_updated: 2026-09-03
decision_refs:
  - ADR-0024
source_refs:
  - HIST-COMPILER-PLAN
---

# ALG-INT-001: Evidence-Complete Integration

## Objective and non-goals

Validate that every source Markdown block and metadata value has an explicit
disposition, every synthesized section is evidence-supported, contradictions
are preserved, and blocking critic findings are resolved before compilation.
This does not determine objective truth.

## Inputs and outputs

Inputs are one approved taxonomy, the exact source block and metadata
inventory, structured synthesis proposals, structured critic reports, curator
approvals/waivers, and provider recordings. Output is an
`ApprovedIntegrationPlan` and a deterministic canonical/stub materialization.

## Validation

For each cluster, construct the expected target set:

```text
T = {(block, BlockId, ContentHash)}
  union {(metadata, DocumentId, key, value-index, ContentHash)}
```

The proposal's disposition keys must equal `T` exactly. Duplicate, missing,
unknown, wrong-cluster, or stale-hash targets fail. `omission_proposed` requires
a bounded rationale and exact approval coverage; other dispositions must not
appear in the omission approval set.

Every non-empty output section requires at least one unique evidence reference
owned by a document in the cluster. Evidence IDs and hashes are revalidated
against the source inventory. A verbatim section additionally binds exact
source bytes. Related links must target an approved cluster or an existing
opaque asset/Canvas/Base identity.

Each declared contradiction set contains at least two distinct evidence-bound
claims and retains their source, optional observation time, and context. The
critic receives the complete source inventory and rendered proposal. It emits
stable finding IDs with severity `minor`, `major`, or `critical`. A major or
critical finding blocks; every minor finding requires a matching current
waiver with curator and rationale.

## Approval identity

```text
ClusterRevisionHash = H("okc:cluster-revision:v3\0" || canonical_json({
  taxonomy_hash, proposal, critic, omission_approvals, minor_waivers
}))

ApprovedIntegrationPlanId = H("okc:integration-plan:v3\0" || canonical_json({
  corpus_hash, policy_hash, taxonomy_approval,
  ordered_cluster_revision_hashes, ordered_recording_hashes
}))
```

Approval binds its target hash, curator, policy version, and kind. Any bound
change makes it stale.

## Materialization

1. Allocate each canonical Markdown path from the approved taxonomy.
2. Render canonical sections in proposal order with stable newline/frontmatter
   rules and seal their hashes.
3. Emit one source redirect stub under
   `legacy/<source-id>/<original-path>.md`, containing only canonical
   frontmatter, a relative link to the canonical note, and provenance identity.
4. Rewrite Markdown and Canvas references through the sealed document-to-
   canonical map. Deduplicate attachments by content hash. Copy Base opaquely
   with a warning.
5. Construct typed provenance from each output section/stub through proposal,
   critic, approval, dispositions, and immutable source evidence.

## Complexity

Validation is `O(T log T + E log E + F log F)` for targets, evidence, and
findings using deterministic maps. Rendering is linear in approved output
bytes plus link rewrites.

## Edge and security cases

Reject forged evidence, duplicate dispositions, raw provider paths, Markdown
traversal, prompt-injection instructions, hidden tool requests, stale taxonomy,
stale critic, unwaived minor findings, waived major/critical findings, and any
approval not bound to the current exact hash.

## Golden vectors

- A singleton cluster still needs synthesis, critic, and approval.
- One block with two dispositions fails.
- One missing frontmatter sequence value fails.
- A contradiction with two contexts remains present in rendered output.
- An approved omission whose content hash changes becomes stale.
- Replaying one complete plan produces byte-identical directory and pack data
  without a provider.

## Correctness and rollback

Successful builds require 100% target disposition coverage and 100% output
section evidence coverage. Failures append task/finding state and publish
nothing. An operator may select an older complete approved run; the journal is
never rewritten.
