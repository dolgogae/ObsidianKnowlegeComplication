---
title: Current State
status: normative-v1
owners:
  - release-maintainer
last_updated: 2026-09-02
decision_refs:
  - ADR-0015
  - ADR-0016
  - ADR-0017
  - ADR-0018
  - ADR-0019
  - ADR-0020
  - ADR-0021
source_refs:
  - HIST-CURRENT-PLAN
---

# Current State

## Snapshot: 2026-09-02

The repository contains a locally verified `0.2.0` OKC V2 development build.
It is not a public stable release: the required remote platform, performance,
fuzzing, PTY, and native-signing evidence does not yet exist.

## Implemented product surface

The Rust workspace is organized as follows:

- `okc-core`: V2 snapshot, IR, deterministic deduplication and planning,
  materialization, compilation, packing, verification, and provenance;
- `okc-protocol`: schema-2 provider-neutral NDJSON types;
- `okc-app`: `.okc-project` storage, private permissions, content-addressed
  objects, SQLite state, writer locking, source rebinding/invalidation,
  progress/cancellation types, and receipt-aware updating;
- `okc`: the only executable, containing the Clap CLI and Ratatui/Crossterm
  TUI reducer/shell;
- `okc-legacy-v1` and `okc-legacy-protocol`: frozen internal readers and
  literal goldens for read-only V1 compatibility;
- `vaultc`: deprecated Rust facade for one minor release, with no executable.

V2 writers use `format_family: "okc"`, schema `2`, `okc:*:v2\0` identity
domains, `.okc/`, `.okcpack`, and pack profile
`okc-tar-zstd-deterministic-v2`. New V1 artifacts are not written.

The core accepts directories, ZIP, `tar.zst`, and `.tzst`, preserves source
Vaults, canonical-sorts sources, rejects duplicate source IDs and duplicate
whole-Vault content identities, and carries optional owner display names
without an MCP-origin field. Markdown, frontmatter, wikilinks, ordinary links,
embeds, assets, typed Canvas nodes/edges/references, and opaque Base artifacts
enter canonical IR. Section/heading paths, ordered typed blocks, source spans,
media types, byte counts, raw SHA-256 values, and resource estimates are
sealed.

Exact duplicate notes and attachments collapse with all provenance edges;
near duplicates remain review-only. Source-local link resolution precedes
cross-source candidates. Path/case/Unicode conflicts retain deterministic
suffixing.

Required Markdown and Canvas ambiguities support only three typed actions:
preserve the original, select one sealed Markdown document, or select one
sealed Canvas target. Decisions bind plan, conflict, conflict hash, curator,
and policy. The Draft Plan remains unchanged. The canonical action/proposal
sets derive one `MaterializationPlan`; compile, verify, provenance, and audit
validation reconstruct it. Direct Markdown and Canvas selection lifecycles
verify suffix/display and unknown-JSON preservation.

Compiled manifests bind compiler/toolchain, plan, materialization, policy,
projection, source snapshots, proposal IDs/hashes, attribution summary,
creation/distribution policy, media type, byte length, raw SHA-256, content
hash, and deterministic Pack profile. User-created Packs are unsigned;
release-binary signing is a separate policy.

The CLI exposes the planned commands, including `tui`, `inspect`, `plan`,
`augment`, `validate`, `replay`, `approve`, `compile`, `verify`, `explain`,
`doctor`, and `update`. No-argument non-TTY execution prints help and exits 2.
V1 dispatch is limited to `verify` and `explain`.

The TUI has all twelve named screens, English/Korean labels, keyboard reducer,
80×24 fallback, ASCII/high-contrast modes, visible escaping for terminal and
bidi controls, and a terminal restoration guard. It is presently a reducer
and navigation shell: core/provider worker effects and full review workflows
are not implemented.

The project store creates the documented layout, uses private local
permissions where supported, WAL/foreign keys/`FULL` SQLite state, no-clobber
object writes, and invalidates downstream state after source changes. Project
data is intentionally plaintext and shared-location warnings are present.

`axoupdater 0.10.0` is pinned. `okc update` supports stable, latest, or an exact
canonical SemVer, refuses cargo/manual copies without a matching cargo-dist
receipt, and requires interactive confirmation unless `--yes` is supplied.
ADR-0021 records that only this library-owned blocking adapter may create a
short-lived current-thread runtime; OKC's TUI and worker architecture do not
use Tokio.

## Local verification evidence

All commands ran on macOS arm64 with Rust 1.97.1:

| Command | Result |
|---|---|
| `cargo test --locked --workspace --all-features --no-fail-fast` | 323 tests plus all doctests passed: 182 V2/application/CLI tests and 141 frozen V1/facade tests |
| `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` | passed with zero warnings |
| `cargo fmt --all -- --check` | passed |
| `cargo test --locked -p okc-core --test documentation_contract` | every repository-relative Markdown link resolved |
| cargo-dist 0.32.0 `dist plan --tag v0.2.0 --output-format=json --allow-dirty` | passed; planned four native archives, two installers, SHA-256, source archive, CycloneDX SBOM, and GitHub attestations |

The cargo-dist binary used for the last check was the official
`cargo-dist-aarch64-apple-darwin.tar.xz`; its observed SHA-256
`aa343b2ff78ec2981f17a65140250c5ad6062c74072163f68c5c2686d94763a7`
matched the publisher's adjacent checksum file.

## Quality-gate status

| Gate | State | Evidence or remaining work |
|---|---|---|
| QG-001 Functional | partial | 323 local tests pass, including a five-style multi-Vault lifecycle; maximum-scale fixtures, full parser corpus, real TUI workflows, and four-host execution remain |
| QG-002 Determinism | partial | same-host plan/artifact/Pack and absolute-root invariance pass; cross-platform/toolchain bytes and reverse-order 10-Vault evidence remain |
| QG-003 Provenance | locally verified | typed graph, audit envelope, decisions/proposals/approvals, attribution, directory/Pack explanation parity, and semantic reseal attacks pass |
| QG-004 Safety | partial | current traversal/archive/symlink/publication/control/provider tests pass; descriptor-relative source opening, Windows reparse execution, fuzz/property campaigns, and process-crash testing remain |
| QG-005 Compatibility | partial | frozen V1 artifacts/Packs can be verified and explained; V1 project import/rebind migration workflow and forward-version matrix remain |
| QG-006 Performance | not passed | the 10-Vault, 100,000-note, 20 GB, ≤20-minute, ≤2 GB RSS benchmark has not run; accepted source members are still buffered |
| QG-007 Documentation | locally passed | current state, traceability, specs, ADRs, decision log, and relative links are synchronized |
| QG-008 Supply chain | partial | lockfile, dual licenses, validated cargo-dist plan, SBOM/attestation configuration, and receipt-aware updater exist; remote builds, audit evidence, protected signing, notarization, and publication remain |

## Release blockers and known gaps

- The TUI does not yet execute real worker effects, persist review choices, or
  provide PTY-tested end-to-end keyboard workflows. Panic/signal/provider-crash
  restoration and publication-barrier cancellation lack PTY/system coverage.
- `OperationControl` and progress types exist, but inspect/plan/compile services
  do not yet consistently report progress or honor pre-publication cancellation.
  Platform process-tree termination still needs non-Unix implementation.
- Accepted directory/archive members are still accumulated in memory under
  bounds. The required streaming blob store, 20 GB benchmark, resume/cleanup
  lifecycle, and peak-RSS proof are incomplete.
- Source checks reject links and restat opened files, but fully
  descriptor-relative/no-follow traversal, Windows reparse-point defenses, and
  adversarial TOCTOU execution are incomplete.
- Project SQLite schema 2 and core workspace schema 2 migrate older local
  layouts, but resumable long operations and V1 project reconstruction are not
  complete. Source paths, plans, decisions, and AI records remain plaintext by
  design.
- A heterogeneous five-Vault MCP-style fixture now covers wikilinks, ordinary
  links, differing frontmatter, Canvas, callout/math/list blocks, attachments,
  exact note/asset deduplication, source-local priority, unique cross-source
  resolution, excluded MCP caches, and reverse-order output/Pack equality. A
  reverse-order 10-Vault fixture, supported-platform metadata matrix, broader
  Markdown/Canvas corpus, and fuzz/property campaigns remain.
- `dist plan` validates configuration only. No release archives have been built
  on all four hosts, no two consecutive same-SHA CI matrices have completed,
  and branch-protection evidence is absent.
- Apple Developer ID signing/notarization, Windows Authenticode with RFC3161,
  protected `release` Environment credentials, and native verification jobs do
  not exist in this checkout.
- QG-006 and the remaining QG-001/002/004/005/008 evidence are mandatory.
  [`RELEASE.md`](RELEASE.md) therefore prohibits publishing stable `0.2.0`.

Experimental memory/retrieval algorithms, MCP server integration, an Obsidian
plugin, registry/marketplace services, near-duplicate auto-merge, unattended AI
approval, project encryption, silent auto-update, and user-Pack publisher
signing remain outside the V2 default compiler.
