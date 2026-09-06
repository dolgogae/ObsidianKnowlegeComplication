---
title: Time-Sensitive Facts and Revalidation Register
status: historical
owners:
  - release-maintainer
  - mcp-adapter-engineer
last_updated: 2026-09-06
decision_refs:
  - ADR-0002
  - ADR-0026
source_refs:
  - HIST-SHARED-CHAT
---

# Time-Sensitive Facts and Revalidation Register

These observations were researched primarily on 2026-08-14. They are not current guarantees.

| Observation as of research date | Risk | Revalidate before |
|---|---|---|
| `lstpsche/obsidian-mcp` appeared suited to BM25/semantic/hybrid retrieval | APIs, quality claims, maintenance, and license can change | adapter implementation or recommendation |
| `totocaster/arrowhead` was the referenced Arrowhead repository and appeared graph/discovery-oriented | identity/features/repository status can change | adapter implementation |
| `bitbonsai/mcpvault` appeared focused on safe file CRUD/materialization | exact safeguards and compatibility can change | adopting code or policy |
| `blacksmithers/vaultforge` appeared useful for themes/clustering/compression | features/license/maintenance can change | experiment or adapter |
| Other MCP candidate rankings and GitHub popularity | stars and activity are volatile and not quality proof | any published comparison |
| Open-source infrastructure versions named in the platform plan (Ubuntu 24.04, PostgreSQL 17, Spring Boot 3.5.x, etc.) | releases/EOL/security support evolve | platform implementation or release |
| README benchmark/performance claims | may be non-comparable or unverified | architecture or performance decision |
| On 2026-09-06, the official PyPI and npm JSON endpoints for the unscoped name `okc-compiler` both returned HTTP 404 | either name can be registered at any time; absence is not a reservation | immediately before creating or publishing a release candidate |

## Required revalidation procedure

1. Use the canonical repository/official documentation, not an aggregator.
2. Record access date, commit/tag/version, license/SPDX, archive status, latest security advisories, and supported transports/platforms.
3. Run the project against pinned fixtures and the framework adapter conformance suite.
4. Benchmark on representative Vault cohorts under equal limits.
5. Store results as a new historical report; never rewrite the old observation.

The current documents intentionally do not freeze GitHub star counts because they are irrelevant to compiler correctness and quickly stale.
