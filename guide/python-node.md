---
title: Python · Node.js 라이브러리
description: CLI 없이 현재 OKC 프로젝트와 승인 워크플로를 실행하는 방법
outline: [2, 3]
---

# Python · Node.js에서 OKC 사용하기

Python과 Node.js 패키지는 같은 Rust application service를 호출합니다.
프로젝트 형식, 승인, artifact byte, provenance와 검증 규칙은 CLI/TUI와
같으며 binding 전용 상태를 만들지 않습니다.

::: warning 아직 registry에 게시하지 않았습니다
현재 `0.3.0` 개발 트리는 wheel·sdist·npm tarball을 빌드하고 설치 검증하는
단계입니다. PyPI/npm publish와 credential은 저장소 CI에 포함되지 않습니다.
아래 설치 이름은 첫 release candidate 계약입니다.
:::

## 지원 환경과 설치 이름

| 환경 | 계약 |
|---|---|
| Python | CPython 3.11+, distribution `okc-compiler`, import `okc`, `abi3-py311` |
| Node.js | 22.13+, package `okc-compiler`, ESM·CommonJS·TypeScript, Node-API 9 |
| native target | Linux x86_64 GNU, Windows x86_64 MSVC, macOS x86_64, macOS arm64 |

릴리스 후에는 각각 `python -m pip install okc-compiler`와
`npm install okc-compiler`를 사용합니다. 현재 source checkout에서는
저장소 루트에서 다음 명령으로 개발 package를 만들 수 있습니다.

```sh
python -m venv .venv-sdk
source .venv-sdk/bin/activate
python -m pip install maturin==1.15.0 pytest==8.4.2 mypy==1.18.2
python -m maturin develop --manifest-path bindings/python/Cargo.toml --release --locked

npm ci --ignore-scripts --prefix bindings/node
npm run build --prefix bindings/node
```

Windows PowerShell에서는 `.venv-sdk\Scripts\Activate.ps1`로 Python 환경을
활성화합니다. Python 확장은 현재 interpreter의 가상환경에 설치되고, Node
빌드는 현재 platform의 native addon을 `bindings/node`에 만듭니다.

## 소스 체크아웃 검증

개발 빌드 직후 공개 API, 전체 현재 승인 흐름, 임시 retired-schema marker의
명시적 unsupported 오류와 타입 계약을 함께 검사합니다.

```sh
python -m pytest bindings/python/tests -q
python -m mypy --strict bindings/python/tests/typing_contract.py

npm test --prefix bindings/node
npm run typecheck --prefix bindings/node
```

wheel, sdist, root npm tarball과 platform addon tarball을 만드는 정확한 clean
install 절차는
[`sdk-bindings.yml`](https://github.com/dolgogae/okc/blob/main/.github/workflows/sdk-bindings.yml)에
고정되어 있습니다. 이 workflow는 각 산출물의 SHA-256과 CycloneDX SBOM을
검사하지만 registry에는 게시하지 않습니다. 로컬에서 workflow 정의를
실행했다는 사실만으로 네 platform gate가 통과한 것은 아닙니다.

## 가장 중요한 경계

- project, source, output, artifact 경로는 모두 절대 경로여야 합니다.
- library는 cwd project 탐색, prompt, TUI, signal handler, updater, native
  keychain 또는 global tracing 설정을 사용하지 않습니다.
- provider profile에는 raw API key가 아니라 환경변수 이름만 넣습니다.
  Rust가 provider job을 시작할 때마다 그 변수를 읽습니다.
- remote cache miss는 호출마다 두 consent 값을 명시해야 합니다. 이 값은
  project에 저장되거나 다음 resume에 재사용되지 않습니다.
- taxonomy와 각 cluster는 사람이 명시적으로 승인해야 합니다. binding은
  AI proposal이나 critic waiver를 자동 승인하지 않습니다.
- 인식 가능한 Schema 1/2 artifact는 읽지 않고
  `ARTIFACT_SCHEMA_UNSUPPORTED`와 detected schema/family를 반환합니다.
- Current Pack writer/reader는 library API에 없습니다.

`client.api_info()` 또는 `client.apiInfo()`로 `apiVersion`, product version,
동시 작업 제한을 확인할 수 있습니다. DTO와 진행 event의 현재 interop schema는
`okc.INTEROP_SCHEMA_VERSION == 2` 또는 `INTEROP_SCHEMA_VERSION === 2`입니다.

## Python

```python
import os
from pathlib import Path

import okc

os.environ["OKC_OPENAI_API_KEY"] = "process-owned-secret"
profile = okc.ProviderProfile(
    name="hosted",
    kind="open_ai",
    endpoint="https://api.openai.com",
    model="configured-model",
    api_key_env="OKC_OPENAI_API_KEY",
)
client = okc.OkcClient([profile], max_concurrent_jobs=4)

project = client.create_project(
    Path("/absolute/work/Notes.okc-project"),
    name="Notes",
    curator_id="curator-id",
    language="ko-KR",
).result()
project.add_source(
    okc.SourceInput("primary", Path("/absolute/vault"))
).result()
project.set_ai_route("hosted").result()

preflight = project.preflight().result()
first = project.integrate(
    allow_remote_provider=True,
    remote_disclosure_confirmed=True,
).result()
taxonomy = project.taxonomy().result()

# taxonomy["taxonomy"]["clusters"]를 검토한 뒤 승인합니다.
project.approve_taxonomy(rationale="전체 문서 배치를 검토함").result()
project.integrate(
    allow_remote_provider=True,
    remote_disclosure_confirmed=True,
).result()

for cluster in project.clusters().result()["payload"]:
    cluster_id = cluster["proposal"]["cluster_id"]
    # omission_rationales와 minor_waivers는 필요한 항목마다 exact ID로 채웁니다.
    project.approve_cluster(cluster_id).result()

project.integrate(
    allow_remote_provider=True,
    remote_disclosure_confirmed=True,
).result()
artifact = project.compile(Path("/absolute/output/CompiledVault")).result()
report = client.verify_artifact(Path(artifact["path"])).result()
explanation = client.explain_artifact(
    Path(artifact["path"]), output_path="knowledge/topic.md"
).result()
assert report["interop_schema_version"] == 2
assert report["valid"] is True
assert explanation["record"]["output_path"] == "knowledge/topic.md"
```

`preflight`, taxonomy, synthesis, critic 결과를 실제로 읽고 필요한 rationale을
입력한 뒤 다음 단계로 진행하세요. 예제의 빈 cluster approval은 omission과
minor finding이 전혀 없는 경우에만 성공합니다.

## Node.js

```js
import {
  OkcClient,
  ProviderProfile,
  SourceInput,
} from 'okc-compiler'

process.env.OKC_OPENAI_API_KEY = 'process-owned-secret'
const profile = new ProviderProfile({
  name: 'hosted',
  kind: 'open_ai',
  endpoint: 'https://api.openai.com',
  model: 'configured-model',
  apiKeyEnv: 'OKC_OPENAI_API_KEY',
})
const client = new OkcClient({
  providerProfiles: [profile],
  maxConcurrentJobs: 4,
})
const project = await client.createProject(
  '/absolute/work/Notes.okc-project',
  { name: 'Notes', curatorId: 'curator-id', language: 'ko-KR' },
).result()

await project.addSource(new SourceInput({
  sourceId: 'primary',
  path: '/absolute/vault',
})).result()
await project.setAiRoute('hosted').result()

const consent = {
  allowRemoteProvider: true,
  remoteDisclosureConfirmed: true,
}
await project.preflight().result()
await project.integrate(consent).result()
const taxonomy = await project.taxonomy().result()
await project.approveTaxonomy({ rationale: '전체 문서 배치를 검토함' }).result()
await project.integrate(consent).result()

for (const cluster of (await project.clusters().result()).payload) {
  await project.approveCluster(cluster.proposal.clusterId).result()
}
await project.integrate(consent).result()
const artifact = await project.compile('/absolute/output/CompiledVault').result()
const report = await client.verifyArtifact(artifact.path).result()
const explanation = await client.explainArtifact(artifact.path, {
  outputPath: 'knowledge/topic.md',
}).result()
console.assert(report.interopSchemaVersion === 2)
console.assert(report.valid === true)
console.assert(explanation.record.outputPath === 'knowledge/topic.md')
```

CommonJS에서는 같은 package를 `const okc = require('okc-compiler')`로
불러옵니다. 공개 값은 JavaScript 관례에 맞게 camelCase입니다.

## Job과 오류 처리

모든 file/provider/compiler 작업은 `Job`을 즉시 돌려줍니다. `state`로 현재
상태를 읽고, `events()`로 최대 64개의 진행 이벤트를 꺼내며, `result()`로
종료 결과를 기다립니다. Python의 `result()`는 현재 thread를 기다리게 하고,
Node.js의 `result()`는 `Promise`를 반환합니다. `events()`는 읽은 event를
queue에서 제거하지만 terminal 결과는 `result()`에 유지됩니다. `cancel()`은
publication 전 `requested`, publication 중 `too_late`, 종료 후
`already_finished`를 반환합니다.

Python은 `OkcError`, Node.js는 `OkcError` rejection을 제공합니다. 메시지를
파싱하지 말고 `code`, `category`, `retryable`, `details`를 사용하세요.

```python
try:
    client.open_project(Path("relative.okc-project")).result()
except okc.OkcError as error:
    assert error.code == "PATH_NOT_ABSOLUTE"
```

```js
try {
  await client.openProject('relative.okc-project').result()
} catch (error) {
  if (error.code === 'PROJECT_BUSY') {
    // 같은 project의 앞선 mutating job이 끝난 뒤 사용자가 다시 시도합니다.
  }
}
```

같은 client에서 서로 다른 project job은 병렬 실행할 수 있습니다. 같은
project의 mutating job을 두 개 동시에 시작하면 뒤 작업을 몰래 대기시키지
않고 `PROJECT_BUSY`로 즉시 실패합니다. 별도 process와의 충돌은 기존
`project.lock`이 막습니다.

자주 처리하는 안정 오류 코드는 다음과 같습니다.

| 코드 | 의미와 대응 |
|---|---|
| `PATH_NOT_ABSOLUTE` | 호출 전에 project/source/output/artifact 경로를 절대 경로로 결정합니다. |
| `PROJECT_BUSY` | 같은 project의 기존 mutating Job 결과를 확인한 뒤 사용자가 재시도합니다. |
| `PROVIDER_ENV_SECRET_MISSING` | profile의 `api_key_env`가 가리키는 변수를 Job 시작 전에 설정합니다. |
| `REMOTE_CONSENT_REQUIRED` | preflight 결과를 검토한 이번 호출에만 두 consent 값을 명시합니다. |
| `APPROVAL_REQUIRED` / `APPROVAL_STALE` | 최신 taxonomy·cluster revision을 다시 검토하고 승인합니다. |
| `OUTPUT_EXISTS` / `OUTPUT_OVERLAP` | 기존 대상을 보존하고 source와 겹치지 않는 새 절대 경로를 사용합니다. |
| `ARTIFACT_SCHEMA_UNSUPPORTED` | 현재 Schema 3 디렉터리를 사용합니다. `details`의 supported/detected 값을 기록합니다. |
| `VERIFICATION_FAILED` | artifact family, manifest, checksums와 provenance를 다시 검사합니다. |

오류별 운영 절차는 [문제 해결](./troubleshooting.md)에도 정리되어 있습니다.

## 다음 단계

CLI와 같은 승인 의미는 [AI 통합](./integration.md), provider와 민감
정보 정책은 [AI Provider](./ai-provider.md), 아직 남은 release blocker는
[현재 구현 상태](./current-state.md)를 확인하세요.
