---
title: ALG-PRV-001 — Provenance and Evidence Closure
status: normative
owners:
  - architect
  - core-rust-engineer
last_updated: 2026-09-06
decision_refs:
  - ADR-0003
  - ADR-0004
  - ADR-0006
  - ADR-0022
  - ADR-0024
  - ADR-0027
source_refs:
  - HIST-KNOWLEDGE-PLATFORM
---

# ALG-PRV-001: Provenance and Evidence Closure

## Objective and non-goals

Make every current canonical note and source redirect explainable from its
sealed integration corpus, proposal, critic, and curator approval. Provenance
proves derivation and internal consistency, not truth or publisher identity.
The retired graph, virtual-root, EvidenceId, and cursor model is historical in
[`ADR-0010`](../../adr/0010-typed-provenance-and-audit-envelope.md); it is not
the current Schema 3 storage or explanation contract.

## Inputs and outputs

Input is one validated `ApprovedIntegrationPlan` and its deterministic
materialized `(output_path, bytes)` sequence. Output is canonical
`.okc/provenance.jsonl` containing one `ProvenanceRecord` per canonical note or
redirect, in unsigned UTF-8 output-path order, with one final LF per record.
Audit files do not receive records for themselves.

## Identities and symbols

Let `H` be SHA-256 and `C` the existing canonical JSON encoder. Current
identities are exactly:

```text
OutputHash = H("okc:output:v3\0" || output_bytes)
RecordHash = H("okc:provenance:v3\0" || C({
  output_path, output_hash, integration_plan_id, kind
}))
RecordId = "record_" || lowercase_hex(RecordHash)
```

`output_path` is the exact safe artifact-relative UTF-8 path. `kind` is
`canonical_note` or `legacy_redirect`. `output_hash` is the typed content-hash
serialization, and `integration_plan_id` binds the complete approved plan.
Hash domains, JSON field names, record IDs, paths, and successful output bytes
are compatibility invariants under ADR-0027.

## Record construction

Every record includes schema 3, record ID, kind, output path/hash, integration
plan ID, cluster ID, proposal hash, critic hash, approved revision hash, ordered
evidence, and an optional source document ID.

- A canonical note binds its cluster revision. Its direct evidence is the
  sorted, deduplicated union of section and contradiction-claim evidence.
  Dispositions and retained metadata remain reachable through the embedded
  plan and the same approved revision.
- A redirect additionally binds its exact source document and ordered source
  blocks. The sealed document resolves its source ID and original path.
- Evidence is a `(DocumentId, BlockId, ContentHash)` tuple. IDs and text hashes
  must match a document in the current cluster; unrelated valid evidence is
  insufficient.

No provider-supplied record ID or graph edge is trusted. The compiler constructs
records after validating ALG-INT-001 and calculating the exact output bytes.

## Verification and explanation

```text
validate the current artifact root and complete file inventory
validate the embedded approved integration plan
regenerate all materialized paths and bytes from the plan
regenerate canonical provenance from those same paths and bytes
require exact path, byte, and provenance equality
verify manifest identities and the complete checksum inventory
for explain: validate one requested relative output path
return its unique ProvenanceRecord
```

An attacker cannot authorize extra content by adding it to both manifest and
checksums. The allowed inventory comes from the plan plus the four fixed audit
files. Missing, additional, duplicate, stale, unsafe, or non-reproducible data
fails verification. Manifest/checksum recursion is excluded as specified in
[`the output contract`](../../specs/compiled-vault-and-vaultpack.md).

`explain(root, output_path)` verifies the entire directory first. It neither
opens an original Vault nor invokes a provider. There is no package, cursor,
pagination, or virtual audit-root explanation surface.

## Complexity and resource limits

For `F` output files, `E` evidence references, and `B` materialized/audit bytes,
ordered construction requires `O(F log F + E log E + B)` work after plan
validation. The current implementation buffers source and plan data;
it has not met the streaming or 100k-note performance gate. Verification must
reject unsafe entries and unexpected paths before reading their content.

## Worked example and golden vectors

`knowledge/rust/ownership.md` has one canonical-note record binding the
approved cluster revision and evidence from its source blocks.
`legacy/team/Ownership.md` has a separate redirect record binding the source
document and the same revision. Reversing source registration changes neither
the plan-derived path order nor the resulting bytes.

Required regressions cover missing/foreign evidence, stale critic/approval,
tampered materialized bytes, additional files even with recomputed checksums,
symlinks, unsafe explanation paths, and repeated offline compilation.
The Rust/Python/Node fixture inventory remains the golden defined in
[`testing and quality gates`](../../specs/testing-and-quality-gates.md).

## Failure and rollback

Any closure or inventory failure prevents publication or successful
verification. Provenance cannot be disabled. Old complete approved runs may be
selected explicitly; their authority and journal history are never rewritten.
