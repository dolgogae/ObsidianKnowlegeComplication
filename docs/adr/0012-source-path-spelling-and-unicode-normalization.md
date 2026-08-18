---
title: ADR-0012 — Source Path Spelling and Unicode Normalization
status: normative-v1
owners:
  - architect
  - core-rust-engineer
  - qa-security-engineer
last_updated: 2026-08-18
decision_refs:
  - ADR-0003
  - ADR-0007
  - ADR-0008
  - ADR-0010
  - ADR-0012
source_refs:
  - HIST-COMPILER-PLAN
---

# ADR-0012: Source Path Spelling and Unicode Normalization

## Status

Accepted on 2026-08-17.

## Context

The V1 scanner normalizes each accepted path component to NFC before it seals
the source manifest. This is correct for portable identity and lookup, but the
initial `0.1.0` working tree discards the pre-normalization spelling. As a
result, two different sources containing NFC `Caf\u{e9}.md` and NFD
`Cafe\u{301}.md` request the same normalized destination, yet the planner sees
two identical strings and reports `PATH_EXACT` instead of
`UNICODE_NORMALIZATION`. The typed provenance graph cannot show which spelling
each immutable source exposed.

Host and archive APIs add another hazard. A ZIP reader may decode an unflagged
name as CP437 or replace malformed UTF-8 before the compiler sees a `Path`.
Accepting that value as though it were the exact source spelling would make
path provenance host/library-dependent. BOM, Unicode case folding, and
unresolved link containment also need an explicit boundary between raw
preservation and semantic normalization.

## Decision

### Two required path forms

Every accepted `SourceFile` carries these required fields:

```rust,ignore
enum SourcePathEncoding {
    Utf8,
}

struct SourceFile {
    original_path: String,
    logical_path: String,
    path_encoding: SourcePathEncoding,
    // sealed identities, kind, length, content hash ...
}
```

`original_path` is the exact valid-UTF-8 component spelling observed at the
source boundary after path syntax has been made portable: components are
joined with `/`, redundant `.` components are omitted, and no absolute,
prefix, empty, or parent-traversing component is permitted. Unicode scalar
values are not normalized. `logical_path` is `N_path(original_path)`: NFC is
applied independently to every component and the result is joined with `/`.
Both forms must satisfy the configured byte/component limits and safety
policy, and `N_path(original_path) == logical_path` is a sealed invariant.

V1 has no lossy or legacy-code-page mode. Directory and tar paths must be
strictly representable as UTF-8. ZIP member names are validated through their
raw central-directory bytes before any library CP437 or replacement-character
decoding; a non-UTF-8 name is rejected. ASCII names remain valid UTF-8 whether
or not the ZIP UTF-8 flag is present. ZIP and tar names are parsed as `/`
paths without host `Path` semantics, and a backslash is rejected on every
platform. Name decoding and path safety occur before file-kind exclusion so a
malformed directory or link name cannot hide in an ignored entry. A future
encoding policy requires a new tagged variant, exact raw-byte representation,
compatibility rules, and an ADR.

### Identity and tamper boundary

`SourceFileId`, `SnapshotId`, and derived Document/Canvas/Base identities keep
the ALG-SNP-001 formulas and use only `logical_path`. Canonically equivalent
NFC/NFD spellings therefore have the same semantic file identity when source,
kind, and bytes are otherwise equal. This is deliberate and avoids a Unicode
normalization form becoming an accidental content fork.

The original spelling is not unsealed metadata. It is included in the
canonical workspace projection, `PlanId`, typed provenance Source record and
its `RecordId`. Compilation reopens the source and requires both the sealed
original/logical path pair and the content identity to match. A spelling-only
rename after planning therefore makes the plan stale even though the lower
semantic file identity formula is unchanged.

### Duplicates, allocation, lookup, and provenance

Within one source, two entries with the same `logical_path` are rejected as a
duplicate normalized path. Across sources, allocation is based on the
normalized logical destination, while collision classification compares each
request's original spelling:

- identical original request spellings: `PATH_EXACT`;
- distinct spellings with equal NFC form: `UNICODE_NORMALIZATION`;
- otherwise equal portable full-Unicode-case-fold keys: `PATH_CASEFOLD`.

`N_key` and portable path keys use a pinned, locale-independent full Unicode
case fold after NFC. Simple lowercase conversion is not conformant: vectors
include `Straße`/`STRASSE` and final/non-final Greek sigma. Folding is lookup
and collision data only; it never rewrites a displayed title, raw link, or
source spelling. V1 pins `unicode-normalization` data to Unicode 17.0.0 for NFC
and `caseless` data to Unicode 16.0.0 for default full case folding. The two
versions are independently asserted in the compiler and frozen by golden
vectors; changing either table is a semantic policy change requiring explicit
compatibility review.

The output path remains canonical NFC and any suffix remains identity-derived.
Both original and normalized source paths plus the `utf8` encoding tag are
required in Vault-file and evidence provenance records. Explanations expose
these values as untrusted source data; absolute host locators remain redacted.

### Text, link, and Canvas boundary

A single UTF-8 BOM is meaningful only at byte offset zero. Markdown parsing
excludes it from frontmatter/body comparison semantics, preserves the three
source bytes, and keeps all spans in coordinates of the original byte stream.
Copy is byte-identical; a rewrite changes only sealed target spans and
preserves the BOM and all bytes outside those spans. A BOM after frontmatter
is content, not a second file marker. CRLF and lone CR become LF only in
comparison projections and are otherwise preserved by the same rule.

A Canvas file-node `file` string is retained exactly as JSON data. Resolution
derives a normalized lookup key without mutating the stored raw value.
Unresolved or waived references retain it exactly; an actual resolved rewrite
replaces it with the canonical destination-relative `/` path and canonicalizes
the rewritten JSON as already required by ADR-0008.

Every preserved unresolved or waived local Markdown/Canvas target must resolve
lexically inside both its source Vault root and its allocated Compiled Vault
root. Parent traversal, absolute paths, URI-like drive prefixes, and UNC-like
forms fail closed; a waiver cannot authorize an escape. The same containment
parser is platform-independent.

Markdown link rewriting retains its parser-recorded delimiters, display text,
embed marker, fragments, and surrounding bytes. Markdown-format destinations
use the frozen percent-encoding routine. V1 does not invent a new escape for a
wikilink filename containing Obsidian syntax delimiters; portable allocation
or a typed non-representable-link decision for those names requires a separate
normative change. The edge corpus may test preservation of existing escaped
syntax but must not silently guess a new encoding.

### Compatibility

This is completion of unpublished `0.1.0` IR/plan/provenance schema version 1.
The new fields are required, have no Serde defaults or aliases, and older
working-tree plans and ledgers fail closed. The source policy remains
`vaultc-source-v1`; strict UTF-8 and platform-independent archive parsing are
the previously documented V1 behavior, not a new permissive interpretation.
The hash domains and public schema numbers remain V1 because no release has
promised compatibility with the partial shape. A published-format migration
must be explicit and must not fabricate an original spelling that was never
recorded.

## Consequences

- Cross-source canonical-equivalence conflicts are correctly typed without
  weakening portable identity.
- Provenance can display the exact accepted UTF-8 spelling and independently
  verify its normalized pair.
- ZIP paths that previously depended on implicit CP437/lossy decoding are now
  rejected; this is the safe V1 behavior.
- Semantic IDs can match for NFC/NFD spellings while Plan and provenance IDs
  differ, reflecting the distinct roles of identity and audit observation.
- Whole-artifact byte equality across hosts requires the source boundary to
  expose the same original UTF-8 spelling. Cross-platform semantic-ID tests
  use archives or fixtures whose raw path spelling is fixed.
