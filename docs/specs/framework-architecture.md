---
title: Framework Architecture
status: normative-v1
owners:
  - architect
  - core-rust-engineer
last_updated: 2026-09-05
decision_refs:
  - ADR-0001
  - ADR-0002
  - ADR-0004
  - ADR-0005
  - ADR-0013
  - ADR-0015
  - ADR-0017
  - ADR-0019
  - ADR-0021
  - ADR-0022
  - ADR-0023
  - ADR-0024
  - ADR-0025
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
         |       `okc` CLI and TUI      |
         +---------------+----------------+
                         |
       +-----------------+------------------+
       | inspect -> IR -> plan -> validate  |
       | -> approve -> compile -> verify    |
       +-----------------+------------------+
                         |
       filesystem / SQLite workspace / pack

Provider process/HTTP -> `okc-ai` -> recorded untrusted proposals
External MCP engines -> adapters -> derived candidates only
```

Dependencies point inward. The compiler core MUST NOT import Obsidian, MCP, hosted LLM, web-framework, or vendor SDK concepts.

## Rust workspace

| Package | State | Responsibility |
|---|---|---|
| `okc-core` | implemented public library | V2 deterministic compiler plus provider-free V3 integration validation, offline canonical/stub materialization, and independent directory verification |
| `okc-protocol` | implemented public library | frozen schema-2 augmentation plus strict schema-3 command-provider envelopes |
| `okc-ai` | implemented application-side library | semantic structured-generation/embedding traits, portable schema validation, injected zeroized credentials, bounded synchronous HTTP, and initial vendor adapters |
| `okc-app` | implemented application library | schema-3 projects, cwd/Vault discovery, provider/keychain services, immutable objects, append-only run/task/exchange/cluster/approval/plan journal, shared integration orchestration, sensitive preflight/routing, updater and worker control types |
| `okc` | implemented sole binary | thin Clap CLI and Ratatui/Crossterm TUI adapters over `okc-app`, human/JSON output, and supervised subprocess provider execution |
| `vaultc` | deprecated facade, no binary | one-minor Rust compatibility aliases over `okc-core` |
| internal V1/V2 readers | private compatibility packages | frozen schema-1 and read-only schema-2 verify/explain boundaries and literal goldens only |
| `okc-mcp` | future adapter | MCP tools that call public library operations |
| `okc-memory` | future experimental | calibrated memory/retrieval models isolated from file compilation |

Circular dependencies are forbidden. `okc-core` may depend on protocol data
types but MUST NOT launch provider processes or make HTTP requests. Process
supervision and HTTP belong to `okc-ai`, `okc-app`, or the `okc` binary.

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
- `materialization`: derive effective operations and `MaterializationId` from the immutable plan, typed action set, and approved proposal set.
- `compile`: stage, materialize, checksum, and atomically publish.
- `pack`: verified deterministic OKCPack creation, atomic no-replace file
  publication, and safe extraction for verification.
- `provenance`: emit and explain exact output-to-source derivations.
- `verify`: independently verify manifest, paths, hashes, sealed audit linkage, approvals, and provenance closure.
- `workspace`: bounded SQLite-backed intermediate state.
- `integration`: schema-3 corpus/taxonomy/disposition/evidence/critic/approval
  validation plus provider-free canonical-note and redirect-stub materialization.

## State machine

```text
Sources
  -> InspectedSnapshotSet
  -> CanonicalWorkspace
  -> DraftPlan
  -> [AugmentationTranscript + Proposals]
  -> ValidatedProposals
  -> DecisionOverlay + ApprovedPlan
  -> MaterializationPlan
  -> StagedOutput
  -> CompiledVault
  -> VerifiedArtifact
```

Operations MUST reject inputs from the wrong state. Any change to source hashes, compiler semantic version, policy configuration, or proposal payload invalidates dependent approvals.

The schema-3 state machine is:

```text
Immutable corpus -> sensitive preflight -> recorded embeddings/candidates
  -> taxonomy proposal -> taxonomy approval
  -> per-cluster synthesis -> critic -> cluster approval
  -> ApprovedIntegrationPlan -> offline materialization -> verification
```

Every transition appends a journal event or immutable object. A source, policy,
route, prompt, schema, taxonomy, proposal, critic, or manual-section change
creates a new dependent identity; it never rewrites earlier authority.

## Data ownership

- Raw snapshots own original bytes.
- Canonical IR owns normalized semantics and source spans.
- A plan owns deterministic output paths, candidates, and base operations.
- Decision and approval logs own authorization; a materialization owns the exact effective operations without mutating the plan.
- A compiled artifact owns only materialized output plus its audit metadata.
- Search indexes and MCP engines own disposable derivatives.

## Persistence

`okc-app` owns `Name.okc-project/manifest.json`, `state.sqlite3`, immutable
`objects/`, `workspace/build.sqlite3`, and the single-writer `project.lock`.
SQLite uses WAL, foreign keys, `FULL` synchronous mode, deterministic
transactions, explicit migration through private `user_version = 4`, and private Unix
permissions. The build workspace has its own core schema and MUST NOT be
initialized with the application-state tables. A source rebind, route, policy,
or language change appends an invalidation event and creates a new run identity;
prior task, exchange, revision, and approval rows remain immutable history.
Project and artifact JSON remain schema 3. State schema 4 appends source-set
revisions and approved-integration-plan object pointers rather than rewriting
historical runs.

Project data is plaintext. Applications MUST warn for likely shared/network
locations and MUST NOT imply encryption.

Large blobs SHOULD be streamed from sources and MUST NOT be duplicated in
memory. The `0.2.0` scanner currently accumulates bounded source bytes in memory
before parsing and therefore does not yet meet this target at the 20 GB
reference workload. Database writes SHOULD be batched in deterministic
primary-key order.

## Errors and diagnostics

Errors mean the operation cannot safely continue. Diagnostics are versioned records with code, severity, source/output identity, optional span, explanatory fields, and remediation. Human text is not a stable API; diagnostic codes are.

Expected error families include unsafe path, unsupported archive, resource limit, malformed input, identity mismatch, plan stale, proposal invalid, approval stale, output exists, verification failed, and platform nondeterminism.

## Extension rules

Public extension points use traits and serializable schemas. Extensions MAY suggest candidates or proposals, but MUST NOT bypass planning, validation, approval, or safe materialization. Unknown enum variants and fields must be handled according to declared protocol compatibility rather than ignored silently.
