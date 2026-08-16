---
title: Source Registry
status: historical
owners:
  - release-maintainer
last_updated: 2026-08-16
decision_refs:
  - ADR-0007
source_refs:
  - HIST-SHARED-CHAT
  - HIST-CURRENT-PLAN
---

# Source Registry

Sources support context and implementation research. Normative behavior remains in specifications/ADRs, and external facts must be revalidated when time-sensitive.

## Conversation sources

- **HIST-SHARED-CHAT:** [Shared ChatGPT conversation — “Obsidian MCP 분석”](https://chatgpt.com/share/6a7f0cef-a02c-83ee-b67c-8ae56c7ebdd6), accessed 2026-08-15/16. Publicly recoverable text is preserved in [`../history/transcripts/shared-chatgpt-obsidian-mcp-analysis.md`](../history/transcripts/shared-chatgpt-obsidian-mcp-analysis.md).
- **HIST-CURRENT-PLAN:** Current framework-planning conversation, recoverable record in [`../history/transcripts/current-framework-planning-session.md`](../history/transcripts/current-framework-planning-session.md).
- **HIST-MCP-RESEARCH:** [`../history/summaries/mcp-research.md`](../history/summaries/mcp-research.md).
- **HIST-KNOWLEDGE-PLATFORM:** [`../history/summaries/knowledge-compilation-platform.md`](../history/summaries/knowledge-compilation-platform.md).
- **HIST-ONPREM-STACK:** [`../history/summaries/on-premise-stack-and-diagrams.md`](../history/summaries/on-premise-stack-and-diagrams.md).
- **HIST-COMPILER-PLAN:** [`../history/summaries/vault-compiler-framework-plan.md`](../history/summaries/vault-compiler-framework-plan.md).

## Primary technical references to consult during implementation

- [Rust Edition Guide — Rust 2024](https://doc.rust-lang.org/edition-guide/rust-2024/index.html)
- [Comrak repository and parser documentation](https://github.com/kivikakk/comrak)
- [Obsidian developer documentation](https://docs.obsidian.md/)
- [Obsidian Flavored Markdown](https://help.obsidian.md/obsidian-flavored-markdown)
- [JSON Canvas specification](https://jsoncanvas.org/spec/1.0/)
- [SQLite documentation](https://www.sqlite.org/docs.html)
- [Model Context Protocol specification](https://modelcontextprotocol.io/specification/)
- [NATS JetStream documentation](https://docs.nats.io/nats-concepts/jetstream)
- [OpenSearch hybrid search documentation](https://docs.opensearch.org/latest/vector-search/ai-search/hybrid-search/)
- [vLLM documentation](https://docs.vllm.ai/)

## Researched repositories

- [`lstpsche/obsidian-mcp`](https://github.com/lstpsche/obsidian-mcp)
- [`totocaster/arrowhead`](https://github.com/totocaster/arrowhead)
- [`bitbonsai/mcpvault`](https://github.com/bitbonsai/mcpvault)
- [`blacksmithers/vaultforge`](https://github.com/blacksmithers/vaultforge)
- [`cyanheads/obsidian-mcp-server`](https://github.com/cyanheads/obsidian-mcp-server)
- [`coddingtonbear/obsidian-local-rest-api`](https://github.com/coddingtonbear/obsidian-local-rest-api)
- [`MarkusPfundstein/mcp-obsidian`](https://github.com/MarkusPfundstein/mcp-obsidian)

Before selecting or shipping a dependency, verify its current repository, release, license, security posture, API/schema, supported transports, and compatibility on target platforms.
