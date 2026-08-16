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

## Planned Rust workspace

| Package | Visibility | Responsibility |
|---|---|---|
| `vaultc` | public library | identifiers, IR, snapshotting, parsing, normalization, deduplication, planning, approval, compilation, verification |
| `vaultc-protocol` | public library | versioned serializable request/response, provider capabilities, proposal and transcript schemas |
| `vaultc-cli` | binary `vaultc` | filesystem orchestration, human/JSON output, subprocess provider execution |
| `vaultc-mcp` | future adapter | MCP tools that call public library operations |
| `vaultc-memory` | future experimental | calibrated memory/retrieval models isolated from file compilation |

Circular dependencies are forbidden. `vaultc` may depend on protocol data types but MUST NOT launch provider processes. Process supervision belongs to the CLI or an application adapter.

## Module boundaries

- `source`: open directory/archive safely, apply exclusions, enumerate bytes.
- `identity`: domain-separated hashes and stable IDs.
- `parse`: Markdown/Canvas/frontmatter decoding with source spans.
- `normalize`: comparison-only canonical forms; never overwrite original bytes.
- `ir`: versioned canonical records and schema migration.
- `dedup`: exact groups and near-duplicate candidates.
- `planner`: output namespace, rewrites, conflicts, diagnostics, operations.
- `proposal`: validate provider proposals and bind them to plan/input hashes.
- `approval`: record explicit decisions and invalidation rules.
- `compile`: stage, materialize, checksum, and atomically publish.
- `verify`: independently verify manifest, paths, hashes, links, and provenance closure.
- `workspace`: bounded SQLite-backed intermediate state.

## State machine

```text
Sources
  -> InspectedSnapshotSet
  -> CanonicalWorkspace
  -> DraftPlan
  -> [AugmentationTranscript + Proposals]
  -> ValidatedPlan
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

The library supports an abstract workspace. V1 provides in-memory operation for small fixtures and SQLite for bounded production workloads. The CLI enables bundled SQLite by default. SQLite is a build workspace, not a registry or long-term canonical service database.

Large blobs SHOULD be streamed from sources and MUST NOT be duplicated in memory. Database writes SHOULD be batched in deterministic primary-key order.

## Errors and diagnostics

Errors mean the operation cannot safely continue. Diagnostics are versioned records with code, severity, source/output identity, optional span, explanatory fields, and remediation. Human text is not a stable API; diagnostic codes are.

Expected error families include unsafe path, unsupported archive, resource limit, malformed input, identity mismatch, plan stale, proposal invalid, approval stale, output exists, verification failed, and platform nondeterminism.

## Extension rules

Public extension points use traits and serializable schemas. Extensions MAY suggest candidates or proposals, but MUST NOT bypass planning, validation, approval, or safe materialization. Unknown enum variants and fields must be handled according to declared protocol compatibility rather than ignored silently.
