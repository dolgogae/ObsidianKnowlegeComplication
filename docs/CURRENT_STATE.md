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
semantic scale, non-Markdown materialization, current OKCPack, real-provider
and all-host cancellation coverage, fuzz/TOCTOU, performance, remote native
matrices, and native signing evidence remain open. No language package has
been published.

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
The correctness audit versions new private sensitive-scan results explicitly;
it does not rewrite prior preflight objects or approved artifact bytes.

## Correctness audit: 2026-09-06

The [detailed audit](history/summaries/2026-09-06-correctness-audit.md) records
four resolved documentation conflicts and sixteen implementation findings.
Existing ADR-0027 authority was used to reconcile algorithms/roles with current
specifications before code changes resumed; no new architecture or weaker
invariant was approved.

Implemented and regression-tested fixes cover stale plans after configuration/
taxonomy/regeneration changes, current-plan-bound verified outputs, plan-derived
artifact inventories, multiline sections, portable Unicode and file/directory
path collisions, source-ID/metadata/duplicate-JSON validation, and safe workspace/
project/object paths. Provider handling now covers metadata-only sensitive
documents, exact loopback parsing, complete cache identities, and total retry
deadlines. Workers retain completion independently, SDK queues are bounded,
distinct clients share project reservations, and Node.js preserves arbitrary
source metadata keys. Core optional-feature builds and relative CLI publication
also pass.

Scanner revision `okc-sensitive-v3-2` uses private `sensitive-findings-v2`
block/metadata findings without matched secret strings. Old journal/object
history remains append-only. The accepted output golden is unchanged. These
fixes do not close the scale, fuzz/TOCTOU, crash, real-provider, or remote native
matrix gates below.

## Stabilization follow-up: 2026-09-06

The [follow-up record](history/summaries/2026-09-06-stabilization-follow-up.md)
adds 20 Rust regressions and a repeatable POSIX PTY harness. Implemented fixes
cover Unix hardlink aliases in managed files/SQLite sidecars, Linux/macOS
descriptor-relative no-follow source-content opens, ambient-ignore-independent
membership, journal invalidation before manifest replacement, bottom-up stage
directory synchronization, explicit staging cleanup errors, and guard lifetime
through atomic publication. Corpus construction drops an obsolete inspection
copy before building the public projection. Public schema/IDs/golden bytes and
dependencies remain unchanged.

Python wheel and sdist now install with pip in fresh virtual environments, and
root/platform npm tarballs install together outside the repository. The host
Python failure was a Homebrew Expat loader mismatch; a process-local
`DYLD_LIBRARY_PATH=/opt/homebrew/opt/expat/lib` resolves it without changing system
files. Linux x86_64 GNU tests also pass in a Docker container under emulation;
this is Linux development evidence, not a native remote release matrix.

Uncontrolled repeated wheels differed only in generated SBOM metadata and
dependent package records. Setting `SOURCE_DATE_EPOCH` to the source commit
timestamp yields byte-identical repeated wheels, retaining the embedded SBOM;
the corrected wheel also clean-installs and passes the Python suite. SDK CI now
sets that epoch and compares repeated wheels. This cached packaging smoke and
matching repeated sdists do not close independent clean-build reproducibility.

The synthetic-provider TUI PTY workflow passes locally through preflight,
explicit approvals, compilation, independent verification, clean terminal
restoration, and restart without more provider calls. CI now includes that
POSIX smoke; its remote execution is not claimed here.

The ingestion-only probe accepted 10 Vaults/100,000 notes/25,600,000 bytes in
174.569 seconds of corpus construction, with 5,360,336,896 bytes maximum RSS.
It performs zero provider calls and does not measure semantic candidates.
This is **not** the 20 GB gate or an accepted performance result. Source
streaming, semantic scale, the current budget/reference machine, and full
release qualification remain open.

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

The latest command evidence, including the follow-up, is recorded after execution on
macOS arm64 with Rust 1.97.1:

| Command | Result |
|---|---|
| `cargo check --locked --workspace --all-targets --all-features` | passed for all seven `0.3.0` packages |
| `cargo test --locked --workspace --all-features --no-fail-fast` | 130 Rust tests passed; every workspace doc-test target passed |
| Docker Linux x86_64 GNU, same Rust command with `--offline --quiet` | 130 tests and all doc-test targets passed under x86_64 emulation; unchanged cross-language artifact golden |
| `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` | passed for all seven packages with no warning |
| `cargo fmt --all -- --check` | passed |
| `cargo clippy --locked -p okc-core --no-default-features --lib -- -D warnings` plus separate `--features archives` and `--features sqlite` runs | all three feature configurations passed |
| `cargo test --locked -p okc-core --test documentation_contract` | repository-relative Markdown links passed |
| Python public API/E2E/typing/wheel/sdist | fresh ABI3 wheel and sdist built and pip-installed outside repository; 12 tests from each installed distribution, isolated import, and strict mypy passed |
| Node.js build/test/declaration/npm Pack smoke | fresh native addon; 13 tests and strict TypeScript; clean root/platform tarball install, CommonJS/ESM imports, project create/open/manifest smoke passed |
| Repeated wheel/sdist packaging; `bindings/build_artifact_manifest.py` and `shasum -a 256 -c SHA256SUMS` | matching repeated sdists and epoch-controlled wheels; retained/validated SBOMs and five final-distribution checksums passed; cached local builds only |
| `python3 tests/tui_pty_smoke.py target/debug/okc` | local POSIX PTY workflow passed; four synthetic loopback calls, no compile/verify/reopen provider calls, source bytes/mtime unchanged, terminal restored |
| `npm run docs:build --prefix guide` | VitePress production build passed |
| `cargo tree --locked --workspace --depth 1 --prefix none` | seven current `0.3.0` workspace packages; no new dependency or lockfile change |

The audit and follow-up reports record exact native tool paths, temporary
environments, and sandbox/host-tool failures separately from product failures.
Prior failed pip evidence remains historical; the follow-up records successful
clean installs. No remote tag checks, all-platform release success, or protected
publication are inferred from the local evidence.

## Quality-gate status

| Gate | State | Evidence or remaining work |
|---|---|---|
| QG-001 Functional | partial | current Markdown/project/CLI/binding vertical slice passes locally; non-Markdown and scale remain |
| QG-002 Determinism | partial | macOS and emulated Linux golden plus fixed creation/root/order properties pass; remaining native-host repetitions remain |
| QG-003 Provenance | locally verified for current output | exact plan/proposal/critic/approval/evidence records and explanation pass; broader materializers pending |
| QG-004 Safety | partial | deterministic publication barriers, source-handle aliases, hardlinks, mutation smoke and fail-closed manifest ordering pass; full fuzz, managed/output ancestor races, crash/platform evidence remain |
| QG-005 Schema boundary | locally verified | current projects need no migration; temporary retired markers/Pack suffixes return explicit unsupported; no old fixtures/readers remain |
| QG-006 Performance | not passed | 100k-note/25.6-MB ingestion probe: 174.569 s, 5.36 GB RSS; full 20 GB semantic workload and accepted reference budget still missing |
| QG-007 Documentation | locally verified | all 56 active `normative-v1` statuses normalized; ADR-0027, traceability, active guide build, and repository-relative link test pass |
| QG-008 Supply chain | partial | lockfiles/package definitions exist; remote matrix, two-pass reproducibility, signatures and publication absent |

## Release blockers

- Complete deterministic chunking/batching, HNSW, candidate union, hierarchical
  synthesis, and the 100k semantic cost/RSS report.
- Manual section amendment and persisted sensitive-finding exceptions.
- A separately reviewed supervised current command adapter, if required.
- Attachment/Canvas/Base carry-through, complete link rewriting, and current
  deterministic OKCPack.
- All-host provider-backed TUI PTY, long cancellation/process-kill injection,
  and supported terminal/filesystem evidence. The current application still
  announces its non-cancellable phase before the complete core compile call;
  finer pre-publication cancellation remains missing.
- Handle-relative enumeration and managed-state/output ancestor pinning,
  Windows no-follow/reparse/hardlink defenses, and full fuzz/property campaigns.
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
