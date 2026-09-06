---
title: Framework Architecture
status: normative
owners:
  - architect
  - core-rust-engineer
last_updated: 2026-09-06
decision_refs:
  - ADR-0001
  - ADR-0002
  - ADR-0004
  - ADR-0005
  - ADR-0019
  - ADR-0021
  - ADR-0022
  - ADR-0023
  - ADR-0024
  - ADR-0025
  - ADR-0026
  - ADR-0027
source_refs:
  - HIST-COMPILER-PLAN
---

# Framework Architecture

## Dependency direction

```text
        okc CLI/TUI       Python       Node.js
             |               |            |
             |          thin PyO3    thin napi-rs
             |               +-----+------+
             |                     |
             +------------- okc-interop
                                   |
                                okc-app <---- okc-ai
                                   |
                                okc-core
                                   |
                     filesystem / SQLite / providers
```

Dependencies point inward. `okc-core` MUST NOT import CLI, terminal,
language-runtime, native-keychain, updater, MCP, hosted-vendor SDK, or web
framework concepts. MCP and Obsidian integrations remain future thin adapters.

## Rust workspace

All workspace packages have version `0.3.0`.

| Package | Responsibility |
|---|---|
| `okc-core` | `CorpusBuilder`, current integration DTOs and validators, provider-free directory compile/verify/explain |
| `okc-ai` | provider profiles, portable request/response schemas, bounded transports, capability checks |
| `okc-app` | Schema 3 projects, journal, source bindings, provider/credential services, disclosure, review, worker, artifact service, updater |
| `okc-interop` | runtime-neutral API-v1 facade, interop-schema-2 DTOs/errors, explicit-path jobs and scheduling |
| `okc` | thin Clap CLI and Ratatui/Crossterm TUI |
| `okc-python` | PyO3 `abi3-py311` native adapter |
| `okc-node` | napi-rs Node-API 9 native adapter |

There is no legacy compiler, protocol crate, deprecated facade, or command
provider on main. A command provider may return only through a separately
reviewed current-schema adapter.

## Core boundaries

The public source-to-corpus boundary is:

```text
SourceSpec[] -> CorpusBuilder::build -> PreparedCorpus
```

`CorpusBuilder` privately reuses the stable source descriptor, snapshot,
Markdown/frontmatter parser, canonical policy, deduplication/planning, resource
limit, archive/path safety, and optional SQLite workspace modules. Intermediate
inspection and draft-plan records are not public API. The default policy,
inspection order, identity/hash domains, corpus sealing, and workspace schema
are byte-compatibility constraints.

The public integration state machine is:

```text
PreparedCorpus
  -> sensitive preflight
  -> recorded embeddings/candidates
  -> TaxonomyProposal -> taxonomy approval
  -> SynthesisProposal -> CriticReport -> ClusterApproval
  -> ApprovedIntegrationPlan
  -> compile -> CompiledVaultManifest
  -> verify / explain -> ProvenanceRecord
```

Providers propose data but cannot approve, mutate source bytes, or publish.
Every transition validates current hashes and appends journal state or an
immutable object. A source, route, policy, prompt, schema, taxonomy, proposal,
critic, or decision change creates a new identity and makes dependent
authority stale.

## Application state and ownership

`okc-app` owns `Name.okc-project/manifest.json`, `state.sqlite3`, immutable
`objects/`, `workspace/build.sqlite3`, and `project.lock`. Project/artifact JSON
uses Schema 3; the private append-only application journal remains schema 4;
the retained build-workspace identifiers remain unchanged. SQLite uses WAL,
foreign keys, `FULL` synchronous mode, explicit migration, and private Unix
permissions.

- Sources own immutable original bytes.
- The sealed corpus owns canonical parsed content and stable identities.
- Provider recordings own untrusted proposals, never authority.
- Approval records own exact hash-bound authority.
- An approved integration plan owns the complete offline materialization recipe.
- A compiled artifact owns output plus audit metadata, not source copies.

Project data is plaintext. Applications MUST warn about likely shared/network
locations and MUST NOT imply encryption.

## Artifact boundary

`ArtifactService` accepts a current Schema 3 directory and exposes only:

```text
verify(path) -> CompiledVaultManifest
explain(path, output_path) -> ProvenanceRecord
```

Detection occurs before full decoding. Retired recognizable schemas receive a
structured unsupported error. Mixed markers, symlinked roots/markers/manifests,
malformed or oversized manifests, unknown families, corrupt bytes, and
unrecognized files fail closed.

## Extension and error rules

Public extensions use typed traits and serializable schemas. They MAY suggest
candidates or proposals but MUST NOT bypass validation, review, approval,
provenance, or safe publication. Human text is not a stable API; language
clients branch on structured error code/category and typed result fields.
