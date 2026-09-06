---
title: ADR-0010 — Typed Provenance Graph and Non-Circular Audit Envelope
status: normative
owners:
  - architect
  - core-rust-engineer
  - qa-security-engineer
last_updated: 2026-08-16
decision_refs:
  - ADR-0003
  - ADR-0004
  - ADR-0006
  - ADR-0009
  - ADR-0010
source_refs:
  - HIST-CURRENT-PLAN
---

# ADR-0010: Typed Provenance Graph and Non-Circular Audit Envelope

## Status

Accepted on 2026-08-16.

## Context

ALG-PRV-001 requires a typed, content-addressed derivation graph for source,
operation, decision, proposal, approval, output, and edge records. The initial
`0.1.0` working tree emitted only one flat output summary per content file.
That projection could prove selected source membership, but could not express
typed graph closure, administrative-file derivation, canonical record
identity, decisions, approvals, or bounded graph explanation.

Putting every administrative file into the stored graph creates an impossible
hash fixed point. `provenance.jsonl` would need to contain its own final hash;
the manifest commits the provenance bytes; and checksums commit both while
also needing a provenance record of their own bytes. A `.vaultpack` has the
same issue because the finished outer archive cannot contain its own final
hash.

Source author and license fields also need a deterministic retention rule.
They are user-declared metadata, not facts the compiler may infer or legal
compatibility decisions it may invent.

## Decision

### Stored graph and virtual envelope

The physical graph `L` in `.vaultc/provenance.jsonl` covers:

- every content output;
- `.vaultc/plan.json`;
- `.vaultc/conflicts.json`;
- `.vaultc/diagnostics.json`;
- `.vaultc/ai-transcript.jsonl`;
- every source, operation, decision, proposal, approval, output, and edge
  needed to close those outputs.

The following self-referential envelope roots are not serialized inside `L`:

- `.vaultc/provenance.jsonl`;
- `.vaultc/manifest.json`;
- `.vaultc/checksums.txt`.

After all three exist, verification and explanation deterministically
construct a virtual graph `E` from their exact bytes and inventories. The
logical explanation graph is `G = L ∪ E`. Virtual records use the same record
schema and identity formula, but carry `storage = "virtual_audit_envelope"`
and MUST NOT appear in `.vaultc/provenance.jsonl`.

Compilation order is fixed:

1. materialize content and the four pre-envelope audit projections;
2. construct, validate, sort, and write `L`;
3. construct and write the manifest;
4. construct and write checksums;
5. independently verify the complete staged tree;
6. publish the absent destination.

The exact inventory sets are:

```text
P = actual files - {provenance, manifest, checksums}
manifest.files = P ∪ {provenance}
checksums entries = P ∪ {provenance, manifest}
```

The manifest and checksums exclude themselves. Paths are strict normalized
logical paths, all sets are exact rather than subsets, and all entries are
strictly path-sorted. Extra, missing, duplicate, case-aliased, or non-regular
members fail closed.

The virtual provenance operation commits the stored graph hash, plan, policy,
and compiler identity. The virtual manifest operation commits its exact
inventory and provenance graph commitment. The virtual checksum operation
commits the exact canonical checksum entry set. These dependencies always run
from derived value to prerequisite, so the envelope remains acyclic.

### Record shape and identity

Every stored or virtual record has required `schema_version = 1`, a required
typed `record_id`, and a required tagged `kind`. JSON uses this envelope:

```json
{
  "schema_version": 1,
  "record_id": "record_<64-lowercase-hex>",
  "kind": { "type": "output", "value": {} }
}
```

Unknown fields, missing fields, duplicate JSON object keys, non-canonical JSON,
blank lines, CRLF, an absent final LF, and over-limit records fail closed.

Stored JSONL limits are fixed at 16 MiB per canonical record line, 2,000,000
records, and 512 MiB for the complete ledger bytes. A verifier checks the
regular file's byte length before allocation and enforces count/aggregate
limits again while decoding; a final-LF separator byte is part of the
aggregate. These are safety ceilings, not target file sizes.

```text
RecordId(r) = SHA-256("vaultc:provenance:v1\0"
                     || canonical_json({schema_version, kind}))
```

The `record_` display prefix is not hashed. An edge is itself a provenance
record and uses `RecordId`; V1 deliberately has no second `EdgeId` formula.
Provider-supplied IDs are never accepted.

Records are strictly ordered by `(record_type_order, raw RecordId bytes)` with
this frozen type order:

```text
source=0, operation=1, decision=2, proposal=3,
approval=4, output=5, edge=6
```

The same ID twice is invalid even when the payload is byte-identical. A
payload whose recomputed ID differs from the stored ID is invalid.

### Record responsibilities

- `source` is either an immutable Vault source identity/evidence descriptor or
  a compiler build input. It never contains an absolute source locator or raw
  source bytes.
- `operation` is a sealed Copy, Markdown rewrite, Canvas rewrite,
  deduplication, approved generation, audit serialization, or virtual package
  operation.
- `decision` is the exact immutable conflict-decision overlay bound to its
  plan and conflict content hash.
- `proposal` binds provider/model identity, proposal/content identity,
  transcript commitment, and a successful validation result. Only structurally
  valid proposals become typed proposal nodes. Rejected provider values may
  contain intentionally unparseable typed IDs; their exact audit bytes remain
  committed by the sealed plan/transcript outputs and MUST NOT be coerced into
  trusted graph identities.
- `approval` binds approver, policy, plan, proposal content, and approved
  materialization commitment.
- `output` binds normalized logical path or virtual package subject, exact
  content hash, byte length, role, storage class, and producing operation.
- `edge` binds a typed relation, dependent `from` record, prerequisite `to`
  record, and either unordered or zero-based ordered position.

Edge direction is always derived/dependent → prerequisite. The permitted V1
relations and cardinalities are:

- Output → Operation: `derived_from`, exactly one producer;
- Copy operation → Source: `derived_from`;
- rewrite operation → Source: `rewritten_from`;
- deduplication operation → every exact member Source: `deduplicates`;
- generation operation → Proposal: `derived_from`, exactly one;
- generation operation → Approval: `approved_by`, exactly one;
- Proposal → evidence Source: `supported_by`, contiguous ordered positions
  matching proposal evidence order;
- an operation preserving an explicitly waived conflict → Decision:
  `decided_by`;
- audit serialization operation → build input or audit record:
  `derived_from`;
- virtual package operation → inner artifact input: `packaged_as`.

Edges cannot target other edge records. Self-edges, dangling endpoints,
relation/type violations, cycles, multiple producers, missing producers, and
unreachable decision/proposal/approval/source records fail closed.

All required Markdown and Canvas link-ambiguity decisions MUST have a typed
subject identifying the source document/link or Canvas/node. Human-readable
messages are not identity or authorization targets.

### Attribution retention

V1 extracts declarations only from successfully decoded Markdown frontmatter
mapping keys whose ASCII-case-folded name is `author`, `authors`, `license`, or
`licenses`. Each declaration stores:

- author or license kind;
- the original source key;
- the canonical JSON representation of the complete typed value;
- the owning source file/document identity.

Declarations are sorted by kind, source-key UTF-8 bytes, and canonical value
bytes, then exact-deduplicated. The compiler does not split names, canonicalize
SPDX expressions, resolve URLs, infer a missing license, or decide legal
compatibility.

Each document source record carries one required state:

- `not_declared` when no recognized key exists;
- `declared` with all recognized declarations;
- `opaque` with the sealed frontmatter hash when frontmatter could not be
  decoded as a canonical mapping.

Assets, Canvas, Base, snapshots, and build inputs use `not_applicable`.
Copied notes retain their original frontmatter bytes. Deduplicated outputs
retain declarations from every exact member. Generated outputs reach the
declarations of every evidence document through their ordered evidence source
records. An opaque state is explicit uncertainty, not permission to invent or
erase metadata.

### Manifest and compatibility

The manifest keeps its pre-release schema number `1` and gains required
`provenance_schema_version` and `provenance_graph_hash` fields. The graph hash
is stored as an unprefixed lowercase-hex `ContentHash` and is computed exactly
as:

```text
StoredGraphHash = SHA-256("vaultc:provenance-graph:v1\0"
                          || exact canonical JSONL bytes of L)
```

This is completion of an unpublished `0.1.0` schema, not a migration promise.
Legacy unversioned flat ledgers and manifests missing the new required fields
fail closed. They are not accepted through serde defaults or aliases. A future
artifact migration must rebuild the graph, manifest, checksums, and artifact
identity explicitly; it must not reinterpret old bytes in place.

### Explanation and pagination

The canonical SDK query accepts an artifact path plus either an artifact
logical path or an explicit package subject. It returns a versioned page of the
reachable induced subgraph in global record order.

- default: 256 records and 4 MiB response bytes;
- hard maximum: 4,096 records and 16 MiB response bytes;
- one record exceeding the byte limit fails without returning a partial page;
- `next_cursor` is bound to the subject's complete explanation hash, subject
  hash, and last sort key;
- malformed, stale, cross-artifact, or cross-subject cursors fail closed;
- a page may contain an edge whose other endpoint is on a later page, and
  therefore reports whether the explanation is complete.

A convenience unpaged API MAY collect all pages only within the hard bounds;
it MUST return `ResourceLimit` instead of silently returning the first page as
a complete explanation.

The ledger decoder reads bounded lines and enforces record/line/aggregate
limits. Its index MAY be memory-backed below policy limits and MUST move to a
bounded on-disk representation before the large-Vault performance gate can
pass.

Subject and cursor byte formulas are fixed:

```text
SubjectHash(ArtifactPath(path)) =
  SHA-256("vaultc:provenance-subject:v1\0" || 0x00
          || u64be(len(path_utf8)) || path_utf8)

SubjectHash(Package) =
  SHA-256("vaultc:provenance-subject:v1\0" || 0x01)

ExplanationGraphHash(subject) =
  SHA-256("vaultc:provenance-explanation:v1\0"
          || exact canonical JSONL bytes of the complete sorted
             reachable logical record set for subject)

Cursor = SHA-256("vaultc:provenance-cursor:v1\0"
                 || raw(ExplanationGraphHash)
                 || raw(SubjectHash)
                 || u8(record_type_order)
                 || raw(last RecordId))
```

`len(path_utf8)` is an unsigned 64-bit big-endian byte length. Record type
order is the single frozen byte `0..=6`, not text or a host integer. Stored,
subject, and explanation hashes are `ContentHash` values rendered as 64
lowercase hexadecimal characters when serialized. Cursor is a typed 32-byte
identity rendered as `cursor_` plus 64 lowercase hexadecimal characters; its
display prefix is not hashed. Because an explicit package explanation includes
the outer archive observation in its logical record set, its cursor is bound
to those exact outer bytes. An inner-path directory/pack explanation has the
same logical records and therefore the same cursor.

### VaultPack boundary

An inner artifact-path explanation is byte-semantically identical whether the
input is a Compiled Vault directory or a `.vaultpack`. Package records are not
automatically injected into inner explanations.

An explicit package query on a `.vaultpack` produces virtual records binding
the observed outer raw SHA-256, byte length, inner artifact ID, deterministic
tar/zstd profile, and ordered member commitments. A package query on a
directory is unsupported. The virtual package record is an integrity
observation, not publisher authenticity. A future detached PackReceipt or
signature profile may authenticate that record without creating a self-hash.

The deterministic profile label is asserted only after the verifier safely
extracts and validates the inner artifact, recreates the package with the exact
current writer/toolchain profile, and byte-compares the complete outer archive.
An archive with equivalent members but different zstd parameters, tar headers,
member order, or extra encoding bytes is non-canonical and fails verification;
it MUST NOT receive the deterministic profile record.

## Consequences

- Every content and pre-envelope audit output has a stored, typed, acyclic
  derivation; every materialized path has either stored or explicitly virtual
  explanation closure.
- Fully resealed graph tampering is rejected by exact reconstruction from the
  sealed approved plan and physical artifact inventory.
- The self-reference exception is narrow, visible, and testable rather than a
  hidden omission.
- Attribution declarations remain inspectable without turning the compiler
  into a license inference engine.
- The public provenance JSON and Rust API change during unpublished `0.1.0`;
  old development artifacts intentionally fail closed.
- Streaming/on-disk indexing, detached signing, and legal license
  compatibility remain separate release work.
