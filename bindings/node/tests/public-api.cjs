'use strict'

const assert = require('node:assert/strict')
const crypto = require('node:crypto')
const fs = require('node:fs')
const http = require('node:http')
const os = require('node:os')
const path = require('node:path')
const test = require('node:test')

const okc = require('../index.cjs')

const FIXTURE_ARTIFACT_SHA256 =
  '452ca0671e806a93b4f36f218cf9e62da899f6404c74705c2cf0ca14e413c7e5'

function artifactDigest(root) {
  const files = []
  function visit(directory, prefix) {
    const entries = fs.readdirSync(directory, { withFileTypes: true })
      .sort((left, right) => Buffer.from(left.name).compare(Buffer.from(right.name)))
    for (const entry of entries) {
      const absolute = path.join(directory, entry.name)
      const relative = `${prefix}${entry.name}`
      if (entry.isDirectory()) visit(absolute, `${relative}/`)
      else if (entry.isFile()) files.push([relative, absolute])
    }
  }
  visit(root, './')
  const inventory = files.map(([relative, absolute]) => {
    const digest = crypto.createHash('sha256').update(fs.readFileSync(absolute)).digest('hex')
    return `${digest}  ${relative}\n`
  }).join('')
  return crypto.createHash('sha256').update(inventory).digest('hex')
}

function structuredOutput(input) {
  if (Object.hasOwn(input, 'semantic_candidates')) {
    return {
      clusters: [{
        cluster_id: 'sdk-fixture',
        title: 'SDK fixture',
        canonical_path: 'sdk/fixture.md',
        document_ids: input.documents.map(document => document.document_id),
      }],
    }
  }
  if (Object.hasOwn(input, 'proposal')) return { findings: [] }

  const dispositions = []
  for (const document of input.documents) {
    for (const block of document.blocks) {
      dispositions.push({
        kind: 'block',
        document_id: document.document_id,
        target_id: block.block_id,
        content_hash: block.content_hash,
        disposition: 'preserved_verbatim',
        rationale: '',
      })
    }
    for (const metadata of document.metadata) {
      dispositions.push({
        kind: 'metadata',
        document_id: document.document_id,
        target_id: metadata.metadata_id,
        content_hash: metadata.content_hash,
        disposition: 'preserved_verbatim',
        rationale: '',
      })
    }
  }
  return { sections: [], related_links: [], dispositions, contradictions: [] }
}

async function fixtureProvider(t) {
  const server = http.createServer((request, response) => {
    const chunks = []
    request.on('data', chunk => chunks.push(chunk))
    request.on('end', () => {
      const body = JSON.parse(Buffer.concat(chunks).toString('utf8'))
      let payload
      if (request.url === '/api/embed') {
        payload = {
          model: 'fixture-model',
          embeddings: body.input.map((_, index) => [1.0, index + 1.0]),
          prompt_eval_count: body.input.length,
        }
      } else if (request.url === '/api/chat') {
        const input = JSON.parse(body.messages[1].content)
        payload = {
          model: 'fixture-model',
          message: {
            role: 'assistant',
            content: JSON.stringify(structuredOutput(input)),
          },
          done: true,
          done_reason: 'stop',
          prompt_eval_count: 1,
          eval_count: 1,
        }
      } else {
        response.writeHead(404).end()
        return
      }
      const encoded = JSON.stringify(payload)
      response.writeHead(200, {
        'content-type': 'application/json',
        'content-length': Buffer.byteLength(encoded),
      })
      response.end(encoded)
    })
  })
  await new Promise((resolve, reject) => {
    server.once('error', reject)
    server.listen(0, '127.0.0.1', resolve)
  })
  t.after(() => new Promise(resolve => server.close(resolve)))
  return `http://127.0.0.1:${server.address().port}`
}

async function invalidProvider(t) {
  const server = http.createServer((request, response) => {
    request.resume()
    request.on('end', () => {
      response.writeHead(200, {
        'content-type': 'application/json',
        'content-length': 2,
        connection: 'close',
      })
      response.end('{}')
    })
  })
  await new Promise((resolve, reject) => {
    server.once('error', reject)
    server.listen(0, '127.0.0.1', resolve)
  })
  t.after(() => new Promise(resolve => server.close(resolve)))
  return `http://127.0.0.1:${server.address().port}`
}

test('CommonJS exposes API info and structured path errors', async () => {
  const client = new okc.OkcClient()
  assert.equal(client.apiInfo().interopSchemaVersion, 2)
  await assert.rejects(client.openProject('relative.okc-project').result(), error => {
    assert.ok(error instanceof okc.OkcError)
    assert.equal(error.code, 'PATH_NOT_ABSOLUTE')
    assert.equal(error.category, 'path')
    return true
  })
})

test('camelCase DTO conversion preserves arbitrary source metadata keys', async () => {
  const value = { release_date: '2026-09-06', releaseDate: 'distinct', nested_map: { source_id: 'user data' } }
  const job = new okc.Job({
    resultJson: async () => JSON.stringify({
      interop_schema_version: 2,
      corpus: { documents: [{ document_id: 'doc_fixture', metadata: [{
        metadata_id: 'metadata_fixture', value_index: 0, content_hash: 'hash', value,
      }] }] },
    }),
  })
  const result = await job.result()
  const metadata = result.corpus.documents[0].metadata[0]
  assert.equal(metadata.metadataId, 'metadata_fixture')
  assert.equal(metadata.valueIndex, 0)
  assert.deepEqual(metadata.value, value)
})

test('project create, open, and manifest round trip', async t => {
  const temporary = fs.mkdtempSync(path.join(os.tmpdir(), 'okc-node-'))
  t.after(() => fs.rmSync(temporary, { recursive: true, force: true }))
  const root = path.join(temporary, 'node.okc-project')
  const client = new okc.OkcClient()
  const project = await client.createProject(root, {
    name: 'Node',
    curatorId: 'curator',
    language: 'ko-KR',
  }).result()
  // Resolve both spellings: Rust may return a Windows extended-length path.
  assert.equal(fs.realpathSync.native(project.path), fs.realpathSync.native(root))
  const manifest = await project.manifest().result()
  assert.equal(manifest.payload.name, 'Node')
  assert.equal(manifest.payload.language, 'ko-KR')
  assert.equal(fs.realpathSync.native((await client.openProject(root).result()).path), fs.realpathSync.native(root))
})

test('remote provider flags must both be explicit', async t => {
  const temporary = fs.mkdtempSync(path.join(os.tmpdir(), 'okc-node-consent-'))
  t.after(() => fs.rmSync(temporary, { recursive: true, force: true }))
  const root = path.join(temporary, 'consent.okc-project')
  const client = new okc.OkcClient()
  const project = await client.createProject(root, {
    name: 'Consent',
    curatorId: 'curator',
  }).result()
  assert.throws(() => project.integrate({ allowRemoteProvider: false }), okc.OkcError)
})

test('missing environment secret is a structured provider error', async () => {
  const variable = 'OKC_NODE_TEST_SECRET_2A44F5_DO_NOT_SET'
  delete process.env[variable]
  const profile = new okc.ProviderProfile({
    name: 'missing-secret',
    kind: 'open_ai_compatible',
    endpoint: 'http://127.0.0.1:9',
    model: 'fixture',
    apiKeyEnv: variable,
  })
  const client = new okc.OkcClient({ providerProfiles: [profile] })
  await assert.rejects(client.testProvider('missing-secret').result(), error => {
    assert.ok(error instanceof okc.OkcError)
    assert.equal(error.code, 'PROVIDER_ENV_SECRET_MISSING')
    assert.equal(error.category, 'provider')
    return true
  })
})

test('invalid provider response is a structured provider error', async t => {
  const endpoint = await invalidProvider(t)
  const profile = new okc.ProviderProfile({
    name: 'invalid-response',
    kind: 'ollama',
    endpoint,
    model: 'fixture',
  })
  const client = new okc.OkcClient({ providerProfiles: [profile] })
  await assert.rejects(client.testProvider('invalid-response').result(), error => {
    assert.ok(error instanceof okc.OkcError)
    assert.equal(error.code, 'PROVIDER_RESPONSE_INVALID')
    assert.equal(error.category, 'provider')
    return true
  })
})

test('remote cache miss requires per-call consent', async t => {
  const temporary = fs.mkdtempSync(path.join(os.tmpdir(), 'okc-node-remote-'))
  t.after(() => fs.rmSync(temporary, { recursive: true, force: true }))
  const source = path.join(temporary, 'source')
  fs.mkdirSync(source)
  fs.writeFileSync(path.join(source, 'Note.md'), '# Remote consent\n')
  const profile = new okc.ProviderProfile({
    name: 'remote',
    kind: 'open_ai',
    endpoint: 'https://provider.invalid',
    model: 'fixture',
    timeoutMs: 10,
  })
  const client = new okc.OkcClient({ providerProfiles: [profile] })
  const project = await client.createProject(path.join(temporary, 'remote.okc-project'), {
    name: 'Remote consent',
    curatorId: 'sdk-test',
  }).result()
  await project.addSource(new okc.SourceInput({ sourceId: 'remote', path: source })).result()
  await project.setAiRoute('remote').result()
  await assert.rejects(
    project.integrate({
      allowRemoteProvider: false,
      remoteDisclosureConfirmed: false,
    }).result(),
    error => {
      assert.ok(error instanceof okc.OkcError)
      assert.equal(error.code, 'REMOTE_CONSENT_REQUIRED')
      assert.equal(error.category, 'consent')
      return true
    }
  )
})

for (const [kind, schema] of [
  ['schema-one-marker', 1],
  ['schema-one-pack', 1],
  ['schema-two-marker', 2],
  ['schema-two-pack', 2],
]) {
  test(`${kind} has a typed unsupported error`, async t => {
    const temporary = fs.mkdtempSync(path.join(os.tmpdir(), 'okc-node-unsupported-'))
    t.after(() => fs.rmSync(temporary, { recursive: true, force: true }))
    let artifact = path.join(temporary, kind)
    if (kind === 'schema-one-marker') {
      fs.mkdirSync(path.join(artifact, '.vaultc'), { recursive: true })
      fs.writeFileSync(path.join(artifact, '.vaultc', 'manifest.json'), '{}')
    } else if (kind === 'schema-two-marker') {
      fs.mkdirSync(path.join(artifact, '.okc'), { recursive: true })
      fs.writeFileSync(
        path.join(artifact, '.okc', 'manifest.json'),
        '{"format_family":"okc","schema_version":2}'
      )
    } else {
      artifact += kind === 'schema-one-pack' ? '.vaultpack' : '.okcpack'
      fs.writeFileSync(artifact, 'retired pack marker')
    }
    const client = new okc.OkcClient()
    for (const job of [
      client.verifyArtifact(artifact),
      client.explainArtifact(artifact, { outputPath: 'knowledge/Topic.md' }),
    ]) {
      await assert.rejects(job.result(), error => {
        assert.ok(error instanceof okc.OkcError)
        assert.equal(error.code, 'ARTIFACT_SCHEMA_UNSUPPORTED')
        assert.equal(error.category, 'verification')
        assert.equal(error.details.supportedSchema, 3)
        assert.equal(error.details.detectedSchema, schema)
        return true
      })
    }
  })
}

test('complete approval, compile, verify, and explain workflow', async t => {
  const endpoint = await fixtureProvider(t)
  const temporary = fs.mkdtempSync(path.join(os.tmpdir(), 'okc-node-current-'))
  t.after(() => fs.rmSync(temporary, { recursive: true, force: true }))
  const source = path.join(temporary, 'source')
  fs.mkdirSync(source)
  fs.writeFileSync(
    path.join(source, 'Fixture.md'),
    '# SDK fixture\n\nEvidence retained verbatim.\n'
  )
  const sourceBytes = fs.readFileSync(path.join(source, 'Fixture.md'))
  const profile = new okc.ProviderProfile({
    name: 'fixture',
    kind: 'ollama',
    endpoint,
    model: 'fixture-model',
  })
  const client = new okc.OkcClient({ providerProfiles: [profile] })
  const project = await client.createProject(path.join(temporary, 'node.okc-project'), {
    name: 'SDK parity',
    curatorId: 'sdk-test',
    language: 'en',
  }).result()
  await project.addSource(new okc.SourceInput({ sourceId: 'fixture', path: source })).result()
  await project.setAiRoute('fixture').result()

  const consent = { allowRemoteProvider: false, remoteDisclosureConfirmed: false }
  const first = await project.integrate(consent).result()
  assert.equal(first.interopSchemaVersion, 2)
  assert.equal(first.checkpoint, 'needs_taxonomy')
  const taxonomy = await project.taxonomy().result()
  assert.equal(taxonomy.interopSchemaVersion, 2)
  const clusterId = taxonomy.taxonomy.clusters[0].clusterId
  await project.approveTaxonomy({ rationale: 'fixture taxonomy reviewed' }).result()

  const second = await project.integrate(consent).result()
  assert.equal(second.checkpoint, 'needs_clusters')
  assert.equal((await project.clusters().result()).payload.length, 1)
  await project.approveCluster(clusterId).result()

  const final = await project.integrate(consent).result()
  assert.equal(final.checkpoint, 'ready_to_compile')
  const output = path.join(temporary, 'node-output')
  const compiled = await project.compile(output).result()
  assert.ok(path.isAbsolute(compiled.path))
  assert.equal(fs.realpathSync.native(compiled.path), fs.realpathSync.native(output))
  assert.equal(artifactDigest(output), FIXTURE_ARTIFACT_SHA256)
  const verification = await client.verifyArtifact(compiled.path).result()
  assert.equal(verification.interopSchemaVersion, 2)
  assert.equal(verification.valid, true)
  assert.equal(verification.artifactPath, compiled.path)
  assert.equal(verification.manifest.schemaVersion, 3)
  const explanation = await client.explainArtifact(compiled.path, {
    outputPath: 'knowledge/sdk/fixture.md',
  }).result()
  assert.equal(explanation.interopSchemaVersion, 2)
  assert.equal(explanation.artifactPath, compiled.path)
  assert.equal(explanation.record.outputPath, 'knowledge/sdk/fixture.md')
  await assert.rejects(project.compile(output).result(), error => {
    assert.equal(error.code, 'OUTPUT_EXISTS')
    return true
  })
  await assert.rejects(project.compile(source).result(), error => {
    assert.equal(error.code, 'OUTPUT_OVERLAP')
    return true
  })
  assert.deepEqual(fs.readFileSync(path.join(source, 'Fixture.md')), sourceBytes)
})
