---
title: ADR-0011 — SDK Augmentation Recording and Offline Replay
status: normative-v1
owners:
  - architect
  - core-rust-engineer
  - algorithms-ai-engineer
  - qa-security-engineer
last_updated: 2026-08-17
decision_refs:
  - ADR-0004
  - ADR-0009
  - ADR-0011
source_refs:
  - HIST-COMPILER-PLAN
---

# ADR-0011: SDK Augmentation Recording and Offline Replay

## Status

Accepted on 2026-08-17.

## Context

ADR-0004 requires provider-neutral, evidence-bound proposals and replayable
transcripts, but the initial `0.1.0` working tree exposes only proposal traits
and validation in the Rust SDK. Plan projection, transcript construction,
redaction, loading, and most replay checks live privately in `vaultc-cli`, with
overlapping checks in approval. A Rust application can call
`validate_proposals`, but that method intentionally has no provider exchange
from which to construct a transcript. Approval currently accepts non-empty
validations with an empty transcript, so such proposals can enter an artifact
without the replay record required by REQ-AI-003.

The framework must support OpenAI, Anthropic, Google, local models, subprocess
providers, and future adapters without moving canonical state or approval
authority into any provider. Live disclosure also needs a runtime consent
boundary distinct from a policy flag. Offline replay must never silently become
another provider call.

## Decision

### One core owner

The `vaultc::augmentation` module owns projection construction, live
in-process orchestration, exchange recording, canonical augmentation JSONL,
redacted-request hydration, deterministic revalidation, and offline replay.
CLI and future MCP/plugin adapters call this module rather than maintaining
their own semantic implementations.

The public V1 surface contains:

```rust,ignore
enum DocumentSelection {
    Explicit(Vec<DocumentId>),
    All,
}

enum RemoteProviderConsent {
    Denied,
    Granted,
}

struct RecordedAugmentation { /* versioned header, transcript, validations */ }
```

`VaultCompiler` exposes:

```rust,ignore
build_augmentation_request(plan, selection) -> AugmentationRequest
augment(plan, selection, augmentor, cancellation, consent)
    -> RecordedAugmentation
record_augmentation_exchange(plan, request, capabilities, response, consent)
    -> RecordedAugmentation
replay_augmentation(plan, recording) -> RecordedAugmentation
```

`RecordedAugmentation` exposes a canonical JSONL decoder/encoder and conversion
to `ValidatedProposals`. Conversion does not bypass approval: approval and
compilation revalidate the plan, validations, transcript, and decisions.

### Projection contract

The request builder first validates the sealed plan and exact compiler-policy
match. `Explicit` must contain at least one unique known `DocumentId`; `All`
must not rely on caller order. Documents are emitted in sealed `DocumentId`
map order and blocks in sealed document order. Every selected document includes
its owning `SnapshotId`, document/body identity, logical path, title, and all
parsed blocks with exact block identity, content hash, and comparison text.

V1 request proposal kinds and limits are exactly those sealed in the plan.
Callers cannot use the live/replay façade to substitute a smaller or larger
limit, omit blocks, add a document, or disclose an absolute source locator.
Selective block disclosure requires a future versioned decision.

### Provider authorization and cancellation

Calling a remote provider requires both:

1. `plan.policy.augmentation.allow_remote_providers = true`; and
2. `RemoteProviderConsent::Granted` for that live call.

Consent is a runtime disclosure authorization. It is not written as a durable
permission and cannot be replayed as authority. Local providers do not require
`Granted`. Offline replay has no consent argument because it performs no
provider, network, MCP, or filesystem-output call, but it still checks that a
recorded remote provider was permitted by the sealed policy at recording time.

`CancellationToken` is part of the public SDK. The façade checks it before
capability negotiation, before disclosing a projection, immediately after the
provider returns, and before returning a recording. In-process cancellation is
cooperative because the trait implementation owns its execution. The CLI
subprocess adapter retains its deadline, bounded writer/reader, process-tree
termination, and reap behavior. Cancellation yields no partial recording.

### Exact four-record transcript

A live exchange has exactly these canonical transcript records:

1. sequence `0`, request ID `capabilities-1`, request,
   `capabilities_request`, payload `{}`;
2. sequence `1`, request ID `capabilities-1`, response,
   `capabilities_response`, payload `ProviderCapabilities`;
3. sequence `2`, request ID `augmentation-1`, request,
   `augmentation_request`, payload `AugmentationRequest`;
4. sequence `3`, request ID `augmentation-1`, response,
   `augmentation_response`, payload `AugmentationResponse`.

`canonical_payload_hash` is the existing domain-separated hash of the complete
unredacted typed payload. Only `DocumentProjection.selected_blocks[*].text` in
the stored augmentation request is replaced by the exact literal
`[redacted]`. IDs, hashes, paths, titles, block count/order, and all other
fields remain. Replay hydrates those texts from the sealed plan, rebuilds the
request through the public builder, and requires exact semantic equality before
checking the payload hash.

Capabilities must negotiate protocol V1, structured knowledge augmentation,
and declared byte limits. Header provider identity, capability provider
identity, and every proposal provider identity must match. The augmentation
response preserves observed proposal order in the transcript; deterministic
validation records are sorted by proposal ID and must equal fresh validation.

The approval matrix is fixed:

| Validations | Transcript | Result |
|---|---|---|
| empty | empty | permitted deterministic no-AI path |
| empty | canonical four-record zero-proposal exchange | permitted recorded no-proposal run |
| non-empty | empty | rejected |
| any | non-empty but not the exact four-record exchange | rejected |

No missing transcript is synthesized from proposals, and a provider cannot
approve its own result.

### Recording format and replay

Augmentation schema version `1` remains line-compatible with the existing CLI
format: one header, four transcript records, then one validation record per
proposal. The header binds schema, plan ID, projection hash, and provider.
Validation records are strictly proposal-ID-sorted.

Every line is compact recursively key-sorted canonical JSON followed by LF;
the file has a final LF. Blank lines, CRLF, an unterminated last record,
unknown or duplicate fields, non-canonical whitespace/key order, missing or
interleaved records, duplicate proposal IDs, and over-limit input fail closed.
V1 retains the existing CLI hard ceilings of 64 MiB per JSONL line and 1 GiB
per augmentation control file; replay additionally limits records to the sealed
proposal maximum plus the five fixed header/transcript records.

`replay_augmentation` is a pure validation/reconstruction operation. It does
not call `KnowledgeAugmentor`, spawn a process, resolve an MCP tool, or use the
network. It rehydrates the request, validates negotiation and transcript
hashes, reconstructs the response, reruns deterministic proposal validation,
and returns the canonical recording. For canonical input, re-encoding is
byte-identical.

The CLI adds:

```text
vaultc replay PLAN --augmentation FILE --out FILE
```

There is deliberately no remote-consent option. Invalid, stale, or
non-canonical recording data uses provider exit family `5`; output collision or
publication I/O uses `6`; internal canonicalization invariants use `70`. The
output is sibling-staged, fsynced, and no-clobber-published. A failed replay
does not create the requested output.

### Compatibility

This is completion of unpublished `0.1.0` schema/API version 1. Protocol and
proposal schema versions remain `1`, and existing canonical CLI recordings
retain their wire shape. Previously tolerated non-canonical JSONL and
AI-bearing approved plans with no transcript now fail closed without defaults
or aliases. The zero-validation/zero-transcript deterministic path remains
valid. A future published format change requires its own compatibility matrix
and migration; it must not fabricate historical provider records.

## Consequences

- Any LLM or local engine can implement one interface while SDK, CLI, and
  future adapters share the same disclosure, validation, approval, and replay
  boundary.
- AI-free builds remain unchanged and deterministic.
- Approved AI output always has a provider exchange that can be revalidated
  offline; replay demonstrates deterministic compiler behavior, not that the
  model would answer identically if called again.
- Runtime consent cannot become a durable authorization by being serialized.
- Existing applications that manually paired non-empty validations with an
  empty transcript must move to live recording or load/replay a canonical
  recording before approval.
- Provider quality, truth, privacy retention promises, selective block
  disclosure, and vendor adapters remain separate concerns.
