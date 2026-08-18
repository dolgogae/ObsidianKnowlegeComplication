---
title: ALG-NRM-001 — Markdown, Canvas, and Link Normalization
status: normative-v1
owners:
  - core-rust-engineer
last_updated: 2026-08-18
decision_refs:
  - ADR-0008
  - ADR-0012
source_refs:
  - HIST-COMPILER-PLAN
---

# ALG-NRM-001: Markdown, Canvas, and Link Normalization

## Objective and non-goals

Create stable comparison and resolution forms while retaining original bytes and source spans for lossless copy/rewrite. Normalization does not reformat user notes, infer meaning, repair malformed content silently, or make host-specific path guesses.

## Inputs and outputs

Inputs are safely decoded Markdown/Canvas bytes, logical source path, and a versioned normalization policy. Outputs are original syntax spans, canonical comparison forms, parsed link targets, and diagnostics.

## Canonical functions

```text
N_text(x) = NFC(normalize_line_endings_to_LF(x))
N_key(x)  = UnicodeCaseFold(NFC(trim(x)))
N_path(p) = join("/", NFC(each safe segment of slash-normalized p))
N_body(d) = canonical_AST_projection(parse(N_text(body_without_frontmatter(d))))
N_fm(d)   = canonical_typed_map(parse_frontmatter(d))
```

`N_body` is a versioned structural projection: it removes parser-position metadata and syntax differences explicitly declared insignificant, but preserves semantic text, heading/block structure, code/math bytes, link target/display distinction, and embed kind. It MUST NOT use locale-dependent case conversion.

The scanner retains `O_path(p)`, the exact valid-UTF-8 component spelling
joined with `/` before NFC. V1 requires `N_path(O_path(p))` to equal the sealed
logical path. `O_path` is audit data and is never substituted for `N_path` in
the ALG-SNP-001 semantic identity formulas.

## Symbols

| Symbol | Meaning | Type/range/unit | Default |
|---|---|---|---|
| `x` | decoded Unicode scalar sequence | valid UTF-8 | required |
| `p` | safe relative logical path | sequence of components | required |
| `d` | parsed document | IR Document | required |
| `NFC` | Unicode canonical composition | Unicode function | pinned Unicode data version |
| `N_text` | line/Unicode comparison form | UTF-8 | LF + NFC |
| `N_key` | title/alias lookup key | UTF-8 | trim + NFC + case-fold |
| `N_path` | portable logical path | UTF-8 slash path | segment NFC |
| `N_body` | canonical AST projection | canonical bytes | schema v1 |
| `N_fm` | normalized frontmatter map | canonical bytes | schema v1 |
| `O_path` | accepted pre-NFC component spelling | UTF-8 slash path | required |

The V1 implementation pins NFC data to Unicode 17.0.0 and default full case
fold data to Unicode 16.0.0. Dependency upgrades MUST NOT silently change
`N_path` or `N_key`; they require golden-vector review and explicit
normalization-version handling.

## Link parsing and rewriting

A wikilink/embed is parsed into `(raw_target, path?, heading?, block_id?, display?, embed)`. Resolution order is explicit path in source namespace, normalized path, filename stem/title/alias candidates, then heading/block validation. Zero or multiple final candidates are unresolved/ambiguous; the compiler does not choose by discovery order.

Rewriting replaces only target spans recorded by the scanner. It preserves surrounding source bytes, display text, embed marker, heading/block suffix, and escaping. The new relative target is calculated from the allocated output path using `/` separators. Markdown-format targets use the frozen V1 percent encoder; wikilink-format targets retain Obsidian delimiters and do not invent escapes for otherwise non-representable filenames.

Canvas parsing types known fields (`nodes`, `edges`, file nodes and IDs) while preserving unknown JSON fields. Referenced files use the same resolver and output path map. Serialization uses deterministic key policy only for generated/rewritten Canvas; unchanged files may be copied byte-for-byte.
Canvas `file` rewrites store the destination-relative semantic path with `/`
separators. They do not apply URL percent encoding or Markdown/wikilink
escaping; the canonical JSON serializer alone escapes the JSON string. A raw
unresolved or waived path may be retained only when lexical resolution from
the allocated Canvas destination remains inside the Compiled Vault root.

Canvas lookup normalizes a derived key only. It never NFC-rewrites the stored
raw JSON `file` value. A leading UTF-8 BOM is excluded from Markdown semantic
comparison only at byte offset zero; its bytes and its effect on all source
span offsets are preserved. A BOM after frontmatter is content. CRLF/lone-CR
normalization likewise applies only to comparison forms. Copy and rewrite keep
every byte outside sealed spans.

Unresolved or waived local Markdown and Canvas targets are preserved only when
platform-independent lexical resolution proves that the target stays inside
both the source Vault and allocated Compiled Vault roots. Parent traversal,
absolute paths, URI-like drive prefixes, and UNC-like forms fail closed.

## Pseudocode

```text
decode bytes strictly as UTF-8 or classify binary/error
locate frontmatter without rewriting it
parse CommonMark AST and record byte spans
scan Obsidian constructs with byte-aware state machine, excluding code spans/fences
cross-check overlapping parser/scanner spans
build comparison projections and lookup keys
parse each target; resolve to zero/one/many document candidates
emit IR plus diagnostics; retain original bytes/hash
when rewriting, apply non-overlapping replacements from highest byte offset downward
reparse rewritten result and verify intended targets
```

## Complexity

Parsing and scanning are `O(n)` expected for `n` bytes; link-index construction is `O(D log D)` or `O(D)` expected with deterministic final ordering. Applying `k` sorted replacements is `O(n + k log k)`.

## Edge and security cases

Cover BOM, CRLF/CR, NFC/NFD, combining marks, emoji/graphemes, malformed YAML, duplicate YAML keys, code containing `[[`, escaped delimiters, nested brackets, headings with aliases, case collisions, Windows reserved names, JSON depth/size, duplicate JSON object keys, duplicate Canvas node IDs, and overlapping spans. Duplicate JSON keys and node IDs fail closed because a last-wins parse cannot preserve or uniquely address them. Never perform regex-only rewriting across a full Markdown document.

## Worked example

Source `notes/A.md` contains `See [[../Topic#Intro|start]].` and the planner maps `Topic.md` to `knowledge/team/topic~d123.md`. Only the target span becomes the correct relative target from `knowledge/.../A.md`; `#Intro`, `|start`, punctuation, and all other bytes remain unchanged. A second `topic.md` with the same normalized key produces ambiguity instead of a guess.

## Golden vectors

| Case | Input | Expected comparison/resolution behavior |
|---|---|---|
| line endings | `a\r\nb\r` | `N_text = "a\nb\n"`; untouched bytes remain original |
| Unicode path | `Cafe` + combining acute | logical path NFC equals `Café`; original spelling remains distinct |
| leading BOM | `EF BB BF` + Markdown | excluded from semantics, retained in source bytes and span offsets |
| full case fold | `Straße` / `STRASSE` | equal lookup/portable key; original strings unchanged |
| wikilink in code | `` `[[A]]` `` | no link token |
| embed | `![[A.png#x]]` | embed=true, path=`A.png`, suffix=`#x` |
| ambiguous title | two `Topic.md` candidates | typed ambiguity conflict |

## Correctness and rollback

Golden files assert byte spans and rewritten bytes. Round-trip/copy paths must preserve source hash. Parser or Unicode data upgrades run the full corpus; changed projections require an explicit normalization version. On parse inconsistency, retain the source as opaque or fail according to policy—never silently apply a guessed rewrite.
