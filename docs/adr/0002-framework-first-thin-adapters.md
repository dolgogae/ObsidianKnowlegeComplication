---
title: ADR-0002 — Framework First and Thin Adapters
status: normative-v1
owners:
  - architect
last_updated: 2026-08-16
decision_refs:
  - ADR-0002
source_refs:
  - HIST-MCP-RESEARCH
  - HIST-COMPILER-PLAN
---

# ADR-0002: Framework First and Thin Adapters

## Status

Accepted on 2026-08-15.

## Context

The product can appear as a library/framework, MCP server, or Obsidian plugin. Making MCP or a plugin primary would couple merge behavior to one host and make testing, reuse, and deterministic builds harder.

## Decision

The Rust framework is the product core. CLI, MCP, Obsidian, web, and registry integrations are thin adapters over public operations. If only one initial artifact is possible, ship framework + CLI.

## Consequences

All clients share one policy and provenance implementation. Adapters remain replaceable and least-authority. Some UX arrives later, and protocol design becomes a first-class responsibility.
