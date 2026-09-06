---
title: ADR-0028 — Bounded Semantic Execution and Performance Qualification
status: proposed
owners:
  - architect
  - algorithms-ai-engineer
  - core-rust-engineer
  - qa-security-engineer
last_updated: 2026-09-06
decision_refs:
  - ADR-0005
  - ADR-0023
  - ADR-0024
  - ADR-0027
  - ADR-0028
source_refs:
  - HIST-COMPILER-PLAN
---

# ADR-0028: Bounded Semantic Execution and Performance Qualification

## Status

Proposed on 2026-09-06; not accepted or implemented. Numbers below are review
proposals, not measured guarantees or current defaults. No current algorithm,
schema, budget, or approval is changed by this document. Review entry point:
[`ADR proposals`](README.md).

## Context and affected contracts

The baseline is commit `f4c50ad71f61478316d5e68182807ef8612f4d2b`. Its
[ingestion probe](../history/summaries/2026-09-06-stabilization-follow-up.md)
processed 100,000 notes/25.6 MB in 174.569 seconds with 5,360,336,896 bytes RSS,
without providers. `PreparedCorpus` owns both block text in documents and a
second block map; private snapshot/planning stages also buffer accepted input.
The development semantic task scores document pairs in memory and is not the
chunk/HNSW/union implementation required by ALG-SEM-001.

REQ-PERF-001/QG-006 require 10 Vaults, 100,000 notes and 20 GB. The historical
[2026-08-15 decision](../history/DECISION_LOG.md) says 20 minutes/2 GB on an
unspecified reference machine. Neither a 25.6-MB fixture nor emulation qualifies
that workload. Mandatory live generation introduces provider time/cost and
human-review time which the original V1 budget did not define.

Affected: REQ-SNP-001/002, REQ-SRC-002, REQ-DED-002, REQ-AI-003/004,
REQ-INT-001/002/004/005, REQ-PERF-001; ALG-SNP-001, ALG-DED-001/002,
ALG-SEM-001, ALG-INT-001; QG-001/002/003/004/006. All named algorithms retain
their current normative status; no experimental memory algorithm is promoted.

## Proposed decision

### 1. Bounded execution, unchanged existing bytes

Introduce an application-owned disk-backed corpus store behind core read-only
cursor/lookup interfaces. SQLite holds ordered identities, offsets, work
status, and indexes; bounded content-addressed spool files hold accepted text
and vector payloads. Files remain outside sources with the same hostile-path
guards and plaintext warning as project objects. The store is derivative,
rebuildable state, not a new approval authority or external database service.

Read/hash/parse one bounded source unit at a time, externally sort manifests,
and stream canonical hashing/serialization in the exact existing key order.
Do not replace canonical JSON with a new encoding inside an existing hash
domain. Keep `CorpusBuilder::build -> PreparedCorpus` as the explicitly
in-memory small-input API; an additive cursor API supports bounded application
execution. CLI/TUI/interop do not materialize the entire corpus into a DTO as
an intermediate step. Requests that exceed the in-memory API's documented
bound return a resource error, not a partial corpus.

Before any expensive task, estimate spool/index/output sizes with checked
arithmetic and reserve a configured disk budget. Low disk, corrupt chunks,
missing identities and exhausted limits fail closed. Spilling is never an
excuse to omit a document, candidate reason, disposition, or critic input.

### 2. Candidate profile to review and freeze

Use a new recorded execution profile, provisionally `semantic-bounded-1`.
It affects new task identities only; completed recordings and approved Schema
3 plans are never recomputed under new defaults. Propose these starting values:

| Parameter | Proposed value/meaning |
|---|---|
| Chunk text ceiling | 8,192 UTF-8 bytes; block boundaries first, then scalar-safe subdivision; no overlap |
| Chunk order | raw DocumentId, original parser block span, subdivision index; keep source-span evidence |
| Empty document | one explicitly identified empty chunk |
| Embedding batch | at most 64 chunks and 524,288 text bytes; JSON framing also fits the provider request bound |
| Model space | one exact provider/model/options/dimension identity per run; no mixed-vector comparison |
| HNSW | M=16, efConstruction=200, efSearch=128, seed=0, one graph insertion worker |
| Retrieval | 32 chunk neighbors; exclude self-document; max chunk-pair score per document pair; best 8 distinct documents per endpoint at score >=0.55 |
| Candidate union | semantic, exact, pinned 128-component/32x4 MinHash, title, alias, and resolved-link reasons; retain all reasons |
| Pair ceiling | 5,000,000 unique unordered document pairs; exceeding it is an explicit resource failure |
| Hot text/vector cache | combined 256 MiB maximum; deterministic 64-MiB sorted spill runs for pair reduction |
| Provider workers | 4 maximum; commit valid results in task order, not response arrival order |

These are bounded starting points, not quality claims. A profile manifest must
also freeze the chosen HNSW implementation/version, RNG and level assignment,
metric arithmetic/normalization, tie rules, and provider token counter/version.
Fixed seed alone is not accepted as cross-host determinism evidence. Validate
finite vectors/dimensions/norms first; stable ties use raw document/chunk IDs.
Both endpoints receive a real nearest-neighbor search: document ordering must
not restrict a query to only later IDs, as in the development implementation.

If provider byte/token/batch limits are lower, derive and seal the effective
profile before scheduling any call. Count full request/schema/output reserve
with a verified counter; if it cannot fit safely, stop. Never truncate, switch
model, reduce evidence, or change chunking midway through a run. Provider
recordings remain immutable proposals, and remote consent/local-sensitive
routing precede every newly disclosed task.

HNSW and pair storage must honor bounded resident memory; an index library
that requires all vectors/adjacency resident is not automatically acceptable.
Cross-host replay must produce identical canonical candidate records from the
same recorded vectors. A new profile invalidates downstream taxonomy and
cluster authority, not source identities.

### 3. Hierarchical tasks without evidence loss

Deterministically partition organizer and synthesis work into manifest-bound
tasks under the sealed context budget. Intermediate summaries are proposals,
not substitutes for the source inventory. Final taxonomy validation covers
every DocumentId exactly once. Synthesis reduce retains the union of original
block/metadata evidence and an exactly-once disposition ledger.

The critic receives source-evidence shards plus the full proposal reference,
with coverage receipts for every target. A final cross-shard contradiction and
coverage check is required; independently clean shards alone do not establish
a clean cluster. Critical/major findings cannot disappear in reduction.
The final curator approval binds all effective tasks, revisions and waivers.

### 4. Qualification profiles and budget proposal

Interpret the current input ceiling explicitly as 20 GiB = 21,474,836,480
accepted uncompressed bytes, matching `SafetyLimits`; accepting this wording
requires updating QG-006 rather than silently changing its unit. Keep the
historical RSS target conservatively at 2,000,000,000 bytes for the complete
OKC process tree, including language runtimes and resident mapped pages.

Proposed reference class: dedicated native Linux x86_64, 8 CPU cores, 16 GiB
RAM, local SSD, no swap, 4 OKC workers, no GPU or co-located live model. Exact
CPU/SSD/kernel/filesystem/toolchain identities and measurement scripts must
be frozen before acceptance of a benchmark result. macOS/Windows remain
functional/determinism release targets, not emulated substitutes.

| Fixture/gate | Proposed qualification |
|---|---|
| Small diagnostic | retain the 100k/25.6-MB probe only as a regression; never label it QG-006 |
| Full Markdown | 10 distinct Vaults, exactly 100,000 Markdown files totaling 20 GiB; varied sizes, links, metadata, Unicode, duplicates and contradictions; each file within existing safety bounds |
| Mixed materialization | separate 20-GiB fixture with explicit note/asset/Canvas/Base counts under the 100,000-total-file limit; supplements, never substitutes for, the 100k-note fixture |
| Deterministic replay | fresh workspace through ingestion, recorded semantic tasks, validation, streamed materialization and independent verify in <=1,200 s and <=2,000,000,000 bytes peak RSS |
| Live-provider qualification | exact model/route/limits and actual request/token/retry/latency receipts; separate input/output-token and monetary ceilings explicitly approved for the run |

Reinterpreting the historical time target as deterministic replay time is a
proposed decision, not an existing waiver of live-provider qualification.
Report live-provider and human-review times separately, never subtract them
from a run labelled end-to-end. No currency price or paid-call authorization
is supplied by this ADR. Missing live budget/model or evidence keeps QG-006 open.

Generate fixtures from pinned seeds and record every source hash, note/byte
distribution, full approval and provider replay inventory. Run three fresh
workspace repetitions; all must meet ceilings, with cold/warm OS-cache state
reported. Count retries and failed work. Artificial all-singleton or empty
provider results cannot replace the evidence-complete semantic fixture.

### 5. Format feasibility is a separate blocker

Current plans embed the corpus and are limited to 512 MiB; manifests are
limited to 1 MiB. Full Markdown source text and an inventory for 100k canonical
notes/redirects can exceed those ceilings regardless of working-memory use.
Keep those security bounds and return an explicit preflight refusal. Do not
raise them or omit records to claim the benchmark passed.

[ADR-0030](0030-current-pack-and-extended-materialization.md) proposes separate
streaming/bounds and extended-envelope decisions. Schema-3-preserving execution work
can proceed independently after approval, but the full end-to-end gate remains
blocked until the format can represent the fixture safely and offline.

## Alternatives and consequences

- Larger memory limits leave superlinear candidate work and wire limits
  unresolved. They also relax, rather than meet, the historical RSS target.
- Pure all-pairs search remains a small-fixture oracle, not the scale path.
- An external vector/graph service adds deployment and canonical-state risks;
  it is not proposed for this local framework.
- Paged stores and deterministic graph/merge code add complexity and require
  corruption, disk exhaustion, cancellation and cross-host tests.

## Acceptance, rollout and rollback

Before acceptance, architect/algorithm/QA/release owners must sign off the
effective profile including numeric engine rules, reference-host fingerprint,
benchmark fixture distributions, live budget, and the replay-time interpretation.
Candidate recall is measured against exact search on a fixed held-out subset;
proposed minimum recall@8 is 0.95, alongside exact equality for replay and all
non-semantic candidate reasons. A threshold change needs a new profile review.

Implementation gates: streamed-vs-current corpus/hash goldens; chunk boundary
and empty-document vectors; context/batch limits; no lost candidate reason;
source/order/host/scheduling replay equality; missing-shard and cross-shard
contradiction refusal; disk/RSS exhaustion; resume after cancellation; full
benchmark receipts. Tests are planned, not present merely by being named here.

Update pipeline/IR/provider/testing specs, ALG-SEM-001/ALG-INT-001 and
TRACEABILITY in the implementation change. Private storage changes require an
explicit versioned migration and recovery test. Rollback disables the new
execution profile for new runs; it neither rewrites old objects nor accepts a
partial plan. Preserve the existing Schema 3 artifact golden throughout
representation-preserving work.
