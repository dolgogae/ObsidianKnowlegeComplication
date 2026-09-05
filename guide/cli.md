---
title: CLI 사용법
description: 실제 Vault를 검사하고 계획하고 컴파일하는 OKC CLI 가이드
---

# CLI로 내 Vault 컴파일하기

새 schema-3 Vault는 project/provider/integration review 흐름을 사용합니다.
전체 예제는 [V3 AI 통합](./v3-integration.md)에 있습니다. 아래의
`inspect → plan → approve` 예제는 frozen V2 회귀·호환 경로를 설명합니다.

OKC의 현재 실사용 인터페이스는 CLI입니다. 원본을 바로 합치지 않고
`inspect → plan → approve → compile → verify` 단계를 파일로 남기므로,
어떤 결정이 결과에 들어갔는지 나중에도 확인할 수 있습니다.

::: tip 처음이라면
먼저 [5분 Quickstart](./index.md)를 완료하세요. 저장소의 충돌 없는 fixture로
전체 흐름을 한 번 통과한 다음 실제 Vault로 바꾸는 편이 쉽습니다.
:::

## 입력 지정하기

입력은 `ID=PATH` 형식을 권장합니다.

```sh
personal=/vaults/Personal
team=/vaults/TeamVault.zip
archive=/vaults/Archive.tar.zst
```

실제 명령에는 다음처럼 넣습니다.

```sh
okc inspect \
  personal=/vaults/Personal \
  team=/vaults/TeamVault.zip
```

지원 입력은 디렉터리, `.zip`, `.tar.zst`, `.tzst`입니다. ID에는 ASCII
영숫자, `-`, `_`, `.`만 사용할 수 있으며 한 source set에서 중복될 수
없습니다. 경로만 주면 마지막 파일명에서 ID를 만들지만 반복 빌드에는
명시적인 안정 ID가 더 안전합니다.

## 실제 작업 순서

### 1. Inspect

입력을 안전하게 열어 snapshot과 canonical IR을 만듭니다. 이 단계는 원본을
수정하거나 Compiled Vault를 만들지 않습니다.

```sh
okc inspect \
  personal=/vaults/Personal \
  team=/vaults/TeamVault.zip \
  --workspace okc-run/build.sqlite
```

전체 구조화 결과가 필요하면 `--format json`을 사용합니다. JSON inspection은
source text와 로컬 경로를 포함할 수 있으므로 공개 로그에 남기지 마세요.

### 2. Plan

```sh
okc plan \
  personal=/vaults/Personal \
  team=/vaults/TeamVault.zip \
  --workspace okc-run/build.sqlite \
  --out okc-run/plan.json
```

`plan.json`에는 출력 경로, deduplication, link rewrite, diagnostics, conflicts,
resource estimate가 들어갑니다. 필수 결정이 있으면 파일을 정상적으로 쓴
뒤 종료 코드 `4`를 반환합니다. 이 경우 [충돌 검토](./conflicts.md)로
이어갑니다.

### 3. Approve

```sh
okc approve okc-run/plan.json \
  --decisions okc-run/decisions.json \
  --out okc-run/approved-plan.json
```

AI를 사용하지 않으면 `--proposals`를 생략합니다. 필수 conflict decision,
plan ID 또는 content hash가 빠지거나 오래되었으면 승인되지 않습니다.

### 4. Compile과 Pack

```sh
okc compile okc-run/approved-plan.json \
  --output "$PWD/CompiledVault" \
  --pack "$PWD/CompiledVault.okcpack"
```

Pack이 필요 없으면 `--pack`을 생략합니다. 출력과 Pack은 모두 존재하지
않는 경로여야 하며 source와 같거나 source 안팎으로 겹칠 수 없습니다.
Compiled Vault 게시와 Pack 게시는 순서가 있는 두 작업이므로, Pack만
실패했을 때 이미 검증된 Vault가 남을 수 있습니다.

상대/절대 출력 경로는 모두 지원됩니다. 예제는 source/output 위치를
분명하게 보이기 위해 절대 경로를 사용합니다.

### 5. Verify와 Explain

```sh
okc verify ./CompiledVault
okc verify ./CompiledVault.okcpack --format json

okc explain ./CompiledVault knowledge/Topic.md
okc explain ./CompiledVault.okcpack --package --format json
```

`explain`의 파일 경로는 Compiled Vault 내부의 `/` 구분 상대 경로입니다.
페이지가 크면 응답의 `next_cursor`를 같은 artifact와 subject에 전달합니다.

```sh
okc explain ./CompiledVault knowledge/Topic.md \
  --limit 256 \
  --cursor 'cursor_...'
```

## 명령 한눈에 보기

| 명령 | 하는 일 | 중요한 동작 |
|---|---|---|
| `project create|upgrade|source|ai-route` | V3 프로젝트 구성 | upgrade는 V2 원본을 수정하지 않음 |
| `provider add|list|show|test|remove` | 전역 profile 관리 | key 값 대신 env 이름만 저장 |
| `integrate` | V3 AI task 실행·재개 | remote 비대화형 실행은 두 consent flag 필요 |
| `review taxonomy ...` | taxonomy 편집·승인 | 전체 taxonomy hash를 다시 seal |
| `review cluster ...` | synthesis/critic 승인 | major/critical finding 승인 불가 |
| `integration status` | run/task 상태 확인 | `--format json` 지원 |
| `inspect SOURCE...` | source snapshot과 IR 검사 | JSON 출력 가능, Vault 출력 없음 |
| `plan SOURCE... --out FILE` | Draft Plan 생성 | 필수 충돌 시 파일을 쓰고 코드 4 |
| `augment PLAN ... --out FILE` | 외부 provider에 제안 요청 | 문서 선택 필수 |
| `validate PLAN --augmentation FILE` | recording 재검증 | 파일과 네트워크를 사용하지 않음 |
| `replay PLAN --augmentation FILE --out FILE` | recording 재검증·재출력 | provider를 호출하지 않음 |
| `approve PLAN --decisions FILE --out FILE` | 결정과 plan 결합 | AI 기록은 `--proposals`로 추가 |
| `compile APPROVED_PLAN --output PATH` | 새 Vault 게시 | 선택적 `--pack`; 기존 대상 보존 |
| `verify PATH_OR_PACK` | artifact 독립 검증 | V3/V2/frozen V1 자동 판별 |
| `explain PATH_OR_PACK ...` | provenance 조회 | 내부 path 또는 `--package` 선택 |
| `doctor` | 환경과 프로젝트 진단 | 버전, TTY, target 출력 |
| `update [stable\|latest\|VERSION]` | receipt 기반 업데이트 | cargo/manual 빌드 덮어쓰기 거부 |
| `tui` | 터미널 UI 실행 | 현재 탐색 셸 상태 |

전체 옵션은 실행 중인 바이너리의 도움말이 가장 정확합니다.

```sh
okc --help
okc plan --help
okc augment --help
```

## 전역 옵션

| 옵션 | 현재 사용하는 명령 | 의미 |
|---|---|---|
| `--project PATH` | V3 integrate/review/compile, `tui`, `doctor` | schema-3 `.okc-project` 선택 |
| `--policy FILE` | `inspect`, `plan`, 선택적 `compile` | compile 시 밀봉된 정책과 일치해야 함 |
| `--workspace FILE` | `inspect`, `plan` | SQLite 작업 공간; 기본 `.okc-work/build.sqlite` |

Clap은 전역 옵션을 다른 하위 명령에서도 파싱하지만, 현재 실제 적용 범위는
표와 같습니다. Plan 이후 작업은 control file에 밀봉된 정책을 사용합니다.

## 자동화와 종료 코드

human 문구는 안정된 API가 아닙니다. 자동화에서는 `--format json`, JSONL
recording, stderr 진단과 다음 종료 코드를 사용하세요.

| 코드 | 의미 |
|---:|---|
| `0` | 성공 |
| `2` | 인자 또는 정책 사용 오류 |
| `3` | unsafe, missing, unreadable, malformed, unsupported source |
| `4` | 계획 충돌 또는 conflict decision 오류 |
| `5` | provider, augmentation, proposal, proposal approval 오류 |
| `6` | approved plan, 출력, compile, Pack publication 오류 |
| `7` | artifact verification 또는 provenance 오류 |
| `70` | 내부 invariant 오류 |

`plan`의 코드 `4`는 검토가 필요한 정상 분기일 수 있습니다. `set -e`나
`&&`만으로 묶은 스크립트에서는 코드 `0`과 `4`를 구분해 처리하세요.

## 다음 가이드

- [필수 충돌 검토하기](./conflicts.md)
- [AI Provider 연결하기](./ai-provider.md)
- [문제 해결](./troubleshooting.md)
