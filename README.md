# Obsidian Knowledge Compilation

OKC turns immutable Obsidian Vault snapshots into a new deterministic,
evidence-traceable Vault. Providers propose taxonomy and synthesis; local
validation, an independent critic, and explicit curator approvals decide what
may enter the final Schema 3 artifact. Compilation, verification, and
provenance explanation are provider-free.

> The repository is a `0.3.0` development tree, not a stable release. The
> current materializer produces Markdown directories. Attachment, Canvas,
> Base, full link-rewrite, current OKCPack, semantic-scale, fuzzing,
> cross-platform, and native-signing gates remain open.

## Current source boundary

Main contains only the current Schema 3 implementation. It does not include a
Schema 1/2 reader, writer, migration, alias, deprecated `vaultc` facade, or
retired protocol/command-provider path. Recognizable older markers return
`ARTIFACT_SCHEMA_UNSUPPORTED` with supported schema 3 and detected details;
mixed, symlinked, malformed, oversized, corrupt, and unknown artifacts fail
closed.

Historical source remains available from annotated archive tags:

| Tag | Commit |
|---|---|
| `archive/v0.1.0` | `7181fc2dea54288f176b66a00e2335da7f58bdfd` |
| `archive/v0.2.0` | `b9f9e88bc531095fbeb2ece4155c980bdf10708b` |

These tags are archives, not releases. See
[ADR-0027](docs/adr/0027-current-schema-single-source.md).

## What is implemented

- safe directory, ZIP, and `tar.zst`/`.tzst` source ingestion;
- immutable snapshots, strict UTF-8/NFC paths, bounded archive/resource
  checks, symlink exclusion, Markdown/frontmatter parsing, and optional SQLite
  build workspace;
- `CorpusBuilder::build(...) -> PreparedCorpus` as the public Rust source
  boundary;
- Schema 3 projects, source sets, role-to-provider routes, private append-only
  journal schema 4, immutable objects, and single-writer locking;
- OpenAI, Anthropic, Gemini, Ollama, and OpenAI-compatible provider adapters;
- pre-disclosure sensitive-data scanning and explicit per-call remote consent;
- complete taxonomy, per-cluster synthesis/evidence/dispositions,
  contradiction records, critic findings, omission/minor-waiver review, and
  hash-bound approvals;
- provider-free directory compile, independent verification, and one-path
  provenance explanation;
- one shared CLI/TUI application layer;
- typed Python 3.11+ and Node.js 22.13+ API-v1 packages using interop DTO
  schema 2.

The current directory contains:

```text
CompiledVault/
├── knowledge/<canonical-path>.md
├── legacy/<source-id>/<original-path>.md
└── .okc/
    ├── integration-plan.json
    ├── provenance.jsonl
    ├── manifest.json
    └── checksums.txt
```

`legacy/` is the current redirect-stub namespace, not an older-schema reader.
Existing Schema 3 fields, IDs/hashes, SQLite identifiers, paths, and output
bytes remain unchanged and require no migration.

## Build

The repository pins Rust 1.97.1 and commits `Cargo.lock`.

```bash
rustup toolchain install 1.97.1
cargo +1.97.1 build --locked -p okc
./target/debug/okc --help
```

With a TTY, running `okc` without a subcommand launches the cwd-first TUI. In
automation, use explicit commands and paths.

## Current CLI workflow

Create a project and add immutable source bindings:

```bash
okc project create ./Knowledge.okc-project \
  --name Knowledge \
  --curator curator-1 \
  --language ko-KR

okc --project ./Knowledge.okc-project \
  project source add personal /absolute/path/to/PersonalVault
okc --project ./Knowledge.okc-project \
  project source add team /absolute/path/to/TeamVault.zip
```

Configure provider profiles. Secrets are referenced by environment-variable
name or an opaque CLI/TUI keychain account and are never written into the
project:

```bash
okc provider add local \
  --kind ollama \
  --endpoint http://127.0.0.1:11434 \
  --model your-model

okc --project ./Knowledge.okc-project project ai-route set local
okc provider test local
```

Run/review the integration and compile only after every required approval:

```bash
okc --project ./Knowledge.okc-project integrate
okc --project ./Knowledge.okc-project review taxonomy show
okc --project ./Knowledge.okc-project review taxonomy approve
okc --project ./Knowledge.okc-project review cluster list
okc --project ./Knowledge.okc-project review cluster show CLUSTER_ID
okc --project ./Knowledge.okc-project review cluster approve CLUSTER_ID
okc --project ./Knowledge.okc-project integration status

okc --project ./Knowledge.okc-project compile \
  --output ./CompiledVault
okc verify ./CompiledVault
okc explain ./CompiledVault knowledge/topic.md
```

A non-interactive remote call requires both explicit flags:

```bash
okc --project ./Knowledge.okc-project integrate \
  --allow-remote-provider --yes
```

Compile may instead consume an explicit complete approved plan:

```bash
okc compile \
  --integration-plan /absolute/path/to/approved-integration-plan.json \
  --output /absolute/path/to/CompiledVault
```

The CLI intentionally has no inspect/plan/augment/replay/validate/old approve,
project-upgrade, policy/workspace globals, positional approved plan, Pack, or
explanation pagination/package surface. See the [CLI guide](guide/cli.md) and
[integration guide](guide/integration.md).

## Rust library

```rust,ignore
use okc_core::{CorpusBuilder, SourceSpec};

let prepared = CorpusBuilder::new()
    .workspace(".okc-work/build.sqlite3")
    .build([
        SourceSpec::directory("personal", "./PersonalVault")?,
        SourceSpec::archive("team", "./TeamVault.zip")?,
    ])?;

println!("{}", prepared.corpus.corpus_hash);
```

Current integration DTOs and the generation-neutral `compile`, `verify`, and
`explain` functions live in `okc_core::integration`.

## Python and Node.js

Both packages use one Rust facade, explicit absolute paths, bounded
cancellable jobs, stable structured errors, and `INTEROP_SCHEMA_VERSION == 2`.
They do not discover cwd projects, prompt, print, access the native keychain,
or invoke the updater.

Python explanation requires a keyword-only output path:

```python
from okc import OkcClient

client = OkcClient()
verified = client.verify_artifact("/absolute/CompiledVault").result()
record = client.explain_artifact(
    "/absolute/CompiledVault",
    output_path="knowledge/topic.md",
).result()

assert verified["interop_schema_version"] == 2
assert verified["valid"] is True
assert record["artifact_path"] == "/absolute/CompiledVault"
```

Node.js explanation requires `{ outputPath }`:

```js
import { OkcClient, INTEROP_SCHEMA_VERSION } from 'okc-compiler'

const client = new OkcClient()
const verified = await client.verifyArtifact('/absolute/CompiledVault').result()
const explained = await client.explainArtifact('/absolute/CompiledVault', {
  outputPath: 'knowledge/topic.md'
}).result()

console.assert(INTEROP_SCHEMA_VERSION === 2)
console.assert(verified.valid === true)
console.assert(explained.record.outputPath === 'knowledge/topic.md')
```

See [Python · Node.js](guide/python-node.md) for project/provider/jobs/error
examples and package build instructions.

## Security model

- Sources are hostile and immutable; the compiler never executes source
  plugins or scripts.
- AI output is a proposal, not policy or approval.
- Provider disclosure is minimized, sensitive work is local, and remote
  consent is per call.
- Output uses sibling staging, verification, and no-replace publication.
- Verification establishes internal consistency, not publisher authenticity.
- Project data is plaintext and source paths/records may be sensitive.

Details are in
[Security and Trust Boundaries](docs/specs/security-and-trust-boundaries.md).

## Workspace

```text
crates/okc-core      corpus and current integration compiler
crates/okc-ai        provider-neutral AI transports and schemas
crates/okc-app       projects, journal, review, artifact/application services
crates/okc-interop   runtime-neutral language facade and jobs
crates/okc           sole CLI/TUI binary
bindings/python      PyO3 package
bindings/node        napi-rs package
docs                 normative contracts, algorithms, ADRs, history
guide                Korean operator guides
```

All retained Rust packages use workspace version `0.3.0`.

## Verification

```bash
cargo check --locked --workspace --all-targets --all-features
cargo test --locked --workspace --all-features --no-fail-fast
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check

cd bindings/python
pytest tests
mypy --strict tests/typing_contract.py

cd ../node
npm run build
npm test
npm run typecheck
npm pack --dry-run

cd ../../guide
npm run docs:build
```

The current cross-language artifact inventory golden is:

```text
452ca0671e806a93b4f36f218cf9e62da899f6404c74705c2cf0ca14e413c7e5
```

Exact evidence and remaining release blockers are tracked in
[CURRENT_STATE](docs/CURRENT_STATE.md) and
[TRACEABILITY](docs/TRACEABILITY.md).

## License

Dual licensed under MIT OR Apache-2.0. See [LICENSE-MIT](LICENSE-MIT) and
[LICENSE-APACHE](LICENSE-APACHE).
