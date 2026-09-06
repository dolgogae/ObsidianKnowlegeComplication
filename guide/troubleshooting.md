---
title: 문제 해결
description: OKC CLI, TUI와 Python·Node.js 라이브러리의 자주 만나는 오류
---

# 문제 해결

OKC는 입력, approval, artifact 또는 출력이 모호하면 추측하거나 덮어쓰지 않고
중단합니다. 먼저 `okc doctor`와 해당 명령의 `--help`를 확인하세요.

## `destination already exists`

Compile은 기존 파일, 디렉터리, symlink를 덮어쓰지 않습니다. 기존 결과를
검토·보관하고 source와 겹치지 않는 새 output 디렉터리를 사용하세요.

```sh
okc --project Team.okc-project compile --output CompiledVault-2
```

## `APPROVAL_REQUIRED` 또는 `APPROVAL_STALE`

최신 taxonomy와 모든 cluster proposal/critic을 다시 확인하세요. Source,
route, policy, taxonomy, proposal, critic, recording이 바뀌면 dependent approval을
재사용할 수 없습니다.

```sh
okc --project Team.okc-project integration status
okc --project Team.okc-project review taxonomy show
okc --project Team.okc-project review cluster list
```

Omission과 minor finding에는 exact ID별 rationale이 필요하고 major/critical은
regeneration으로 해결해야 합니다. [검토와 충돌](./conflicts.md)을 참고하세요.

## `ARTIFACT_SCHEMA_UNSUPPORTED`

현재 binary와 language package는 Schema 3 directory만 verify/explain합니다.
오류 `details`의 `supported_schema`/`supportedSchema`,
`detected_schema`/`detectedSchema`, format family를 기록하세요. 이전 marker나
`.vaultpack`/`.okcpack`은 읽거나 migration하지 않습니다. 해당 시점의 source가
필요하면 ADR-0027이 기록한 archive tag를 별도 checkout에서 검토합니다.

Mixed marker, symlinked marker/manifest, malformed/oversized JSON, unknown
family/schema는 unsupported로 추정하지 않고 `VERIFICATION_FAILED`로 닫힙니다.

## Language binding의 `PATH_NOT_ABSOLUTE`

Python과 Node.js는 cwd를 탐색하지 않습니다. 호출 application이 project,
source, output, artifact 경로를 `.`/`..` 없는 절대 lexical path로 만들어
전달합니다.

```python
from pathlib import Path

project = client.open_project(Path("work/Team.okc-project").resolve()).result()
```

```js
import { resolve } from 'node:path'

const project = await client.openProject(resolve('work/Team.okc-project')).result()
```

## `PROVIDER_ENV_SECRET_MISSING`

`api_key_env`/`apiKeyEnv`에는 secret이 아니라 환경변수 이름을 넣습니다.
Provider Job을 만들기 전에 같은 process environment에 값을 설정하세요.
Secret을 options, project, log 또는 오류에 복사하지 마세요.

## `REMOTE_CONSENT_REQUIRED`

Remote cache miss가 생기는 호출에 두 값을 모두 지정합니다. 먼저 preflight와
전송 범위를 검토하세요. Consent는 저장되지 않으므로 resume이나 cluster
regeneration에서도 새 remote miss에 다시 필요합니다.

```python
project.integrate(
    allow_remote_provider=True,
    remote_disclosure_confirmed=True,
).result()
```

```js
await project.integrate({
  allowRemoteProvider: true,
  remoteDisclosureConfirmed: true,
}).result()
```

## `PROJECT_BUSY`

같은 canonical project에 이미 mutating Job이 있으면 다음 작업은 즉시
실패합니다. 앞선 Job의 `events()`와 `result()`를 확인한 뒤 사용자가 다시
시도하세요. 다른 process가 소유한 `project.lock`을 수동 삭제하지 마세요.

## `source ... unsupported` 또는 unsafe path

Source가 실제 디렉터리, `.zip`, `.tar.zst`, `.tzst` regular file인지
확인합니다. Symlink, 특수 파일, traversal/reserved/invalid UTF-8 path,
archive bomb와 resource limit 초과는 거부됩니다. 같은 전체 Vault를 ID만
바꿔 두 번 등록할 수도 없습니다.

## Explain이 output을 찾지 못함

두 번째 인자는 artifact 내부의 정확한 `/` 구분 상대 경로입니다. Python은
keyword-only `output_path`, Node.js는 필수 `{outputPath}`를 사용합니다. 먼저
manifest의 current output path를 확인하고 `okc verify DIRECTORY`가 통과하는지
검사하세요.

## Node.js native addon을 찾을 수 없음

Source checkout에서는 다음을 실행합니다.

```sh
npm ci --ignore-scripts --prefix bindings/node
npm run build --prefix bindings/node
```

Release candidate 검증은 root tarball과 현재 platform addon tarball을 함께
clean project에 설치해야 합니다. 다른 OS/architecture addon을 복사하지
마세요.

## TUI가 열리지 않음

`okc doctor`에서 stdin/stdout TTY가 모두 `true`인지 확인하고 터미널을 80×24
이상으로 키웁니다. Pipe/CI에서는 no-argument 실행이 help와 exit 2를
반환합니다. 작업 중 `Esc` 또는 `Ctrl+C`로 취소 확인을 열고, project를 다시
열어 append-only journal의 첫 미완료 단계부터 재개할 수 있습니다.

## `okc update`가 source build를 거부함

정상 동작입니다. Update는 일치하는 cargo-dist receipt가 있는 공식 installer
copy만 수정합니다. 공개 안정 installer가 없으므로 source build는 원하는
checkout에서 다시 빌드하세요.

## 종료 코드

| 코드 | 먼저 볼 곳 |
|---:|---|
| `2` | 명령 인자와 제거된 option 여부 |
| `3` | source/project/path와 안전 제한 |
| `4` | 최신 taxonomy/cluster approval |
| `5` | provider capability, secret, disclosure |
| `6` | output 존재/overlap/publication |
| `7` | artifact schema/manifest/bytes/provenance |
| `70` | 재현 정보와 함께 bug report |
