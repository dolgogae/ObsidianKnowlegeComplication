---
title: Knowledge Compilation Platform Summary
status: historical
owners:
  - architect
last_updated: 2026-08-16
decision_refs:
  - ADR-0002
  - ADR-0003
source_refs:
  - HIST-SHARED-CHAT
---

# Knowledge Compilation Platform Summary

Historical source material. Not normative. Use current specifications and accepted ADRs for implementation.

## Product concept

Archive local Obsidian Vaults as immutable versions, analyze and benchmark them, combine selected Vaults through a canonical knowledge layer and complementary MCP strategies, then distribute installable knowledge packs. Obsidian is the first storage/client format; the long-term product is a Knowledge Package Registry usable by coding agents and editors.

## Pipeline

Client scan and delta manifest → immutable raw snapshot → security scan → Markdown/Canvas parse → normalization → entities/claims/evidence → embeddings and graph → Vault benchmark → capability router → knowledge compiler → merged evaluation → signed `.vaultpack` → generic Obsidian installer or local coding-agent MCP.

## Canonical and evidence model

Proposed platform entities include Vault, VaultSnapshot, Document, Section, Chunk, Entity, Claim, Relationship, SourceEvidence, Topic, Embedding, BenchmarkRun/Case/Result, KnowledgePack, and version. Every Claim links to evidence. Conflicting claims coexist with source, time, version, and context; majority vote does not overwrite them.

Generated notes carry pack/source/generated/confidence metadata. Source licenses, authors, and provenance survive compilation. BM25/vector/graph indexes are rebuildable derivatives.

## Evaluation model

Avoid one absolute score across unrelated topics. Expose topic-independent Vault Health, topic-specific retrieval and same-topic percentile with confidence interval, Agent Utility, Evidence Quality, and Freshness. Retrieval uses originating notes as traceable ground truth and reports Recall@5/10, MRR, and nDCG. Merge evaluation covers coverage/knowledge gain, retrieval dilution, novelty, redundancy, conflict, and provenance coverage.

“Knowledge DNA” was proposed as topic distribution plus depth, breadth, freshness, connectivity, evidence, and originality for gap-aware recommendations.

## Brain-inspired analogy

Immutable snapshots resemble episodic memory; generalized entities/claims semantic memory; graph neighborhoods associative memory; background consolidation a knowledge consolidation job. Topic-dependent decay lowers retrieval weight instead of deleting facts. All of these are computational analogies, not claims of human-brain equivalence.
