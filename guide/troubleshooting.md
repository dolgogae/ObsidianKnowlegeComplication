---
title: 문제 해결
description: OKC CLI, TUI와 Python·Node.js 라이브러리에서 자주 만나는 오류 해결법
---

# 문제 해결

OKC는 입력, 결정 또는 출력이 모호하면 기존 데이터를 추측해서 덮어쓰는 대신
중단합니다. 먼저 `okc doctor`와 해당 명령의 `--help`를 확인하세요.

## `staging and destination are not siblings`

현재 `0.3.0` 트리에서는 상대/절대 parent를 canonicalize해 이 결함을
수정했습니다. 최신 소스인데도 이 메시지가 보이면 다른 `okc` 바이너리를
실행 중인지 `okc doctor`로 확인하세요. `0.2.0` 바이너리를 계속 써야 한다면
출력과 Pack을 절대 경로로 지정할 수 있습니다.

```sh
okc compile okc-run/approved-plan.json \
  --output "$PWD/CompiledVault" \
  --pack "$PWD/CompiledVault.okcpack"
```

이미 생성된 출력이 있다면 새 경로를 사용하세요. OKC는 기존 대상을
덮어쓰지 않습니다.

## `destination already exists`

Control file, Compiled Vault와 Pack은 기존 대상을 덮어쓰지 않습니다. 기존
결과를 검토·보관하고 새 출력 이름으로 실행하세요.

## language binding의 `PATH_NOT_ABSOLUTE`

Python과 Node.js 라이브러리는 cwd를 탐색하거나 상대 경로의 의미를 추측하지
않습니다. 호출하는 application이 project, source, output, artifact 경로를
절대 경로로 결정한 뒤 넘기세요. `.` 또는 `..` segment도 허용되지 않습니다.

```python
from pathlib import Path

project_path = Path("work/Notes.okc-project").resolve()
project = client.open_project(project_path).result()
```

```js
import { resolve } from 'node:path'

const project = await client.openProject(resolve('work/Notes.okc-project')).result()
```

## `PROVIDER_ENV_SECRET_MISSING`

`ProviderProfile.api_key_env` 또는 `apiKeyEnv`에는 secret 값이 아니라 환경변수
이름을 지정합니다. 해당 변수는 provider Job이 시작될 때 Rust가 읽으므로,
Job을 만들기 전에 같은 process 환경에 값을 설정하세요. Secret을 provider
options, project, 로그 또는 오류 메시지로 복사하지 마세요.

## `REMOTE_CONSENT_REQUIRED`

remote provider에 cache miss가 생긴 호출에는 `allow_remote_provider`와
`remote_disclosure_confirmed` 또는 각각의 camelCase 값을 모두 명시해야 합니다.
먼저 local preflight 결과와 전송 범위를 검토하세요. Consent는 project에
저장되지 않으므로 resume이나 cluster regeneration에서도 필요할 때 다시
제공합니다.

## `PROJECT_BUSY`

같은 project에 이미 mutating Job이 있으면 다음 작업은 자동으로 기다리지 않고
즉시 실패합니다. 진행 중인 Job의 `events()`와 `result()`를 확인하고, 종료된
뒤 명시적으로 재시도하세요. 다른 process가 소유한 `project.lock`을 수동으로
지우지 마세요.

## Node.js에서 native addon을 찾을 수 없음

현재 패키지는 npm에 게시되지 않았습니다. Source checkout에서는
`npm ci --ignore-scripts --prefix bindings/node` 다음
`npm run build --prefix bindings/node`를 실행해야 합니다. Release candidate를
검증할 때는 root tarball과 현재 platform addon tarball을 함께 clean project에
설치해야 하며, 다른 OS/architecture용 addon을 대신 복사해서는 안 됩니다.

## `required conflict(s) ... decision`

Plan은 만들어졌지만 필수 link ambiguity가 남아 있습니다. `plan.json`에서
`required: true`, `resolution: "unresolved"`인 항목을 찾아
[충돌 검토 가이드](./conflicts.md)대로 결정합니다.

## `supplied policy does not match`

`compile --policy`가 plan에 밀봉된 정책과 다릅니다. Plan 생성에 쓴 동일한
정책을 사용하거나 compile의 `--policy`를 생략해 embedded policy를
사용하세요.

## `source ... unsupported`

입력이 실제 디렉터리 또는 `.zip`, `.tar.zst`, `.tzst` regular file인지
확인합니다. Symlink, 특수 파일, 지원하지 않는 archive 확장자는 거부됩니다.

## 같은 Vault를 ID만 바꾸어 두 번 넣을 수 없음

OKC는 source ID가 달라도 전체 accepted content가 동일한 Vault를 중복
등록하지 않습니다. 복사본 하나를 source 목록에서 제거하세요.

## TUI가 열리지 않음

`okc doctor`에서 stdin/stdout TTY가 모두 `true`인지 확인하고 터미널을
80×24 이상으로 키웁니다. TUI는 network·검사·compile·verify를 worker에서
실행합니다. 작업 중 문제가 생기면 `Esc` 또는 `Ctrl+C`로 취소 확인을 열고,
같은 project를 다시 열어 완료되지 않은 task부터 재개할 수 있습니다.

## `okc update`가 cargo/manual 설치를 거부함

정상 동작입니다. Update는 일치하는 cargo-dist installation receipt가 있는
공식 installer copy만 수정합니다. 현재 공개 안정 installer가 없으므로
source build는 고정된 checkout에서 다시 빌드하세요.

## V1 artifact 확인

과거 `.vaultpack` 또는 `.vaultc/manifest.json`이 있는 디렉터리는 `verify`와
`explain`으로 읽을 수 있습니다. V1 plan/project를 이어서 수정하거나 V2로
컴파일하는 migration UI는 아직 없습니다.

## 종료 코드 빠른 확인

| 코드 | 먼저 볼 곳 |
|---:|---|
| `2` | 명령 인자와 policy |
| `3` | source path, archive, 권한, 안전 제한 |
| `4` | conflict와 decisions JSON |
| `5` | provider protocol, recording, proposal approval |
| `6` | approved plan, destination, Pack publication |
| `7` | artifact 무결성과 provenance subject/cursor |
| `70` | 재현 정보와 함께 bug report |
