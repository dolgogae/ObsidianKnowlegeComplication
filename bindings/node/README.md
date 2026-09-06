# okc-compiler for Node.js

Node.js 22.13+ native bindings for the Obsidian Knowledge Compiler. Both ESM
and CommonJS are supported, and TypeScript declarations are included. This
`0.3.0` package is a development candidate and has not been published to npm.

All project, source, artifact, and output paths must be absolute. Provider API
keys are referenced by environment-variable name and resolved by Rust for each
job; this package never accepts a raw API key. Remote cache misses require both
disclosure-consent booleans on every call, and taxonomy/cluster proposals still
require explicit human approval.

```js
import { OkcClient } from 'okc-compiler'

const client = new OkcClient()
const project = await client.createProject('/absolute/notes.okc-project', {
  name: 'Notes',
  curatorId: 'curator',
}).result()
```

Filesystem, provider, and compiler operations return `Job<T>`. Use `state`,
`events()`, asynchronous `result()`, and `cancel()`; branch on the structured
`OkcError.code` or `.category`, never its message. Only current Schema 3
directories can be verified or explained; recognizable Schema 1/2 inputs
return `ARTIFACT_SCHEMA_UNSUPPORTED`.

From the repository root, build and test the current native addon with:

```sh
npm ci --ignore-scripts --prefix bindings/node
npm run build --prefix bindings/node
npm test --prefix bindings/node
npm run typecheck --prefix bindings/node
```

The complete current approval example, CommonJS usage, and release limitations are
in the
[Python · Node.js guide](https://github.com/dolgogae/okc/blob/main/guide/python-node.md).
