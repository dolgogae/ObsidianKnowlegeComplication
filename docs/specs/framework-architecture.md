---
title: Framework Architecture
status: normative-v1
owners:
  - architect
  - core-rust-engineer
last_updated: 2026-08-16
decision_refs:
  - ADR-0001
  - ADR-0002
  - ADR-0004
  - ADR-0005
source_refs:
  - HIST-COMPILER-PLAN
---

# Framework Architecture

## Dependency direction

```text
Obsidian plugin       MCP server       Other applications
       \                  |                  /
        \          CLI / stable protocols  /
         +---------------v----------------+
         |         public `vaultc` API     |
         +---------------+----------------+
                         |
       +-----------------+------------------+
       | inspect -> IR -> plan -> validate  |
       | -> approve -> compile -> verify    |
       +-----------------+------------------+
                         |
       filesystem / SQLite workspace / pack

Provider process -> versioned NDJSON -> untrusted proposals
External MCP engines -> adapters -> derived candidates only
```

Dependencies point inward. The compiler core MUST NOT import Obsidian, MCP, hosted LLM, web-framework, or vendor SDK concepts.

## Rust workspace

| Package | State | Responsibility |
|---|---|---|
| `vaultc` | implemented public library | identifiers, IR, snapshotting, parsing/normalization, deduplication, planning, approval, compilation, packing, provenance, verification, workspace persistence |
| `vaultc-protocol` | implemented public library | versioned serializable request/response, provider capabilities, proposal/evidence, and transcript schemas |
| `vaultc-cli` | implemented binary `vaultc` | filesystem/control-file orchestration, human/JSON output, supervised subprocess provider execution |
| `vaultc-mcp` | future adapter | MCP tools that call public library operations |
| `vaultc-memory` | future experimental | calibrated memory/retrieval models isolated from file compilation |

Circular dependencies are forbidden. `vaultc` may depend on protocol data types but MUST NOT launch provider processes. Process supervision belongs to the CLI or an application adapter.

## Module boundaries

- `source`: validate typed directory/archive descriptors and source IDs.
- `snapshot`: open sources safely, apply exclusions/limits, enumerate bytes, and seal snapshots.
- `identity`: domain-separated hashes and stable IDs.
- `canonical`: deterministic JSON and content-hash serialization.
- `parse`: Markdown/Canvas/frontmatter decoding, source spans, and comparison-only normalization that never overwrites source bytes.
- `ir`: versioned canonical records; schema migration is future work.
- `dedup`: exact groups and near-duplicate candidates.
- `plan`: output namespace, rewrites, conflicts, diagnostics, sealed operations, and integrity revalidation.
- `provider`: provider-neutral traits plus capability/evidence/proposal validation.
- `approval`: record explicit decisions and invalidation rules.
- `compile`: stage, materialize, checksum, and atomically publish.
- `pack`: deterministic VaultPack creation and safe extraction for verification.
- `provenance`: emit and explain exact output-to-source derivations.
- `verify`: independently verify manifest, paths, hashes, sealed audit linkage, approvals, and provenance closure.
- `workspace`: bounded SQLite-backed intermediate state.

## State machine

```text
Sources
  -> InspectedSnapshotSet
  -> CanonicalWorkspace
  -> DraftPlan
  -> [AugmentationTranscript + Proposals]
  -> ValidatedProposals
  -> ApprovedPlan
  -> StagedOutput
  -> CompiledVault
  -> VerifiedArtifact
```

Operations MUST reject inputs from the wrong state. Any change to source hashes, compiler semantic version, policy configuration, or proposal payload invalidates dependent approvals.

## Data ownership

- Raw snapshots own original bytes.
- Canonical IR owns normalized semantics and source spans.
- A plan owns output-path choices and rewrite decisions.
- An approval log owns authorization for optional changes.
- A compiled artifact owns only materialized output plus its audit metadata.
- Search indexes and MCP engines own disposable derivatives.

## Persistence

The current library performs inspection/planning in memory and may persist an
inspection index to bundled SQLite. The CLI enables that SQLite workspace by
default. SQLite uses WAL, foreign keys, `FULL` synchronous mode, deterministic
transaction ordering, schema `user_version = 1`, and private Unix permissions.
It is a build workspace, not a registry or long-term canonical service
database.

The current SQLite layer cannot reload/resume an inspection and is reset on
each persisted inspection. Migration, cleanup, encryption, and resumability are
open V1 work. Calling it a bounded production streaming workspace before those
features and QG-006 evidence exist is forbidden.

Large blobs SHOULD be streamed from sources and MUST NOT be duplicated in
memory. The `0.1.0` scanner currently accumulates bounded source bytes in memory
before parsing and therefore does not yet meet this target at the 20 GB
reference workload. Database writes SHOULD be batched in deterministic
primary-key order.

## Errors and diagnostics

Errors mean the operation cannot safely continue. Diagnostics are versioned records with code, severity, source/output identity, optional span, explanatory fields, and remediation. Human text is not a stable API; diagnostic codes are.

Expected error families include unsafe path, unsupported archive, resource limit, malformed input, identity mismatch, plan stale, proposal invalid, approval stale, output exists, verification failed, and platform nondeterminism.

## Extension rules

Public extension points use traits and serializable schemas. Extensions MAY suggest candidates or proposals, but MUST NOT bypass planning, validation, approval, or safe materialization. Unknown enum variants and fields must be handled according to declared protocol compatibility rather than ignored silently.
