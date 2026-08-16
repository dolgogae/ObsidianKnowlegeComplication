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
  Markdown rewrites with sealed output commitments and reverse verification,
  typed Canvas reference resolution and rewriting, immutable approvals, atomic
  compilation, provenance, deterministic packing, independent verification,
  and SQLite workspace state;
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
after an explicit content-hash-bound approval. Generated-note approvals also
seal their destination, canonical body/output hashes, ordered EvidenceId list,
and operation ID. Conflict decisions are immutable approval overlays. V1
permits only `waived_by_policy`; it does not mislabel an
ambiguous link as user-resolved without a typed target/rewrite action.

## Verification evidence

All commands below passed on macOS arm64 with Rust 1.97.1:

| Check | Result |
|---|---|
| `cargo test --workspace --all-features --no-fail-fast` | 75 tests and all doctests passed |
| `cargo clippy --workspace --all-features --all-targets -- -D warnings` | passed with zero warnings |
| `cargo fmt --all -- --check` | passed |

The 75 tests comprise 15 `vaultc` unit tests, 5 Canvas integration tests, 4
generated-provenance tests, 3 pack integration tests, 8 pipeline tests, 5
provider/approval tests, 18 security tests, 9 CLI unit tests, 6 CLI integration
tests, and 2 protocol tests. They cover, among other cases:

- source immutability, deterministic plan/output, absolute-source-location
  independence, and byte-identical VaultPacks on one supported host;
- exact note and attachment provenance, Markdown link rewrites, portable path
  collisions, stale sources, and tampered sealed plans;
- exact Markdown span application and expected output hashes, reverse
  reconstruction to the sealed source hash, byte-identical copy commitments,
  missing required rewrite fields, and semantic output/provenance/manifest
  attacks that are fully resealed without changing the sealed plan;
- Document, Asset, Canvas, and Base file-node resolution; deterministic
  destination-relative Canvas rewrites; unknown-field preservation; unchanged
  byte copies; node-scoped ambiguity waivers; output-root containment; duplicate
  JSON-key/node rejection; and independently resealed semantic/target-removal
  tampering;
- explicit proposal approvals, evidence binding, generated-frontmatter
  injection resistance, generated body/output commitments, canonical
  EvidenceId frontmatter/provenance closure, fully resealed generated-output
  attack rejection, conflict waiver binding, and transcript audit closure;
- file-level evidence with no span, exact block hash/span evidence, rejection of
  arbitrary/mismatched spans, and provider-visible snapshot identity binding;
- ZIP/tar traversal and links, duplicate ZIP members, decompression ratio,
  malformed UTF-8/JSON, resource limits, exclusions, output no-clobber, and
  independently resealed artifact tampering;
- CLI/SDK plan parity and the full plan → approve → compile → pack → verify →
  explain lifecycle, including stable plan input/decision/internal exit families
  and fail-closed pre-output-hash plan rejection;
- provider deadline and SIGINT cancellation while input is blocked, bounded
  shutdown, process-group reaping, and non-publication of partial augmentation.

The exact requirement-to-test mapping is in
[`TRACEABILITY.md`](TRACEABILITY.md).

## Quality-gate status

| Gate | State | Evidence or remaining work |
|---|---|---|
| QG-001 Functional | implemented on macOS arm64 | current automated suite is green; the complete Markdown/Canvas golden corpus and supported-platform matrix are not complete |
| QG-002 Determinism | implemented on one platform | same-host bytes and absolute-location independence pass; Linux/Windows/toolchain comparison remains |
| QG-003 Provenance | partial | content outputs have exact source/proposal linkage and audit comparison; the full ALG-PRV-001 typed graph, administrative-file closure, record/edge IDs, and attribution nodes are missing |
| QG-004 Safety | implemented corpus green | current hostile-input and control-file tests pass; fuzz/property campaigns remain |
| QG-005 Compatibility | documented | no migration/version compatibility matrix is implemented yet |
| QG-006 Performance | not verified | the 100,000-note/20 GB/20-minute/2 GB RSS benchmark has not run |
| QG-007 Documentation | passed for this change | current state, traceability, specs, and append-only decision log are updated; all repository-relative Markdown links resolve |
| QG-008 Supply chain | partial | dual licenses and `Cargo.lock` exist; audit policy, SBOM, release provenance, signing, and clean-room release automation remain |

## Known implementation gaps

- Canvas file references now resolve through typed Document, Asset, Canvas, and
  Base targets and rewritten outputs are independently reconstructed. The
  complete normative Markdown/Canvas golden corpus—including broader Unicode,
  escaping, self-reference, and mixed-target vectors—and all
  platform-normalization vectors are not yet present.
- Markdown/frontmatter/wikilink/embed/ordinary-link parsing and exact byte-span
  rewrites are output-hash-bound and independently reversed to their sealed
  source hash. The remaining corpus gaps include CRLF/BOM, broader Unicode and
  escaping, and multiple grow/shrink replacement combinations.
- Input paths are stored after NFC normalization. The original NFD/NFC spelling
  is not retained, so some cross-source normalization collisions are typed as
  `PATH_EXACT` and raw-path provenance is incomplete.
- Provenance covers implemented content operations and exact duplicate source
  closure. Generated outputs bind the exact approved body, rendered bytes,
  proposal-order EvidenceId values, provenance sources, and operation identity,
  but provenance does not yet implement the full ALG-PRV-001 typed graph,
  compiler-generated administrative files, record/edge IDs, decision/approval
  nodes, or author/license attribution. V1 currently supports only spanless
  file/body evidence or exact block-hash evidence with an omitted or exact
  block span; arbitrary byte-span evidence is rejected.
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
  the target format are not implemented. Canvas and Markdown rewrite bytes are
  independently reconstructed from sealed operations, and every source-derived
  output is hash-committed; generated bodies/frontmatter source IDs now have an
  equivalent approved-proposal derivation check. An unsigned, wholly resealed
  artifact still has no external authenticity anchor.
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

1. Complete the full ALG-PRV-001 typed graph, including the non-circular audit
   envelope boundary and attribution records.
2. Add the public SDK augmentation request/record/replay surface and enforce
   canonical transcripts for every approved AI proposal.
3. Complete the remaining ALG-NRM-001 Markdown/Canvas golden corpus.
4. Add Linux, Windows, and macOS matrix CI plus cross-platform semantic and
   artifact determinism fixtures.
5. Stream large blobs and run QG-006 at the full reference workload.
6. Define schema migrations and run property/fuzz and fault-injection suites.
7. Complete QG-008 release automation, SBOM/provenance, and a versioned signing
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
