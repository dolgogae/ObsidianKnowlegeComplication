# Obsidian Knowledge Compilation (OKC)

`okc` compiles immutable Obsidian Vault snapshots into a new deterministic,
auditable Compiled Vault. The Rust SDK owns inspection, planning, approval,
compilation, verification, and provenance. AI providers, MCP servers, and
Obsidian plugins are optional adapters.

The implementation contract starts at [`AGENTS.md`](AGENTS.md) and
[`docs/INDEX.md`](docs/INDEX.md).

> **Development status (2026-09-02):** the `0.2.0` OKC V2 tree is locally
> verified on macOS arm64 (323 tests plus doctests), but it is **not a public
> stable release**. Remote four-platform evidence, the reference performance
> run, fuzz/PTY coverage, and native signing/notarization are still required.
> See [`docs/CURRENT_STATE.md`](docs/CURRENT_STATE.md) and
> [`docs/RELEASE.md`](docs/RELEASE.md) before packaging or publishing it.

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
- accepts optional provider-neutral AI proposals as data, then validates their
  identities, evidence, and approvals;
- materializes only a sealed `ApprovedPlan` through a verified sibling stage
  and a native atomic no-replace directory commit, preserving any existing or
  concurrently created destination;
- creates deterministic `.okcpack` archives through one SDK/CLI verified,
  atomic no-clobber publisher and independently verifies their checksums, plan
  linkage, source-derived output commitments, audit files, and provenance.

User-created `.okcpack` files are deterministic and internally verifiable but
unsigned. Application release signing/notarization is a separate mandatory
release gate. See [`docs/CURRENT_STATE.md`](docs/CURRENT_STATE.md) for the exact
implementation and verification boundary.

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

## V2 format and V1 compatibility

| Surface | OKC V2 value |
|---|---|
| Format family | `okc` |
| Schema | `2` |
| Identity domain prefix | `okc:*:v2\0` |
| Audit directory | `.okc/` |
| Pack extension | `.okcpack` |
| Pack profile | `okc-tar-zstd-deterministic-v2` |

V2 writers never create `.vaultc`, `.vaultpack`, or `vaultc:*:v1` artifacts.
Frozen V1 artifacts and Packs remain available only through `okc verify` and
`okc explain`. A V1 project must eventually reconnect and verify its original
sources and generate a fresh V2 inspection and plan; that project-import
workflow is not implemented yet, and old approvals, AI proposals, and conflict
decisions will not migrate.

## Rust workspace

- `okc-core`: compiler library and public phased API;
- `okc-protocol`: provider-neutral capabilities, proposal, evidence, and
  transcript schemas;
- `okc-app`: project persistence and long-running application services;
- `okc`: the only executable, containing the CLI, Ratatui TUI, and supervised
  NDJSON subprocess adapter;
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

[`CI`](.github/workflows/ci.yml) defines locked, host-native test jobs for Linux
x86_64, Windows x86_64, macOS x86_64, and macOS arm64, plus formatting and
warning-free Clippy. Its OKCPack test compares the complete archive against a
single literal SHA-256 golden on every host. The workflow definition is not
release evidence by itself; the required same-commit remote runs have not yet
completed.

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
committed. The store uses WAL, foreign keys, an explicit schema-2 migration,
and a single-writer lock. Project data is intentionally not encrypted: source
paths, plans, decisions, and AI records are plaintext. OKC applies restrictive
local permissions where supported and warns about likely shared locations.

With an interactive terminal, running `okc` without a subcommand opens the
TUI; non-TTY execution prints help and exits `2`.

```sh
cargo run -p okc
cargo run -p okc -- tui
cargo run -p okc -- --project Team.okc-project
```

The current TUI implements the twelve-screen Ratatui/Crossterm navigation
shell, English/Korean labels, keyboard reducer, 80×24 fallback, high-contrast
and ASCII modes, hostile-control escaping, and terminal restoration. It does
not yet run the real inspect/plan/provider/compile workers or persist review
choices, so use the CLI or Rust API for end-to-end compilation today.

## Rust SDK example

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

## CLI example

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

The minimum decision document for a conflict-free, AI-free plan is:

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

There is no supported `0.2.0` stable installer yet. For development, build and
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

- Real TUI worker effects, persisted review flows, PTY keyboard/cancellation,
  and terminal-restoration system tests.
- Streaming accepted files rather than bounded whole-member buffering, plus
  the 10-Vault/100,000-note/20 GB time and RSS benchmark.
- Descriptor-relative no-follow source traversal, Windows reparse defenses,
  fuzz/property campaigns, and broader Markdown/Canvas corpora.
- V1 project reconstruction, resumable long operations, consistent progress
  and cancellation wiring, and cross-platform process-tree termination.
- Two complete four-platform CI passes on one commit, supply-chain audit
  evidence, protected signing credentials, native signature verification, and
  Apple notarization.

Stable publication is fail-closed: any unfinished QG-001 through QG-008 or
native-signing requirement prohibits publishing `0.2.0`.

## Design contract

- Normative specifications: [`docs/specs/`](docs/specs/)
- Stable and experimental algorithms: [`docs/algorithms/README.md`](docs/algorithms/README.md)
- Requirement-to-code/test map: [`docs/TRACEABILITY.md`](docs/TRACEABILITY.md)
- Accepted architectural decisions: [`docs/adr/`](docs/adr/)
- Current limitations and verified gates: [`docs/CURRENT_STATE.md`](docs/CURRENT_STATE.md)
- Fail-closed packaging and signing procedure: [`docs/RELEASE.md`](docs/RELEASE.md)
