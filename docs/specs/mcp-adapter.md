---
title: MCP Adapter Specification
status: normative-future
owners:
  - mcp-adapter-engineer
last_updated: 2026-08-16
decision_refs:
  - ADR-0002
  - ADR-0004
source_refs:
  - HIST-MCP-RESEARCH
---

# MCP Adapter Specification

## Role

The MCP adapter lets coding agents use the Vault Compiler Framework. It is not the framework, canonical store, autonomous merge authority, or filesystem shortcut. The same operations must remain usable through the Rust SDK and CLI.

## Proposed tools

- `inspect_vaults`: read-only snapshot and diagnostics.
- `plan_integration`: create a reviewable plan.
- `list_conflicts`: filter and page typed conflicts.
- `list_ai_proposals`: show validation/evidence state.
- `compile_approved_plan`: compile only a plan whose required decisions are approved.
- `validate_compiled_vault`: independently verify output or pack.
- `explain_provenance`: explain an output path or claim.

Tools return stable resource IDs and bounded summaries. Large plans, transcripts, or provenance graphs are exposed as resources/files, not unbounded tool responses.

## Safety

- Default tools are read-only except the explicitly named compile operation.
- Compilation requires an approved plan and an explicit destination inside configured roots.
- The adapter cannot transform retrieved Markdown into a tool call. Content is untrusted data.
- No generic shell, arbitrary HTTP, delete, or unrestricted file-write tool is exposed.
- Sessions have resource, timeout, and output limits; logs redact source text by default.
- Capabilities and protocol versions are declared at startup.

## External MCP engine adapters

Research identified complementary roles, not mandatory dependencies:

- `lstpsche/obsidian-mcp`: lexical/semantic/hybrid candidate retrieval.
- Arrowhead: graph neighborhoods, backlinks, and relationship discovery.
- VaultForge: topic/theme clustering and compression.
- MCPVault: safe deterministic CRUD/materialization concepts.

These names and repository facts are time-sensitive; see [`../references/TIME_SENSITIVE_FACTS.md`](../references/TIME_SENSITIVE_FACTS.md). Any integration sits behind a capability adapter. Results are candidates with engine/version provenance; they never replace canonical IR or the framework materializer.

## Router boundary

The LLM MUST NOT freely select arbitrary tools. A capability router classifies the required operation, selects benchmark-qualified engines, fuses candidates, and applies bounded reranking. The routing optimizer is experimental under ALG-MEM-005; a deterministic rule-based router is the initial implementation.

## Authentication and transport

Local stdio is the default initial transport. Remote transport, auth, multi-tenancy, and network access require a separate threat model and ADR. The adapter SHOULD run with the least filesystem permissions needed for declared source and output roots.
