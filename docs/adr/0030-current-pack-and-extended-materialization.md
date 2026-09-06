---
title: ADR-0030 — Current Pack and Extended Materialization Boundaries
status: proposed
owners:
  - architect
  - core-rust-engineer
  - qa-security-engineer
  - release-maintainer
last_updated: 2026-09-06
decision_refs:
  - ADR-0006
  - ADR-0012
  - ADR-0013
  - ADR-0014
  - ADR-0024
  - ADR-0027
  - ADR-0030
source_refs:
  - HIST-COMPILER-PLAN
---

# ADR-0030: Current Pack and Extended Materialization Boundaries

## Status

Proposed on 2026-09-06; not accepted or implemented. Its three decisions are
separately reviewable. Neither a `.okcpack` command, raised parser limit, nor a
new format is enabled by this file. Current Schema 3-only directory behavior,
strict bounds, unsupported errors, and byte goldens remain authoritative.
Review entry point: [`ADR proposals`](README.md).

## Context and affected contracts

The current plan describes Markdown blocks/metadata and approved clusters;
the verifier derives every allowed output from that plan. Adding an attachment
to the manifest does not grant authority to emit it. Canvas/Base and human
amendment provenance also lack complete current DTO representations.

The reserved Pack profile `okc-tar-zstd-deterministic-v3` is already embedded in
existing manifests, but no current writer exists. Packaging the same verified
directory does not itself require changing its files. Separately, 512-MiB plan
and 1-MiB manifest limits constrain the full-scale workload even if internal
processing becomes streaming.

Affected: REQ-PAR-002/003, REQ-MAT-001, REQ-PRV-001, REQ-CMP-001/002/003,
REQ-SEC-001, REQ-INT-002/005/006, REQ-SDK-001/002, REQ-PERF-001;
ALG-SNP-001, ALG-NRM-001, ALG-CNF-001, ALG-PRV-001, ALG-INT-001;
QG-001 through QG-008. No experimental algorithm or source execution is proposed.

## Proposed decision A — Pack the existing current directory

### Payload and deterministic profile

Introduce a current-only Pack writer after independent verification of a
Schema 3 directory. Its logical payload is exactly that directory's accepted
regular-file inventory, including the four audit files; no extra source,
credential, environment, installation script, or Pack metadata is inserted
into existing artifact bytes. Container representation and expanded artifact
identity remain distinct. The manifest's existing profile literal is retained.

Propose a frozen profile with these values, requiring archive byte vectors
before acceptance:

| Field | Proposed encoding |
|---|---|
| Member order | relative UTF-8 path byte order; files only, no directory entries |
| Root prefix | none; paths begin directly with `knowledge/`, `legacy/`, `.okc/` |
| Tar metadata | uid/gid/mtime=0; empty user/group names; regular files mode 0644 |
| Long paths | local PAX `path` record only when USTAR cannot encode the path; deterministic header name/index and UTF-8 length calculation |
| Extensions | reject global PAX, sparse files, links, devices, GNU long-name records and unrecognized PAX keys |
| End/padding | two zero tar blocks; zero entry padding; no extra noncanonical tar padding |
| zstd | single frame, level 3, one worker, no dictionary, content checksum enabled, no embedded content-size field |
| Compressor contract | exact encoder/decoder/version/options and golden bytes frozen in the acceptance change; no dependency upgrade implied here |

The future decoder bounds compressed bytes, expanded bytes, member count,
per-file size, path depth/length and expansion ratio before and while reading.
It rejects duplicate logical names, portable case/NFC/prefix collisions,
unknown/mixed markers, malformed header lengths, trailing frames/data, unsafe
links and unplanned files. PAX header bytes count toward resource bounds.
Compression options cannot be changed under the same profile string merely
because extracted files remain equal.

### Shared verification and publication

Core verification reads a bounded artifact-entry abstraction backed by either
pinned directory handles or a strictly validated Pack stream/spool. The
manifest is not trusted as an extraction allowlist: the embedded approved
plan independently derives the inventory and expected content. Explain returns
the same typed per-output provenance for the same logical payload. No arbitrary
filesystem extraction occurs before validation, and no source/provider is
opened to fill missing evidence.

Pack publication writes a private regular stage in the destination's parent,
finishes and syncs the encoder, independently verifies that stage, atomically
publishes without replacement, then syncs the parent where qualified. Directory
and Pack are two ordered publications, never a combined transaction. A Pack
failure preserves an already verified directory; postcommit durability failure
preserves the complete Pack and reports its exact state.

Proposed additive CLI/API operations must be documented together across core,
app, CLI and bindings. Explicit Pack detection inspects bounded internal schema
markers; a suffix alone is not proof of current format. Schema 1/2 Packs remain
unsupported with no retired reader. Existing directory return fields remain
unchanged; any new structured partial-success error requires interop review.

Pack integrity is not publisher authenticity. This decision adds no signature
trust anchor, registry, auto-install, script execution or source Pack fetch.

## Proposed decision B — Bounded large control-file execution

Do not equate large JSON with permission to allocate a large `serde_json::Value`.
The recommended first investigation is a streaming parser/validator/writer
for the *existing Schema 3 shape and exact canonical bytes*, with temporary
SQLite indexes for cross-record uniqueness and closure. This can retain
identities without introducing a new schema merely to change storage strategy.

Only after that implementation is independently tested, consider an explicit
`large-artifact-1` runtime profile with proposed absolute ceilings:

- approved plan: 64 GiB;
- manifest: 256 MiB;
- derived artifact entries: 500,000;
- aggregate expanded output/audit bytes: 128 GiB;
- memory remains the 2,000,000,000-byte process-tree target from ADR-0028;
- source safety limits, per-string/section/path bounds, JSON nesting limits,
  strict duplicate/unknown-field rejection and exact closure remain in force.

These ceilings are review candidates, not permission to raise current limits.
The proposed larger *aggregate control-file* limits expressly amend the
current 512-MiB/1-MiB limits only if accepted with evidence. Existing in-memory
entry points keep their small-input limits and explicitly refuse a large plan;
there is no silent fallback that loads it anyway. A runtime resource profile
must not be inserted into an existing semantic hash or change output bytes.

A sizing report must show the full fixture's corpus, proposal, recording,
inventory and provenance overhead before approving these numbers. Preflight
requires sufficient configured disk budget; arithmetic overflow, recursive
references, truncated JSON or exhausted limits fail without publication.
If the current shape cannot support bounded standalone validation, reject this
option and return for the explicit new-format decision below. Do not claim
QG-006 passed based on a fixture that was refused at plan sealing.

## Proposed decision C — Typed extended evidence and materialization

### Recommended next-format design

For semantics absent from the current shape, use an explicitly discriminated
next-format envelope instead of optional unsealed fields or overloading
`SourceBlock.text` with encoded attachments. Schema 4 is a **candidate**, not an
allocated current version. This draft does not approve a product version,
migration, parallel reader or change to ADR-0027's single-current boundary.

The envelope has one root approval binding:

```text
format/profile versions and canonical manifest digest
approved integration root
ordered typed evidence/proposal/critic/approval/amendment object digests
complete selected non-Markdown source-file inventory and disposition digest
canonical source-to-output and link-resolution map digest
complete output recipe and payload inventory digest
```

Large records are canonical content-addressed pages with declared count,
length, type and digest, all rooted in the envelope. Each page is capped at a
proposed 4 MiB; counts and aggregate bytes have independent hard limits.
Large file payloads are bounded streams, not JSON/base64 blobs. Resolve only
objects carried by the input bundle; no network or mutable project lookup can
repair an incomplete artifact. Use an acyclic declared object graph, detect
duplicate/missing/extra objects and validate all references before publication.
The format must finalize its exact key order, page boundary algorithm, hash
domains and worked byte vectors before implementation.

The root, not a loose manifest or an additional sidecar approval, is the sole
compile authority. Its identity covers every selected asset and rewrite.
Existing Schema 3 hashes never acquire new meanings. Human-origin amendment
records from ADR-0029 have a distinct typed origin linked to their parent
provider proposal, critic and curator approval.

### Selected content rules

| Kind | Proposed materialization and evidence |
|---|---|
| Markdown | approved canonical notes and redirects; exact span-aware typed link rewrite map; no blind text substitution |
| Attachment | stream verified selected bytes; deduplicate by content with deterministic path allocation; retain all source/attribution edges |
| Canvas | preserve nodes/edges/unknown data; rewrite only recognized, approved file-reference fields; canonicalize JSON when rewritten; never execute content |
| Base | copy selected opaque bytes with an explicit warning; do not claim semantic understanding or silently rewrite its internals |
| Human amendment | embed canonical amendment/evidence chain, never a fabricated provider response |

Every eligible non-Markdown source entry has one explicit selected/omitted
disposition, with curator-reviewed omission rationale. Selected records bind
snapshot, source/document or file identity, original/logical path, length and
content hash. Source bytes are captured once through pinned immutable-input
handles into approved payload objects; offline compilation never reopens the
live Vault. No full `_sources` mirror, source archive, `.obsidian` plugin,
secret file or executable is carried through.

An accepted attachment/Base is selected output content under ADR-0006, not a
license to copy every raw source. Assets receive per-output provenance even
when several sources deduplicate to one file. Unknown Canvas fields are data,
not authority to resolve a new path or URL.

Resolve source links using the sealed source-specific canonical map. Preserve
display text, embed markers and supported fragments. Ambiguous targets need
explicit hash-bound resolution. Escaping, outside-root and unrepresentable
targets fail; a waiver cannot authorize them. Safe unresolved targets may be
preserved only with an explicit approved disposition and visible diagnostic.
Headings/block fragments must have a real sealed destination mapping; a note
mapping alone cannot claim those fragments survived synthesis. Base references
are warning-bearing opaque data, not part of that rewrite guarantee.

`preserved_verbatim` must continue to mean the exact sealed text projection.
Do not silently rewrite links inside such a block. If a target change is
needed, propose an `integrated` section with the original block evidence and
the explicit rewrite, then obtain fresh critic/curator approval. Freeze the
rewrite map before the final critic reviews the rendered proposal. Introducing
a different preservation action would instead require an explicit disposition
contract change in the new format.

### Compatibility choice required before acceptance

| Option | Tradeoff and proposed position |
|---|---|
| Put new semantics into old fields | keeps the number but breaks approval/provenance meaning; reject |
| Explicit next schema, archived previous implementation, separate reconstructed project | recommended if retaining ADR-0027's single-current policy; breaking transition needs explicit owner approval, archive/recovery procedure and fresh approvals |
| Explicit next schema plus read-only Schema 3 support | preserves in-process inspection but amends the single-current policy and adds reader/security maintenance; not assumed authorized |

Until this choice is accepted, Schema 3 remains the only current product and
its projects/bytes remain unchanged. No archive tag is created by this draft.
Any future project reconstruction copies source bindings/curator display data
only and recomputes all semantic/disclosure/review authority; it never upgrades
old approvals into new-format authority. Keep a matching original binary and
project/artifact backup as the rollback path. A new format's release version
and interop version require a separate explicit entry in the version matrix.

## Acceptance and implementation order

Architect, core, security and release owners may accept A independently of B/C.
Acceptance must name the exact subdecision and update affected current specs;
the word "accepted" cannot ambiguously enable all three.

1. Qualify source/managed/output handles and cancellation (ADR-0031).
2. Freeze A's tar/PAX/zstd bytes and bounded hostile Pack corpus; introduce only
   the current Schema 3 container path and directory/Pack parity tests.
3. Evaluate B's streaming shape with exact old goldens, full sizing/RSS report,
   duplicate/ordering/truncation fuzz and disk exhaustion before raising bounds.
4. Resolve C's format/version/compatibility policy and exact schemas; then
   implement typed assets/Canvas/Base/amendments with closed approval/provenance.

Planned tests cover directory/Pack inventory and explain equality; host byte
goldens; malicious tar/PAX/zstd; races and residual publication state; streaming
vs in-memory parity; orphan/missing/extra pages; altered blob/rewrite/approval
hashes; duplicate-asset attribution; Unicode collisions; Canvas unknown fields
and links; Base warnings; no raw source mirror; old-schema explicit refusal.

Update compiled-format, IR, pipeline, provenance, security, public SDK/CLI,
testing and release specifications together with each accepted behavior change.
No proposed format or larger limit is a reason to mark an existing release gate
green. Rolling back a writer never deletes an already published artifact.
