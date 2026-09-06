export type ProviderKind = 'open_ai' | 'anthropic' | 'gemini' | 'ollama' | 'open_ai_compatible'
export type AiRole = 'embedding' | 'organizer' | 'synthesis' | 'critic'
export type JobState = 'queued' | 'running' | 'cancelling' | 'publishing' | 'completed' | 'failed' | 'cancelled'
export type CancelOutcome = 'requested' | 'too_late' | 'already_finished'

export const INTEROP_SCHEMA_VERSION: number

export interface ProviderProfileOptions {
  name: string
  kind: ProviderKind
  endpoint: string
  model: string
  apiKeyEnv?: string | null
  timeoutMs?: number
  maxResponseBytes?: number
  maxInputBytes?: number
  maxBatchItems?: number
  options?: Readonly<Record<string, unknown>>
}

export class ProviderProfile {
  constructor(options: ProviderProfileOptions)
  readonly name: string
  readonly kind: ProviderKind
  readonly endpoint: string
  readonly model: string
  readonly apiKeyEnv: string | null
  readonly timeoutMs: number
  readonly maxResponseBytes: number
  readonly maxInputBytes: number
  readonly maxBatchItems: number
  readonly options: Readonly<Record<string, unknown>>
}

export interface SourceInputOptions {
  sourceId: string
  path: string
  ownerDisplayName?: string | null
  snapshotId?: string | null
}

export class SourceInput {
  constructor(options: SourceInputOptions)
  readonly sourceId: string
  readonly path: string
  readonly ownerDisplayName: string | null
  readonly snapshotId: string | null
}

export interface ProgressEvent {
  interopSchemaVersion: number
  sequence: number
  operation: string
  state: JobState
  phase: string
  completed: number
  total: number | null
  currentItem: string | null
}

export class OkcError extends Error {
  readonly code: string
  readonly category: string
  readonly retryable: boolean
  readonly details: Readonly<Record<string, unknown>>
}

export class Job<T> {
  readonly state: JobState
  events(): ProgressEvent[]
  result(): Promise<T>
  cancel(): CancelOutcome
}

export interface ClientOptions {
  providerProfiles?: readonly ProviderProfile[]
  maxConcurrentJobs?: number
}

export interface RemoteConsent {
  allowRemoteProvider: boolean
  remoteDisclosureConfirmed: boolean
}

export type JsonObject = Record<string, unknown>

export class OkcClient {
  constructor(options?: ClientOptions)
  readonly providerProfiles: readonly ProviderProfile[]
  apiInfo(): JsonObject
  createProject(path: string, options: { name: string; curatorId: string; policyVersion?: string; language?: string | null }): Job<Project>
  openProject(path: string): Job<Project>
  testProvider(name: string): Job<JsonObject>
  verifyArtifact(path: string): Job<JsonObject>
  explainArtifact(path: string, options?: { outputPath?: string | null; package?: boolean; limit?: number | null; cursor?: string | null }): Job<JsonObject>
}

export class Project {
  readonly path: string
  manifest(): Job<JsonObject>
  status(): Job<JsonObject>
  addSource(source: SourceInput): Job<JsonObject>
  rebindSource(sourceId: string, path: string, options?: { snapshotId?: string | null }): Job<JsonObject>
  replaceSources(sources: readonly SourceInput[]): Job<JsonObject>
  setLanguage(language: string | null): Job<JsonObject>
  setAiRoute(profileName: string, options?: { role?: AiRole | null }): Job<JsonObject>
  preflight(): Job<JsonObject>
  integrate(consent: RemoteConsent): Job<JsonObject>
  taxonomy(): Job<JsonObject>
  approveTaxonomy(options?: { editedClusters?: readonly JsonObject[] | null; rationale?: string | null }): Job<JsonObject>
  clusters(): Job<JsonObject>
  approveCluster(clusterId: string, options?: { omissionRationales?: Readonly<Record<string, string>>; minorWaivers?: Readonly<Record<string, string>> }): Job<JsonObject>
  regenerateCluster(clusterId: string, feedback: string, consent: RemoteConsent): Job<JsonObject>
  compile(output: string): Job<JsonObject>
}

declare const api: {
  INTEROP_SCHEMA_VERSION: typeof INTEROP_SCHEMA_VERSION
  Job: typeof Job
  OkcClient: typeof OkcClient
  OkcError: typeof OkcError
  Project: typeof Project
  ProviderProfile: typeof ProviderProfile
  SourceInput: typeof SourceInput
}

export default api
