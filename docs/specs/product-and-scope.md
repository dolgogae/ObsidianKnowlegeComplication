---
title: Product and Scope Specification
status: normative
owners:
  - architect
last_updated: 2026-09-06
decision_refs:
  - ADR-0001
  - ADR-0002
  - ADR-0003
  - ADR-0019
  - ADR-0022
  - ADR-0023
  - ADR-0024
  - ADR-0025
  - ADR-0026
  - ADR-0027
source_refs:
  - HIST-KNOWLEDGE-PLATFORM
  - HIST-COMPILER-PLAN
---

# Product and Scope Specification

## Mission

Compile heterogeneous Obsidian Vault snapshots into a new, safe,
deterministic, evidence-traceable knowledge artifact. Libraries, the CLI, and
the TUI orchestrate one shared policy; providers and external adapters never
own canonical state or approval authority.

## Current users and surfaces

| User | Job | Current surface |
|---|---|---|
| Rust developer | Build a sealed corpus or validate/materialize an approved integration | `okc-core` |
| Python/Node.js developer | Automate an explicit-path project and approval workflow | `okc-compiler` packages |
| Build/release engineer | Compile and independently verify a directory | `okc` CLI |
| Knowledge curator | Review taxonomy, synthesis, critic findings, and provenance | shared CLI/TUI services |
| AI integration developer | Configure a supported local or hosted provider | `okc-ai` and `okc-app` profiles |
| Coding-agent/plugin developer | Add a least-authority adapter | future MCP/plugin packages |

## Functional requirements

- **REQ-SNP-001:** The compiler MUST NOT modify a source Vault's bytes,
  permissions, or timestamps.
- **REQ-SNP-002:** Every accepted source file, snapshot, document, block, and
  output MUST retain a stable content identity in its existing domain.
- **REQ-SRC-001:** MCP names and implementation metadata MUST NOT influence
  source IDs, the corpus, approvals, or output.
- **REQ-SRC-002:** Source order MUST NOT affect the sealed corpus or compiled
  bytes. Duplicate source IDs and duplicate whole-Vault content MUST fail.
- **REQ-PAR-001:** Markdown, YAML frontmatter, links, headings, block
  references, tags, callouts, code, math, and ordinary links MUST be parsed
  without destroying source spans.
- **REQ-PAR-002:** JSON Canvas references and unknown fields MUST be parsed by
  the retained safety pipeline; current materialization MUST NOT claim Canvas
  output support until its rewrite gate passes.
- **REQ-PAR-003:** Base files MUST be treated as opaque hostile input; current
  materialization MUST NOT claim Base output support.
- **REQ-DED-001:** Exact content identities MUST remain deterministic and retain
  source provenance.
- **REQ-DED-002:** Semantic or near-duplicate grouping MUST enter only as a
  proposal and MUST NOT bypass synthesis, critic, or curator approval.
- **REQ-CNF-001:** Contradictory claims MUST preserve their source, time, and
  context and MUST NOT be decided by majority vote.
- **REQ-MAT-001:** Materialization MUST derive only from one immutable,
  validated `ApprovedIntegrationPlan`.
- **REQ-PRV-001:** Every materialized canonical note and redirect stub MUST
  have closed provenance to source evidence, proposal, critic, and approval.
- **REQ-AI-001:** AI integration MUST be provider-neutral and
  capability-driven.
- **REQ-AI-002:** Provider output MUST be treated as hostile proposal data and
  pass strict schema, bounds, identity, evidence, and closure validation.
- **REQ-AI-003:** Provider requests and responses used by an integration MUST
  be recorded for deterministic replay and offline compilation.
- **REQ-AI-004:** Every build MUST use approved taxonomy and per-cluster
  synthesis/critic records; final compile MUST make no live provider call.
- **REQ-CMP-001:** Compilation MUST stage beside an absent destination,
  validate the stage, and publish without replacement.
- **REQ-CMP-002:** A future current-schema `.okcpack` writer MUST reproduce
  identical bytes for identical approved inputs. No Pack writer is currently
  exposed.
- **REQ-CMP-003:** Current artifacts MUST use Schema 3. Recognizable Schema 1 or
  Schema 2 inputs MUST return `ARTIFACT_SCHEMA_UNSUPPORTED` with
  `supported_schema = 3` and detected details. No retired reader, writer,
  migration, or alias is permitted on main.
- **REQ-SEC-001:** Directories, archives, symlinks, paths, Markdown, metadata,
  manifests, journals, and provider output MUST be treated as hostile data.
- **REQ-SEC-002:** Content MUST be scanned before disclosure. Remote calls
  require explicit per-call consent, and sensitive semantic work MUST be
  locally routed.
- **REQ-SEC-003:** Secrets MUST resolve only from a named process environment
  variable or an opaque native keychain account. Values and lengths MUST NOT
  enter stored state, recordings, diagnostics, debug output, or UI output.
- **REQ-INT-001:** Every Markdown document MUST occur in exactly one approved
  taxonomy cluster; singleton clusters are not exempt.
- **REQ-INT-002:** Every source block and frontmatter value MUST have exactly
  one `integrated`, `preserved_verbatim`, or `omission_proposed` disposition.
- **REQ-INT-003:** Every non-empty synthesized section MUST cite current source
  evidence; contradictory claims MUST retain all cited sides.
- **REQ-INT-004:** A critic MUST compare synthesis with the complete cluster
  inventory. Critical/major findings block; minor findings require an exact
  waiver.
- **REQ-INT-005:** Taxonomy, cluster, omission, finding-waiver, and plan
  approvals MUST be immutable and hash-bound. A dependency change makes old
  authority stale.
- **REQ-INT-006:** Current materialization MUST emit canonical Markdown notes
  and source redirect stubs with reproducible bytes and closed provenance.
- **REQ-SDK-001:** Public Rust and CLI surfaces MUST expose only current corpus,
  project, provider, integration, review, compile, directory verify, and
  directory explain operations. Retired phase commands and Pack options MUST
  remain absent.
- **REQ-SDK-002:** Python and Node.js MUST use one Rust interop facade, explicit
  absolute paths, per-call remote consent, bounded cancellable jobs, stable
  structured errors, typed interop-schema-2 results, and equivalent artifact
  bytes without CLI/TUI side effects.
- **REQ-APP-001:** One `okc` executable MUST expose CLI and TUI adapters backed
  by the same `okc-app` services and private resumable project format.
- **REQ-APP-002:** Cwd discovery MUST be bounded and deterministic; projects and
  output MUST remain outside every source; long work MUST use bounded workers
  with cancellation before publication.
- **REQ-REL-001:** A stable release MUST pass QG-001 through QG-008 on every
  supported target and include checksums, SBOMs, attestations, and required
  native signatures.
- **REQ-MCP-001:** A future MCP server MUST remain a stateless,
  least-authority adapter over public application calls.
- **REQ-OBS-001:** A future generic Obsidian plugin MAY review and install many
  packages; package-specific executable plugins are forbidden.
- **REQ-PERF-001:** Before stable release, the 10-Vault, 100,000-note, 20 GB
  workload and semantic candidate path MUST have time and peak-RSS evidence.
- **REQ-MEM-001:** Experimental memory/retrieval models MUST remain isolated
  from the default compiler path until promoted by ADR and evaluation gates.

## Inputs and outputs

`CorpusBuilder` accepts typed directory, ZIP, and `tar.zst`/`.tzst` sources with
project-unique source IDs. The safety pipeline excludes `.obsidian/**` and
`.git/**`, rejects traversal and unsafe archive members, does not follow source
symlinks, applies bounded resource policy, and can persist private inspection
state in SQLite.

The current output is a new Schema 3 directory containing canonical
`knowledge/` notes, source-specific `legacy/` redirects, and `.okc/` manifest,
approved-plan, provenance, and checksums. It is Markdown-only today. A Pack,
attachments, Canvas, Base, and complete link rewriting remain future release
work.

## Success criteria

A cold-start developer can integrate multiple immutable Vaults, inspect and
approve complete taxonomy and cluster records, compile the same bytes without
a provider, verify the directory independently, explain every output path, and
switch capable providers without changing core policy. Existing Schema 3
projects and fixture bytes continue to work without migration.
