---
title: ADR-0027 — Current Schema Single Source
status: normative
owners:
  - architect
  - release-maintainer
last_updated: 2026-09-06
decision_refs:
  - ADR-0018
  - ADR-0022
  - ADR-0026
  - ADR-0027
source_refs:
  - HIST-CURRENT-PLAN
---

# ADR-0027: Current Schema Single Source

## Status

Accepted on 2026-09-06. This decision supersedes ADR-0018, supersedes the
legacy-reader and project-upgrade portions of ADR-0022, and amends ADR-0026's
artifact dispatch and interop DTO version. The retained Schema 3 format and
approval decisions in ADR-0022 remain in force.

## Context

The main branch carried three generations of compiler code, two retired
artifact readers, a deprecated facade, and public names decorated with a `V3`
suffix. That made retired behavior look current, duplicated safety policy, and
left the CLI and language libraries claiming compatibility that the product no
longer intends to maintain. The current Schema 3 artifact and project bytes are
already the implementation contract and must not change as a side effect of
source cleanup.

## Decision

The main branch contains only the current Schema 3 product path. Schema 1 and
Schema 2 compiler implementations, mutable workflows, readers, fixtures,
protocol crates, and compatibility aliases are removed. The deprecated
`vaultc` facade is removed. No legacy reader, migration, deprecation shim, or
project upgrade is provided.

The removed sources remain recoverable from annotated archive tags:

- `archive/v0.1.0` points to commit
  `7181fc2dea54288f176b66a00e2335da7f58bdfd`;
- `archive/v0.2.0` points to commit
  `b9f9e88bc531095fbeb2ece4155c980bdf10708b`.

These are archive tags, not releases. They do not authorize a GitHub Release or
package publication.

Current public Rust types and methods use generation-neutral names. The former
inspection and draft-planning layers are private implementation details behind
`CorpusBuilder::build`, which returns a sealed `PreparedCorpus`. Snapshot,
Markdown parsing, archive/path/resource safety, canonical policy encoding,
inspection order, corpus sealing, and SQLite workspace behavior remain intact.
All retained workspace crates use product version `0.3.0`.

The current provider kind is limited to OpenAI, Anthropic, Gemini, Ollama, and
OpenAI-compatible HTTP. It contains no reserved command value or command
execution adapter.

The CLI exposes the current project/provider/integration/review/TUI workflow,
`compile`, directory-only `verify`, directory-only `explain`, `doctor`, and
`update`. It does not expose retired inspect, plan, augmentation, replay,
validation, approval, Pack, policy, workspace, pagination, or project-upgrade
shapes. Compile accepts either a current project or an explicit approved
integration plan.

`ArtifactService` is a typed current-schema boundary:

- `verify(path) -> CompiledVaultManifest`;
- `explain(path, output_path) -> ProvenanceRecord`.

Recognizable Schema 1 or Schema 2 markers and Pack suffixes fail with
`ARTIFACT_SCHEMA_UNSUPPORTED`, `supported_schema = 3`, and detected schema and
format-family details. Mixed, symlinked, malformed, oversized, corrupt, or
unrecognized artifacts continue to fail closed as verification errors.

The Python and Node.js DTO boundary advances to
`interop_schema_version = 2`. Verification returns
`{interop_schema_version, valid, artifact_path, manifest}` and explanation
returns `{interop_schema_version, artifact_path, record}`, using snake_case in
Python and camelCase in Node.js. Python requires
`explain_artifact(path, *, output_path=...)`; Node.js requires
`explainArtifact(path, {outputPath})`. This is an intentional breaking DTO
change and no schema-1 interop alias is provided.

## Preserved contracts

This source cleanup MUST NOT alter an existing Schema 3 project or artifact.
Schema 3 JSON fields, IDs, hash domains, SQLite identifiers, project journal
schema 4, output paths, and output bytes remain unchanged. Contractual version
literals such as `schema_version = 3`, `okc:*:v3\0`, `_v3` database names,
`policy-v3`, prompt/schema/cache revisions, the reserved Pack profile,
`.okc/legacy/`, external `/v1` APIs, and the keyring feature remain where they
describe stored or external protocols rather than source generations.

## Consequences

- Main has one compiler policy owner and one public artifact family.
- Applications needing retired execution or inspection must deliberately use
  the matching archive tag; current packages do not interpret retired bytes.
- Existing Schema 3 projects open without migration and deterministic fixture
  inventories remain byte-identical.
- Documentation and tests use temporary retired markers only to verify the
  explicit unsupported error; committed retired artifact fixtures are gone.
- A future command-provider or Pack implementation must enter through the
  current contract and a separately reviewed decision.
