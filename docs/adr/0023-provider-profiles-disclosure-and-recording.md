---
title: ADR-0023 — Provider Profiles, Disclosure, and Recording
status: normative
owners:
  - architect
  - algorithms-ai-engineer
  - qa-security-engineer
last_updated: 2026-09-06
decision_refs:
  - ADR-0004
  - ADR-0011
  - ADR-0023
  - ADR-0027
source_refs:
  - HIST-COMPILER-PLAN
---

# ADR-0023: Provider Profiles, Disclosure, and Recording

## Status

Accepted on 2026-09-03. Amended by ADR-0027 on 2026-09-06: the current public
capability name is generation-neutral, and no command-provider kind or adapter
is exposed on main. The original decision text below is preserved as the
historical decision.

## Context

V3 requires structured generation and embeddings, but compilation policy must
not depend on a vendor HTTP vocabulary. Long-lived credentials must not enter a
project, artifact, transcript, diagnostic, or command line. Remote disclosure
also requires a preflight that cannot be bypassed by role routing.

## Decision

The separate `okc-ai` crate owns provider adapters. `okc-core` does not depend
on it. The stable capability boundary is semantic:

```rust,ignore
trait StructuredGenerator {
    fn capabilities(&self) -> ProviderCapabilitiesV3;
    fn generate_structured(
        &self,
        request: &StructuredGenerationRequest,
        cancellation: &CancellationToken,
    ) -> Result<StructuredGenerationResponse, ProviderError>;
}

trait Embedder {
    fn capabilities(&self) -> ProviderCapabilitiesV3;
    fn embed(
        &self,
        request: &EmbeddingBatchRequest,
        cancellation: &CancellationToken,
    ) -> Result<EmbeddingBatchResponse, ProviderError>;
}
```

Every response binds the actual provider/model identity, canonical request and
response hashes, finish state, and a usage receipt. Structured generation is
requested with a portable JSON Schema subset and is always decoded and
semantically validated locally. Web search, MCP, function/tool execution, and
automatic truncation are forbidden.

The initial adapters are OpenAI Responses/Embeddings, Anthropic Messages
structured outputs, Gemini Interactions/embedContent, Ollama structured
generation/native embed, generic OpenAI-compatible HTTP, and a supervised
command adapter. Anthropic declares no native embedding capability. Model IDs
are explicit profile values and are never silently replaced with a fashionable
default.

The HTTP worker is synchronous. Remote endpoints require HTTPS, platform
certificate verification, no redirects, bounded request/response sizes, a
global timeout, and cancellation checks before and after network access. Only
loopback hosts are local; LAN endpoints remain remote. Authentication,
authorization, rate limiting, timeout, context exhaustion, refusal, malformed
response, and transport failures normalize to typed errors. Only 408, 409,
429, 5xx, or a transport failure before a response are retryable, at most twice,
and `Retry-After` is honored within the configured deadline.

Global TOML stores profile type, endpoint, exact model, options, and only the
name of an API-key environment variable. Projects store role-to-profile names
for `default`, `embedding`, `organizer`, `synthesis`, and `critic`. Secret
values MUST NOT be serialized or included in `Debug`, display, logs, errors,
recordings, or hashes.

Before the first disclosure, the application shows the selected provider,
local/remote boundary, estimated request count, and bounded input byte/token
range. A non-interactive remote run requires both
`--allow-remote-provider` and `--yes`; consent is one-run authority and is not
stored as a reusable permission.

Every completely received and locally validated exchange is atomically stored
as a content-addressed recording. Its task key binds stage, prompt, schema,
source hash, provider/model/adapter identity, and canonical options. Failed or
partial exchanges do not produce a reusable task result.

## Consequences

- Vendor changes are isolated behind conformance-tested adapters.
- Offline replay and compile do not require a key or installed live provider.
- Role-specific provider routing cannot bypass local-only sensitive content.
- Provider pricing is not embedded; recorded usage is factual and estimates
  describe workload rather than currency.
