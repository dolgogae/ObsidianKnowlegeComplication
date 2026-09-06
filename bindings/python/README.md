# okc-compiler for Python

Python 3.11+ bindings for the Obsidian Knowledge Compiler. The import name is
`okc`; the distribution name is `okc-compiler`. This `0.3.0` package is a
development candidate and has not been published to PyPI.

All project, source, artifact, and output paths must be absolute. Provider API
keys are referenced by environment-variable name and are read by Rust when a
job starts; raw keys are never accepted by this API. Remote cache misses require
both disclosure-consent flags on every call, and taxonomy/cluster proposals
still require explicit human approval.

```python
from pathlib import Path
import okc

client = okc.OkcClient()
project = client.create_project(
    Path("/absolute/notes.okc-project"),
    name="Notes",
    curator_id="curator",
).result()
```

Filesystem, provider, and compiler operations return `Job[T]`. Use `state`,
`events()`, blocking `result()`, and `cancel()`; branch on the structured
`OkcError.code` or `.category`, never its message. Only current Schema 3
directories can be verified or explained; recognizable Schema 1/2 inputs
return `ARTIFACT_SCHEMA_UNSUPPORTED`.

From the repository root, build and test the extension with:

```sh
python -m pip install maturin==1.15.0 pytest==8.4.2 mypy==1.18.2
python -m maturin develop --manifest-path bindings/python/Cargo.toml --release --locked
python -m pytest bindings/python/tests -q
python -m mypy --strict bindings/python/tests/typing_contract.py
```

The complete current approval example and release limitations are in the
[Python · Node.js guide](https://github.com/dolgogae/okc/blob/main/guide/python-node.md).
