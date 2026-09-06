---
title: Glossary
status: normative-v1
owners:
  - architect
last_updated: 2026-09-06
decision_refs:
  - ADR-0003
  - ADR-0015
  - ADR-0017
  - ADR-0022
  - ADR-0024
  - ADR-0026
source_refs:
  - HIST-KNOWLEDGE-PLATFORM
---

# Glossary

| Term | Definition |
|---|---|
| Vault | A directory or archive containing Obsidian-compatible knowledge files and assets. |
| Vault snapshot | An immutable, content-addressed observation of one Vault at a point in time. |
| Source ID | User-supplied or derived stable identifier distinguishing input Vaults. |
| Document | A parsed Markdown file plus normalized metadata and structural blocks. |
| Block | The smallest provenance-addressable Markdown region used by the compiler. |
| Canonical IR | Versioned internal representation used for planning and compilation; the framework's source of truth. |
| Evidence reference | A pointer to a source snapshot, document/block ID, content hash, and optional source span that supports a proposal or claim. |
| Claim | A proposition represented with context, evidence, time, and confidence; conflicting claims may coexist. |
| Compilation plan | Deterministic, reviewable set of intended output operations, conflicts, and diagnostics. |
| Materialization plan | Immutable effective operation set derived from one Draft Plan, sealed conflict actions, and approved proposals. |
| Proposal | Provider-generated suggestion that is untrusted until locally validated and explicitly approved; V3 requires proposals but never trusts them as authority. |
| Approval | Immutable user or policy decision permitting a validated proposal to enter a particular plan revision. |
| Integration corpus | Schema-3 sealed inventory of all Markdown documents, blocks, and individual frontmatter values. |
| Taxonomy | Complete, exactly-once assignment of every V3 Markdown document to an approved cluster and canonical path. |
| Disposition | Exactly one V3 treatment for a block or metadata value: integrated, preserved verbatim, or omission proposed. |
| Critic report | Independent structured comparison of a synthesis proposal with its complete source evidence; major/critical findings block approval. |
| Approved integration plan | Complete schema-3 offline compilation authority binding corpus, taxonomy, synthesis, critic, approvals, waivers/omissions, and provider recordings. |
| Interop DTO | Runtime-neutral, schema-versioned value crossing the shared Rust facade into a language binding; Python projects names as snake_case and Node.js as camelCase. |
| Job | Bounded asynchronous filesystem/provider/compiler operation with progress events, cooperative cancellation, publication-barrier semantics, and a retained terminal result. |
| Provider profile | Immutable non-secret provider configuration. Language bindings may name a process environment variable but never accept or persist its raw API-key value. |
| Language binding | Thin Python or Node.js adapter over `okc-interop`; it shares project and compiler policy and never owns canonical state. |
| Compiled Vault | Newly materialized Obsidian-compatible output containing selected and generated knowledge, not embedded raw source snapshots. |
| OKCPack | Deterministic `tar.zst` distribution artifact with content, manifest, provenance, and checksums; V2 is implemented, while the V3 profile remains unimplemented. |
| MCP adapter | Thin Model Context Protocol surface over framework operations or an external retrieval engine. |
| Derived index | Replaceable BM25, vector, or graph representation built from canonical data; never the source of truth. |
| Conflict | Two or more inputs or decisions that cannot safely share the same semantic or output identity without explicit handling. |
| Exact duplicate | Inputs whose canonical content and normalized frontmatter hashes match. |
| Near duplicate | Similarity candidate above the configured MinHash threshold; never automatically merged in V2. |
| Deterministic core | Compiler path whose result depends only on versioned inputs, configuration, and approved replay transcript. |
| Knowledge compilation | Parsing, normalizing, reconciling, planning, validating, and materializing knowledge with provenance. |
| Consolidation | Experimental promotion of repeated or useful episodic items into generalized knowledge; not a V2 file-compiler behavior. |
| Interference | Retrieval penalty for redundant or competing items that consume context without useful gain. |
