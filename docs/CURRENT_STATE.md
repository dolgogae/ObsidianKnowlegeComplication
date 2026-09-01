---
title: Current State
status: normative-v1
owners:
  - release-maintainer
last_updated: 2026-09-01
decision_refs:
  - ADR-0001
  - ADR-0003
  - ADR-0004
  - ADR-0009
  - ADR-0010
  - ADR-0011
  - ADR-0012
  - ADR-0013
  - ADR-0014
source_refs:
  - HIST-CURRENT-PLAN
---

# Current State

## Snapshot: 2026-09-01

The repository now contains a working `0.1.0` Rust framework and CLI. The
implemented vertical slice covers deterministic inspection through independent
verification, including the provider-neutral proposal/approval path. It is a
development implementation, not a cross-platform V1 release.

Implemented production packages:

- `vaultc`: safe snapshotting, canonical IDs/IR, Markdown and Canvas parsing,
  exact pre-NFC UTF-8 source-path retention plus NFC logical paths and pinned
  full-Unicode case-fold lookup, exact and review-only near deduplication,
  conflict/path planning, source-aware
  Markdown rewrites with sealed output commitments and reverse verification,
  typed Canvas reference resolution and rewriting, immutable approvals,
  verified sibling-staged compilation with native atomic no-replace directory
  publication, provider-neutral augmentation request/authorization/recording
  and offline replay, typed content-addressed provenance with a non-circular
  audit envelope, deterministic packing with verified atomic no-clobber pack
  publication, independent verification, and SQLite workspace state;
- `vaultc-protocol`: versioned provider capabilities, projections, evidence,
  proposals, and transcript records without a vendor SDK dependency;
- `vaultc-cli`: `inspect`, `plan`, `augment`, `replay`, `approve`, `compile`,
  `verify`, and `explain`, including a bounded NDJSON subprocess provider.

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
and operation ID. SDK and CLI live calls share an exact pre-disclosure policy
and consent gate; canonical four-record recordings can be replayed offline
without invoking a provider. Conflict decisions are immutable approval
overlays. V1 permits only `waived_by_policy`; it does not mislabel an
ambiguous link as user-resolved without a typed target/rewrite action.

## Verification evidence

All commands below passed on macOS arm64 with Rust 1.97.1:

| Check | Result |
|---|---|
| `cargo test --workspace --all-features --no-fail-fast` | 173 tests and all doctests passed |
| `cargo clippy --workspace --all-features --all-targets -- -D warnings` | passed with zero warnings |
| `cargo fmt --all -- --check` | passed |

The 173 tests comprise 27 `vaultc` unit tests, 5 Canvas integration tests, 10
atomic-directory-publication tests, 1 documentation-integrity test, 4
generated-provenance tests, 22 normalization/archive tests, 5 pack integration
tests, 11 atomic-pack-publication tests, 8 pipeline tests, 5 provider/approval
tests, 14 SDK augmentation/replay tests, 18 security tests, 9 typed-provenance
tests, 13 CLI unit tests, 3 CLI replay tests, 16 CLI lifecycle tests, and 2
protocol tests. They cover, among other cases:

- source immutability, deterministic plan/output, absolute-source-location
  independence, byte-identical VaultPacks on one supported host, and a literal
  complete-VaultPack SHA-256 golden shared by the platform workflow;
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
- typed source/operation/decision/proposal/approval/output/edge records,
  literal RecordId vectors, declared/absent/opaque attribution, exact dedup and
  generated closure, stored and virtual audit-envelope reconstruction,
  bounded pagination/cursors, directory/pack explanation parity, and fully
  resealed graph attacks;
- ZIP/tar traversal and links, duplicate ZIP members, decompression ratio,
  malformed UTF-8/JSON, resource limits, exclusions, output no-clobber, and
  independently resealed artifact tampering;
- exact pre-NFC/NFC source-path pairs, semantic-ID versus Plan/Record identity
  boundaries, spelling-only stale detection, Unicode-normalization and pinned
  full-case-fold collisions, BOM/CRLF/lone-CR byte spans, multi-span rewrites,
  raw ZIP central-name/Unicode-extra attacks, strict PAX paths, every-member
  archive accounting, and complete `tar.zst` stream-ratio enforcement;
- canonical VaultPack outer-byte verification, rejection of alternate zstd
  encodings or policy-mismatched levels, and streamed outer expansion-ratio
  enforcement before materialization;
- SDK/CLI pack staging, input and staged-pack verification, atomic no-replace
  publication, existing file/directory/live-or-dangling-symlink preservation,
  bidirectional containment and portable alias rejection, deterministic
  concurrent single-winner publication, and injected pre/post-commit faults;
- SDK/CLI Compiled Vault sibling staging, full-tree synchronization and
  independent staged verification, native no-replace publication, late
  file/directory/live-or-dangling-symlink race winners, concurrent distinct
  build isolation, source/output/pack disjointness, explicit cleanup or marked
  retention, unsupported-primitive fail-closed behavior, and post-commit
  durability uncertainty without starting the optional pack;
- CLI/SDK plan parity and the full plan → approve → compile → pack → verify →
  explain lifecycle, including stable plan input/decision/internal exit families
  and fail-closed pre-output-hash plan rejection;
- provider deadline and SIGINT cancellation while input is blocked, bounded
  shutdown, process-group reaping, and non-publication of partial augmentation.
- deterministic SDK projection construction, exact redacted four-record
  recordings, provider-free byte-identical replay, fresh validation at the
  approval boundary, remote policy/consent preflight before disclosure,
  cooperative cancellation, strict nested wire schemas, stale/header/validation
  attacks, SDK/CLI byte parity, and replay no-clobber publication.

The exact requirement-to-test mapping is in
[`TRACEABILITY.md`](TRACEABILITY.md).

## Quality-gate status

| Gate | State | Evidence or remaining work |
|---|---|---|
| QG-001 Functional | implemented on macOS arm64 | current automated suite is green; the complete Markdown/Canvas golden corpus and supported-platform matrix are not complete |
| QG-002 Determinism | implemented on one platform | same-host bytes and absolute-location independence pass; Linux/Windows/toolchain comparison remains |
| QG-003 Provenance | implemented and locally verified | typed stored graph, virtual audit envelope, RecordIds, decisions/approvals, frontmatter attribution, pagination, exact reconstruction, and adversarial reseal tests pass on macOS arm64; platform matrix remains |
| QG-004 Safety | implemented corpus green | current hostile-input, control-file, directory/pack publication race, and fault-seam tests pass; fuzz/property campaigns remain |
| QG-005 Compatibility | documented | no migration/version compatibility matrix is implemented yet |
| QG-006 Performance | not verified | the 100,000-note/20 GB/20-minute/2 GB RSS benchmark has not run |
| QG-007 Documentation | passed for this change | current state, traceability, specs, and append-only decision log are updated; a direct repository test requires every relative Markdown link to resolve |
| QG-008 Supply chain | partial | dual licenses, `Cargo.lock`, and a read-only CI workflow with a full-commit-pinned checkout action exist; audit policy, SBOM, release provenance, signing, and clean-room release automation remain |

## Known implementation gaps

- Canvas file references now resolve through typed Document, Asset, Canvas, and
  Base targets and rewritten outputs are independently reconstructed. The
  complete normative Markdown/Canvas golden corpus—including delimiter
  escaping, self-reference, and mixed-target vectors—and the supported-platform
  normalization matrix are not yet complete.
- Markdown/frontmatter/wikilink/embed/ordinary-link parsing and exact byte-span
  rewrites are output-hash-bound and independently reversed to their sealed
  source hash. BOM-at-zero versus content BOM, CRLF/lone CR, Unicode combining
  marks/full-fold vectors, and multiple grow/shrink replacements are covered;
  the remaining corpus gap is broader escaped/delimiter syntax.
- Every accepted source path retains its exact portable UTF-8 spelling before
  NFC plus its NFC logical path. Semantic source IDs remain logical-path based,
  while Plan and typed provenance identities bind the original spelling.
  Directory, raw ZIP central-directory, tar/PAX, collision classification,
  spelling-only stale-source, and fully resealed provenance attacks are covered
  locally; other filesystems and supported platforms remain unverified.
- Provenance now implements the ALG-PRV-001 typed stored graph for content and
  pre-envelope audit outputs plus deterministic virtual records for provenance,
  manifest, and checksums. Record identity, graph order, closure, decisions,
  proposals, approvals, frontmatter author/license declarations, pagination,
  and directory/pack explanation parity are verified locally. Attribution is
  deliberately retained as the original declared typed value; SPDX inference,
  license compatibility, manifest summaries, and a detached authenticity
  signature remain future profiles. V1 supports only spanless file/body
  evidence or exact block-hash evidence with an omitted or exact block span;
  arbitrary byte-span evidence is rejected.
- Some source and pack members are buffered under hard limits. The V1 20 GB
  workload and ≤2 GB RSS target cannot be claimed until streaming and the
  reference benchmark are verified. Augmentation recordings share immutable
  decoded state across replay clones, but canonical 1 GiB control-file decoding
  and re-encoding are not yet a fully streaming pipeline.
- Only macOS arm64 has been exercised in this workspace. A locked host-native
  GitHub Actions matrix is defined for Linux x86_64, Windows x86_64, macOS
  x86_64, and macOS arm64, but this checkout has no configured remote and those
  jobs have not run; filesystem normalization and cross-platform evidence
  therefore remain unverified.
- `.vaultpack` is deterministic and checksum/audit-verified but unsigned. The
  signing profile, key lifecycle, revocation, SBOM, and release artifacts are
  future work.
- Schema migrations, resume semantics, workspace encryption policy, broader
  property/fuzz testing, and a process-crash harness for directory
  materialization remain. Pack publication has deterministic in-process fault
  injection at every write-to-parent-sync boundary.
- SDK and CLI share one verified sibling-staging/no-replace pack publisher.
  The Compiled Vault and pack remain two ordered publications rather than one
  combined filesystem transaction. A runtime pack failure returns an explicit
  `PackPublicationAfterCompile` state and keeps the valid Compiled Vault; a
  post-commit parent-sync failure keeps the complete pack and reports uncertain
  durability.
- Source opening and existing ancestors are rechecked but are not yet fully
  descriptor-relative/no-follow. Compiled Vault publication now uses a native
  no-replace directory commit and deterministic late-winner fault barriers on
  macOS; the Linux and Windows implementations compile behind target-specific
  backends but still require execution on their supported local filesystems.
  VaultPack extraction bounds the outer file, declared and streamed expansion
  ratio, member count, per-file size, and aggregate size.
- Source archive container size, every effective member (including excluded and
  non-file entries), declared aggregate sizes, ZIP central-directory names, and
  the complete `tar.zst` decoder stream are bounded. Nested archives remain
  opaque rather than recursively extracted, and accepted entries are still
  buffered before sealing.
- The artifact manifest is still minimal: media types, license/attribution
  summaries, creation-policy/distribution metadata, and signature metadata from
  the target format are not implemented. Canvas and Markdown rewrite bytes are
  independently reconstructed from sealed operations, and every source-derived
  output is hash-committed; generated bodies/frontmatter source IDs now have an
  equivalent approved-proposal derivation check. An unsigned, wholly resealed
  artifact still has no external authenticity anchor.
- Directory and pack no-clobber plus symlink/race behavior are verified locally
  on macOS; Linux filesystem execution, Windows reparse-point execution, and
  the full supported-filesystem matrix remain. There is no installer or
  permission/license/signature display surface.
- Typed conflict actions are not modeled yet. `user_resolved` and
  `provider_suggested` remain reserved states; V1 only accepts an explicit
  policy waiver overlay for a required conflict.
- MCP, Obsidian plugin, registry/marketplace, claims, benchmarking, and every
  ALG-MEM experimental or research-only algorithm remain unimplemented and do
  not affect the compiler path.
- The cross-platform CI workflow is implemented but has not run on a connected
  remote. There is no branch-protection evidence or published release artifact
  yet.

## Next implementation slices

1. Run the committed Linux, Windows, macOS x86_64, and macOS arm64 matrix on a
   connected repository, retain its native publication/reparse/path/golden
   evidence, and make the required jobs branch-protection gates.
2. Define schema migrations and run property/fuzz plus process-crash suites.
3. Complete QG-008 release automation, SBOM/provenance, and a versioned signing
   ADR before calling a pack signed or marketplace-ready.
4. Stream large blobs and control files and run QG-006 at the full reference
   workload.

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
