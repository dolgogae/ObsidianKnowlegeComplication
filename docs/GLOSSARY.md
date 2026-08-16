---
title: Glossary
status: normative-v1
owners:
  - architect
last_updated: 2026-08-16
decision_refs:
  - ADR-0003
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
| Proposal | Optional provider-generated suggestion that is untrusted until validated and explicitly approved. |
| Approval | Immutable user or policy decision permitting a validated proposal to enter a particular plan revision. |
| Compiled Vault | Newly materialized Obsidian-compatible output containing selected and generated knowledge, not embedded raw source snapshots. |
| VaultPack | Deterministic `tar.zst` distribution artifact with content, manifest, provenance, checksums, and optional signature. |
| MCP adapter | Thin Model Context Protocol surface over framework operations or an external retrieval engine. |
| Derived index | Replaceable BM25, vector, or graph representation built from canonical data; never the source of truth. |
| Conflict | Two or more inputs or decisions that cannot safely share the same semantic or output identity without explicit handling. |
| Exact duplicate | Inputs whose canonical content and normalized frontmatter hashes match. |
| Near duplicate | Similarity candidate above the configured MinHash threshold; never automatically merged in V1. |
| Deterministic core | Compiler path whose result depends only on versioned inputs, configuration, and approved replay transcript. |
| Knowledge compilation | Parsing, normalizing, reconciling, planning, validating, and materializing knowledge with provenance. |
| Consolidation | Experimental promotion of repeated or useful episodic items into generalized knowledge; not a V1 file-compiler behavior. |
| Interference | Retrieval penalty for redundant or competing items that consume context without useful gain. |
