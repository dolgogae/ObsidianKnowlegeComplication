'use strict'

const native = require('./native.cjs')

const INTEROP_SCHEMA_VERSION = native.INTEROP_SCHEMA_VERSION

function camelKey(key) {
  return key.replace(/_([a-z])/g, (_, letter) => letter.toUpperCase())
}

function snakeKey(key) {
  return key.replace(/[A-Z]/g, letter => `_${letter.toLowerCase()}`)
}

function transformKeys(value, transform) {
  if (Array.isArray(value)) return value.map(item => transformKeys(item, transform))
  if (value === null || typeof value !== 'object') return value
  return Object.fromEntries(
    Object.entries(value).map(([key, item]) => [transform(key), transformKeys(item, transform)])
  )
}

function parseNative(value) {
  return transformKeys(JSON.parse(value), camelKey)
}

class OkcError extends Error {
  constructor({ code, category, message, retryable = false, details = {} }) {
    super(message)
    this.name = 'OkcError'
    this.code = code
    this.category = category
    this.retryable = retryable
    this.details = Object.freeze({ ...details })
  }
}

function asOkcError(error) {
  try {
    const payload = JSON.parse(error && error.message ? error.message : String(error))
    return new OkcError(transformKeys(payload, camelKey))
  } catch {
    return new OkcError({
      code: 'INTERNAL',
      category: 'internal',
      message: 'native binding returned an invalid structured error',
      retryable: false,
      details: {},
    })
  }
}

function nativeCall(call) {
  try {
    return call()
  } catch (error) {
    throw asOkcError(error)
  }
}

function argumentError(message) {
  return new OkcError({
    code: 'INVALID_ARGUMENT',
    category: 'argument',
    message,
    retryable: false,
    details: {},
  })
}

function remoteConsent(value) {
  if (
    !value ||
    typeof value.allowRemoteProvider !== 'boolean' ||
    typeof value.remoteDisclosureConfirmed !== 'boolean'
  ) {
    throw argumentError(
      'allowRemoteProvider and remoteDisclosureConfirmed must both be explicit booleans'
    )
  }
  return value
}

class ProviderProfile {
  constructor(input) {
    if (!input || typeof input !== 'object' || Array.isArray(input)) {
      throw argumentError('ProviderProfile requires an options object')
    }
    const allowed = new Set([
      'name', 'kind', 'endpoint', 'model', 'apiKeyEnv', 'timeoutMs',
      'maxResponseBytes', 'maxInputBytes', 'maxBatchItems', 'options',
    ])
    const unknown = Object.keys(input).filter(key => !allowed.has(key))
    if (unknown.length !== 0) {
      throw argumentError(`ProviderProfile contains unknown field: ${unknown[0]}`)
    }
    const {
      name,
      kind,
      endpoint,
      model,
      apiKeyEnv = null,
      timeoutMs = 120_000,
      maxResponseBytes = 16 * 1024 * 1024,
      maxInputBytes = 64 * 1024 * 1024,
      maxBatchItems = 2_048,
      options = {},
    } = input
    this.name = name
    this.kind = kind
    this.endpoint = endpoint
    this.model = model
    this.apiKeyEnv = apiKeyEnv
    this.timeoutMs = timeoutMs
    this.maxResponseBytes = maxResponseBytes
    this.maxInputBytes = maxInputBytes
    this.maxBatchItems = maxBatchItems
    this.options = Object.freeze({ ...options })
    Object.freeze(this)
  }

  _nativeValue() {
    return {
      name: this.name,
      kind: this.kind,
      endpoint: this.endpoint,
      model: this.model,
      api_key_env: this.apiKeyEnv,
      timeout_ms: this.timeoutMs,
      max_response_bytes: this.maxResponseBytes,
      max_input_bytes: this.maxInputBytes,
      max_batch_items: this.maxBatchItems,
      options: this.options,
    }
  }
}

class SourceInput {
  constructor({ sourceId, path, ownerDisplayName = null, snapshotId = null }) {
    this.sourceId = sourceId
    this.path = path
    this.ownerDisplayName = ownerDisplayName
    this.snapshotId = snapshotId
    Object.freeze(this)
  }

  _nativeValue() {
    return {
      source_id: this.sourceId,
      path: this.path,
      owner_display_name: this.ownerDisplayName,
      snapshot_id: this.snapshotId,
    }
  }
}

class Job {
  constructor(nativeJob, mapper = value => value) {
    this._native = nativeJob
    this._mapper = mapper
  }

  get state() {
    return parseNative(nativeCall(() => this._native.state()))
  }

  events() {
    return parseNative(nativeCall(() => this._native.eventsJson()))
  }

  async result() {
    try {
      const payload = parseNative(await this._native.resultJson())
      return this._mapper(payload)
    } catch (error) {
      if (error instanceof OkcError) throw error
      throw asOkcError(error)
    }
  }

  cancel() {
    return parseNative(nativeCall(() => this._native.cancel()))
  }
}

class OkcClient {
  constructor({ providerProfiles = [], maxConcurrentJobs = 4 } = {}) {
    this.providerProfiles = Object.freeze([...providerProfiles])
    const profiles = this.providerProfiles.map(profile => {
      if (!(profile instanceof ProviderProfile)) {
        throw argumentError('providerProfiles must contain ProviderProfile instances')
      }
      return profile._nativeValue()
    })
    this._native = nativeCall(
      () => new native.NativeClient(JSON.stringify(profiles), maxConcurrentJobs)
    )
  }

  apiInfo() {
    return parseNative(nativeCall(() => this._native.apiInfoJson()))
  }

  createProject(path, { name, curatorId, policyVersion = 'policy-v3', language = null }) {
    return new Job(
      this._native.createProject(path, name, curatorId, policyVersion, language),
      value => this._projectResult(value)
    )
  }

  openProject(path) {
    return new Job(this._native.openProject(path), value => this._projectResult(value))
  }

  testProvider(name) {
    return new Job(this._native.testProvider(name))
  }

  verifyArtifact(path) {
    return new Job(this._native.verifyArtifact(path))
  }

  explainArtifact(path, options) {
    if (!options || typeof options !== 'object' || Array.isArray(options)) {
      throw argumentError('explainArtifact requires an options object')
    }
    const { outputPath } = options
    if (typeof outputPath !== 'string' || outputPath.length === 0) {
      throw argumentError('explainArtifact requires a non-empty outputPath')
    }
    return new Job(this._native.explainArtifact(path, outputPath))
  }

  _projectResult(value) {
    if (!value || value.resultType !== 'project' || typeof value.path !== 'string') {
      throw new OkcError({
        code: 'INTERNAL',
        category: 'internal',
        message: 'project job returned an invalid descriptor',
        retryable: false,
        details: {},
      })
    }
    return new Project(this, nativeCall(() => this._native.projectHandle(value.path)))
  }
}

class Project {
  constructor(client, nativeProject) {
    this._client = client
    this._native = nativeProject
  }

  get path() {
    return this._native.path()
  }

  manifest() {
    return new Job(this._native.manifest())
  }

  status() {
    return new Job(this._native.status())
  }

  addSource(source) {
    if (!(source instanceof SourceInput)) throw argumentError('source must be a SourceInput')
    return new Job(nativeCall(() => this._native.addSource(JSON.stringify(source._nativeValue()))))
  }

  rebindSource(sourceId, path, { snapshotId = null } = {}) {
    return new Job(this._native.rebindSource(sourceId, path, snapshotId))
  }

  replaceSources(sources) {
    const values = sources.map(source => {
      if (!(source instanceof SourceInput)) throw argumentError('sources must contain SourceInput instances')
      return source._nativeValue()
    })
    return new Job(nativeCall(() => this._native.replaceSources(JSON.stringify(values))))
  }

  setLanguage(language) {
    return new Job(this._native.setLanguage(language))
  }

  setAiRoute(profileName, { role = null } = {}) {
    return new Job(nativeCall(() => this._native.setAiRoute(role, profileName)))
  }

  preflight() {
    return new Job(this._native.preflight())
  }

  integrate(consent) {
    const value = remoteConsent(consent)
    return new Job(
      this._native.integrate(value.allowRemoteProvider, value.remoteDisclosureConfirmed)
    )
  }

  taxonomy() {
    return new Job(this._native.taxonomy())
  }

  approveTaxonomy({ editedClusters = null, rationale = null } = {}) {
    const encoded = editedClusters === null
      ? null
      : JSON.stringify(transformKeys(editedClusters, snakeKey))
    return new Job(nativeCall(() => this._native.approveTaxonomy(encoded, rationale)))
  }

  clusters() {
    return new Job(this._native.clusters())
  }

  approveCluster(clusterId, { omissionRationales = {}, minorWaivers = {} } = {}) {
    return new Job(
      nativeCall(() => this._native.approveCluster(
        clusterId,
        JSON.stringify(omissionRationales),
        JSON.stringify(minorWaivers)
      ))
    )
  }

  regenerateCluster(clusterId, feedback, consent) {
    const value = remoteConsent(consent)
    return new Job(
      this._native.regenerateCluster(
        clusterId,
        feedback,
        value.allowRemoteProvider,
        value.remoteDisclosureConfirmed
      )
    )
  }

  compile(output) {
    return new Job(this._native.compile(output))
  }
}

module.exports = {
  INTEROP_SCHEMA_VERSION,
  Job,
  OkcClient,
  OkcError,
  Project,
  ProviderProfile,
  SourceInput,
}
