---
title: MCP Adapter Engineer Role Guide
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

# MCP Adapter Engineer Role Guide

## Responsibilities

Expose bounded framework tools/resources to coding agents, implement external engine capability adapters, preserve identity/provenance on candidates, enforce least-authority roots, and implement deterministic fallbacks and health/capability reporting.

## Non-responsibilities

Do not duplicate compiler logic, expose generic shell/HTTP/delete/write tools, let retrieved Markdown cause calls, make an external MCP index canonical, or let an LLM select arbitrary engines without router policy.

## Mandatory reading

Read [`../../AGENTS.md`](../../AGENTS.md), [`../../PROJECT_CONTEXT.md`](../../PROJECT_CONTEXT.md), [`../specs/framework-architecture.md`](../specs/framework-architecture.md), [`../specs/public-sdk-and-cli.md`](../specs/public-sdk-and-cli.md), [`../specs/mcp-adapter.md`](../specs/mcp-adapter.md), [`../specs/security-and-trust-boundaries.md`](../specs/security-and-trust-boundaries.md), ALG-MEM-004/005, and [`../references/TIME_SENSITIVE_FACTS.md`](../references/TIME_SENSITIVE_FACTS.md).

## Owned interfaces and invariants

- future MCP tool input/output schemas and pagination;
- configured source/output roots and write authorization;
- adapter capability/version/health contract;
- engine-result canonicalization and provenance.

## Required tests

MCP contract/error tests, root escape, prompt injection, oversized resources, timeouts/cancellation, stale engine IDs, partial engine failure, deterministic fusion/fallback, and proof that compile rejects unapproved plans.

## Handoff

Give client/AI engineers tool schemas, permission model, resource URI conventions, engine versions/licensing, benchmark evidence, fallback behavior, and operational limits. Revalidate external projects before release.
