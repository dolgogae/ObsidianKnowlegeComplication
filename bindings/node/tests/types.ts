import {
  ExplanationResult,
  Job,
  OkcClient,
  Project,
  ProviderProfile,
  SourceInput,
  VerificationResult,
} from '../index.js'

const profile = new ProviderProfile({
  name: 'local',
  kind: 'ollama',
  endpoint: 'http://127.0.0.1:11434',
  model: 'test',
})
const client = new OkcClient({ providerProfiles: [profile], maxConcurrentJobs: 2 })
const projectJob: Job<Project> = client.createProject('/tmp/types.okc-project', {
  name: 'Types',
  curatorId: 'curator',
})
const verificationJob: Job<VerificationResult> = client.verifyArtifact('/tmp/CompiledVault')
const explanationJob: Job<ExplanationResult> = client.explainArtifact('/tmp/CompiledVault', {
  outputPath: 'knowledge/topic.md',
})
void verificationJob
void explanationJob
void projectJob.result().then(project => {
  const source = new SourceInput({ sourceId: 'source-a', path: '/tmp/vault' })
  void project.addSource(source)
  void project.integrate({
    allowRemoteProvider: false,
    remoteDisclosureConfirmed: false,
  })
})
