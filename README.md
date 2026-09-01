# Vault Compiler Framework

`vaultc` compiles immutable Obsidian Vault snapshots into a new deterministic,
auditable Compiled Vault. The Rust SDK owns inspection, planning, approval,
compilation, verification, and provenance. AI providers, MCP servers, and
Obsidian plugins are optional adapters.

The implementation contract starts at [`AGENTS.md`](AGENTS.md) and
[`docs/INDEX.md`](docs/INDEX.md).

## What open-source project is this?

Vault Compiler Framework is an MIT/Apache-2.0 open-source toolkit for safely
combining knowledge from multiple Obsidian Vaults. It is a compiler rather than
a note-sync service: source Vaults are treated as immutable, potentially hostile
inputs; `vaultc` parses them into a canonical model, plans deduplication and link
rewrites, requires explicit decisions where meaning is ambiguous, and writes a
new independently verifiable Vault.

The project is intended to be embedded in:

- local developer tools and knowledge-management applications through the Rust
  SDK;
- reproducible automation and review workflows through the `vaultc` CLI;
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
- creates deterministic `.vaultpack` archives through one SDK/CLI verified,
  atomic no-clobber publisher and independently verifies their checksums, plan
  linkage, source-derived output commitments, audit files, and provenance.

The current `.vaultpack` format is deterministic but unsigned. Signing, MCP,
the Obsidian installer, the registry, and brain-inspired retrieval algorithms
remain later layers. See [`docs/CURRENT_STATE.md`](docs/CURRENT_STATE.md) for the
exact implementation and verification boundary.

## Rust workspace

- `vaultc`: compiler library and public phased API;
- `vaultc-protocol`: provider-neutral capabilities, proposal, evidence, and
  transcript schemas;
- `vaultc-cli`: the `vaultc` command-line application and supervised NDJSON
  subprocess adapter.

This repository currently targets Rust 1.97.1 and uses the Rust 2024 Edition.

## Build and test

```sh
cargo build --workspace
cargo test --workspace --all-features
```

The toolchain is pinned by [`rust-toolchain.toml`](rust-toolchain.toml). The
framework is dual-licensed under MIT OR Apache-2.0.

[`CI`](.github/workflows/ci.yml) defines locked, host-native test jobs for Linux
x86_64, Windows x86_64, macOS x86_64, and macOS arm64, plus formatting and
warning-free Clippy. Its VaultPack test compares the complete archive against a
single literal SHA-256 golden on every host.

## Rust SDK example

The API makes each state transition visible. A deterministic build without AI
looks like this:

```rust,ignore
use vaultc::{CompilerPolicy, SourceSpec, VaultCompiler};

fn main() -> vaultc::Result<()> {
    let compiler = VaultCompiler::builder()
        .policy(CompilerPolicy::default())
        .workspace(".vaultc-work/build.sqlite")
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
use vaultc::CompileOptions;

let artifact = compiler.compile_with_options(
    &approved,
    "./CompiledVault",
    &CompileOptions {
        create_pack: Some("./CompiledVault.vaultpack".into()),
    },
)?;
```

These are two ordered publications, not one cross-filesystem transaction. If
the later pack step fails, the error identifies the valid Compiled Vault that
remains; vaultc never exposes a partial requested pack file.

For AI-assisted builds, an application implements provider-neutral
`KnowledgeAugmentor` rather than depending on one LLM vendor:

```rust,ignore
use vaultc::{
    CancellationToken, DocumentSelection, KnowledgeAugmentor,
    RemoteProviderConsent, VaultCompiler,
};

fn record_and_replay(
    compiler: &VaultCompiler,
    plan: &vaultc::DraftPlan,
    augmentor: &impl KnowledgeAugmentor,
) -> vaultc::Result<vaultc::ValidatedProposals> {
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
Required conflicts use a separate immutable `ConflictDecisionLog` overlay. V1
accepts only the explicit `waived_by_policy` conflict resolution until typed
rewrite/target actions are defined.

## CLI example

Sources use `ID=PATH`; a bare path derives its ID from the final path component.
Input kind is detected from a directory or the `.zip`, `.tar.zst`, or `.tzst`
extension.

```sh
cargo run -p vaultc-cli -- inspect personal=./PersonalVault team=./TeamVault.zip --format json
cargo run -p vaultc-cli -- plan personal=./PersonalVault team=./TeamVault.zip --out plan.json
cargo run -p vaultc-cli -- augment plan.json --provider-cmd ./provider --all-documents --out augmentation.jsonl
cargo run -p vaultc-cli -- replay plan.json --augmentation augmentation.jsonl --out replayed.jsonl
cargo run -p vaultc-cli -- approve plan.json --decisions decisions.json --proposals replayed.jsonl --out approved-plan.json
cargo run -p vaultc-cli -- compile approved-plan.json --output CompiledVault --pack CompiledVault.vaultpack
cargo run -p vaultc-cli -- verify CompiledVault.vaultpack --format json
cargo run -p vaultc-cli -- explain CompiledVault.vaultpack knowledge/Topic.md --format json
```

The minimum decision document for a conflict-free, AI-free plan is:

```json
{
  "schema_version": 1,
  "plan_id": "<plan_id from plan.json>",
  "decisions": [],
  "conflicts": []
}
```

`plan` still writes `plan.json` but exits with code `4` when required conflicts
need decisions. `compile` requires absent output and pack destinations. The
complete command, decision schema, provider flow, and frozen exit-code families
are in [`docs/specs/public-sdk-and-cli.md`](docs/specs/public-sdk-and-cli.md).

## Design contract

- Normative specifications: [`docs/specs/`](docs/specs/)
- Stable and experimental algorithms: [`docs/algorithms/README.md`](docs/algorithms/README.md)
- Requirement-to-code/test map: [`docs/TRACEABILITY.md`](docs/TRACEABILITY.md)
- Accepted architectural decisions: [`docs/adr/`](docs/adr/)
- Current limitations and verified gates: [`docs/CURRENT_STATE.md`](docs/CURRENT_STATE.md)
