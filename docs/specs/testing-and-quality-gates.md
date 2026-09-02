---
title: Testing and Quality Gates
status: normative-v1
owners:
  - qa-security-engineer
  - core-rust-engineer
last_updated: 2026-09-02
decision_refs:
  - ADR-0004
  - ADR-0007
  - ADR-0008
  - ADR-0010
  - ADR-0011
  - ADR-0012
  - ADR-0013
  - ADR-0014
  - ADR-0015
  - ADR-0017
  - ADR-0018
  - ADR-0019
  - ADR-0020
source_refs:
  - HIST-COMPILER-PLAN
---

# Testing and Quality Gates

## Release gates

- **QG-001 Functional:** all stable requirement tests pass on supported platforms.
- **QG-002 Determinism:** identical fixtures/configuration/transcript produce identical manifest, output hashes, and OKCPack bytes across two clean runs; cross-platform semantic hashes match.
- **QG-003 Provenance:** every output node has a complete derivation path; generated items have evidence and approval.
- **QG-004 Safety:** hostile-input corpus, traversal/symlink/archive and malicious-proposal tests pass fail-closed.
- **QG-005 Compatibility:** schema backward/forward behavior matches the version matrix.
- **QG-006 Performance:** reference workload meets ≤20 minutes and ≤2 GB peak RSS without AI on 8 cores, 16 GB RAM, NVMe.
- **QG-007 Documentation:** Markdown links resolve; traceability reflects code/tests; release changes and ADR impact are recorded.
- **QG-008 Supply chain:** licenses, lockfiles, vulnerability policy, provenance/SBOM, and reproducible release process pass.

## Current `0.2.0` evidence

The exact local command results and test count are recorded in
[`../CURRENT_STATE.md`](../CURRENT_STATE.md). Local evidence never substitutes
for the supported-platform or protected-release gates:

| Gate | Current state |
|---|---|
| QG-001 | partial: current functions pass locally; the complete Markdown/Canvas corpora and supported-platform matrix remain |
| QG-002 | partial: same-host and cross-absolute-source-root byte equality pass; cross-platform/toolchain comparison remains |
| QG-003 | implemented and locally verified: typed stored graph, virtual audit envelope, RecordIds, attribution retention, bounded explanation, and semantic reseal cases pass; supported-platform evidence remains |
| QG-004 | partial: targeted hostile-input suite passes; fuzz/property campaigns and remaining platform/adversarial classes remain |
| QG-005 | partial: frozen V1 verify/explain and schema-2 workspace migration exist; V1 project reconstruction and broader forward-compatibility evidence remain |
| QG-006 | not run at the 100,000-note/20 GB reference workload |
| QG-007 | evaluated per change after link, traceability, current-state, ADR, and decision-log validation |
| QG-008 | partial: licenses, lockfile, cargo-dist/SBOM/attestation configuration, and receipt-aware updater exist; protected native signing/notarization and published evidence do not |

The authoritative live status and exact limitations are in
[`../CURRENT_STATE.md`](../CURRENT_STATE.md); the mapping to code/tests is in
[`../TRACEABILITY.md`](../TRACEABILITY.md).

## Cross-platform CI contract

The required hosted baseline has four host-native jobs: Linux x86_64, Windows
x86_64, macOS x86_64, and macOS arm64. Each job MUST use the repository-pinned
Rust toolchain and `Cargo.lock`, report its actual Rust host triple, and run the
complete workspace test suite with all features. The matrix MUST use
`fail-fast: false` so one platform failure does not erase evidence from the
others.

At least one job MUST additionally enforce workspace rustfmt, all-feature
all-target Clippy with warnings denied, and repository-relative Markdown link
integrity. A canonical basic-Vault build MUST compare its complete OKCPack
bytes against a literal raw-SHA-256 golden on every host; same-run equality
alone is not cross-platform evidence.

Workflow permissions MUST be read-only unless a separate reviewed publication
workflow requires more. Third-party or GitHub-maintained actions MUST be pinned
to reviewed full commit identities, with the human-readable release recorded
in a comment. The workflow definition being present or compiling locally is
not a passed platform gate: only completed remote jobs on the named hosts count
as Linux, Windows, or macOS CI evidence. Symlink/reparse-point skips MUST state
the missing runner capability rather than silently count as coverage.

A stable tag additionally requires two consecutive complete matrix successes
on the exact same full commit SHA. A successful run plus a rerun of only failed
jobs is not sufficient.

## Test layers

### Unit and golden tests

- domain-separated identifiers and canonical serialization;
- Markdown/frontmatter source spans and rewrite output;
- wikilinks, embeds, aliases, heading/block refs, callouts, math, code fences;
- JSON Canvas typed fields and unknown-field preservation;
- exact and near-duplicate golden vectors;
- path/case/title/frontmatter conflict resolution;
- checksums, provenance records, approval invalidation, and pack archive headers.
- literal typed provenance RecordId vectors, strict record ordering, allowed
  edge matrix/cardinality, attribution states, and cursor bindings.

### Property and fuzz tests

- parser/scanner never panics for arbitrary bytes;
- normalization is idempotent;
- path allocator returns safe unique paths or an explicit error;
- compilation never mutates source fixture hashes;
- provenance graph is closed and acyclic in derivation edges;
- serialization/parse round trips preserve semantic identity;
- concurrent and sequential planning yield the same result.

### Adversarial corpus

Include ZIP slip, tar traversal, symlink/hardlink escape, decompression bomb, huge frontmatter, deeply nested Markdown/JSON, duplicate archive members, malformed UTF-8/YAML/JSON, NFC/NFD/case collisions, Windows reserved names, control characters, malicious HTML, fake tool instructions, forged/stale proposal IDs, invalid evidence spans, extension spoofing, and interrupted writes.

The normalization corpus MUST also cover directory/archive original-path
spelling, strict raw ZIP/tar UTF-8 rejection, leading BOM byte offsets,
frontmatter-adjacent BOM content, CRLF/lone-CR preservation outside rewritten
spans, combining marks and emoji, full-fold `ß/ss` and sigma vectors, multiple
grow/shrink rewrites, existing escaped delimiters, root-escape links, and
Canvas raw file values whose lookup key normalizes while stored JSON remains
unchanged.

### Integration and end-to-end

- Rust SDK and CLI create equivalent plans.
- Provider subprocess capability negotiation, timeout, crash, oversized/malformed response, cancellation, and transcript replay.
- Public SDK projection/live recording/offline replay parity, exact four-record
  transcripts, remote policy plus per-call consent, cooperative cancellation,
  and byte-identical SDK/CLI canonical recordings.
- Compile interruption never publishes partial output.
- Compiled Vault publication preserves every existing file, directory,
  symlink/reparse point, and deterministic pre-commit race winner; concurrent
  SDK/CLI creators have exactly one complete verified winner.
- Public SDK/CLI pack creation never exposes a partial requested destination,
  never overwrites an existing entry or symlink referent, and has exactly one
  winner under concurrent publication.
- Independent verifier catches each intentionally corrupted artifact class.
- TUI reducer and render snapshots cover all screens, English/Korean, 80×24
  fallback, long Unicode paths, hostile terminal controls, and
  color-independent status. PTY E2E covers keyboard-only conflict selection,
  cancellation, provider crash, signal/panic restoration, and the publication
  barrier.
- Future MCP and Obsidian adapters pass contract and permission tests without bypassing framework invariants.

Typed provenance acceptance MUST cover Copy, Markdown/Canvas rewrite, exact
note/asset deduplication, generated evidence and approval, Markdown/Canvas
waivers, the four stored audit paths, the three virtual audit-envelope paths,
directory/pack inner parity, and an explicit virtual package subject. Fully
resealed attacks include missing/extra/duplicate producer edges, dangling or
wrong-kind endpoints, cycles, unrelated valid source substitution, ordered
evidence changes, attribution removal, stale decisions/approvals, legacy flat
records, graph schema changes, and cursor replay across subject/artifact.

### Required regression status

The SDK/CLI augmentation suite now implements projection determinism, exact
four-record live recording, empty-transcript rejection, fresh approval
validation, policy-and-consent preflight, provider-free byte-identical replay,
canonical JSONL, nested unknown/duplicate-field rejection, header binding,
SDK/CLI parity, cancellation, stale/cross-plan attacks, and replay no-clobber.

The normalization/original-path and archive-accounting regressions are now
implemented, including
`cross_source_nfc_nfd_collision_is_typed_and_preserves_original_path` and
`archive_declared_member_count_and_compressed_bytes_are_bounded`.

The locally implemented atomic-pack acceptance includes
`sdk_pack_failure_does_not_leave_partial_destination`,
`sdk_pack_existing_destination_is_preserved`,
`sdk_pack_dangling_symlink_is_rejected_without_following_target`,
`sdk_pack_destination_inside_compiled_vault_is_rejected_without_mutation`,
`sdk_pack_concurrent_publish_has_exactly_one_winner`,
`compile_options_reject_output_pack_aliases_before_publication`, and
`compile_with_pack_failure_leaves_valid_compiled_vault_and_no_partial_pack`.
Fault-seam tests distinguish pre-commit absence from the complete published
file returned with `PublishedButDurabilityUncertain` after a parent-sync
failure. The private fault seam additionally verifies the exact
write-to-parent-sync order and all pre/post-commit states. Supported-platform
concurrent destination and Windows reparse-point tests remain release-matrix
requirements even after local acceptance passes.

The locally implemented atomic-directory acceptance includes
`sdk_directory_concurrent_creators_have_exactly_one_winner`,
`sdk_directory_distinct_concurrent_builds_never_mix_or_replace_winner`,
`sdk_directory_existing_file_and_directory_are_preserved`,
`sdk_directory_live_and_dangling_symlinks_are_rejected_without_following_referents`,
`directory_publish_barrier_preserves_external_file_directory_and_symlink_winners`,
`directory_precommit_faults_cleanup_staging_by_default`,
`directory_retain_policy_marks_only_the_exact_failed_stage`,
`directory_cleanup_failure_reports_residue`,
`directory_parent_sync_failure_retains_verified_output`, and
`directory_unsupported_primitive_never_falls_back_to_replacing_rename`.
The source/publication disjointness matrix covers the Compiled Vault and
integrated Pack, equality, both containment directions, lexical `..`,
existing-ancestor symlink aliases, and portable case/normalization aliases.
Normal concurrent builds are supplemental; the
private before-publish barrier is required to prove that a late empty-directory
or symlink winner is not replaced.

These behaviors have both public SDK/CLI coverage and a private deterministic
barrier/fault seam. They are locally verified on macOS arm64; Linux, Windows,
reparse-point, filesystem, and process-crash evidence remains mandatory before
the parent release requirement is called cross-platform verified.

## Algorithm status gates

Stable algorithms require worked examples, golden vectors, boundary tests, and cross-platform determinism. Experimental algorithms require offline baselines, held-out evaluation, calibration/error bars, ablation, resource cost, safety/privacy review, and rollback behavior. Passing an experiment does not promote it; an ADR and normative spec update are also required.

## Benchmark hygiene

LLM-generated queries cannot be both the sole test generator and judge. Retrieval benchmarks use source notes as traceable ground truth, split generation/evaluation sources when possible, avoid train/test leakage, report topic/cohort sizes and confidence intervals, and retain failure cases. There is no single absolute quality score across unrelated topics.
