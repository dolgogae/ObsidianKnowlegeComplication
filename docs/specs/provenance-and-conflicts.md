---
title: Provenance, Deduplication, and Conflicts
status: normative-v1
owners:
  - architect
  - core-rust-engineer
last_updated: 2026-08-16
decision_refs:
  - ADR-0003
  - ADR-0006
  - ADR-0009
source_refs:
  - HIST-KNOWLEDGE-PLATFORM
  - HIST-COMPILER-PLAN
---

# Provenance, Deduplication, and Conflicts

## Provenance invariant

Every output file has at least one derivation edge. A copied or rewritten note points to its source snapshot/file and relevant source spans. A deduplicated output points to every exact member. An AI-generated output points to an approved proposal and all supporting evidence. A synthesized claim without evidence is invalid.

The independent verifier MUST reconstruct the expected record for each sealed
output operation or approved generated note and compare it exactly with the
published provenance ledger. It is insufficient to check only that each named
source exists: an unrelated valid source is not provenance for an output.
Standalone audit summaries and the embedded approved plan MUST agree exactly.

The detailed record algorithm is [`../algorithms/stable/provenance-and-evidence.md`](../algorithms/stable/provenance-and-evidence.md).

## Duplicate policy

Exact duplicate identity requires both canonical Markdown body hash and normalized frontmatter hash to match. Same body with different normalized metadata is a frontmatter conflict, not an exact duplicate. Binary assets use the domain-separated byte `ContentHash` independently.

When exact notes are unified:

- choose the canonical output path through the stable tie-break algorithm;
- preserve provenance for all members;
- rewrite every inbound link to the chosen path;
- do not invent a union of metadata because values happen not to collide.

Near-duplicate analysis uses MinHash/LSH only for candidate generation. A score
at or above the default `0.85` threshold means “review similarity,” not semantic
equivalence. V1 retains both candidates. A curator may record a
`waived_by_policy` overlay, but cannot merge, delete, or select content through
an untyped decision.

## Conflict kinds

| Code family | Meaning | Default V1 behavior |
|---|---|---|
| `PATH_EXACT` | multiple non-identical files request same path | stable suffix; typed external action unavailable in V1 |
| `PATH_CASEFOLD` | names collide on case-insensitive targets | stable suffix; error if impossible |
| `UNICODE_NORMALIZATION` | distinct source names normalize alike | stable suffix and warning |
| `TITLE_AMBIGUITY` | multiple documents claim same title | retain both; links may require rewrite/decision |
| `ALIAS_AMBIGUITY` | alias resolves to multiple targets | record ambiguous link conflict |
| `FRONTMATTER_VALUE` | same semantic field has differing values | retain source-specific values; no silent union |
| `LINK_AMBIGUITY` | link has multiple candidates | required conflict; preserve raw target until a future typed action |
| `CONTENT_NEAR_DUPLICATE` | high candidate similarity | review only |

A link with zero candidates is preserved and emits an unresolved-link
diagnostic; it is not a `LINK_AMBIGUITY` conflict. The current path-normalizer
can collapse NFD/NFC spelling before conflict classification, so full
`UNICODE_NORMALIZATION` typing/original-spelling provenance remains incomplete.

Future, unimplemented conflict families include `CLAIM_CONTRADICTION` (retain
context/time-scoped claims with evidence) and `LICENSE_INCOMPATIBLE` (hard
policy error). They are not variants of the V1 `ConflictKind` enum.

## Resolution states and approval overlays

`unresolved`, `auto_resolved_by_normative_rule`, `user_resolved`,
`provider_suggested`, and `waived_by_policy` are distinct schema states. A
provider suggestion never becomes a resolution without approval.

The `DraftPlan` is immutable. Each conflict has a canonical `content_hash`
computed from its kind, required flag, optional typed subject, ordered
documents, message, and score; the resolution is deliberately excluded from
this hash. A Canvas-reference subject binds the `CanvasId`, unique node ID, and
raw file path. Each ambiguous Canvas file node receives its own subject and
conflict even when its human-readable path and candidate documents equal
another node's. External decisions live
in `ApprovedPlan.conflict_decisions` and bind the sealed `plan_id`,
`conflict_id`, `conflict_content_hash`, resolver, and policy version. Unknown,
duplicate, wrong-plan, stale-hash, or already-resolved decisions fail closed.
All required unresolved conflicts need decision coverage before compilation.

V1 external decisions support only `waived_by_policy`. A waiver means policy
explicitly accepts preserving the unresolved source representation; it does
not claim the ambiguity was fixed. `user_resolved` and `provider_suggested`
remain reserved until the decision schema carries a typed target/rewrite action
that the planner, compiler, provenance writer, and verifier can validate. See
ADR-0009.

## Future claim semantics

Claims are first-class, context- and time-scoped propositions. Contradictory claims can both be retained when their sources, dates, environment versions, or contexts differ. Confidence is evidence support, not a truth decree. Copied sources are correlated and must not count as independent confirmation.

## Explanation API

The normative V1 `explain_provenance` result is the typed directed derivation
graph required by ALG-PRV-001: source, operation, decision, proposal, approval,
output, and edge records, including compiler-generated administrative files and
author/license attribution. It accepts either a Compiled Vault directory or a
`.vaultpack` and never calls an AI provider or original network location.

The current `0.1.0` implementation is a partial projection of that contract. It
returns deterministic output records containing output hash, producing
operation ID, complete source identities for copied/rewritten/deduplicated
content, original logical paths/spans, and generated proposal/evidence. It does
not yet emit typed record IDs/edges, administrative-file derivations,
decision/approval nodes, or author/license nodes. Therefore REQ-PRV-001 and
QG-003 are not release-complete.

An explanation value is not itself a trust verdict. A caller must run `verify`
before treating an artifact or explanation as internally consistent; the full
sealed plan and conflict/proposal audit remain under `.vaultc/`.
