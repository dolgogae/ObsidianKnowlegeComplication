---
title: Testing and Quality Gates
status: normative
owners:
  - qa-security-engineer
  - release-maintainer
last_updated: 2026-09-06
decision_refs:
  - ADR-0003
  - ADR-0004
  - ADR-0012
  - ADR-0014
  - ADR-0019
  - ADR-0020
  - ADR-0022
  - ADR-0024
  - ADR-0025
  - ADR-0026
  - ADR-0027
source_refs:
  - HIST-COMPILER-PLAN
---

# Testing and Quality Gates

No stable-release claim is allowed until every required gate passes on its
declared matrix. A local macOS arm64 pass is development evidence, not
cross-platform release evidence.

## Gates

| Gate | Requirement |
|---|---|
| QG-001 Functional | current source, project, provider, review, compile, verify, explain, CLI/TUI, and language API behavior matches specifications |
| QG-002 Determinism | identical approved inputs produce identical Schema 3 IDs, paths, manifests, provenance, and every file byte across repetitions, source order, absolute roots, and supported hosts |
| QG-003 Provenance | every output record closes over exact source evidence, proposal, critic, approval, and plan; forgery/staleness fails |
| QG-004 Safety | hostile directories, archives, symlinks, paths, JSON/Markdown, provider responses, output races, and cancellation fail safely |
| QG-005 Schema boundary | current Schema 3 projects open without migration; recognizable Schema 1/2 inputs get only structured unsupported errors; unknown/mixed/corrupt inputs fail closed |
| QG-006 Performance | 10 Vaults, 100,000 notes, and 20 GB meet the accepted time/RSS budget with semantic candidate evidence |
| QG-007 Documentation | current state, requirements, algorithms, ADRs, traceability, guides, help, and relative links agree |
| QG-008 Supply chain | locked dependencies, audits, licenses, checksums, SBOMs, attestations, clean installs, and required native signing pass |

## Required current-schema coverage

Automated tests MUST cover:

- immutable source bytes and metadata; directory, ZIP, and `tar.zst` safety;
- traversal, absolute/reserved names, symlinks, duplicate source IDs/content,
  excluded `.obsidian`/`.git` data, bounds, and SQLite persistence;
- `CorpusBuilder` order invariance and deterministic sealed corpus/block maps;
- complete taxonomy, dispositions, evidence, contradiction, critic, omission,
  waiver, approval, recording, and stale-hash closure;
- provider profile/schema/bounds/credentials/disclosure, transport error
  normalization, cancellation, and deterministic recording/resume;
- two-directory compile equality, no-clobber publication, missing authority,
  materialized-byte regeneration, manifest/checksum/provenance integrity;
- ArtifactService root/marker/manifest symlinks, mixed markers, malformed and
  oversized manifests, corrupt content, and safe output-path explanation;
- current project schema opening and private journal schema behavior without
  migration;
- CLI help and parse rejection proving retired commands/options do not return;
- Python and Node.js type declarations, structured errors, scheduler lifecycle,
  consent, output safety, source immutability, and complete current workflow.

The 2026-09-06 correctness audit adds mandatory regressions for:

- multiline Markdown sections, portable Unicode/prefix/file-directory output
  collisions, strict source-ID deserialization, and conflicting metadata slots;
- unplanned files with attacker-resealed manifests/checksums, core verifier
  root aliases, source/workspace overlap, project/object/database symlinks;
- language/route/taxonomy/regeneration invalidation, current-plan-bound verified
  outputs, and stage/request/provider/candidate-sensitive cache identity;
- PEM markers, exact UTF-8 email ranges, metadata-only sensitive documents,
  spoofed loopback DNS names, nested duplicate provider JSON, total retry deadlines;
- full progress/work queues, non-blocking shutdown, cross-client project
  exclusion, reservation release before completion, and unmodified arbitrary
  Node.js metadata keys;
- a one-component relative CLI output path and library-only feature builds.

Concrete test-to-requirement mappings are in
[`TRACEABILITY.md`](../TRACEABILITY.md); findings and validation scope are in
the [audit report](../history/summaries/2026-09-06-correctness-audit.md).

The stabilization follow-up also requires managed-file/sidecar hardlink
rejection, source root/leaf/ancestor replacement tests, ambient-ignore
independence, journal/manifest failure ordering, bottom-up staging durability,
deterministic publish-barrier winners, distinct-plan concurrency, cleanup error
reporting, and preserved post-commit output. Fixed-seed JSON/ZIP mutations and
source creation/root/order properties run with the ordinary Rust suite.

`python3 tests/tui_pty_smoke.py target/debug/okc` exercises the POSIX terminal
against a bounded synthetic loopback provider: provider-free preflight,
explicit taxonomy/cluster approvals, compile, independent verify, restart,
immutable source bytes/mtime, and terminal restoration. It does not qualify
Windows ConPTY, remote vendors, long cancellation, or process-kill injection.

Retired artifacts MUST NOT be committed as active fixtures. Tests create
minimal temporary `.vaultc`, `.vaultpack`, Schema 2 `.okc`, and `.okcpack`
markers, then assert `ARTIFACT_SCHEMA_UNSUPPORTED`, `supported_schema = 3`, and
the exact detected schema/family through Rust service, CLI, Python, and Node.js.
Mixed, symlinked, malformed, oversized, and unknown inputs remain ordinary
fail-closed verification errors.

## Byte invariant

The language-binding fixture inventory is a cross-surface golden. Rust,
Python, and Node.js MUST independently compile the current fixture and produce
SHA-256:

```text
452ca0671e806a93b4f36f218cf9e62da899f6404c74705c2cf0ca14e413c7e5
```

The inventory includes sorted relative paths and each file's exact bytes. A
source-only cleanup MUST additionally compare manifest, approved plan,
provenance, file paths, IDs/hashes, and all output bytes before/after. Any
difference is a compatibility failure unless an accepted format ADR explicitly
authorizes it.

## Rust checks

The pinned Rust 1.97.1 toolchain MUST pass:

```text
cargo check --locked --workspace --all-targets --all-features
cargo test --locked --workspace --all-features --no-fail-fast
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
cargo tree --workspace
```

The optional core features MUST also compile independently; check the library
with `--no-default-features`, then with only `archives`, and only `sqlite`.

Source hygiene MUST show no removed workspace member, crate dependency,
generation-suffixed public symbol, retired CLI implementation, committed
retired fixture, or deprecated facade. Contractual stored/protocol literals
listed in ADR-0027 are excluded from cosmetic hygiene checks.

## Python checks

Against a freshly built native extension and clean environment:

```text
pytest bindings/python/tests
mypy --strict bindings/python/tests/typing_contract.py
maturin build --release --locked
maturin sdist
```

The wheel and source distribution MUST install without repository imports;
`import okc`, API info, interop schema 2, native loading, and a bounded smoke
operation MUST work. Stubs must expose typed `VerificationResult` and
`ExplanationResult`, and `output_path` is required for explanation.

Release wheel builds MUST set `SOURCE_DATE_EPOCH` to the selected source
commit's Unix timestamp and compare repeated wheel bytes, including the
embedded SBOM. The pinned Maturin otherwise includes a fresh SBOM UUID and
timestamp. Repeating packaging against cached native output is a useful local
regression, not a substitute for independent clean-build/native CI evidence.

## Node.js checks

Against a freshly built native addon and clean package install:

```text
npm run build
npm test
npm run typecheck
npm pack --dry-run
```

CommonJS and ESM imports, Node-API loading, interop schema 2, declarations,
typed verify/explain results, and mandatory `{outputPath}` MUST pass. A root
package and applicable platform addon must install together without source-tree
fallback.

## Documentation and release checks

The repository-relative Markdown test and `guide` production build MUST pass.
The active specifications and guides MUST contain no retired execution or
compatibility promise. ADR bodies and append-only history remain historical
records and may name old generations; ADR-0027 markings define current
precedence.

SDK CI covers CPython 3.11–3.14, Node.js 22.13/current, and Linux x86_64 GNU,
Windows x86_64 MSVC, macOS x86_64, and macOS arm64 native artifacts. A release
requires remote success, reproducibility evidence, checksums/SBOMs, protected
publication, and signing/notarization where applicable.

## Fuzz, property, and performance backlog

Before stable release, fuzz/property campaigns must cover archive paths and
sizes, Markdown/frontmatter/Canvas, JSON schemas, provenance, manifests,
publisher races, and cancellation. The full performance fixture must report
wall time, peak RSS, accepted bytes/files, candidate counts, provider token
cost, and failure slices. The current bounded in-memory source accumulation is
a known blocker for the 20 GB gate.

The provider-free `corpus_probe` example generates only disposable fixtures
under hard source limits. Build with `cargo build --locked --release -p okc-core
--example corpus_probe`, then run the platform RSS tool and
`corpus_probe SOURCES NOTES BYTES_PER_NOTE` to record exact counts, source
bytes, corpus hash, and build time. It MUST report `qg_006_pass: false`: a
small-byte 100k-note ingestion probe is not the 20 GB semantic workload. The
historical V1 20-minute/2-GB target and the unresolved current reference-machine/
budget adoption are recorded in `history/OPEN_QUESTIONS.md`; no relaxed
threshold is inferred.
