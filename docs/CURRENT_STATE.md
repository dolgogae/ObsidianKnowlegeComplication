---
title: Current State
status: normative-v1
owners:
  - release-maintainer
last_updated: 2026-09-05
decision_refs:
  - ADR-0015
  - ADR-0016
  - ADR-0017
  - ADR-0018
  - ADR-0019
  - ADR-0020
  - ADR-0021
  - ADR-0022
  - ADR-0023
  - ADR-0024
  - ADR-0025
source_refs:
  - HIST-CURRENT-PLAN
---

# Current State

## Snapshot: 2026-09-05

The repository contains a locally verified `0.3.0` schema-3 development slice
and frozen V1/V2 read-only artifact compatibility. It is not a complete or
public stable V3 release: semantic scale, V3 Pack/non-Markdown materialization,
provider-backed PTY, remote-platform, fuzzing, performance, and native-signing
evidence do not yet exist.

## Implemented product surface

The Rust workspace is organized as follows:

- `okc-core`: V2 snapshot, IR, deterministic deduplication and planning,
  materialization, compilation, packing, verification, and provenance plus V3
  integration validation and provider-free directory materialization;
- `okc-protocol`: frozen schema-2 and strict schema-3 provider envelopes;
- `okc-ai`: provider-neutral structured generation/embedding, strict portable
  schemas, bounded synchronous HTTP, environment/keychain credential references, and
  mock-conformance-tested OpenAI/Anthropic/Gemini/Ollama/OpenAI-compatible
  adapters; opt-in live smoke and complete vendor conformance remain open;
- `okc-app`: `.okc-project` storage, private permissions, content-addressed
  objects, internal schema-4 append-only source-set/run/task/exchange/revision/
  approval/plan/output/feedback journal, cwd discovery, sensitive
  preflight/routing, native credential service, integration review/compile
  service, bounded worker, writer locking, and receipt-aware updating;
- `okc`: the only executable, containing the Clap CLI and Ratatui/Crossterm
  V3 CLI and functional ten-screen TUI;
- `okc-legacy-v1` and `okc-legacy-protocol`: frozen internal readers and
  literal goldens for read-only V1 compatibility;
- `okc-legacy-v2`: frozen read-only V2 `verify`/`explain` boundary;
- `vaultc`: deprecated Rust facade for one minor release, with no executable.

## Implemented V3 development slice

New projects use schema 3, product `0.3.0`, V3 identity domains, AI role routes,
content-addressed objects, and append-only runs/tasks/exchanges/cluster
revisions/approvals. `project upgrade --out` creates a new schema-3 project and
copies only V2 source bindings, leaving the V2 project unchanged.

`okc integrate` currently executes sensitive preflight, one embedding input per
Markdown document, deterministic exact cosine candidates, organizer taxonomy,
per-cluster synthesis, and critic. Each provider request/response is locally
schema-validated, recorded, and resumable by a cache key binding stage, prompt,
schema, source, provider/model/adapter, and options. Remote non-interactive runs
require both consent flags. Any sensitive finding forces local embedding and
organizer plus local synthesis/critic for affected clusters.

Taxonomy proposals can be shown, exported as an editable cluster array, resealed,
and approved. Cluster synthesis/critic output can be listed, shown, exported,
and approved; each omission/minor finding requires an exact rationale mapping,
while major/critical findings are rejected. Feedback regeneration creates a new
proposal/critic revision bound to the prior hashes and immediately makes an old
approval stale. Fully approved state automatically seals an
`ApprovedIntegrationPlan` pointer in the project object store.

V3 compile is provider-free and validates exact taxonomy, disposition, evidence,
contradiction, critic, omission/waiver, approval, and recording closure. It
atomically emits canonical Markdown below `knowledge/`, source redirect stubs
below `legacy/`, retained non-omitted metadata, preserved-verbatim blocks, and
closed `.okc` manifest/plan/provenance/checksum records. V3 directory `verify`
recreates expected bytes; `explain` returns a verified per-path record. Missing
AI recordings or approvals fail deterministically.

## Frozen V2 implementation

V2 artifacts use `format_family: "okc"`, schema `2`, `okc:*:v2\0` identity
domains, `.okc/`, `.okcpack`, and pack profile
`okc-tar-zstd-deterministic-v2`. No V1/V2 writer is exposed from the V3
compatibility modules.

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

The TUI has the ten V3 screens `Workspace`, `AI Connection`, `Vaults`,
`Preflight`, `Taxonomy`, `Clusters`, `Build`, `Verify`, `Provenance`, and
`Settings`. It discovers or creates cwd projects, selects a complete active
source set, stores/test credentials through environment references or the OS
keychain, displays local preflight before consent, supports taxonomy
rename/path/move/merge/split and cluster three-pane review/regeneration, then
compiles and independently verifies. A bounded worker forwards progress and
cancellation; cancellation closes at the publication barrier. The reducer also
retains the 80×24 fallback, ASCII/high-contrast modes, hostile-control escaping,
and terminal restoration guard.

The current Korean [`V3 AI integration`](../guide/v3-integration.md), frozen V2
[`5-minute Quickstart`](../guide/index.md), and focused [`CLI`](../guide/cli.md),
[`TUI`](../guide/tui.md), conflict, provider, and troubleshooting pages are
Markdown sources rendered as a VitePress website. They separate the frozen
AI-free V2 regression lifecycle from the V3 provider/project/taxonomy/cluster
path without presenting the development tree as a stable release.

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
| `cargo test --workspace --all-features --no-fail-fast` | passed: all V3 provider/journal/integration/CLI tests, the prior 323-test V1/V2 baseline, and all doctests |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | passed with zero warnings |
| `cargo fmt --all -- --check` | passed after formatting |
| `cargo test --locked -p okc-core --test documentation_contract` | every repository-relative Markdown link resolved |
| debug CLI schema-3 project/source/default-and-role route/status smoke | passed; invalid Clap positional ordering found during smoke was corrected and regression-tested |
| release build plus the guide's AI-free Quickstart lifecycle | inspect, plan, approve, compile, directory/Pack verify, and provenance explain passed with absolute output destinations |
| `cd guide && npm ci && npm run docs:build` | VitePress production site built successfully with Node.js 24.13.1 |
| `cd guide && npm audit --audit-level=high` | zero known vulnerabilities reported |
| cargo-dist 0.32.0 `dist plan --tag v0.2.0 --output-format=json --allow-dirty` | passed; planned four native archives, two installers, SHA-256, source archive, CycloneDX SBOM, and GitHub attestations |

The cargo-dist binary used for the last check was the official
`cargo-dist-aarch64-apple-darwin.tar.xz`; its observed SHA-256
`aa343b2ff78ec2981f17a65140250c5ad6062c74072163f68c5c2686d94763a7`
matched the publisher's adjacent checksum file.

## Quality-gate status

| Gate | State | Evidence or remaining work |
|---|---|---|
| QG-001 Functional | partial | V3 Markdown directory workflow and the prior 323-test regression baseline pass locally; maximum-scale fixtures, full parser corpus, provider-backed PTY execution, and four-host evidence remain |
| QG-002 Determinism | partial | same-host plan/artifact/Pack and absolute-root invariance pass; cross-platform/toolchain bytes and reverse-order 10-Vault evidence remain |
| QG-003 Provenance | locally verified | typed graph, audit envelope, decisions/proposals/approvals, attribution, directory/Pack explanation parity, and semantic reseal attacks pass |
| QG-004 Safety | partial | current traversal/archive/symlink/publication/control/provider tests pass; descriptor-relative source opening, Windows reparse execution, fuzz/property campaigns, and process-crash testing remain |
| QG-005 Compatibility | partial | frozen V1 artifacts/Packs can be verified and explained; V1 project import/rebind migration workflow and forward-version matrix remain |
| QG-006 Performance | not passed | the 10-Vault, 100,000-note, 20 GB, ≤20-minute, ≤2 GB RSS benchmark has not run; accepted source members are still buffered |
| QG-007 Documentation | locally passed | current state, traceability, specs, ADRs, decision log, root/docs/guide relative links, and the VitePress production build are synchronized |
| QG-008 Supply chain | partial | lockfile, dual licenses, validated cargo-dist plan, SBOM/attestation configuration, and receipt-aware updater exist; remote builds, audit evidence, protected signing, notarization, and publication remain |

## Release blockers and known gaps

- The `0.3.0` development CLI still exposes the frozen V2 inspect/plan/augment/
  approve/compile commands to retain the existing regression harness. The
  ADR-0022 public `verify`/`explain`-only legacy boundary is therefore not yet
  complete even though V2 verification and explanation already dispatch
  through `okc-legacy-v2`.
- V3 currently embeds one whole Markdown document per request input and uses a
  bounded exact cosine pass. Deterministic block chunking/batching, fixed-seed
  HNSW, and union with exact/MinHash/title/alias/link candidates from
  `ALG-SEM-001` remain incomplete; the 100k-note path is not ready.
- V3 synthesis lacks hierarchical evidence extraction/reduce and manual section
  amendments. Persisted sensitive false-positive
  exceptions and the schema-3 supervised command adapter are also missing.
- V3 materialization does not yet copy/deduplicate attachments, safely rewrite
  Canvas, preserve opaque Base artifacts, rewrite all internal Markdown/Canvas
  links, or write/verify deterministic V3 OKCPack bytes.
- The TUI worker/review/compile path has reducer and service tests but not a
  provider-backed PTY end-to-end keyboard run. Panic/signal/provider-crash
  restoration and publication-barrier cancellation still lack PTY/system and
  supported-platform evidence.
- `OperationControl` reaches shared preflight/compile/verify and live provider
  calls. The core inspect/plan API itself still exposes only phase-boundary
  cancellation rather than inner-loop cancellation. Platform process-tree
  termination still needs non-Unix implementation.
- Accepted directory/archive members are still accumulated in memory under
  bounds. The required streaming blob store, 20 GB benchmark, resume/cleanup
  lifecycle, and peak-RSS proof are incomplete.
- Source checks reject links and restat opened files, but fully
  descriptor-relative/no-follow traversal, Windows reparse-point defenses, and
  adversarial TOCTOU execution are incomplete.
- Project/artifact format remains schema 3 while private application SQLite is
  schema 4; the separate core workspace retains schema 2 for frozen
  inspection/planning internals. Full crash-injected
  resume and V1 project reconstruction are incomplete. Source paths, plans,
  decisions, and AI records remain plaintext by design.
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
  [`RELEASE.md`](RELEASE.md) therefore prohibits publishing stable `0.3.0`.

Experimental memory/retrieval algorithms (`ALG-MEM-006` included), uncalibrated
claim confidence, MCP server integration, an Obsidian plugin,
registry/marketplace services, unattended AI approval, project encryption,
silent auto-update, and user-Pack publisher signing remain outside the V3
default compiler.
