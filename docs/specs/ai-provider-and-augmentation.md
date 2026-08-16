---
title: AI Provider and Augmentation Contract
status: normative-v1
owners:
  - algorithms-ai-engineer
  - qa-security-engineer
last_updated: 2026-08-16
decision_refs:
  - ADR-0004
source_refs:
  - HIST-COMPILER-PLAN
---

# AI Provider and Augmentation Contract

## Principle

AI is optional, replaceable, and outside the trusted compiler core. OpenAI, Anthropic, Google, local vLLM, command-line models, and future providers connect through the same capability interfaces. No provider is required for correct V1 compilation.

## Capability interfaces

The conceptual interfaces are:

```rust,ignore
trait TextGenerator {
    fn capabilities(&self) -> ProviderCapabilities;
    fn generate(&self, request: GenerateRequest, cancel: CancellationToken)
        -> Result<GenerateResponse, ProviderError>;
}

trait EmbeddingProvider {
    fn embed(&self, request: EmbedRequest, cancel: CancellationToken)
        -> Result<EmbedResponse, ProviderError>;
}

trait RerankProvider {
    fn rerank(&self, request: RerankRequest, cancel: CancellationToken)
        -> Result<RerankResponse, ProviderError>;
}

trait KnowledgeAugmentor {
    fn propose(&self, request: AugmentationRequest, cancel: CancellationToken)
        -> Result<Vec<KnowledgeProposal>, ProviderError>;
}
```

Capabilities include protocol versions, operation types, context/input limits, output limits, structured-output support, streaming, determinism controls, model identity, local/remote data handling declaration, and supported content classes. The caller negotiates requirements before sending source projections.

## Universal subprocess protocol

Non-Rust providers use versioned NDJSON over standard input/output. Each line is an envelope with protocol version, request ID, message type, payload, and optional trace metadata. Standard output is protocol-only; provider logs go to standard error. Messages are length-, count-, and time-bounded.

The CLI supplies a `CommandProvider` that launches an explicitly configured executable without a shell, with a sanitized environment, working directory, deadline, cancellation, output cap, and platform-appropriate process-tree termination. Provider packages and vendor examples live outside compiler core.

## Proposal model

A proposal MUST contain:

- unique proposal ID and schema version;
- originating plan ID and input projection hash;
- operation kind and complete structured payload;
- every referenced `DocumentId`, `BlockId`, and expected content hash;
- one or more `EvidenceRef` values for generated knowledge;
- provider/model identity and request/response transcript references;
- optional uncertainty and rationale fields clearly marked non-authoritative.

Allowed future proposal kinds include generated MOC, summary, tag/alias suggestion, candidate relationship, conflict explanation, and claim representation. V1 policy decides which kinds are enabled.

## Validation and approval

Validation is deterministic and includes schema, size, identity, content-hash, source-span, evidence closure, output path, extension, link target, frontmatter, forbidden instruction, and policy checks. Provider text is DATA. It cannot invoke tools, initiate HTTP, execute commands, delete files, or grant itself approval.

Approval records bind `plan_id + proposal_content_hash + decision + approver + policy_version`. Any bound value change makes the approval stale. Bulk approval is allowed only through an explicit user action naming the selection rule; it is still recorded per proposal or as a verifiable set commitment.

## Record and replay

`ai-transcript.jsonl` records canonical request envelopes, response envelopes, provider/model declarations, timing metadata excluded from semantic hashes, and validation outcomes. Sensitive values are referenced or redacted under policy. Given identical snapshots, configuration, compiler version, approved transcript, and decisions, compilation MUST not contact a provider and MUST produce the same output.

## Privacy and failure

The caller MUST know whether a provider is local or remote before content is sent. Default policy sends the minimum necessary blocks, not entire Vaults. Provider failure, timeout, malformed output, unsupported capability, or refusal leaves the deterministic plan usable without AI. It never creates a partial approved change.

## Built-in adapters

The core ships no OpenAI-only or other vendor-only dependency. Reference adapters may be separate packages and examples. The universal command adapter is the compatibility floor.
