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
source_refs:
  - HIST-KNOWLEDGE-PLATFORM
---

# ALG-PRV-001: Provenance and Evidence Closure

## Objective and non-goals

Make every output explainable from immutable inputs, deterministic operations, and approved provider proposals. Provenance demonstrates derivation and attribution; it does not prove source truth, legal sufficiency, or independent corroboration.

## Inputs and outputs

Input is a plan, snapshots, operation results, proposal transcript, approvals, and output file hashes. Output is canonical JSON Lines forming a typed directed derivation graph plus indexes for explanation.

## Record identity

```text
RecordId(r) = H("vaultc:provenance:v1\0" || canonical_json(r without record_id))
EvidenceId(e) = H("vaultc:evidence:v1\0" || SnapshotId || DocumentId
                  || optional(BlockId, byte_start, byte_end) || ContentHash)
```

Every JSON value uses the versioned canonical encoder; records are emitted in `(record_type_order, record_id)` order.

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

- `source`: snapshot/file/block identity, path, author/license attribution.
- `operation`: copy, rewrite, deduplicate, generate, serialize, package.
- `decision`: conflict resolution or explicit waiver.
- `proposal`: provider/model/transcript hash and validation result.
- `approval`: approver/policy decision bound to proposal and plan hashes.
- `output`: output path/hash and producing operation.
- `edge`: typed `derived_from`, `supported_by`, `rewritten_from`, `deduplicates`, `approved_by`, or `packaged_as` relationship.

## Closure rules

1. Every output has exactly one direct producing operation.
2. Every operation reaches one or more source records, except compiler-generated administrative files whose source is the plan/config/toolchain records.
3. A `generate` operation reaches one approved proposal and at least one valid evidence record.
4. Every evidence span rehashes to the declared content hash.
5. Every exact-deduplicated output reaches all group members.
6. Derivation edges are acyclic; associative/semantic relationships use a separate graph.

## Pseudocode

```text
records = construct typed records from sealed plan and staged output
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
```

## Complexity

For `V` records, `E` edges, and evidence bytes already hashed during snapshotting: `O(V + E)` graph validation plus `O(V log V)` canonical ordering. Verification can stream JSONL with an on-disk index.

## Edge and security cases

Reject out-of-bounds spans, source-hash mismatch, dangling IDs, cycles, duplicate record IDs with distinct payloads, stale approvals, provider-supplied record IDs, license/author erasure, and disclosure of secrets in rationale/log fields. Paths in provenance are data and never opened without safe resolution.

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

Exact canonical-JSON RecordId fixtures MUST be frozen with schema implementation.

## Correctness and rollback

Primary metric is 100% provenance closure over every artifact in every successful build. Attribution-policy tests must have zero silent drops. Any closure failure prevents publish/pack verification. Provenance cannot be disabled for performance.
