---
title: Public SDK and CLI Contract
status: normative-v1
owners:
  - core-rust-engineer
last_updated: 2026-09-05
decision_refs:
  - ADR-0001
  - ADR-0004
  - ADR-0009
  - ADR-0010
  - ADR-0011
  - ADR-0013
  - ADR-0014
  - ADR-0015
  - ADR-0017
  - ADR-0018
  - ADR-0019
  - ADR-0020
  - ADR-0022
  - ADR-0023
  - ADR-0024
  - ADR-0025
source_refs:
  - HIST-COMPILER-PLAN
---

# Public SDK and CLI Contract

## Schema-3 application workflow

`--project PATH` always selects that project. Otherwise schema-3 CLI commands
and the no-argument TUI discover the current directory's hidden default,
eligible sibling, and direct-child `.okc-project` candidates. Ambiguous
interactive discovery is reviewed in the TUI; non-interactive ambiguity or a
missing required project fails with a specific diagnostic. CLI and TUI call
the same `WorkspaceBootstrap`, `ProviderService`, and `IntegrationService`
implementations and never invoke each other.

The `0.3.0` application surface is project-first:

```text
okc project create PATH --name NAME --curator ID [--language BCP-47]
okc project upgrade V2_PROJECT --out V3_PROJECT
okc --project PATH project source add SOURCE_ID SOURCE_PATH
okc provider add|list|show|test|remove
okc --project PATH project ai-route set [ROLE] PROFILE
okc --project PATH integrate [--allow-remote-provider --yes]
okc --project PATH review taxonomy show|export|approve
okc --project PATH review cluster list|show|export|approve|regenerate
okc --project PATH integration status [--format json]
okc [--project PATH] compile --integration-plan FILE --output PATH
okc verify PATH
okc explain PATH OUTPUT_PATH
```

Project format schema 3 uses private application-state schema 4 to store
append-only source-set revisions, runs, task definitions/events, provider
exchanges, cluster revisions/feedback, approvals, plan pointers, and verified
outputs plus content-addressed objects.
`integrate` automatically resumes a task only when its complete cache key
matches. Taxonomy edits are imported as a complete cluster array and resealed
before approval. Cluster approval requires a separate exact ID/rationale entry
for every omission and minor critic waiver; major and critical findings cannot
be waived. Regeneration feedback binds the prior proposal/critic hashes and
creates a new synthesis plus critic revision that immediately stales old
authority.

The sealed integration plan is printed as a content-addressed project object.
`compile --integration-plan` makes no provider call, refuses an incomplete or
stale approval closure, and publishes a new directory without replacement.
`verify` and `explain` auto-detect schema 3, V2, and V1. ADR-0022 requires those
two read operations to become the complete public V1/V2 compatibility boundary.
The `0.3.0` development binary still exposes schema-2 writer commands for the
existing regression harness, so this part of the release contract is not yet
implemented and MUST NOT be described as complete.

The following planned surfaces are not implemented in this development slice:
manual section amendment, persisted sensitive-finding exceptions, V3 Pack, and
provider-backed PTY/platform qualification. The command adapter profile is
reserved but fails capability testing until its schema-3 supervisor lands.

## Frozen schema-2 SDK workflow

### SDK workflow

The public API exposes explicit, typed phases rather than one opaque merge
call. The implemented deterministic path is:

```rust,ignore
use okc_core::{CompilerPolicy, OkcCompiler, SourceSpec};

fn compile_vaults() -> okc_core::Result<()> {
    let compiler = OkcCompiler::builder()
        .workspace(".okc-work/build.sqlite")
        .policy(CompilerPolicy::from_file("okc.toml")?)
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
or constructs `okc-protocol` messages. `build_augmentation_request` creates
the exact sealed projection; `augment` records an in-process provider;
an external transport calls `authorize_augmentation_exchange` after capability
negotiation and before disclosure, then consumes that opaque authorization in
`record_augmentation_exchange`; and `replay_augmentation` performs
provider-free offline revalidation. The result converts to
`ValidatedProposals` for `approve` or
`approve_with_conflicts`. No path permits a provider to mutate or approve the
plan.

## Public operations

| Phase | Current Rust surface | Input | Output | Mutates destination? |
|---|---|---|---|---|
| inspect | `OkcCompiler::inspect` | `SourceSpec` values + policy | sealed `Inspection` | no; optional SQLite workspace is updated transactionally |
| plan | `OkcCompiler::plan` | `&Inspection` | immutable `DraftPlan` | no |
| augment | `OkcCompiler::{build_augmentation_request,augment,authorize_augmentation_exchange,record_augmentation_exchange}`; CLI `augment` | sealed plan + explicit selection + provider capabilities + live consent | canonical `RecordedAugmentation` | no |
| replay | `OkcCompiler::replay_augmentation`; CLI `replay` | sealed plan + canonical recording | provider-free revalidated recording | no |
| validate | `OkcCompiler::validate_proposals` | `&DraftPlan` + proposals | deterministic `ValidatedProposals` | no |
| approve | `approve`, `approve_with_conflicts`, `approve_without_augmentation` | owned `DraftPlan` + validation/decision logs | `ApprovedPlan` | no |
| compile | `OkcCompiler::{compile,compile_with_options}` | `&ApprovedPlan` + absent destination + optional pack path | `CompiledArtifact` | yes; the directory and optional pack are two ordered publications |
| pack | `okc_core::pack::create_pack`; CLI `compile --pack` | verified Compiled Vault + absent disjoint `.okcpack` path | deterministic pack | yes; sibling-stages, synchronizes, verifies, and atomically publishes without replacement |
| verify | `OkcCompiler::verify` | Compiled Vault or `.okcpack` | `VerificationReport` | no |
| explain | `OkcCompiler::explain_provenance_page`; bounded `explain_provenance` convenience | artifact + typed path/package query | versioned `ProvenancePage` or complete bounded explanation | no |

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

The `0.2.0` command surface is:

```text
okc [--project PATH]
okc tui [--project PATH]
okc [--policy FILE] [--workspace FILE] inspect SOURCE... [--format human|json]
okc [--policy FILE] [--workspace FILE] plan SOURCE... --out FILE [--format human|json]
okc augment PLAN --provider-cmd PROGRAM [--provider-arg ARG]...
    (--document-id DOCUMENT_ID... | --all-documents) --out FILE
    [--provider-working-directory DIRECTORY]
    [--provider-timeout-seconds SECONDS]
    [--provider-max-output-bytes BYTES]
    [--provider-max-line-bytes BYTES]
    [--provider-max-messages COUNT]
    [--allow-remote-provider]
okc replay PLAN --augmentation FILE --out FILE
okc validate PLAN --augmentation FILE
okc approve PLAN --decisions FILE [--proposals FILE] --out FILE
okc [--policy FILE] compile APPROVED_PLAN --output PATH
    [--pack FILE] [--format human|json]
okc verify PATH_OR_PACK [--format human|json]
okc explain PATH_OR_PACK OUTPUT_PATH
    [--limit COUNT] [--cursor CURSOR] [--format human|json]
okc explain PACK --package
    [--limit COUNT] [--cursor CURSOR] [--format human|json]
okc doctor
okc update [stable|latest|VERSION]
```

With no subcommand, a TTY starts the TUI. A non-TTY prints help and exits 2.
CLI and TUI call the same application/core services and MUST NOT spawn one
another. `validate` performs provider-free validation and writes no file.

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
The subprocess capability response is parsed strictly and passed through the
core authorization before any augmentation projection is written to the child.
The resulting authorization is an in-memory one-exchange token, not a stored
permission.

`replay` accepts only canonical schema-2 augmentation JSONL and never invokes a
provider, process, network, MCP server, or output compiler. It rehydrates the
redacted projection from the sealed plan, rebuilds the public request, checks
the exact four-record transcript, and reruns proposal validation. There is no
remote-consent flag because replay discloses no source text. A valid replay is
byte-identical to its input and is published atomically without overwrite.

The provider program is launched directly, never through a shell. Standard
output is NDJSON protocol-only. The CLI applies line/message/total-output and
deadline bounds, writes provider input on a supervised thread, and on deadline
or SIGINT terminates and reaps the provider process tree on supported Unix
platforms. It never publishes a partial augmentation file.

`compile` rejects an output or pack that exists. The directory publisher uses
the ADR-0014 native no-replace primitive, preserves a destination created after
preflight, and fails closed where that primitive is unsupported. The output is
and an optional integrated Pack are also rejected before staging if either
aliases, contains, or is nested within an immutable source. A pack path must end in
`.okcpack`, must be disjoint from the Compiled Vault in either containment
direction, and is sibling-staged and atomically published with no-clobber file
semantics. `OkcCompiler::compile_with_options` connects the public
`CompileOptions.create_pack` path; the simpler `compile` uses default options.
The directory compilation and subsequent pack creation are two publications,
not one combined transaction, so a runtime pack failure leaves the already
valid Compiled Vault and returns `PackPublicationAfterCompile`.

Caught staging cleanup or incomplete-marker failures retain the original
compilation/publication error in `StagingDispositionFailed`. A directory
parent-sync failure returns `PublishedButDurabilityUncertain`, leaves the
complete verified output visible, and does not begin optional Pack publication.

Every caught failure before pack publication leaves no okc-created file at
the requested pack destination. A parent-directory synchronization failure
after publication returns `PublishedButDurabilityUncertain`; the complete pack
is retained because deleting it would not restore atomicity and could destroy
an observable result. Standalone pack creation independently verifies both its
input Compiled Vault and staged pack, but that integrity check is not publisher
authentication.

`explain` emits schema-versioned typed provenance. Its default and hard limits
are 256/4,096 records and 4/16 MiB respectively. A returned cursor is bound to
the graph, subject, and last global sort key. Package queries are valid only
for `.okcpack` and return virtual outer-package integrity records; inner-path
queries remain identical between a directory and its pack. Invalid query
limits are exit `2`; malformed, stale, or cross-artifact cursors are exit `7`.

## Decision document

`approve --decisions` consumes schema version 2:

```json
{
  "schema_version": 2,
  "plan_id": "plan_...",
  "decisions": [
    {
      "plan_id": "plan_...",
      "proposal_id": "proposal-1",
      "proposal_content_hash": "<canonical content hash>",
      "approved": true,
      "approver": "curator-id",
      "policy_version": "team-policy-v2"
    }
  ],
  "conflicts": [
    {
      "plan_id": "plan_...",
      "conflict_id": "conflict_...",
      "conflict_content_hash": "<sealed conflict hash>",
      "action": { "type": "waive_preserve_original" },
      "decided_by": "curator-id",
      "policy_version": "team-policy-v2",
      "rationale": "retain the ambiguous source representation"
    }
  ]
}
```

`decisions` may be empty when no augmentation is approved. `conflicts` may be
empty when the plan has no unresolved required conflict. A conflict action is
`waive_preserve_original`, `select_markdown_target` with a sealed
`target_document_id`, or `select_canvas_target` with a sealed typed target.
Free-form replacement strings and targets outside the candidate set fail
closed. ADR-0017 supersedes the V1 waiver-only restriction in ADR-0009.

## Output and exit behavior

Machine-readable command output uses JSON or the versioned augmentation JSONL
file. Human output goes to standard output; diagnostics go to standard error.
Secrets and complete source content MUST NOT be logged by default. Human text
is not stable; JSON field meanings, schema versions, diagnostic codes, and exit
families are compatibility surfaces.

The numeric exit codes are frozen for the `0.2.x` CLI:

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
schema versions. CLI human text is not stable. V2 writers emit schema 2 only.
`okc verify` and `okc explain` additionally detect frozen V1 Compiled Vaults
and `.vaultpack` inputs and dispatch to a read-only reader. Migration rebuilds
V2 state from relinked, identity-verified sources and never carries V1
approvals, proposals, or conflict decisions forward. The pre-release schema 1
`RewriteMarkdown.expected_output_hash` field is required, so earlier
working-tree plans without it fail closed rather than receiving a default.
The same pre-release schema-completion rule applies to the required approved
proposal materialization field; earlier working-tree approval files without it
fail closed.
AI-bearing development approvals with non-empty validations and an empty
transcript now also fail closed. Canonical schema-2 CLI recordings retain their
wire shape; non-canonical JSONL that earlier CLI readers tolerated has no
compatibility alias.
Deprecations require one minor-version migration window before `1.0` where
practical and two after `1.0`.

## Language bindings

V2 offers the Rust crates, TUI, and CLI/JSON/NDJSON interoperability. Python and Node
bindings are later conveniences, not separate compiler implementations. Other
languages SHOULD invoke the CLI or implement the subprocess protocol until
stable bindings exist.
