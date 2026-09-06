---
title: ALG-CNF-001 — Conflict Resolution and Output Layout
status: normative
owners:
  - core-rust-engineer
last_updated: 2026-09-02
decision_refs:
  - ADR-0003
  - ADR-0006
  - ADR-0017
source_refs:
  - HIST-COMPILER-PLAN
---

# ALG-CNF-001: Conflict Resolution and Output Layout

## Objective and non-goals

Allocate a unique, portable, explainable output path for every retained item and rewrite references consistently. The algorithm does not semantically choose which conflicting claim is true or merge near-duplicate prose.

## Inputs and outputs

Input is the ordered canonical IR, duplicate groups, conflicts, path policy,
typed decision overlays, and approved proposal commitments. Output is a total
input-to-output map, an immutable Draft Plan, typed resolutions, diagnostics,
stable operation IDs, and one derived Materialization Plan.

## Deterministic ordering and suffix

For each retained document define the ordering tuple:

```text
T(d) = (N_path(preferred_path(d)), SourceId(d), SnapshotId(d), DocumentId(d))
suffix(d) = "~" || first_collision_free_prefix(base32(DocumentId(d)), q)
```

Sort tuple fields by unsigned UTF-8 bytes (IDs by raw bytes). Start `q=8` Base32 characters and extend deterministically until unique. For an exact group, choose the minimum `T(d)` as canonical and attach all group provenance. For distinct documents requesting colliding paths, the minimum tuple keeps the unsuffixed safe path when policy allows; others receive the stable suffix before extension.

## Symbols

| Symbol | Meaning | Type/range/unit | Default |
|---|---|---|---|
| `d` | retained document | IR Document | required |
| `N_path` | normalized portable path | ALG-NRM-001 string | v1 |
| `preferred_path` | policy-selected initial destination | relative safe path | source-relative under `knowledge/` |
| `T` | deterministic tie-break tuple | total-order tuple | defined above |
| `SourceId` | stable source identity | UTF-8 identity | required |
| `SnapshotId` | immutable snapshot identity | 256-bit ID | required |
| `DocumentId` | document identity | 256-bit ID | required |
| `q` | visible Base32 prefix length | integer `[8,52]` | 8, extended as needed |

## Allocation pseudocode

```text
representatives = collapse exact groups, retaining member provenance
for d in sort(representatives by T):
    requested = safe_output_path(d)
    collision_key = portable_casefold_key(requested)
    if key unused and requested valid:
        allocate requested
    else:
        candidate = insert_suffix(requested, suffix(d))
        extend suffix until exact and portable keys are unused
        allocate candidate; emit typed resolution
for assets grouped by SHA-256:
    allocate attachments/<first-two-hex>/<full-hash>-<sanitized-basename>
for every resolved link/canvas reference:
    compute destination-relative target from allocated maps
validate paths and reparsed rewrites
seal DraftPlan and its conflict candidate sets
validate each DecisionOverlay against plan/conflict/hash/candidate set
action_set_hash = canonical_hash("okc:action-set:v2\\0", sorted overlays)
proposal_set_hash = canonical_hash("okc:proposal-set:v2\\0", approved commitments)
effective_operations = compose typed selections over a copy of DraftPlan operations
sort effective_operations by destination bytes and OperationId
materialization_id = canonical_hash("okc:materialization:v2\\0",
    base_plan_id, action_set_hash, proposal_set_hash, effective_operations)
```

Generated notes live under `knowledge/_generated/`. Canvas and Base artifacts live under `canvases/` and `views/`. Directory and filename components are sanitized through a versioned portable policy; original paths remain in provenance.

## Typed ambiguity actions and materialization

Only a required Markdown or Canvas `LINK_AMBIGUITY` may change an existing
output operation. `SelectMarkdownTarget` accepts one `DocumentId` from that
conflict's sealed candidate set. `SelectCanvasTarget` accepts one exact
`CanvasReferenceTarget` from its sealed candidate set. `WaivePreserveOriginal`
retains the already sealed operation. A decision may not supply arbitrary
replacement text or a path.

Markdown selection rewrites only the target while preserving embed state,
display text, heading, and block suffix. Canvas selection rewrites only the
typed file target while retaining every unknown JSON field. Multiple decisions
against one file are composed in increasing source-span order before one
effective operation is sealed. The Draft Plan is never mutated.

Every decision binds `PlanId`, `ConflictId`, the conflict content hash,
curator identity, and policy version. Duplicate decisions, stale identities,
action/subject mismatches, or targets outside the sealed candidate set fail
closed. Approval, compilation, provenance, and verification MUST call the same
materialization derivation and compare the resulting `MaterializationId`.

## Complexity

For `D` retained documents, `L` links, and `A` accepted actions: allocation is
`O(D log D)`, base rewriting is `O(L + total bytes)`, and materialization is
`O(A log A + total rewritten bytes)` plus path-map lookups. Collision-prefix
extension is bounded by the full 256-bit identity.

## Edge and security cases

Check case-insensitive collisions even on case-sensitive hosts, NFC/NFD
collisions, reserved Windows names, trailing dot/space, extension spoofing,
maximum path/component length, empty names, attachment basename injection,
link escape, destination symlinks, stale decision hashes, duplicate actions,
and out-of-set targets. No path is emitted from unvalidated provider text or
curator-supplied replacement text.

## Worked example

Sources `alpha` and `beta` each contain non-identical `Topic.md`. Suppose `T(alpha) < T(beta)`. Outputs become `knowledge/Topic.md` and `knowledge/Topic~K7P4D2QX.md`, where the suffix derives from beta's `DocumentId`. All links originally resolving to beta are rewritten to its suffixed path; links resolving to alpha remain unsuffixed. Reversing input argument order changes nothing.

## Golden vectors

| Candidate set | Expected |
|---|---|
| exact duplicates at `A.md` and `Copies/A.md` | one canonical path selected by `T`; both provenance edges |
| distinct `Topic.md` from sources `a`,`b` | one unsuffixed, one identity-suffixed |
| `Readme.md` and `README.md` | portable collision even on Linux |
| NFC and NFD spellings of same path | normalization collision and stable suffix |
| two IDs with same first 8 Base32 chars | extend both/required candidate deterministically until unique |
| ambiguous `![[Topic#Part|shown]]` with a sealed selection | only target changes; embed, heading, and display suffix survive |
| Canvas file node with unknown keys and a sealed selection | file target changes; unknown JSON keys survive |
| selected target absent from candidate set | reject before materialization |

## Correctness and rollback

Properties: total allocation, no exact or portable collision, input-order
invariance, stable suffixes, all resolved links target allocated items, an
unchanged Draft Plan, deterministic materialization, and no output escapes
root. On any unresolved required conflict, stale overlay, out-of-set target, or
failed reparse, stop before staging. There is no “last writer wins” fallback.
