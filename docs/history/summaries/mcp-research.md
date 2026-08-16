---
title: MCP Research Summary
status: historical
owners:
  - mcp-adapter-engineer
last_updated: 2026-08-16
decision_refs:
  - ADR-0002
source_refs:
  - HIST-SHARED-CHAT
---

# MCP Research Summary

Historical source material. Not normative. Use current specifications and accepted ADRs for implementation.

The 2026-08-14 research prioritized Claude Code and Codex connectivity, stdio/transport convenience, retrieval speed and token efficiency, large-Vault behavior, write safety, and maintenance evidence. README claims were not independently benchmarked.

## Selected complementary roles

1. **`lstpsche/obsidian-mcp`** — primary lexical BM25, semantic, and hybrid candidate retrieval.
2. **`totocaster/arrowhead`** — relationship/backlink neighborhoods and cross-Vault discovery. The user explicitly confirmed that “Arrowhead” meant this repository.
3. **`bitbonsai/mcpvault`** — safe deterministic CRUD/materialization concepts: path, symlink, extension, and frontmatter safeguards. It was not selected as the main search engine.
4. **`blacksmithers/vaultforge`** — topic/theme discovery, clustering, and Vault-level compression/summarization.

Other candidates included MegaMem, `cyanheads/obsidian-mcp-server`, Local REST API MCP, `obsidian-mcp-fast`, Kika's Obsidian/Codex MCP, and `MarkusPfundstein/mcp-obsidian`.

## Architectural conclusion

No MCP server is canonical. Put each behind a capability adapter and benchmark it per Vault. Start with a rule-based router: candidate pools such as BM25 50, semantic 50, and graph 30 feed RRF, an optional bounded reranker, then top 5. Model output cannot choose arbitrary tools or write directly.

Repository state, stars, capabilities, and licenses require revalidation through [`../../references/TIME_SENSITIVE_FACTS.md`](../../references/TIME_SENSITIVE_FACTS.md).
