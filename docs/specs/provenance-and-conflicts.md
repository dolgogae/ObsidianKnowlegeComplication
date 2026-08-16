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
  - ADR-0010
source_refs:
  - HIST-KNOWLEDGE-PLATFORM
  - HIST-COMPILER-PLAN
---

# Provenance, Deduplication, and Conflicts

## Provenance invariant

Every output file has exactly one direct producing operation. A copied or
rewritten note reaches its source snapshot/file and relevant source spans. A
deduplicated output reaches every exact member. An AI-generated output reaches
one current approval, its approved proposal, and all ordered supporting
evidence. A synthesized claim without evidence is invalid.

The independent verifier MUST reconstruct the expected record for each sealed
output operation or approved generated note and compare it exactly with the
published provenance ledger. It is insufficient to check only that each named
source exists: an unrelated valid source is not provenance for an output.
Standalone audit summaries and the embedded approved plan MUST agree exactly.

The detailed record algorithm is [`../algorithms/stable/provenance-and-evidence.md`](../algorithms/stable/provenance-and-evidence.md).

Every physical content path and the plan/conflict/diagnostic/transcript audit
paths are represented in the stored typed graph. The graph uses required
schema-1 `record_` identities, the fixed record order, dependent-to-prerequisite
edge direction, relation/type matrix, and closure rules in ADR-0010. The graph
is canonical JSON Lines with exactly one canonical record per non-empty line
and a final LF.

The ledger cannot contain its own final hash. Therefore provenance, manifest,
and checksums are a narrow virtual audit envelope constructed from final bytes
by verification and explanation. This is not missing provenance and MUST NOT
be replaced by a forged self-referential stored edge. A VaultPack package
subject is also virtual and separately requested. Stored and virtual records
share the same identity formula and public record schema.

Parsed Markdown frontmatter author/license declarations are retained without
legal inference. Recognized keys are ASCII-case-insensitive `author`,
`authors`, `license`, and `licenses`; the original key and complete canonical
typed JSON value are preserved. Missing, declared, opaque, and not-applicable
states are distinct. Exact deduplication and generated evidence retain every
contributing document's declarations.

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
this hash. A Markdown-link subject binds the source `DocumentId`, `LinkId`, and
raw target. A Canvas-reference subject binds the `CanvasId`, unique node ID,
and raw file path. Each ambiguous Markdown link or Canvas file node receives
its own subject and conflict even when its human-readable target and candidate
documents equal another node's. External decisions live
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

The normative V1 explanation result is a versioned page of the reachable typed
directed derivation graph required by ALG-PRV-001: source, operation, decision,
proposal, approval, output, and edge records, including compiler-generated
administrative files and author/license declarations. It accepts either a
Compiled Vault directory or a `.vaultpack` and never calls an AI provider or
original network location.

The default page limit is 256 records and 4 MiB; hard limits are 4,096 records
and 16 MiB. Cursors bind graph, subject, and last global sort key. A stale,
tampered, cross-artifact, or cross-subject cursor fails closed. A convenience
full explanation may collect pages only within the hard bounds and otherwise
returns a resource-limit error.

Inner-path explanations from a directory and VaultPack are identical. An
explicit package query returns a virtual package output/operation binding the
observed outer bytes and inner artifact identity; it is not added to inner
queries and does not authenticate the publisher.

An explanation value is not itself a trust verdict. A caller must run `verify`
before treating an artifact or explanation as internally consistent; the full
sealed plan and conflict/proposal audit remain under `.vaultc/`.
