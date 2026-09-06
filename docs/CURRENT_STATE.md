---
title: Current State
status: normative
owners:
  - release-maintainer
last_updated: 2026-09-06
decision_refs:
  - ADR-0019
  - ADR-0020
  - ADR-0021
  - ADR-0022
  - ADR-0023
  - ADR-0024
  - ADR-0025
  - ADR-0026
  - ADR-0027
source_refs:
  - HIST-CURRENT-PLAN
---

# Current State

## Snapshot: 2026-09-06

The repository contains one current Schema 3 `0.3.0` development product. Main
has no Schema 1/2 compiler, reader, writer, migration, public alias, deprecated
`vaultc` facade, retired augmentation protocol, or command-provider kind or
implementation. Previous sources remain recoverable from annotated archive
tags, not runtime compatibility packages.

The current Markdown-directory integration path and typed Python/Node.js
bindings are locally implemented. This is not a complete or stable release:
semantic scale, non-Markdown materialization, current OKCPack, provider-backed
PTY, fuzz/TOCTOU, performance, remote native matrices, and native signing
evidence remain open. No language package has been published.

## Workspace and implementation

All retained crates use workspace version `0.3.0`:

- `okc-core`: private hostile-input snapshot/Markdown/analysis/workspace stages
  behind `CorpusBuilder::build -> PreparedCorpus`; public current integration
  DTO validation and provider-free `compile`, `verify`, and `explain`;
- `okc-ai`: provider-neutral structured generation/embedding, portable Schema
  3 validation, bounded HTTP adapters, redacted credentials;
- `okc-app`: current projects, private append-only journal schema 4, immutable
  objects, source/provider routes, disclosure, review, artifact verification,
  workers, native credentials, and updater services;
- `okc-interop`: runtime-neutral API-v1 client/project/jobs/errors using interop
  DTO schema 2;
- `okc`: sole current CLI/TUI;
- `okc-python` and `okc-node`: thin typed native adapters.

Existing Schema 3 projects open without migration. Schema 3 artifact JSON,
identity/hash domains, project and SQLite identifiers, journal schema 4,
output paths, and deterministic output bytes remain unchanged. Contractual
stored/protocol version literals listed in ADR-0027 remain intentionally.

## Current public boundary

Rust public names are generation-neutral: `SourceBlock`, `MetadataValue`,
`ManifestFile`, `CompiledVaultManifest`, `CompiledArtifact`,
`ProvenanceRecord`, `DataBoundary`, and `ProviderCapabilities`. Current
integration operations are `compile`, `verify`, `explain`, and
`ProjectStore::compile`.

The CLI exposes project/provider/integrate/integration/review/TUI, compile,
directory verify/explain, doctor, and update. Compile consumes the selected
project's latest approved plan or `--integration-plan`. Retired phase commands,
project upgrade, policy/workspace globals, positional approved plans, Pack, and
explanation pagination/package options are absent and parse-tested.

`ArtifactService` exposes only typed current operations:

```text
verify(path) -> CompiledVaultManifest
explain(path, output_path) -> ProvenanceRecord
```

Recognizable Schema 1/2 markers and Pack suffixes return
`ARTIFACT_SCHEMA_UNSUPPORTED` with `supported_schema = 3` plus detected
schema/family. Mixed markers, root/marker/manifest symlinks,
malformed/oversized manifests, unknown schemas, corrupt files, and unsafe
output paths fail closed.

Python exposes snake_case results and requires
`explain_artifact(path, *, output_path)`. Node.js exposes camelCase and requires
`explainArtifact(path, {outputPath})`. Their exact typed results are:

```text
VerificationResult = {interop_schema_version, valid, artifact_path, manifest}
ExplanationResult  = {interop_schema_version, artifact_path, record}
```

with camelCase keys in Node.js. No generic artifact-family/payload DTO remains.

## Archive source recovery

The annotated tags were created and pushed to `origin`:

| Tag | Peeled commit |
|---|---|
| `archive/v0.1.0` | `7181fc2dea54288f176b66a00e2335da7f58bdfd` |
| `archive/v0.2.0` | `b9f9e88bc531095fbeb2ece4155c980bdf10708b` |

They are not release tags and have no GitHub Release or package publication.

## Deterministic evidence

The current Rust, Python, and Node.js fixtures independently produce artifact
inventory SHA-256:

```text
452ca0671e806a93b4f36f218cf9e62da899f6404c74705c2cf0ca14e413c7e5
```

This golden covers the sorted artifact paths and exact bytes, including
manifest, approved plan, provenance, IDs/hashes, canonical notes, redirects,
and checksums. The cleanup is required to retain that value.

## Local verification evidence

The final command evidence for this change is recorded here after execution on
macOS arm64 with Rust 1.97.1:

| Command | Result |
|---|---|
| `cargo check --locked --workspace --all-targets --all-features` | passed for all seven `0.3.0` packages |
| `cargo test --locked --workspace --all-features --no-fail-fast` | 86 Rust tests passed; every workspace doc-test target passed |
| `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` | passed for all seven packages with no warning |
| `cargo fmt --all -- --check` | passed |
| Python public API/E2E/typing/wheel/sdist smoke | ABI3 wheel built; 12 tests and strict mypy passed; unpacked sdist rebuilt offline and imported interop schema 2 outside the repository |
| Node.js build/test/declaration/npm Pack smoke | native addon built; 12 tests and strict TypeScript passed; dry-run Pack contained six intended root-package files |
| `npm run docs:build --prefix guide` | VitePress production build passed |
| source hygiene, `cargo tree --locked --workspace --depth 1 --prefix none`, and archive tags | only seven current `0.3.0` packages; removed names/files absent; both annotated remote tags peeled to the ADR-0027 commits |

## Quality-gate status

| Gate | State | Evidence or remaining work |
|---|---|---|
| QG-001 Functional | partial | current Markdown/project/CLI/binding vertical slice passes locally; non-Markdown and scale remain |
| QG-002 Determinism | partial | same-host corpus/order/output golden passes; supported-host repetitions remain |
| QG-003 Provenance | locally verified for current output | exact plan/proposal/critic/approval/evidence records and explanation pass; broader materializers pending |
| QG-004 Safety | partial | current archive/symlink/manifest/provider/publication tests pass; fuzz, portable no-follow, crash/platform evidence remain |
| QG-005 Schema boundary | locally verified | current projects need no migration; temporary retired markers/Pack suffixes return explicit unsupported; no old fixtures/readers remain |
| QG-006 Performance | not passed | 10 Vaults/100,000 notes/20 GB time and RSS report absent; sources remain buffered under bounds |
| QG-007 Documentation | locally verified | all 56 active `normative-v1` statuses normalized; ADR-0027, traceability, active guide build, and repository-relative link test pass |
| QG-008 Supply chain | partial | lockfiles/package definitions exist; remote matrix, two-pass reproducibility, signatures and publication absent |

## Release blockers

- Complete deterministic chunking/batching, HNSW, candidate union, hierarchical
  synthesis, and the 100k semantic cost/RSS report.
- Manual section amendment and persisted sensitive-finding exceptions.
- A separately reviewed supervised current command adapter, if required.
- Attachment/Canvas/Base carry-through, complete link rewriting, and current
  deterministic OKCPack.
- Provider-backed TUI PTY, cancellation/crash injection, supported-platform
  terminal and filesystem evidence.
- Portable descriptor-relative/no-follow source traversal, Windows reparse
  defenses, and full fuzz/property campaigns.
- Four-target native Python/Node package matrix, clean installs, repeated
  same-commit bytes, checksums/SBOM/attestations, protected PyPI/npm workflow.
- Apple Developer ID/notarization, Windows Authenticode/timestamp, and complete
  QG-001–008 release evidence.

Publishing stable `0.3.0` remains prohibited.

## User documentation

- [Quickstart](../guide/index.md)
- [AI integration](../guide/integration.md)
- [CLI](../guide/cli.md)
- [TUI](../guide/tui.md)
- [Python and Node.js](../guide/python-node.md)
- [Troubleshooting](../guide/troubleshooting.md)
