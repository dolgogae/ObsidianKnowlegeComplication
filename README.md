# Obsidian Knowledge Compilation (OKC)

`okc` compiles immutable Obsidian Vault snapshots into a new deterministic,
auditable Compiled Vault. The Rust core owns inspection, planning, approval,
compilation, verification, and provenance; the CLI/TUI and typed Python and
Node.js libraries call the same application services. V3 requires recorded AI
integration proposals; live providers remain replaceable adapters and are
never required for offline replay/compile/verify. MCP servers and Obsidian
plugins are optional adapters.

The implementation contract starts at [`AGENTS.md`](AGENTS.md) and
[`docs/INDEX.md`](docs/INDEX.md).

> **Development status (2026-09-06):** `0.3.0` contains a locally verified
> schema-3 AI integration/directory-compilation slice plus frozen V1/V2 readers,
> but it is **not a complete or public stable release**. V3 HNSW/chunking,
> manual amendment, Pack and non-Markdown materialization, command-provider,
> remote four-platform evidence, performance, fuzz/PTY, and native signing are
> still required.
> See [`docs/CURRENT_STATE.md`](docs/CURRENT_STATE.md) and
> [`docs/RELEASE.md`](docs/RELEASE.md) before packaging or publishing it.

## Implementation status at a glance

| Surface | Current `0.3.0` development state |
|---|---|
| Schema-3 projects | Implemented: cwd discovery/creation, V2 source-binding-only upgrade, absolute source sets, AI routes, append-only schema-4 application journal |
| AI integration | Implemented development path: sensitive preflight, embedding, organizer, taxonomy approval, per-cluster synthesis/critic/approval |
| Offline output | Implemented for V3 directories: canonical Markdown, legacy redirect stubs, manifest, checksums, closed provenance, verify, explain |
| Provider adapters | OpenAI, Anthropic, Gemini, Ollama, and OpenAI-compatible HTTP adapters are mock-conformance tested; environment references and native OS-keychain references are supported; live smoke tests are opt-in |
| V3 Pack and non-Markdown output | Not implemented: attachment/Canvas/Base carry-through, complete link rewriting, and V3 OKCPack remain blockers |
| TUI | Functional cwd workflow: provider setup, Vault selection, preflight/consent, taxonomy editing, cluster review/regeneration, compile, verify, and provenance; PTY/platform evidence remains |
| Python / Node.js | Initial typed `okc-compiler` packages expose the current V3 approval workflow and V1/V2/V3 read-only verification through one Rust facade; remote four-platform package evidence remains |
| Legacy boundary | V1/V2 artifacts dispatch through read-only readers, but development-only V2 writer commands remain exposed for the regression harness |

Start with the Korean [`V3 AI integration guide`](guide/v3-integration.md).
The [`5-minute Quickstart`](guide/index.md) preserves the frozen V2 regression
walkthrough; the focused [`CLI`](guide/cli.md), [`TUI`](guide/tui.md), and
[`Python · Node.js`](guide/python-node.md) guides
cover the non-interactive and interactive workflows.

## Overview

OKC is an MIT/Apache-2.0 open-source toolkit for safely combining knowledge
from multiple Obsidian Vaults. It is a compiler rather than a note-sync
service: source Vaults are treated as immutable, potentially hostile inputs;
`okc` parses them into a canonical model, plans deduplication and link
rewrites, requires explicit decisions where meaning is ambiguous, and writes
a new independently verifiable Vault.

OKC consumes the Markdown, YAML, Canvas, Base, and attachment files produced by
Vault tooling. It never connects to, identifies, or federates the community
Obsidian MCP server that produced them. MCP-specific databases, plugin code,
caches, search indexes, and executables do not become canonical compiler state.

The project is intended to be embedded in:

- local developer tools and knowledge-management applications through the Rust
  SDK;
- CPython 3.11+ and Node.js 22.13+ applications through the typed
  `okc-compiler` language packages;
- reproducible automation and review workflows through the `okc` CLI;
- provider-neutral AI workflows where an LLM may propose content but cannot
  silently change the compilation plan;
- future thin MCP, coding-agent, Obsidian, registry, and marketplace adapters
  that consume the same compiler contract instead of becoming separate sources
  of truth.

This repository is not an AI model, a hosted cloud marketplace, or an Obsidian
sync replacement. It is the deterministic compilation and trust layer those
products can build on. The brain-inspired consolidation and retrieval ideas in
the research documents are deliberately outside the default compiler until
their algorithms and evaluation gates become stable.

## What the framework does

The framework is the policy and integrity boundary between untrusted source
Vaults and a publishable result. It:

- seals directory, ZIP, and `tar.zst` inputs without changing them;
- retains each accepted UTF-8 source path both as its exact pre-NFC spelling
  and as its canonical NFC logical path, with host-independent archive-name
  validation and full-Unicode case-fold collision checks;
- parses Markdown/frontmatter/Obsidian links, resolves and rewrites typed Canvas
  file references, and inventories Bases and attachments in a canonical IR;
- plans exact deduplication, link rewrites, portable output paths, and typed
  conflicts before writing anything;
- obtains mandatory V3 organizer/synthesis/critic proposals through
  provider-neutral interfaces, then validates identities, evidence,
  dispositions, critic findings, and approvals;
- materializes only a sealed approved plan (`ApprovedIntegrationPlan` in V3)
  through a verified sibling stage and a native atomic no-replace directory
  commit, preserving any existing or concurrently created destination;
- preserves the frozen V2 deterministic `.okcpack` writer/verifier as a
  regression surface; the schema-3 Pack writer is not implemented yet.

Frozen V2 user-created `.okcpack` files are deterministic and internally
verifiable but unsigned. A V3 Pack cannot currently be produced. Application
release signing/notarization is a separate mandatory release gate. See
[`docs/CURRENT_STATE.md`](docs/CURRENT_STATE.md) for the exact implementation
and verification boundary.

## Multi-Vault behavior

Each source has a project-unique stable `source_id` and may have an optional
display-only owner. Sources are canonically sorted, so their command-line order
and MCP origin do not affect identities or output. Duplicate source IDs and
duplicate whole-Vault snapshots are rejected.

- Byte-identical notes and attachments are emitted once with provenance from
  every source. Near duplicates remain separate review candidates.
- Same-path, case, and Unicode collisions keep every item under deterministic
  suffixes and produce typed diagnostics.
- Links first resolve inside the source Vault. Cross-source resolution is used
  only when the original Vault has no matching target.
- A single sealed candidate is rewritten deterministically. Multiple candidates
  create `LINK_AMBIGUITY`; a curator may preserve the original or select one
  exact sealed Markdown/Canvas target. Free-form replacement paths or text are
  rejected.
- Attachment lookup remains source-local and does not guess from a matching
  filename in another Vault.

The V2 support contract is at most 10 input Vaults, 100,000 notes, and 20 GB
per project. Those limits are enforced or estimated by the implementation, but
the full reference performance workload has not yet passed QG-006.

## V3 format and legacy compatibility

| Surface | OKC V3 value |
|---|---|
| Format family | `okc` |
| Schema | `3` |
| Identity domain prefix | `okc:*:v3\0` |
| Audit directory | `.okc/` |
| Canonical notes | `knowledge/<taxonomy>/<slug>.md` |
| Source redirects | `legacy/<source-id>/<original-path>.md` |

V3 compile requires a complete approved integration plan and sealed provider
recordings, but performs no live provider call. ADR-0022 defines V1 and V2 as
read-only through `okc verify` and `okc explain`; `project upgrade --out`
constructs a separate V3 project from V2 source bindings without copying
downstream authority. The development CLI still exposes V2 writer commands for
the existing regression harness, so the public legacy boundary is not yet a
completed release gate.

### Frozen V2 literals

| Surface | OKC V2 value |
|---|---|
| Format family | `okc` |
| Schema | `2` |
| Identity domain prefix | `okc:*:v2\0` |
| Audit directory | `.okc/` |
| Pack extension | `.okcpack` |
| Pack profile | `okc-tar-zstd-deterministic-v2` |

The `0.2.0` core remains in-tree to verify/explain existing artifacts. New V3
compatibility modules do not expose V1/V2 inspection, planning, approval,
compilation, or Pack writing.

## Rust workspace

- `okc-core`: compiler library and public phased API;
- `okc-ai`: provider-neutral structured generation/embedding and HTTP adapters;
- `okc-protocol`: frozen V2 and schema-3 provider envelopes;
- `okc-app`: schema-3 project persistence, schema-4 application journal,
  disclosure, and services;
- `okc-interop`: runtime-neutral API-v1 client/project/job facade shared by
  language bindings;
- `okc-python`: PyO3 `abi3-py311` adapter for the `okc-compiler` Python
  distribution and `okc` import module;
- `okc-node`: napi-rs Node-API 9 adapter behind ESM/CommonJS `okc-compiler`
  entry points;
- `okc`: the only executable, containing the CLI, Ratatui TUI, and supervised
  NDJSON subprocess adapter;
- `okc-legacy-v1` / `okc-legacy-v2`: verify/explain-only readers;
- `vaultc`: deprecated Rust facade for one minor release, with no executable.

This repository currently targets Rust 1.97.1 and uses the Rust 2024 Edition.

## Build and test

```sh
cargo build --locked --workspace
cargo test --locked --workspace --all-features --no-fail-fast
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
cargo test --locked -p okc-core --test documentation_contract
```

The toolchain is pinned by [`rust-toolchain.toml`](rust-toolchain.toml). The
framework is dual-licensed under MIT OR Apache-2.0.

### Guide website

The source pages under [`guide/`](guide/index.md) are ordinary Markdown rendered
as a VitePress website. Node.js 22 or newer is required for local preview:

```sh
cd guide
npm ci
npm run docs:dev
```

Run `npm run docs:build` in the same directory to validate a production build.

[`CI`](.github/workflows/ci.yml) defines locked, host-native test jobs for Linux
x86_64, Windows x86_64, macOS x86_64, and macOS arm64, plus formatting and
warning-free Clippy. Its OKCPack test compares the complete archive against a
single literal SHA-256 golden on every host. The workflow definition is not
release evidence by itself; the required same-commit remote runs have not yet
completed.

### Python and Node.js development packages

The language packages are implemented but not published to PyPI or npm. Their
first-candidate names and runtime contracts are:

| Runtime | Package contract |
|---|---|
| Python | distribution `okc-compiler`, import `okc`, CPython 3.11+, `abi3-py311` |
| Node.js | package `okc-compiler`, Node.js 22.13+, Node-API 9, ESM/CommonJS/TypeScript |

Build and exercise them from this source checkout with the pinned development
tools used by the package workflow:

```sh
python -m venv .venv-sdk
source .venv-sdk/bin/activate
python -m pip install maturin==1.15.0 pytest==8.4.2 mypy==1.18.2
python -m maturin develop --manifest-path bindings/python/Cargo.toml --release --locked
python -m pytest bindings/python/tests
python -m mypy --strict bindings/python/tests/typing_contract.py

npm ci --ignore-scripts --prefix bindings/node
npm run build --prefix bindings/node
npm test --prefix bindings/node
npm run typecheck --prefix bindings/node
```

All project, source, output, and artifact paths passed through these libraries
must be absolute. Provider profiles accept an environment-variable name, never
a raw secret, and every remote cache miss requires explicit per-call disclosure
consent. Filesystem, provider, and compiler work returns a cancellable `Job`;
taxonomy and cluster approvals remain explicit human actions. See the complete
[`Python · Node.js guide`](guide/python-node.md) and the normative
[`API-v1 contract`](docs/specs/public-sdk-and-cli.md).

## Projects and TUI

The `okc-app` crate stores long-lived work in a directory ending in
`.okc-project`:

```text
ProjectName.okc-project/
├── manifest.json
├── state.sqlite3
├── objects/
├── workspace/build.sqlite3
└── project.lock                 # present only while a writer owns the project
```

Immutable objects are written without clobbering before SQLite references are
committed. The store uses WAL, foreign keys, explicit schema-4 application-state
migrations, and a single-writer lock; the project/artifact format remains
schema 3. Project data is intentionally not encrypted: source paths, plans,
decisions, and AI records are plaintext. OKC applies restrictive local
permissions where supported and warns about likely shared locations.

With an interactive terminal, running `okc` without a subcommand opens the
TUI; non-TTY execution prints help and exits `2`.

```sh
cargo run -p okc
cargo run -p okc -- tui
cargo run -p okc -- --project Team.okc-project
```

The current TUI implements the ten-screen V3 workflow. From any desired folder,
`okc` discovers projects and safe Vault candidates, configures and tests a live
provider, performs local preflight before consent, persists taxonomy and cluster
reviews, regenerates rejected clusters, and compiles then independently verifies
a new directory. Network and compilation work runs on a bounded single-operation
worker with cancellation before the publication barrier. The UI retains the
80×24 fallback, hostile-control escaping, ASCII/high-contrast modes, and terminal
restoration guard.

### V3 CLI development flow

Provider tests use fixed synthetic content rather than Vault data. Profiles
store only an environment-variable name or OS-keychain account reference; a
secret value is never written to TOML or project state.

```sh
okc project create Team.okc-project --name Team --curator alice --language ko-KR
okc --project Team.okc-project project source add personal ./PersonalVault
okc provider add default --kind ollama --endpoint http://127.0.0.1:11434 --model MODEL
okc provider test default
okc --project Team.okc-project integrate
okc --project Team.okc-project integration status --format json
okc --project Team.okc-project review taxonomy show
okc --project Team.okc-project review taxonomy approve
okc --project Team.okc-project integrate
okc --project Team.okc-project review cluster list
okc --project Team.okc-project review cluster show CLUSTER_ID
okc --project Team.okc-project review cluster approve CLUSTER_ID
okc --project Team.okc-project integrate
```

Approve every cluster and rerun `integrate`; the command prints the immutable
object path containing the complete integration plan. Then compile and inspect
the V3 directory without contacting a provider:

```sh
okc --project Team.okc-project compile \
  --integration-plan Team.okc-project/objects/PLAN_OBJECT_HASH \
  --output CompiledVault
okc verify CompiledVault
okc explain CompiledVault knowledge/TAXONOMY/NOTE.md --format json
```

Each omission needs its exact
`--omission-rationale 'DOCUMENT_ID:TARGET_ID=reason'`; each minor critic finding
needs `--minor-waiver 'FINDING_ID=reason'`. Major and critical findings cannot
be approved and instead use `review cluster regenerate --feedback ...` before
rerunning integration. Remote non-interactive runs require both
`--allow-remote-provider` and `--yes`.

## Frozen V2 Rust SDK example

The API makes each state transition visible. A deterministic build without AI
looks like this:

```rust,ignore
use okc_core::{CompilerPolicy, OkcCompiler, SourceSpec};

fn main() -> okc_core::Result<()> {
    let compiler = OkcCompiler::builder()
        .policy(CompilerPolicy::default())
        .workspace(".okc-work/build.sqlite")
        .build()?;

    let inspection = compiler.inspect([
        SourceSpec::directory("personal", "./PersonalVault")?,
        SourceSpec::archive("team", "./TeamVault.zip")?,
    ])?;
    let draft = compiler.plan(&inspection)?;

    // This succeeds only when no required conflict still needs a decision.
    let approved = compiler.approve_without_augmentation(draft)?;
    let artifact = compiler.compile(&approved, "./CompiledVault")?;
    let report = compiler.verify(&artifact.path)?;
    assert!(report.valid);

    let explanation = compiler.explain_provenance(
        &artifact.path,
        "knowledge/Topic.md",
    )?;
    println!("{explanation:#?}");
    Ok(())
}
```

To publish a Compiled Vault and an optional pack through the same core policy,
use `compile_with_options`:

```rust,ignore
use okc_core::CompileOptions;

let artifact = compiler.compile_with_options(
    &approved,
    "./CompiledVault",
    &CompileOptions {
        create_pack: Some("./CompiledVault.okcpack".into()),
    },
)?;
```

These are two ordered publications, not one cross-filesystem transaction. If
the later pack step fails, the error identifies the valid Compiled Vault that
remains; okc never exposes a partial requested pack file.

For AI-assisted builds, an application implements provider-neutral
`KnowledgeAugmentor` rather than depending on one LLM vendor:

```rust,ignore
use okc_core::{
    CancellationToken, DocumentSelection, KnowledgeAugmentor,
    RemoteProviderConsent, OkcCompiler,
};

fn record_and_replay(
    compiler: &OkcCompiler,
    plan: &okc_core::DraftPlan,
    augmentor: &impl KnowledgeAugmentor,
) -> okc_core::Result<okc_core::ValidatedProposals> {
    let recording = compiler.augment(
        plan,
        &DocumentSelection::All,
        augmentor,
        &CancellationToken::default(),
        RemoteProviderConsent::Denied,
    )?;

    // This validates the saved exchange without calling the provider again.
    let replayed = compiler.replay_augmentation(plan, &recording)?;
    assert_eq!(
        recording.to_canonical_jsonl()?,
        replayed.to_canonical_jsonl()?,
    );
    Ok(replayed.into_validated())
}
```

Remote providers require both sealed-policy permission and explicit live-call
consent. External transports first obtain an opaque core authorization after
capability negotiation and before disclosing source text. Returned proposals
remain untrusted data and need content-hash-bound `ApprovalLog` decisions.
Required conflicts use a separate immutable `DecisionOverlayLog`. A curator
may explicitly preserve the original or select one exact sealed Markdown or
Canvas target; free-form replacement text is never accepted.

## Frozen V2 CLI example

Sources use `ID=PATH`; a bare path derives its ID from the final path component.
Input kind is detected from a directory or the `.zip`, `.tar.zst`, or `.tzst`
extension.

```sh
cargo run -p okc -- inspect personal=./PersonalVault team=./TeamVault.zip --format json
cargo run -p okc -- plan personal=./PersonalVault team=./TeamVault.zip --out plan.json
cargo run -p okc -- augment plan.json --provider-cmd ./provider --all-documents --out augmentation.jsonl
cargo run -p okc -- validate plan.json --augmentation augmentation.jsonl
cargo run -p okc -- replay plan.json --augmentation augmentation.jsonl --out replayed.jsonl
cargo run -p okc -- approve plan.json --decisions decisions.json --proposals replayed.jsonl --out approved-plan.json
cargo run -p okc -- compile approved-plan.json --output CompiledVault --pack CompiledVault.okcpack
cargo run -p okc -- verify CompiledVault.okcpack --format json
cargo run -p okc -- explain CompiledVault.okcpack knowledge/Topic.md --format json
cargo run -p okc -- doctor
```

The minimum decision document for a conflict-free, AI-free V2 plan is:

```json
{
  "schema_version": 2,
  "plan_id": "<plan_id from plan.json>",
  "decisions": [],
  "conflicts": []
}
```

`plan` still writes `plan.json` but exits with code `4` when required conflicts
need decisions. `compile` requires absent output and pack destinations. The
complete command, decision schema, provider flow, and frozen exit-code families
are in [`docs/specs/public-sdk-and-cli.md`](docs/specs/public-sdk-and-cli.md).

## Installation, updates, and release

There is no supported `0.3.0` stable installer yet. For development, build and
run the repository-pinned source tree:

```sh
cargo build --locked --release -p okc
./target/release/okc doctor
```

The validated cargo-dist 0.32.0 plan targets Linux x86_64, Windows x86_64,
macOS x86_64, and macOS arm64, with shell/PowerShell installers, SHA-256 files,
a source archive, CycloneDX SBOM, and GitHub artifact attestations. These are
configured release outputs, not published artifacts.

`okc update [stable|latest|VERSION]` is receipt-aware. It updates only an
installation made by the matching cargo-dist installer, refuses to overwrite
a cargo/manual build, defaults to stable, treats `latest` as an explicit
prerelease opt-in, asks before installing, and emits no telemetry. Application
binaries still require Apple Developer ID signing/notarization or Windows
Authenticode with an RFC3161 timestamp; this is separate from unsigned
user-created `.okcpack` files.

## Stable-release blockers

- Complete `ALG-SEM-001`, manual-amendment review, sensitive
  exceptions, the schema-3 command adapter, V3 Pack, and attachment/Canvas/Base
  carry-through with link/provenance parity.
- PTY keyboard/cancellation and terminal-restoration system tests for the real
  TUI worker workflow.
- Streaming accepted files rather than bounded whole-member buffering, plus
  the 10-Vault/100,000-note/20 GB time and RSS benchmark.
- Descriptor-relative no-follow source traversal, Windows reparse defenses,
  fuzz/property campaigns, and broader Markdown/Canvas corpora.
- V1 project reconstruction, crash-injected end-to-end resume/cleanup, complete
  inner-loop progress/cancellation wiring, and cross-platform process-tree
  termination.
- Two complete four-platform CI passes on one commit, supply-chain audit
  evidence, protected signing credentials, native signature verification, and
  Apple notarization.

Stable publication is fail-closed: any unfinished QG-001 through QG-008 or
native-signing requirement prohibits publishing `0.3.0`.

## Design contract

- Normative specifications: [`docs/specs/`](docs/specs/)
- Stable and experimental algorithms: [`docs/algorithms/README.md`](docs/algorithms/README.md)
- Requirement-to-code/test map: [`docs/TRACEABILITY.md`](docs/TRACEABILITY.md)
- Accepted architectural decisions: [`docs/adr/`](docs/adr/)
- Current limitations and verified gates: [`docs/CURRENT_STATE.md`](docs/CURRENT_STATE.md)
- Fail-closed packaging and signing procedure: [`docs/RELEASE.md`](docs/RELEASE.md)
