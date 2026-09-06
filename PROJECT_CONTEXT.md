---
title: Obsidian Knowledge Compilation Project Context
status: normative
owners:
  - architect
last_updated: 2026-09-06
decision_refs:
  - ADR-0001
  - ADR-0002
  - ADR-0003
  - ADR-0004
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

# Obsidian Knowledge Compilation (OKC)

OKC compiles immutable Obsidian Vault snapshots into a new deterministic,
auditable Vault. It is a knowledge compiler rather than a folder merger:
Markdown and frontmatter enter a canonical corpus, every document belongs to
an approved taxonomy cluster, AI produces evidence-complete proposals, a
critic evaluates them, and a curator explicitly approves the materialized
result. Provider exchanges are recorded so final compilation and verification
remain offline and deterministic.

## Current product boundary

The repository implements one Schema 3 product at version `0.3.0`:

- a Rust `okc-core` library for safe corpus construction, integration records,
  offline compilation, directory verification, and provenance explanation;
- shared `okc-app` services for projects, journals, providers, disclosure,
  review, credentials, workers, and updates;
- one `okc` CLI/TUI;
- API-v1 Python and Node.js packages over interop DTO schema 2.

MCP servers, Obsidian plugins, web services, and registries are adapters or
future products. They do not own canonical state, approval authority, or
compilation policy. The compiler never identifies or federates an MCP server
that produced a source Vault.

Main has no Schema 1 or Schema 2 implementation, reader, migration, alias, or
deprecated facade. Recognizable retired artifacts receive
`ARTIFACT_SCHEMA_UNSUPPORTED`; other invalid artifacts fail closed. Historical
source is recoverable from the annotated `archive/v0.1.0` and
`archive/v0.2.0` tags defined by ADR-0027.

## Schema 3 contract

- Every Markdown document belongs to exactly one approved taxonomy cluster.
- Every source block and frontmatter value has exactly one disposition.
- Every cluster, including a singleton, requires structured synthesis,
  evidence closure, a critic report, and curator approval.
- `ApprovedIntegrationPlan` is the only compile authority. Compilation invokes
  no provider and publishes a new directory without replacement.
- Sensitive-content preflight precedes disclosure. Remote calls require an
  approved route and explicit consent for that call; sensitive semantic work
  remains local.
- Canonical notes live below `knowledge/`; source-specific redirect stubs below
  `legacy/` preserve navigation and provenance.
- Artifacts use schema 3 and the existing `okc:*:v3\0` domains. Projects use
  schema 3 with a private append-only SQLite journal at schema 4.
- Existing Schema 3 JSON, identities, database identifiers, paths, and bytes
  are compatibility invariants and require no migration.
- `cd <workspace> && okc` performs bounded project/Vault discovery. CLI, TUI,
  Python, and Node.js call shared application services.

The implemented materializer is currently Markdown-only. Attachment, Canvas,
and Base carry-through, complete link rewriting, and a current OKCPack writer
remain release blockers and MUST NOT be presented as implemented.

## Durable principles

1. Source Vaults are immutable, hostile inputs.
2. Canonical IR and recorded evidence are the source of truth; indexes and
   provider output are replaceable derivatives.
3. AI returns proposals only. Local schema/evidence validation and explicit
   approval precede compilation.
4. Conflicting claims retain source, time, and context instead of being decided
   by majority vote.
5. Hash-bound changes invalidate dependent authority rather than rewriting it.
6. Safe materialization stages and verifies an absent destination before
   no-replace publication.
7. Experimental algorithms cannot affect the default compiler path without an
   accepted ADR and quality-gate evidence.

## Non-goals

- Executing source plugins, scripts, or uploaded code.
- Treating synthesis or model confidence as objective truth.
- Losslessly interpreting Obsidian Base semantics in the current release.
- Shipping vendor-specific model SDKs in the compiler core.
- Claiming a stable release before all required quality gates pass.

## Read next

- Navigation and roles: [`docs/INDEX.md`](docs/INDEX.md)
- Current implementation: [`docs/CURRENT_STATE.md`](docs/CURRENT_STATE.md)
- Product requirements: [`docs/specs/product-and-scope.md`](docs/specs/product-and-scope.md)
- Architecture: [`docs/specs/framework-architecture.md`](docs/specs/framework-architecture.md)
- Algorithms: [`docs/algorithms/README.md`](docs/algorithms/README.md)
- Current-only decision: [`docs/adr/0027-current-schema-single-source.md`](docs/adr/0027-current-schema-single-source.md)
