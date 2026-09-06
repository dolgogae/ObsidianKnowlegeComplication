---
title: AI Provider and Proposal Contract
status: normative
owners:
  - algorithms-ai-engineer
  - core-rust-engineer
last_updated: 2026-09-06
decision_refs:
  - ADR-0004
  - ADR-0022
  - ADR-0023
  - ADR-0024
  - ADR-0025
  - ADR-0026
  - ADR-0027
source_refs:
  - HIST-KNOWLEDGE-PLATFORM
  - HIST-COMPILER-PLAN
---

# AI Provider and Proposal Contract

## Ownership

`okc-ai` owns live provider I/O and portable Schema 3 request/response types.
`okc-app` owns profiles, routing, disclosure authorization, recordings,
resumption, and review orchestration. `okc-core` owns corpus, proposal/evidence
validation, approval closure, and provider-free compilation. A provider never
receives filesystem or publication authority.

There is no general augmentation protocol crate, command-provider kind, or
command execution adapter on main. The `command` profile value is unknown and
MUST be rejected during profile decoding. Adding such an adapter requires a
separately reviewed current-schema decision under ADR-0027.

## Capabilities and roles

The public generation-neutral types are `ProviderCapabilities`,
`DataBoundary`, `StructuredGenerator`, and `Embedder`. Capabilities bind exact
provider/model/adapter identity, response model, structured-generation and
embedding support, strict JSON Schema support, local/remote boundary, and
input/output/batch limits.

The current roles are:

- `embedding`: bounded semantic vector proposals;
- `organizer`: exactly-one taxonomy assignment and canonical paths;
- `synthesis`: complete sections, evidence, dispositions, links, and
  contradictions for one cluster;
- `critic`: independent comparison of synthesis against the entire cluster
  inventory.

Projects seal a default profile plus optional role overrides. A remote cache
miss requires `allow_remote_provider` and
`remote_disclosure_confirmed` for that invocation. Consent is not stored or
reused. Content with effective sensitive findings and affected semantic work
MUST use a local route.
Rejected route edits MUST leave the existing configuration unchanged in memory
and on disk; validate the candidate before replacing the active value.

The scanner also covers frontmatter keys and values, including documents with
no body blocks. Private scanner revision `okc-sensitive-v3-2` records separate
block and metadata findings under `sensitive-findings-v2`; metadata byte spans
refer to UTF-8 compact JSON of `{key, value}`, not raw YAML. Findings retain
identities/ranges/hashes, never matched text. The versioned preflight cache is
recomputed without rewriting old objects or changing approved artifact bytes.

## Portable requests and recordings

Every structured request binds Schema 3, role, stable task ID, system
instruction, input, schema name, output schema, token bound, and temperature.
Every response binds exact request/response hashes, provider identity, finish
reason, output, and usage receipt. Embedding responses additionally bind vector
count and dimensions and reject non-finite values.

The application validates the portable JSON Schema subset locally before any
call and validates returned JSON independently of provider claims. It records
canonical request and response bytes plus content hashes in immutable project
objects. Resume may reuse a response only when the complete cache key matches
the source/corpus, route, provider/model, role, prompt, schema, and revision.

Cache identity includes provider kind, endpoint, options, and input/response/
batch limits; credential references and secret values are excluded. Organizer
input identity includes semantic candidates, not just the corpus. Reusing a
task key with a different stage or request object MUST fail. Latest tasks are
ordered by journal events, not lexicographic task hashes.

Provider recordings are evidence of what was proposed, not approval. Final
compile accepts a sealed `ApprovedIntegrationPlan` and performs no provider,
network, environment-secret, keychain, or process access.

## Untrusted-output validation

Provider responses MUST be rejected for any of the following:

- unknown or duplicate fields, invalid JSON, wrong schema/revision/task or
  request hash;
- an identity different from the selected provider/model response;
- missing, extra, duplicate, foreign, or stale document/block/metadata IDs;
- evidence outside the current cluster or with a mismatched content hash;
- incomplete taxonomy/disposition/source inventory;
- invalid or traversal-bearing canonical paths;
- non-finite or wrong-dimension embeddings;
- an unsupported/uncited section, malformed contradiction, or out-of-bound
  response;
- any attempt to provide curator approval or alter source/project state.

Duplicate JSON keys are rejected recursively in transport envelopes and
embedded generated JSON before conversion into maps can erase ambiguity.

Organizer, synthesis, and critic output is resealed locally. Critical/major
critic findings block. Minor findings and omissions require individually keyed
curator rationales. Regeneration binds feedback to the previous proposal and
critic hashes and creates a new revision, invalidating earlier authority.

## Provider profiles and credentials

Profiles support OpenAI, Anthropic, Gemini, Ollama, and OpenAI-compatible
adapters. They specify endpoint, model, limits, and options. The CLI may hold an
environment-variable reference or an opaque OS-keychain account. Python and
Node.js may hold only an environment-variable name. Raw secrets and
credential-like option keys are forbidden.

Secrets resolve immediately before the call, live in zeroizing/redacted types,
and MUST NOT appear in JSON/TOML, SQLite, request recordings, provenance,
errors, debug output, terminal output, or event queues. Keychain failure never
falls back to plaintext.

## Transport safety

HTTP transports use direct validated endpoints, bounded request/response
sizes, deadlines, cancellation, and a maximum of two retry attempts for
explicitly retryable failures. Credentials in URLs are forbidden. Redirect,
proxy, DNS, TLS, and error normalization policy MUST not disclose secrets or
reinterpret a failed response as valid output.

The configured timeout bounds the entire call, including retries and
`Retry-After` delays. Each attempt receives only the remaining budget; a delay
that would exhaust it fails with the timeout class without sleeping past it.

Provider errors are normalized into authentication, authorization,
rate-limit, timeout, context-limit, refusal, invalid request/response,
response-too-large, transport, cancellation, unsupported-capability, and
remote-policy classes. Language adapters map them to stable structured errors;
callers do not parse human text.

## Determinism and experimental isolation

Live generation is nondeterministic; recording makes its exact output an
immutable build input. Local validation, approval, compilation, verification,
and explanation are deterministic. Experimental retrieval or memory models
may annotate research results but cannot change default candidates, taxonomy,
approval, or output without promotion under the algorithm registry.
