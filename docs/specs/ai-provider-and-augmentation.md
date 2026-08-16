---
title: AI Provider and Augmentation Contract
status: normative-v1
owners:
  - algorithms-ai-engineer
  - qa-security-engineer
last_updated: 2026-08-16
decision_refs:
  - ADR-0004
  - ADR-0009
source_refs:
  - HIST-COMPILER-PLAN
---

# AI Provider and Augmentation Contract

## Principle

AI is optional, replaceable, and outside the trusted compiler core. OpenAI,
Anthropic, Google, local vLLM, command-line models, and future providers connect
through the same provider-neutral boundaries. No provider is required for
correct V1 compilation, and no provider can mutate a Vault or approve its own
proposal.

## Rust capability interfaces

The implemented `vaultc::provider` traits are:

```rust,ignore
trait TextGenerator {
    fn capabilities(&self) -> ProviderCapabilities;
    fn generate(
        &self,
        request: &TextGenerationRequest,
        cancellation: &CancellationToken,
    ) -> vaultc::Result<TextGenerationResponse>;
}

trait EmbeddingProvider {
    fn capabilities(&self) -> ProviderCapabilities;
    fn embed(
        &self,
        request: &EmbeddingRequest,
        cancellation: &CancellationToken,
    ) -> vaultc::Result<EmbeddingResponse>;
}

trait RerankProvider {
    fn capabilities(&self) -> ProviderCapabilities;
    fn rerank(
        &self,
        request: &RerankRequest,
        cancellation: &CancellationToken,
    ) -> vaultc::Result<RerankResponse>;
}

trait KnowledgeAugmentor {
    fn capabilities(&self) -> ProviderCapabilities;
    fn propose(
        &self,
        request: &AugmentationRequest,
        cancellation: &CancellationToken,
    ) -> vaultc::Result<Vec<KnowledgeProposal>>;
}
```

`KnowledgeAugmentor` is re-exported from the crate root; the other traits and
request/response types are available under `vaultc::provider`. Wire types live
in the independent `vaultc-protocol` crate.

V1 `ProviderCapabilities` contains provider identity, supported protocol
versions and operations, maximum input/output bytes, structured-output,
streaming and deterministic-control declarations, and a `local` or labeled
`remote` data boundary. Context-token limits and supported content-class lists
are not part of schema version 1.

## Universal subprocess protocol

Non-Rust providers use protocol version 1 NDJSON over standard input/output.
Each envelope has exactly `protocol_version`, `request_id`, `message_type`, and
`payload`. Standard output is protocol-only; provider logs go to standard
error. The CLI exchange is exactly:

1. `capabilities-1` / `capabilities_request`;
2. `capabilities-1` / `capabilities_response`;
3. `augmentation-1` / `augmentation_request`;
4. `augmentation-1` / `augmentation_response`.

The provider must negotiate protocol V1, `knowledge_augmentation`, and
structured output. The CLI verifies declared input/output limits and proposal
provider identity. Extra protocol messages, a malformed line, refusal, crash,
non-success exit, or failure to close after the response is an error.

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
projections, and output limits. The CLI requires explicit `--document-id`
selection or `--all-documents`; it never defaults to sending the whole plan.

The current projection granularity sends every parsed block of each selected
document. Block-level disclosure selection is future work, so policy and UI
must not claim a finer minimum-disclosure guarantee. Remote providers are
default-denied and require both `allow_remote_providers = true` in the sealed
policy and explicit CLI `--allow-remote-provider` consent.

## Proposal model

A V1 proposal contains:

- schema version, unique proposal ID, originating plan ID, and projection hash;
- provider/model/version identity;
- one structured proposal kind and payload;
- zero or more snapshot/document/block/span/content-hash evidence references;
- optional uncertainty in `[0, 1]` and optional rationale.

The two V1 kinds are:

- `create_generated_note`: title, Markdown body, and optional safe `.md` path;
- `explain_conflict`: existing conflict ID and explanation text.

Generated notes require at least one evidence reference. A conflict explanation
is advisory data; it is not a conflict decision and cannot change the sealed
plan. Proposal-local transcript reference fields and bulk approval commitments
are not part of schema version 1.

## Validation and approval

Deterministic proposal validation covers:

- proposal count/schema, unique bounded ID, plan ID, and projection hash;
- provider identity and uncertainty range;
- generated title/body bounds, `.md` extension, and safe generated path;
- referenced conflict existence;
- evidence identity parsing, snapshot/document ownership, block identity and
  content hash, exact block span when supplied, document hash, and span bounds;
- non-empty evidence for generated notes.

Provider text is DATA. It cannot invoke tools, initiate HTTP, execute commands,
delete files, change policy, or grant approval. Link-target semantics,
frontmatter-policy analysis, and instruction-content classification are not
implemented proposal validators in V1 and must not be advertised as such.

An `ApprovalDecision` binds `plan_id`, `proposal_id`, canonical
`proposal_content_hash`, approval boolean, approver, and policy version. Any
bound value change makes it stale. Only approved valid proposals are retained
in `ApprovedPlan`; rejected decisions are not materialized.

Conflict authorization is a separate ADR-0009 overlay bound to plan and
conflict content hashes. The only external V1 resolution is
`waived_by_policy`. A provider explanation never becomes a waiver or typed
resolution.

## Transcript and offline compilation

The CLI augmentation JSONL contains one header, four canonical transcript
records, and one validation record per proposal. A transcript record stores
sequence, request ID, direction, message type, canonical payload hash, and
payload. There is no timing-metadata field in V1.

For the stored augmentation request, block text is replaced by
`"[redacted]"`, while `canonical_payload_hash` commits to the original sent
projection. Approval rehydrates the request from the sealed plan and validates
the commitment, request/response sequence, provider identity, and proposals.
The Compiled Vault stores the approved transcript audit file, and the
independent verifier requires exact semantic equality with `ApprovedPlan`.

Compilation of an `ApprovedPlan` never contacts a provider. Given identical
snapshots, policy, compiler version, approved proposal contents, conflict
decisions, and transcript, the deterministic materialization path is reusable
offline. A dedicated record-to-replay byte-equality E2E remains a required
quality-gate test.

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
