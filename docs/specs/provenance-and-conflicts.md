---
title: Provenance, Deduplication, and Conflicts
status: normative
owners:
  - architect
  - core-rust-engineer
last_updated: 2026-09-06
decision_refs:
  - ADR-0003
  - ADR-0006
  - ADR-0009
  - ADR-0010
  - ADR-0012
  - ADR-0016
  - ADR-0022
  - ADR-0024
  - ADR-0027
source_refs:
  - HIST-KNOWLEDGE-PLATFORM
  - HIST-COMPILER-PLAN
---

# Provenance, Deduplication, and Conflicts

## Provenance invariant

Every current output path has exactly one canonical `ProvenanceRecord`. A
canonical note record binds its output hash, integration plan, taxonomy
cluster, synthesis proposal, critic report, approved cluster revision, and all
ordered supporting evidence. A source redirect record binds the same plan plus
its source document and blocks. Generated content without current evidence is
invalid.

The verifier rederives materialized bytes and provenance from the embedded
approved plan, compares the complete manifest/checksum inventory, and rejects
missing, additional, stale, or duplicate records. Merely naming an unrelated
valid source is not provenance.

`explain(root, output_path)` and `ArtifactService::explain` first perform full
verification, validate a safe relative output path, and return exactly one
typed `ProvenanceRecord`. They do not contact a provider or read an original
Vault. Missing, duplicate, unsafe, or corrupt explanations fail closed. There
is no Pack, package-level, pagination, cursor, or generic artifact-family
explanation surface.

## Approval and invalidation

- Taxonomy changes stale every cluster proposal and approval.
- Proposal or synthesis revision changes stale its critic and approval.
- Critic changes stale the cluster approval.
- Omission authority binds the exact disposition target/content hash, curator,
  policy, and rationale.
- Minor waivers bind the exact finding ID, target report, curator, and
  rationale.
- Source, policy, route, schema, prompt, or recording changes create a new
  dependent identity.

No plan or historical journal record is edited in place to preserve old
authority. Validation always rederives the hash-bound closure before compile.

## Duplicate and conflict policy

Exact identities use deterministic canonicalization and retain all source
origins. Near or semantic similarity generates proposals only; it cannot
delete, replace, or merge content without the complete synthesis, critic, and
approval workflow.

Current contradiction records contain at least two context-scoped claims with
independent source evidence. The compiler preserves every supported side and
does not grant providers or a majority vote authority to choose truth.

Path, case-fold, Unicode, title, alias, frontmatter, and link ambiguity are
handled in the retained private parser/analysis safety path. Unsafe or
unrepresentable ambiguity fails closed. Those internal records are not a
retired public draft-plan/decision API.

## Stored representation

`.okc/provenance.jsonl` contains canonical Schema 3 records with one JSON value
per non-empty line and a final LF. `CompiledVaultManifest.files` inventories
the artifact excluding the manifest/checksum recursion boundary; checksums
then cover the finalized inventory according to the compile algorithm.

The provenance ledger cannot contain its own final hash. Manifest, checksums,
and provenance therefore form the explicitly verified audit envelope rather
than a forged self-reference. Artifact verification proves internal
consistency and reproducibility, not publisher authenticity.

The output directory name `legacy/` denotes current source redirect stubs. It
does not mean the artifact includes or can decode an older schema.

## Research boundary

Future probabilistic Claim records may model time/context-scoped propositions,
but confidence is evidence support rather than a truth decree. Correlated
copies do not count as independent confirmation. Such records and experimental
memory scores cannot affect current compilation without promotion through the
algorithm and ADR gates.
