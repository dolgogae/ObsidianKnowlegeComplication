# Vault Compiler Framework

`vaultc` compiles immutable Obsidian Vault snapshots into a new deterministic,
auditable Compiled Vault. The Rust SDK owns inspection, planning, approval,
compilation, verification, and provenance. AI providers, MCP servers, and
Obsidian plugins are optional adapters.

The implementation contract starts at [`AGENTS.md`](AGENTS.md) and
[`docs/INDEX.md`](docs/INDEX.md).

## What the framework does

The framework is the policy and integrity boundary between untrusted source
Vaults and a publishable result. It:

- seals directory, ZIP, and `tar.zst` inputs without changing them;
- parses Markdown/frontmatter/Obsidian links, resolves and rewrites typed Canvas
  file references, and inventories Bases and attachments in a canonical IR;
- plans exact deduplication, link rewrites, portable output paths, and typed
  conflicts before writing anything;
- accepts optional provider-neutral AI proposals as data, then validates their
  identities, evidence, and approvals;
- materializes only a sealed `ApprovedPlan` into a new destination;
- creates deterministic `.vaultpack` archives and independently verifies their
  checksums, plan linkage, source-derived output commitments, audit files, and
  provenance.

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

For AI-assisted builds, an application implements `KnowledgeAugmentor` or uses
the `vaultc-protocol` wire types, passes returned `KnowledgeProposal` values to
`validate_proposals`, and supplies content-hash-bound `ApprovalLog` decisions.
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
cargo run -p vaultc-cli -- approve plan.json --decisions decisions.json --out approved-plan.json
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
