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
source_refs:
  - HIST-KNOWLEDGE-PLATFORM
  - HIST-COMPILER-PLAN
---

# Provenance, Deduplication, and Conflicts

## Provenance invariant

Every output file has at least one derivation edge. A copied or rewritten note points to its source snapshot/file and relevant source spans. A deduplicated output points to every exact member. An AI-generated output points to an approved proposal and all supporting evidence. A synthesized claim without evidence is invalid.

The detailed record algorithm is [`../algorithms/stable/provenance-and-evidence.md`](../algorithms/stable/provenance-and-evidence.md).

## Duplicate policy

Exact duplicate identity requires both canonical Markdown body hash and normalized frontmatter hash to match. Same body with different normalized metadata is a frontmatter conflict, not an exact duplicate. Binary assets use byte SHA-256 independently.

When exact notes are unified:

- choose the canonical output path through the stable tie-break algorithm;
- preserve provenance for all members;
- rewrite every inbound link to the chosen path;
- do not invent a union of metadata because values happen not to collide.

Near-duplicate analysis uses MinHash/LSH only for candidate generation. A score at or above the default `0.85` threshold means “review similarity,” not semantic equivalence. V1 can keep both or apply an explicit curator decision; it cannot auto-synthesize one note.

## Conflict kinds

| Code family | Meaning | Default V1 behavior |
|---|---|---|
| `PATH_EXACT` | multiple non-identical files request same path | stable suffix or explicit decision |
| `PATH_CASEFOLD` | names collide on case-insensitive targets | stable suffix; error if impossible |
| `UNICODE_NORMALIZATION` | distinct source names normalize alike | stable suffix and warning |
| `TITLE_AMBIGUITY` | multiple documents claim same title | retain both; links may require rewrite/decision |
| `ALIAS_AMBIGUITY` | alias resolves to multiple targets | record ambiguous link conflict |
| `FRONTMATTER_VALUE` | same semantic field has differing values | retain source-specific values; no silent union |
| `LINK_AMBIGUITY` | link has zero/multiple candidates | preserve and diagnose or explicit mapping |
| `CONTENT_NEAR_DUPLICATE` | high candidate similarity | review only |
| `CLAIM_CONTRADICTION` | future claims conflict in context/time | coexist with evidence; never majority overwrite |
| `LICENSE_INCOMPATIBLE` | planned output use violates policy | hard error |

## Resolution states

`unresolved`, `auto_resolved_by_normative_rule`, `user_resolved`, `provider_suggested`, and `waived_by_policy` are distinct. A provider suggestion never becomes a resolution without approval. Every resolution stores the conflict content hash and becomes stale if candidates change.

## Future claim semantics

Claims are first-class, context- and time-scoped propositions. Contradictory claims can both be retained when their sources, dates, environment versions, or contexts differ. Confidence is evidence support, not a truth decree. Copied sources are correlated and must not count as independent confirmation.

## Explanation API

For any output path, `explain_provenance` returns a deterministic graph containing output hash, producing operations, rewrites, decisions, proposals, evidence references, original source paths/spans, and snapshot hashes. Explanations MUST be possible without calling an AI provider or accessing original network locations.
