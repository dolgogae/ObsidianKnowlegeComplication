---
title: Public SDK and CLI Contract
status: normative-v1
owners:
  - core-rust-engineer
last_updated: 2026-08-16
decision_refs:
  - ADR-0001
  - ADR-0004
  - ADR-0009
  - ADR-0010
source_refs:
  - HIST-COMPILER-PLAN
---

# Public SDK and CLI Contract

## SDK workflow

The public API exposes explicit, typed phases rather than one opaque merge
call. The implemented deterministic path is:

```rust,ignore
use vaultc::{CompilerPolicy, SourceSpec, VaultCompiler};

fn compile_vaults() -> vaultc::Result<()> {
    let compiler = VaultCompiler::builder()
        .workspace(".vaultc-work/build.sqlite")
        .policy(CompilerPolicy::from_file("vaultc.toml")?)
        .build()?;

    let inspection = compiler.inspect([
        SourceSpec::directory("personal", "./PersonalVault")?,
        SourceSpec::archive("team", "./TeamVault.zip")?,
    ])?;
    let draft = compiler.plan(&inspection)?;
    draft.validate_integrity()?;

    let approved = compiler.approve_without_augmentation(draft)?;
    let artifact = compiler.compile(&approved, "./CompiledVault")?;
    let report = compiler.verify(&artifact.path)?;
    assert!(report.valid);
    let explanation = compiler.explain_provenance(
        &artifact.path,
        "knowledge/topic.md",
    )?;
    println!("{explanation:#?}");
    Ok(())
}
```

For augmentation, a Rust application implements `KnowledgeAugmentor::propose`
or constructs `vaultc-protocol` messages, then calls
`VaultCompiler::validate_proposals`. It passes the returned
`ValidatedProposals`, an `ApprovalLog`, and, when needed, a
`ConflictDecisionLog` to `approve` or `approve_with_conflicts`.

The current `0.1.0` SDK does not expose `VaultCompiler::augment` or a public
plan-to-`AugmentationRequest` projection builder. That is an explicit
REQ-SDK-001 gap, not permission for applications to bypass validation. The CLI
contains the reference projection builder and supervised subprocess adapter.

## Public operations

| Phase | Current Rust surface | Input | Output | Mutates destination? |
|---|---|---|---|---|
| inspect | `VaultCompiler::inspect` | `SourceSpec` values + policy | sealed `Inspection` | no; optional SQLite workspace is updated transactionally |
| plan | `VaultCompiler::plan` | `&Inspection` | immutable `DraftPlan` | no |
| augment | `KnowledgeAugmentor::propose`; CLI `augment` | selected plan projection + capabilities | untrusted proposals + transcript | no |
| validate | `VaultCompiler::validate_proposals` | `&DraftPlan` + proposals | deterministic `ValidatedProposals` | no |
| approve | `approve`, `approve_with_conflicts`, `approve_without_augmentation` | owned `DraftPlan` + validation/decision logs | `ApprovedPlan` | no |
| compile | `VaultCompiler::compile` | `&ApprovedPlan` + absent destination | `CompiledArtifact` | yes; sibling-stages then renames a new directory; portable race-free no-clobber remains open |
| pack | `vaultc::pack::create_pack`; CLI `compile --pack` | verified Compiled Vault + absent `.vaultpack` path | deterministic pack | yes; SDK writes directly, CLI stages/no-clobbers |
| verify | `VaultCompiler::verify` | Compiled Vault or `.vaultpack` | `VerificationReport` | no |
| explain | `VaultCompiler::explain_provenance_page`; bounded `explain_provenance` convenience | artifact + typed path/package query | versioned `ProvenancePage` or complete bounded explanation | no |

All serialized plans and approvals are untrusted control files. Approval,
compilation, and verification revalidate sealed plan/proposal/conflict
identities. Applications MUST NOT mutate a `DraftPlan` and retain its original
`plan_id`.

Each `ApprovedProposal` has a required tagged materialization. An advisory
proposal is `non_materializing`; a generated note seals its destination,
canonical emitted-body hash, complete rendered-output hash, ordered
`EvidenceId` values, and operation ID. Missing, mismatched, or stale
materialization fields fail approval, compilation, and verification rather
than receiving a compatibility default.

Planning reopens each source that contains a materialized Markdown rewrite once
to compute its exact expected output hash; it does not serialize those source
bytes into the plan. A changed, missing, unreadable, malformed, unsafe, or
unsupported source during this phase is CLI input exit `3`; invalid policy is
exit `2`, stale planning state or required decisions are exit `4`, and an
unexpected planning invariant is exit `70`. `approve` treats a malformed or
pre-output-hash `DraftPlan` as a plan/decision error (`4`), not a provider error.

## CLI commands

The frozen `0.1.0` command forms are:

```text
vaultc [--policy FILE] [--workspace FILE] inspect SOURCE... [--format human|json]
vaultc [--policy FILE] [--workspace FILE] plan SOURCE... --out FILE [--format human|json]
vaultc augment PLAN --provider-cmd PROGRAM [--provider-arg ARG]...
    (--document-id DOCUMENT_ID... | --all-documents) --out FILE
    [--provider-working-directory DIRECTORY]
    [--provider-timeout-seconds SECONDS]
    [--provider-max-output-bytes BYTES]
    [--provider-max-line-bytes BYTES]
    [--provider-max-messages COUNT]
    [--allow-remote-provider]
vaultc approve PLAN --decisions FILE [--proposals FILE] --out FILE
vaultc [--policy FILE] compile APPROVED_PLAN --output PATH
    [--pack FILE] [--format human|json]
vaultc verify PATH_OR_PACK [--format human|json]
vaultc explain PATH_OR_PACK OUTPUT_PATH
    [--limit COUNT] [--cursor CURSOR] [--format human|json]
vaultc explain PACK --package
    [--limit COUNT] [--cursor CURSOR] [--format human|json]
```

`SOURCE` is `ID=PATH`. A bare path derives an ASCII-safe ID from its final
component. Directories and the `.zip`, `.tar.zst`, and `.tzst` extensions are
recognized. Source IDs must be unique and contain only ASCII alphanumerics,
`-`, `_`, or `.`.

`plan` writes the sealed plan before returning exit `4` when required conflicts
need decisions. `augment` requires an explicit disclosure selection: one or
more `--document-id` flags or `--all-documents`. Each selected document's
current projection includes all parsed blocks; selective block projection is
not implemented. Each projection includes the sealed owning snapshot ID plus
document/block IDs and hashes so a stateless provider can return valid evidence.
Remote capability declarations require both a policy that allows remote
providers and the per-command `--allow-remote-provider` consent.

The provider program is launched directly, never through a shell. Standard
output is NDJSON protocol-only. The CLI applies line/message/total-output and
deadline bounds, writes provider input on a supervised thread, and on deadline
or SIGINT terminates and reaps the provider process tree on supported Unix
platforms. It never publishes a partial augmentation file.

`compile` rejects an output or pack that exists when checked. A pack path must end in
`.vaultpack`, must be outside the Compiled Vault tree, and is published with
no-clobber file semantics. The directory compilation and subsequent pack
creation are two publications, not one combined transaction. The V1 contract
still requires closing the cross-platform destination check/rename race before
release qualification.

`explain` emits schema-versioned typed provenance. Its default and hard limits
are 256/4,096 records and 4/16 MiB respectively. A returned cursor is bound to
the graph, subject, and last global sort key. Package queries are valid only
for `.vaultpack` and return virtual outer-package integrity records; inner-path
queries remain identical between a directory and its pack. Invalid query
limits are exit `2`; malformed, stale, or cross-artifact cursors are exit `7`.

## Decision document

`approve --decisions` consumes schema version 1:

```json
{
  "schema_version": 1,
  "plan_id": "plan_...",
  "decisions": [
    {
      "plan_id": "plan_...",
      "proposal_id": "proposal-1",
      "proposal_content_hash": "<canonical content hash>",
      "approved": true,
      "approver": "curator-id",
      "policy_version": "team-policy-v1"
    }
  ],
  "conflicts": [
    {
      "plan_id": "plan_...",
      "conflict_id": "conflict_...",
      "conflict_content_hash": "<sealed conflict hash>",
      "resolution": "waived_by_policy",
      "resolver": "curator-id",
      "policy_version": "team-policy-v1",
      "rationale": "retain the ambiguous source representation"
    }
  ]
}
```

`decisions` may be empty when no augmentation is approved. `conflicts` may be
empty when the plan has no unresolved required conflict. Per ADR-0009, V1
rejects `user_resolved` and every other external conflict resolution until a
typed target/rewrite action exists.

## Output and exit behavior

Machine-readable command output uses JSON or the versioned augmentation JSONL
file. Human output goes to standard output; diagnostics go to standard error.
Secrets and complete source content MUST NOT be logged by default. Human text
is not stable; JSON field meanings, schema versions, diagnostic codes, and exit
families are compatibility surfaces.

The numeric exit codes are frozen for the `0.1.x` CLI:

| Code | Family |
|---:|---|
| `0` | success |
| `2` | invocation, argument, or policy usage error |
| `3` | unsafe, missing, unreadable, malformed, or unsupported input source |
| `4` | planning conflict or conflict-decision error |
| `5` | provider, augmentation, proposal, or proposal-approval error |
| `6` | approved-plan, destination, compilation, or pack publication error |
| `7` | artifact verification or provenance explanation error |
| `70` | internal invariant failure |

## Compatibility

Rust APIs follow SemVer. JSON schemas and NDJSON envelopes carry independent
schema versions. CLI human text is not stable. The current verifier accepts
only its exact schema/compiler version; migration and compatibility matrices
are not implemented yet. The pre-release schema 1
`RewriteMarkdown.expected_output_hash` field is required, so earlier
working-tree plans without it fail closed rather than receiving a default.
The same pre-release schema-completion rule applies to the required approved
proposal materialization field; earlier working-tree approval files without it
fail closed.
Deprecations require one minor-version migration window before `1.0` where
practical and two after `1.0`.

## Language bindings

V1 offers the Rust crates and CLI/JSON/NDJSON interoperability. Python and Node
bindings are later conveniences, not separate compiler implementations. Other
languages SHOULD invoke the CLI or implement the subprocess protocol until
stable bindings exist.
