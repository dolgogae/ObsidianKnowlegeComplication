---
title: ALG-PRV-001 — Provenance and Evidence Closure
status: normative-v1
owners:
  - architect
  - core-rust-engineer
last_updated: 2026-08-16
decision_refs:
  - ADR-0003
  - ADR-0004
  - ADR-0006
  - ADR-0010
source_refs:
  - HIST-KNOWLEDGE-PLATFORM
---

# ALG-PRV-001: Provenance and Evidence Closure

## Objective and non-goals

Make every output explainable from immutable inputs, deterministic operations, and approved provider proposals. Provenance demonstrates derivation and attribution; it does not prove source truth, legal sufficiency, or independent corroboration.

## Inputs and outputs

Input is a plan, snapshots, operation results, proposal transcript, approvals,
output file hashes, and the audit-envelope inventory. Output is canonical JSON
Lines forming a typed directed derivation graph plus bounded indexes for
explanation. The stored graph and the virtual self-referential envelope are
separated exactly as defined by ADR-0010.

## Record identity

```text
RecordId(r) = H("vaultc:provenance:v1\0"
                || canonical_json({schema_version: 1, kind: r.kind}))
EvidenceId(e) = H("vaultc:evidence:v1\0" || raw(SnapshotId)
                  || raw(DocumentId) || mode
                  || mode_fields || raw(ContentHash))
```

Every JSON value uses the versioned canonical encoder. `RecordId` renders as
`record_` followed by 64 lowercase hexadecimal characters; that display prefix
is excluded from the hash. Edges are records and use `RecordId`, not a separate
`EdgeId`. Records are emitted in `(record_type_order, raw RecordId bytes)`
order, where source, operation, decision, proposal, approval, output, and edge
have orders 0 through 6 respectively.

`raw(TypedId)` is the fixed 32-byte hash without its display prefix. `mode` is
one byte: `0x00` for file/body evidence with no block or span, `0x01` for block
evidence with no repeated span, and `0x02` for block evidence with its exact
sealed span. `mode_fields` is empty for `0x00`, the raw 32-byte `BlockId` for
`0x01`, or the raw `BlockId` followed by unsigned 64-bit big-endian
`byte_start` and `byte_end` for `0x02`. Partial spans, a file-level span, and
`start > end` are invalid. Display strings, JSON, host endianness, and the
generic length-prefixed identity helper are not part of this formula.

## Symbols

| Symbol | Meaning | Type/range/unit | Default |
|---|---|---|---|
| `r` | complete provenance record payload | versioned JSON object | required |
| `e` | source evidence descriptor | versioned record | required for generated knowledge |
| `H` | SHA-256 | bytes → 32 bytes | SHA-256 |
| `SnapshotId` | immutable source snapshot | 256-bit ID | required |
| `DocumentId` | parsed document identity | 256-bit ID | required |
| `BlockId` | optional block identity | 256-bit ID | absent for file-level evidence |
| `byte_start/end` | half-open source span | integers, `0 ≤ start ≤ end ≤ source_len` bytes | optional |
| `ContentHash` | hash of referenced exact bytes/semantic item | 256-bit ID | required |

## Record types

- `source`: snapshot/file/evidence identity, path, declared/opaque attribution
  state, or plan/policy/toolchain build input.
- `operation`: copy, rewrite, deduplicate, generate, serialize, package.
- `decision`: conflict resolution or explicit waiver.
- `proposal`: provider/model/transcript hash and validation result.
- `approval`: approver/policy decision bound to proposal and plan hashes.
- `output`: output path/hash and producing operation.
- `edge`: typed `derived_from`, `supported_by`, `rewritten_from`,
  `deduplicates`, `approved_by`, `decided_by`, or `packaged_as` relationship,
  directed from derived/dependent record to prerequisite.

The exact record fields, edge type matrix, ordered-position encoding, stored
versus virtual storage class, and author/license declaration projection are
normative in ADR-0010. Human-readable conflict messages and source display
names are never authorization identities.

## Closure rules

1. Every output has exactly one direct producing operation.
2. Every operation reaches one or more source records, except compiler-generated administrative files whose source is the plan/config/toolchain records.
3. A `generate` operation reaches one approved proposal and at least one valid evidence record.
4. Every evidence span rehashes to the declared content hash.
5. Every exact-deduplicated output reaches all group members.
6. Derivation edges are acyclic; associative/semantic relationships use a separate graph.
7. A generated-note approval seals its destination, canonical emitted body
   hash, complete rendered-output hash, ordered EvidenceId values, and
   operation ID. Compilation and verification independently reconstruct that
   materialization from the approved proposal.
8. Every applied decision, materializing proposal, approval, and source record
   is reachable from at least one output. The serialized plan audit output also
   closes non-materializing validated proposals and decision history.
9. Stored graph records cover content plus plan/conflict/diagnostic/transcript
   audit files. Provenance, manifest, and checksums are virtual audit-envelope
   roots constructed only after their final bytes exist.
10. Successfully decoded `author(s)` and `license(s)` frontmatter declarations
    remain reachable from every copied, deduplicated, or evidence-derived
    output. Opaque frontmatter is recorded as opaque with its sealed hash;
    attribution is never guessed.

## Pseudocode

```text
records = construct stored typed records from sealed plan and pre-envelope output
for evidence in records:
    validate IDs, bounds, source content hash, attribution
for output in manifest:
    require one producing operation and matching output hash
walk reverse derivation graph from every output
require closure according to operation kind
require proposals validated and approvals current
topologically validate derivation edges
canonicalize records; compute record IDs; sort; emit JSONL
re-read emitted file and repeat closure validation
write manifest, then checksums
construct virtual envelope graph from final physical bytes
validate full logical graph = stored graph union virtual envelope
```

## Complexity

For `V` records, `E` edges, and evidence bytes already hashed during snapshotting: `O(V + E)` graph validation plus `O(V log V)` canonical ordering. Verification can stream JSONL with an on-disk index.

## Edge and security cases

Reject out-of-bounds spans, source-hash mismatch, dangling IDs, self-edges,
edge-to-edge endpoints, cycles, duplicate record IDs even with identical
payloads, non-canonical records, stale approvals, provider-supplied record IDs,
license/author erasure, and disclosure of secrets in rationale/log fields.
Paths in provenance are data and never opened without safe resolution.

The stored JSONL safety ceilings are 16 MiB per record line, 2,000,000
records, and 512 MiB aggregate including each final LF. Exceeding any ceiling
is a resource-limit failure before graph semantics are trusted.

`.vaultc/provenance.jsonl`, `.vaultc/manifest.json`, and
`.vaultc/checksums.txt` MUST NOT claim stored records for themselves. Their
records are marked virtual and synthesized from final bytes. A `.vaultpack`
package record is likewise virtual and exists only for an explicit package
query; it does not prove publisher authenticity.

## Worked example

`knowledge/_generated/rust-map.md` is produced by operation `op-g`. `op-g` derives from approved proposal `p1`; `p1` is supported by blocks in `alpha/ownership.md` and `beta/borrowing.md`; approval `a1` binds the exact `p1` and plan hashes. Explanation walks output → `op-g` → `p1` → evidence → two immutable source snapshots and separately → `a1`.

## Golden vectors

| Graph | Expected |
|---|---|
| copied note → copy op → one source file | valid |
| exact output → dedup op → two exact source docs | valid |
| generated output without `supported_by` | invalid |
| generated output with proposal but stale approval | invalid |
| evidence span end greater than file length | invalid |
| derivation A → B → A | invalid cycle |

The frozen file-level EvidenceId vector uses raw snapshot bytes `0x11` × 32,
raw document bytes `0x22` × 32, mode `0x00`, and raw content-hash bytes `0x33`
× 32. Its typed rendering is
`evidence_ca746ceb4dbbe4c963a82668cc4aa11852a41d84dcea24e8aa36407b52993bbe`.
Block mode `0x01` and exact-span mode `0x02` MUST produce different identities
for the same block and content hash.

Exact canonical-JSON RecordId fixtures MUST be frozen with schema implementation.

The stored graph, subject, explanation, and cursor identities use the exact
ADR-0010 formulas. In particular, the cursor is
`H("vaultc:provenance-cursor:v1\0" || raw(ExplanationGraphHash) ||
raw(SubjectHash) || u8(record_type_order) || raw(last RecordId))`, rendered as
`cursor_` plus lowercase hexadecimal. Artifact-path subjects use a `0x00` tag,
an unsigned 64-bit big-endian UTF-8 byte length, and the path bytes; package
subjects use only tag `0x01`. A decoder may find the opaque cursor by
recomputing candidates while scanning; it must not trust caller-provided
offsets.

## Correctness and rollback

Primary metric is 100% provenance closure over every artifact in every successful build. Attribution-policy tests must have zero silent drops. Any closure failure prevents publish/pack verification. Provenance cannot be disabled for performance.
