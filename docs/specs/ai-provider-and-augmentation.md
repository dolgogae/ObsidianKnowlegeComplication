---
title: AI Provider and Augmentation Contract
status: normative-v1
owners:
  - algorithms-ai-engineer
  - qa-security-engineer
last_updated: 2026-09-02
decision_refs:
  - ADR-0004
  - ADR-0009
  - ADR-0011
  - ADR-0017
source_refs:
  - HIST-COMPILER-PLAN
---

# AI Provider and Augmentation Contract

## Principle

AI is optional, replaceable, and outside the trusted compiler core. OpenAI,
Anthropic, Google, local vLLM, command-line models, and future providers connect
through the same provider-neutral boundaries. No provider is required for
correct V2 compilation, and no provider can mutate a Vault or approve its own
proposal.

## Rust capability interfaces

The implemented `okc_core::provider` traits are:

```rust,ignore
trait TextGenerator {
    fn capabilities(&self) -> ProviderCapabilities;
    fn generate(
        &self,
        request: &TextGenerationRequest,
        cancellation: &CancellationToken,
    ) -> okc_core::Result<TextGenerationResponse>;
}

trait EmbeddingProvider {
    fn capabilities(&self) -> ProviderCapabilities;
    fn embed(
        &self,
        request: &EmbeddingRequest,
        cancellation: &CancellationToken,
    ) -> okc_core::Result<EmbeddingResponse>;
}

trait RerankProvider {
    fn capabilities(&self) -> ProviderCapabilities;
    fn rerank(
        &self,
        request: &RerankRequest,
        cancellation: &CancellationToken,
    ) -> okc_core::Result<RerankResponse>;
}

trait KnowledgeAugmentor {
    fn capabilities(&self) -> ProviderCapabilities;
    fn propose(
        &self,
        request: &AugmentationRequest,
        cancellation: &CancellationToken,
    ) -> okc_core::Result<Vec<KnowledgeProposal>>;
}
```

`KnowledgeAugmentor` is re-exported from the crate root; the other traits and
request/response types are available under `okc_core::provider`. Wire types live
in the independent `okc-protocol` crate.

ADR-0011 adds a provider-neutral façade rather than a vendor client:

```rust,ignore
let request = compiler.build_augmentation_request(&plan, &selection)?;
let recording = compiler.augment(
    &plan,
    &selection,
    &augmentor,
    &cancellation,
    RemoteProviderConsent::Denied,
)?;
let replayed = compiler.replay_augmentation(&plan, &recording)?;
assert_eq!(
    recording.to_canonical_jsonl()?,
    replayed.to_canonical_jsonl()?,
);
```

`DocumentSelection` is either a non-empty unique explicit `DocumentId` list or
`All`. `RecordedAugmentation` owns the versioned header, exact transcript, and
deterministic validation records. A lower-level transport first calls
`authorize_augmentation_exchange` after capability negotiation but before
source projection disclosure. The returned opaque, non-serializable
authorization is then consumed by `record_augmentation_exchange` with the
response. This gives non-Rust and non-trait transports the same consent,
validation, and recording boundary without taking ownership of policy.

V2 `ProviderCapabilities` contains provider identity, supported protocol
versions and operations, maximum input/output bytes, structured-output,
streaming and deterministic-control declarations, and a `local` or labeled
`remote` data boundary. Context-token limits and supported content-class lists
are not part of schema version 2.

## Universal subprocess protocol

Non-Rust providers use protocol version 2 NDJSON over standard input/output.
Each envelope has exactly `protocol_version`, `request_id`, `message_type`, and
`payload`. Standard output is protocol-only; provider logs go to standard
error. The CLI exchange is exactly:

1. `capabilities-1` / `capabilities_request`;
2. `capabilities-1` / `capabilities_response`;
3. `augmentation-1` / `augmentation_request`;
4. `augmentation-1` / `augmentation_response`.

The provider must negotiate protocol V2, `knowledge_augmentation`, and
structured output. The CLI verifies declared input/output limits and proposal
provider identity. It invokes the core pre-disclosure authorization after the
capability response and before sending `augmentation_request`. Extra protocol
messages, a malformed line, duplicate or unknown fields at any JSON depth,
refusal, crash, non-success exit, or failure to close after the response is an
error.

`CommandProvider` launches an explicitly configured executable directly, never
through a shell. It uses a sanitized environment, optional working directory,
bounded stderr capture, and limits for deadline, line length, total output, and
message count. CLI defaults are 120 seconds, 16 MiB per line, 16 MiB total, and
8 messages. Hard maxima are 3,600 seconds, 32 MiB per line, 64 MiB total, and
64 messages. The line limit cannot exceed the total limit.

Provider input is written on a supervised thread so a child that stops reading
cannot bypass deadline or cancellation. The CLI polls at bounded intervals and
on deadline or SIGINT kills/reaps the provider process tree on supported Unix
platforms. It does not publish a partial augmentation file.

## Projection and privacy boundary

An augmentation request binds the sealed `plan_id`, the complete canonical
workspace `projection_hash`, allowed proposal kinds, selected document
projections, and output limits. Every live SDK or CLI call requires an explicit
document selection; it never defaults to sending the whole plan.

Each `DocumentProjection` carries the owning sealed `snapshot_id`, a document
object reference and content hash, logical path, title, and selected blocks.
Each projected block carries its block ID, content hash, and text. This is
sufficient for a stateless provider to construct either spanless file/body
evidence or block evidence without access to a private plan file.

The current projection granularity sends every parsed block of each selected
document. Block-level disclosure selection is future work, so policy and UI
must not claim a finer minimum-disclosure guarantee. Remote providers are
default-denied and require both `allow_remote_providers = true` in the sealed
policy and explicit runtime `RemoteProviderConsent::Granted`. CLI
`--allow-remote-provider` is one UI for that runtime consent. Consent authorizes
only that live disclosure and is not serialized as a reusable permission.

The in-process façade checks cancellation before capability negotiation,
before projection disclosure, after the provider returns, and before returning
a recording. `KnowledgeAugmentor::capabilities()` is local, bounded,
side-effect-free metadata and MUST NOT perform network or blocking negotiation.
`propose` implementations must cooperate with `CancellationToken`; the core
cannot forcibly stop arbitrary provider code. Transports with live capability
negotiation use their own deadline/cancellation and MUST obtain the opaque core
authorization before sending source text. The subprocess adapter retains its
stronger deadline and process-tree termination behavior.

## Proposal model

A V2 proposal contains:

- schema version, unique proposal ID, originating plan ID, and projection hash;
- provider/model/version identity;
- one structured proposal kind and payload;
- zero or more snapshot/document/block/span/content-hash evidence references;
- optional uncertainty in `[0, 1]` and optional rationale.

The two V2 kinds are:

- `create_generated_note`: title, Markdown body, and optional safe `.md` path;
- `explain_conflict`: existing conflict ID and explanation text.

Generated notes require at least one evidence reference. A conflict explanation
is advisory data; it is not a conflict decision and cannot change the sealed
plan. Proposal-local transcript reference fields and bulk approval commitments
are not part of schema version 2.

## Validation and approval

Deterministic proposal validation covers:

- proposal count/schema, unique bounded ID, plan ID, and projection hash;
- provider identity and uncertainty range;
- generated title/body bounds, `.md` extension, and safe generated path;
- referenced conflict existence;
- evidence identity parsing and snapshot/document ownership;
- file/body evidence only when `block_id`, `byte_start`, and `byte_end` are all
  absent and the hash matches the source file or normalized body;
- block evidence only when the block ID/content hash match and the span is
  either fully absent or exactly equals that block's sealed byte span;
- non-empty evidence for generated notes.

Provider text is DATA. It cannot invoke tools, initiate HTTP, execute commands,
delete files, change policy, or grant approval. Link-target semantics,
frontmatter-policy analysis, and instruction-content classification are not
implemented proposal validators in V2 and must not be advertised as such.

An `ApprovalDecision` binds `plan_id`, `proposal_id`, canonical
`proposal_content_hash`, approval boolean, approver, and policy version. Any
bound value change makes it stale. Only approved valid proposals are retained
in `ApprovedPlan`; rejected decisions are not materialized.

Conflict authorization is a separate ADR-0017 overlay bound to plan and
conflict content hashes. The curator may preserve the original or select one
sealed Markdown/Canvas target. A provider cannot choose, waive, or approve a
conflict action; an explanation remains advisory data.

When an approved proposal creates a note, approval derives a required
materialization commitment from the sealed plan and exact proposal content.
It binds destination, canonical emitted-body hash, complete rendered-output
hash, proposal-order EvidenceId values, and operation ID. Providers cannot
supply or override this compiler-owned commitment.

## Transcript and offline compilation

The canonical augmentation JSONL contains one header, exactly four canonical
transcript records, and one proposal-ID-sorted validation record per proposal.
A transcript record stores
sequence, request ID, direction, message type, canonical payload hash, and
payload. There is no timing-metadata field in V2.

For the stored augmentation request, block text is replaced by
`"[redacted]"`, while `canonical_payload_hash` commits to the original sent
projection. Approval and replay rehydrate the request from the sealed plan,
rebuild the public projection for exactly the recorded documents, and validate
the commitment, all-block projection, document-to-snapshot identity,
request/response sequence, negotiated limits, provider identity, and proposals.
Empty transcript is valid only when validations are also empty. A canonical
four-record exchange with zero proposals is valid; any non-empty validation
set without the exchange fails closed.

Every JSONL line is compact recursively key-sorted JSON followed by LF, and the
file ends in LF. Blank/CRLF/unterminated/non-canonical lines, unknown or
duplicate fields, record reordering, and over-limit input are rejected. V2
hard limits are 64 MiB per line, 1 GiB per file, and the sealed maximum proposal
count plus five fixed records. The decoder validates typed nested payloads and
header/provider/plan/projection self-consistency before exposing a recording;
replay adds sealed-plan hydration and fresh validation. A live recorder MUST
also prove that the canonical encoding fits those bounds before returning.
The Compiled Vault stores the approved transcript audit file, and the
independent verifier requires exact semantic equality with `ApprovedPlan`.

Compilation of an `ApprovedPlan` never contacts a provider. Offline
`replay_augmentation` likewise never contacts a provider, network, MCP server,
or output destination; no live remote consent is required, although the sealed
plan must have permitted the recorded remote capability. Given identical
snapshots, policy, compiler version, approved proposal contents, conflict
decisions, and transcript, the deterministic materialization path is reusable
offline. Canonical record-to-replay JSONL bytes, approval bytes, Compiled Vault,
and OKCPack outputs MUST be equal in the required replay E2E.

## Failure behavior

Provider failure, timeout, cancellation, malformed output, unsupported
capability, policy refusal, invalid proposal, or stale approval leaves the
sealed deterministic plan usable without AI. It never creates a partial
approved change or output artifact.

## Adapter boundary

The compiler core ships no OpenAI-only or other vendor-only dependency.
Reference vendor adapters may be separate packages and examples. The universal
command adapter is the cross-language compatibility floor; MCP servers remain
thin adapters over the same validation and approval boundaries.
