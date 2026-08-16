---
title: Current State
status: normative-v1
owners:
  - release-maintainer
last_updated: 2026-08-16
decision_refs:
  - ADR-0001
  - ADR-0003
  - ADR-0004
  - ADR-0009
source_refs:
  - HIST-CURRENT-PLAN
---

# Current State

## Snapshot: 2026-08-16

The repository now contains a working `0.1.0` Rust framework and CLI. The
implemented vertical slice covers deterministic inspection through independent
verification, including the provider-neutral proposal/approval path. It is a
development implementation, not a cross-platform V1 release.

Implemented production packages:

- `vaultc`: safe snapshotting, canonical IDs/IR, Markdown and Canvas parsing,
  exact and review-only near deduplication, conflict/path planning, source-aware
  Markdown rewrites, immutable approvals, atomic compilation, provenance,
  deterministic packing, independent verification, and SQLite workspace state;
- `vaultc-protocol`: versioned provider capabilities, projections, evidence,
  proposals, and transcript records without a vendor SDK dependency;
- `vaultc-cli`: `inspect`, `plan`, `augment`, `approve`, `compile`, `verify`,
  and `explain`, including a bounded NDJSON subprocess provider.

The compiler accepts directories, ZIP, `tar.zst`, and `.tzst` sources. It
rejects or excludes traversal, duplicate archive members, archive expansion
bombs, external links, special files, named secret files, executable classes,
malformed structured content, and unsafe portable paths. Source Vaults remain
unchanged. A build publishes only a new Compiled Vault and optional
deterministic `.vaultpack`; source locations are redacted from artifact audit
data.

AI is not required. The implemented augmentation types are
`create_generated_note` and `explain_conflict`. Proposals are bound to a sealed
plan/projection/evidence set, validated as untrusted data, and materialized only
after an explicit content-hash-bound approval. Conflict decisions are immutable
approval overlays. V1 permits only `waived_by_policy`; it does not mislabel an
ambiguous link as user-resolved without a typed target/rewrite action.

## Verification evidence

All commands below passed on macOS arm64 with Rust 1.97.1:

| Check | Result |
|---|---|
| `cargo test --workspace --all-features --no-fail-fast` | 59 tests and all doctests passed |
| `cargo clippy --workspace --all-features --all-targets -- -D warnings` | passed with zero warnings |
| `cargo fmt --all -- --check` | passed |

The 59 tests comprise 15 `vaultc` unit tests, 3 pack integration tests, 8
pipeline tests, 4 provider/approval tests, 15 security tests, 8 CLI unit tests,
5 CLI integration tests, and 1 protocol test. They cover, among other cases:

- source immutability, deterministic plan/output, absolute-source-location
  independence, and byte-identical VaultPacks on one supported host;
- exact note and attachment provenance, Markdown link rewrites, portable path
  collisions, stale sources, and tampered sealed plans;
- explicit proposal approvals, evidence binding, generated-frontmatter
  injection resistance, conflict waiver binding, and transcript audit closure;
- ZIP/tar traversal and links, duplicate ZIP members, decompression ratio,
  malformed UTF-8/JSON, resource limits, exclusions, output no-clobber, and
  independently resealed artifact tampering;
- CLI/SDK plan parity and the full plan → approve → compile → pack → verify →
  explain lifecycle;
- provider deadline and SIGINT cancellation while input is blocked, bounded
  shutdown, process-group reaping, and non-publication of partial augmentation.

The exact requirement-to-test mapping is in
[`TRACEABILITY.md`](TRACEABILITY.md).

## Quality-gate status

| Gate | State | Evidence or remaining work |
|---|---|---|
| QG-001 Functional | implemented on macOS arm64 | current automated suite is green; the supported-platform matrix and remaining parser/Canvas gaps are not complete |
| QG-002 Determinism | implemented on one platform | same-host bytes and absolute-location independence pass; Linux/Windows/toolchain comparison remains |
| QG-003 Provenance | partial | content outputs have exact source/proposal linkage and audit comparison; the full ALG-PRV-001 typed graph, administrative-file closure, record/edge IDs, and attribution nodes are missing |
| QG-004 Safety | implemented corpus green | current hostile-input and control-file tests pass; fuzz/property campaigns remain |
| QG-005 Compatibility | documented | no migration/version compatibility matrix is implemented yet |
| QG-006 Performance | not verified | the 100,000-note/20 GB/20-minute/2 GB RSS benchmark has not run |
| QG-007 Documentation | passed for this change | current state, traceability, ADR-0009, specs, and append-only decision log are updated; all relative links across 60 Markdown files resolve |
| QG-008 Supply chain | partial | dual licenses and `Cargo.lock` exist; audit policy, SBOM, release provenance, signing, and clean-room release automation remain |

## Known implementation gaps

- Canvas JSON and file-reference records are parsed and unknown JSON is
  preserved, but Canvas references are not yet resolved or rewritten. Therefore
  REQ-PAR-002 is only partially implemented.
- Markdown/frontmatter/wikilink/embed/ordinary-link parsing and byte-span
  rewrites exist, but the complete normative Markdown/Canvas golden corpus and
  all platform-normalization vectors are not yet present.
- Input paths are stored after NFC normalization. The original NFD/NFC spelling
  is not retained, so some cross-source normalization collisions are typed as
  `PATH_EXACT` and raw-path provenance is incomplete.
- Provenance covers implemented content operations and exact duplicate source
  closure, but not the full ALG-PRV-001 typed graph, compiler-generated
  administrative files, record/edge IDs, decision/approval nodes, or
  author/license attribution. Arbitrary non-block evidence spans are currently
  bounds-checked rather than rehashed as exact span bytes.
- Some source and pack members are buffered under hard limits. The V1 20 GB
  workload and ≤2 GB RSS target cannot be claimed until streaming and the
  reference benchmark are verified.
- Only macOS arm64 has been exercised in this workspace. Linux, Windows, macOS
  x86_64, filesystem normalization, and deterministic cross-platform CI remain.
- `.vaultpack` is deterministic and checksum/audit-verified but unsigned. The
  signing profile, key lifecycle, revocation, SBOM, and release artifacts are
  future work.
- Schema migrations, resume semantics, workspace encryption policy, broader
  property/fuzz testing, and an interrupted-materialization fault-injection
  harness remain.
- CLI `compile --pack` stages both results, rejects destinations already
  observed to exist, and publishes the pack file with no-clobber semantics, but
  the Compiled Vault and pack are not one combined filesystem transaction. A
  pack failure after Vault publication can leave the valid Compiled Vault.
- Source opening is rechecked but not yet descriptor-relative/no-follow, pack
  extraction lacks an outer compressed-to-expanded ratio, and portable
  no-clobber directory publication is not proven race-free on every platform.
- Source archive compressed-byte size and the count of every visited member
  (including excluded/non-file entries) are not separately bounded. Nested
  archives are treated as opaque rather than recursively extracted.
- The artifact manifest is still minimal: media types, license/attribution
  summaries, creation-policy/distribution metadata, and signature metadata from
  the target format are not implemented. Verification checks internal
  consistency but does not rederive rewritten output bytes from immutable
  source bytes stored outside the artifact.
- The public SDK pack writer writes directly to its destination; atomic
  no-clobber pack staging is currently a CLI-only wrapper. There is no installer
  or permission/license/signature display surface.
- Typed conflict actions are not modeled yet. `user_resolved` and
  `provider_suggested` remain reserved states; V1 only accepts an explicit
  policy waiver overlay for a required conflict.
- MCP, Obsidian plugin, registry/marketplace, claims, benchmarking, and every
  ALG-MEM experimental or research-only algorithm remain unimplemented and do
  not affect the compiler path.
- There is no CI workflow or published release artifact yet.

## Next implementation slices

1. Complete ALG-NRM-001 Canvas reference resolution/rewriting and the normative
   Markdown/Canvas golden corpus.
2. Add Linux, Windows, and macOS matrix CI plus cross-platform semantic and
   artifact determinism fixtures.
3. Stream large blobs and run QG-006 at the full reference workload.
4. Define schema migrations and run property/fuzz and fault-injection suites.
5. Complete QG-008 release automation, SBOM/provenance, and a versioned signing
   ADR before calling a pack signed or marketplace-ready.

No MCP, plugin, marketplace, or neuroscience-inspired runtime behavior should
enter the default compiler path while these stable V1 gates remain open.

## Status vocabulary

- `documented`: contract exists but no production code is present.
- `implemented`: production code exists, but one or more required verification
  gates may remain.
- `verified`: all verification named for that scoped behavior passes on the
  stated platform matrix.
- `blocked`: a named external decision or dependency prevents progress.

Status always applies to the exact scope and evidence stated here; it is not a
release-quality claim by itself.
