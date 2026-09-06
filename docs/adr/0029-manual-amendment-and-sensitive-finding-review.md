---
title: ADR-0029 — Manual Amendment and Sensitive-Finding Review
status: proposed
owners:
  - architect
  - algorithms-ai-engineer
  - qa-security-engineer
last_updated: 2026-09-06
decision_refs:
  - ADR-0023
  - ADR-0024
  - ADR-0025
  - ADR-0027
  - ADR-0029
source_refs:
  - HIST-COMPILER-PLAN
---

# ADR-0029: Manual Amendment and Sensitive-Finding Review

## Status

Proposed on 2026-09-06; not accepted or implemented. This draft does not grant
an exception, approve a proposal, authorize a provider call, or alter current
Schema 3 recordings. Review entry point: [`ADR proposals`](README.md).

## Context and affected contracts

Feedback-driven regeneration already creates an append-only, hash-bound
revision. Direct human amendment and durable scanner-exception storage do
not have a complete public/persistence contract. A human edit cannot be
represented as though the provider emitted it, and a dismissed finding must
not become reusable consent to disclose sensitive content remotely.

Affected: REQ-AI-002/003, REQ-SEC-002/003, REQ-INT-002/003/004/005,
REQ-PRV-001, REQ-SDK-002; ALG-INT-001, ALG-PRV-001; QG-001/003/004/005.
The current single synthesis-recording reference, metadata-only scanner
locations, and immutable approval/golden contracts are explicit constraints.

## Proposed decision

### 1. Compare-and-append human amendments

`okc-app` owns one typed amendment operation for CLI/TUI/interop. A caller
supplies the current run, cluster, expected taxonomy/proposal/critic hashes,
stable section IDs, replacement heading/body, curator identity, and rationale.
All paths remain explicit in language bindings; no adapter edits SQLite or
files directly. Acquire the project writer lock and check the expected hashes
again before accepting the revision; a stale editor receives a stale-state
error and never overwrites newer work.

The first version permits replacing text of existing sections only. Preserve
section IDs and evidence maps, dispositions, related links, contradictions,
omission targets and waivers as historical input, not as fresh authority.
Adding/removing sections or changing evidence/contradiction topology uses a
separately specified regeneration/review path. Validate normal text bounds,
evidence closure and terminal escaping before persistence. Reject an amendment
with an unknown section, foreign evidence, empty body or no actual change.

Append a canonical `human-amendment-1` object containing:

```text
run and cluster IDs
expected taxonomy/proposal/critic hashes
ordered section-ID / replacement-text / unchanged-evidence bindings
curator ID and bounded rationale
resulting proposal hash and monotonically increasing revision
```

Curator IDs are attribution within the local project trust model, not digital
signatures. No timestamp, absolute source path, secret, or UI state contributes
to semantic identity. Propose the distinct domain
`okc:human-amendment:v1\0`; finalize canonical field names/vectors before use.
The original provider response and synthesis recording remain byte-identical.
The amendment is a human-origin revision linked to that recording, never a
fabricated successful provider exchange.

### 2. Mandatory critic and fresh approval

After the amendment is durably appended, invalidate the affected cluster
approval, latest compiled-plan authority, and current verified-output status.
The edited proposal is not approved. Schedule a fresh critic over the complete
current source inventory and amended proposal, applying the normal disclosure
gate. Even previously waived minor findings require current hash-bound waivers;
major/critical findings remain unwaivable.

| Event | Resulting authority |
|---|---|
| Stale/invalid edit | no revision or invalidation committed |
| Valid edit saved | prior approval stale; new revision pending critic |
| Critic cancelled/failed | new revision retained but cannot compile; resume exact task |
| Critic has blocking finding | review required; no automatic rollback to old approval |
| Critic passes and curator approves | seal a new revision/plan; unaffected clusters may be reused only under identical dependencies |
| User wants to undo | append a new revision with earlier text, then critic and fresh approval |

Remote critic calls still require explicit consent for that invocation. A
missing consent or local model leaves the amendment pending; saving text is
not provider-call permission. Replays must distinguish original synthesis,
human amendment, critic and final approval in provenance.

### 3. Exact scanner review, never a remote bypass

Persist scanner review as append-only false-positive annotations. The initial
proposal deliberately keeps routing conservative: an acknowledged finding may
be hidden from the active review queue, but its document/cluster still requires
local semantic processing. Neither false-positive annotation nor curator text
authorizes remote disclosure. A future routing-relaxation policy needs a
separate security ADR and must not be inferred from this one.

An annotation binds project/run scope, scanner revision, category, document ID,
target kind/ID, exact content hash, coordinate system, UTF-8 byte range, curator,
and rationale. Block and metadata findings use a tagged target, not a fake
BlockId for metadata. For current metadata findings, the range belongs to
compact JSON `{key,value}`, not the original YAML source span. Record no
matched secret text, resolved credential value, credential-size diagnostics or
reusable consent. Typed scanner locations remain the existing review coordinates.

Proposed operations are annotate-false-positive, revoke-annotation and list
current review status. Reject unknown findings, duplicates, invalid ranges,
empty attribution/rationale and source/hash/scanner mismatch. Annotation text
is bounded hostile input, screened for credentials and excluded from provider
requests, public artifacts and logs. Revocation appends an event. Any source
or scanner-revision change makes old annotations inactive; no wildcard, path,
document-wide or category-wide exception is supported.

This intentionally does not expose the existing in-memory exception helper as
a remote-routing authority. Acceptance must document the distinction between
review status and effective disclosure findings so clients cannot conflate them.

### 4. Persistence and format boundary

Amendment and scanner-review events belong to the private project journal and
immutable objects, shared by every surface. Their addition needs a reviewed
migration, recovery and exact versioned object schema; do not silently write a
new table contract while calling it unchanged journal schema 4.

Current Schema 3 `SynthesisProposal` has one synthesis-recording hash, and
provenance has no explicit human-amendment chain. Do not insert a fake provider
receipt, conceal an edit behind an unchanged hash, or rely on a mutable project
to explain a supposedly self-contained artifact. The recommended format path
is an explicit human-origin variant plus amendment digest in the next bounded
evidence envelope described by
[ADR-0030](0030-current-pack-and-extended-materialization.md). No edited-plan
export/compile capability may ship until that representation and standalone
verification are accepted. Scanner review can be implemented independently
after its own persistence/security review because it grants no output authority.

## Alternatives and consequences

- Editing generated Markdown after publication invalidates checksums and
  bypasses critic/approval; it remains ordinary detected tampering.
- Forging a provider recording preserves a convenient field shape but destroys
  origin attribution; rejected.
- Automatic approval after a clean critic still bypasses curator authority;
  rejected.
- Remote-enabling exceptions reduce false-positive friction but weaken the
  current privacy rule; explicitly deferred. Local-only annotations have less
  convenience benefit but a smaller authority surface.
- Full amendment provenance requires explicit format work, not just a new UI
  button. It increases stored revision and review workload.

## Acceptance, rollout and rollback

Architect/QA/algorithm owners must review the allowed edit set, human-origin
encoding, conservative annotation semantics, migration and interop DTOs.
Use additive typed methods where possible; an incompatible public DTO requires
an explicit interop-version decision. Old APIs, old projects and approved plans
must not silently change interpretation.

Planned regressions: two stale concurrent editors; blocked/pending critic;
restart between amendment and invalidation; repeated undo; unchanged evidence
maps; old omission/minor-waiver refusal; tampered amendment history; offline
human/provider provenance; metadata-only finding; Unicode range; changed
source/scanner; revocation; no secret/rationale disclosure; remote route remains
blocked despite annotations; equivalent CLI/Python/Node outcomes.

On implementation update the provider, IR, provenance, pipeline, public API
and testing specs, ALG-INT-001/ALG-PRV-001 and TRACEABILITY. Rollback disables
new writes, retains all history, and never revives an old approval or deletes
an already published artifact. Format-incompatible data must fail explicitly,
not be projected into an older shape.
