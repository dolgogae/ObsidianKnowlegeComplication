---
title: TUI 사용법
description: 현재 폴더에서 provider 연결부터 V3 compile·verify까지 실행하는 방법
---

# TUI 사용법

원하는 작업 폴더로 이동해 `okc`를 실행하면 그 폴더가 workspace 기준이
됩니다. TUI 안에서 AI 연결, Vault 선택, 공개 사전 점검, taxonomy와 cluster
검토, compile, 독립 verify를 순서대로 완료할 수 있습니다.

::: warning 개발 빌드 범위
현재 TUI 출력은 Markdown-only V3 directory입니다. V3 OKCPack과
attachment/Canvas/Base carry-through, 대규모 HNSW 경로는 아직 release
blocker입니다. source Vault는 읽기 전용으로 취급되며 출력은 source 밖의 새
경로에만 생성됩니다.
:::

## 1. 시작하기

실제 키 입력을 받을 수 있는 80×24 이상의 터미널에서 실행합니다.

```sh
cd /path/to/workspace
okc
```

명시한 project가 있으면 항상 우선합니다.

```sh
okc --project /path/to/Team.okc-project tui
```

`--project`가 없으면 다음 범위만 결정적으로 탐색합니다.

- cwd의 `.okc-work/workspace.okc-project`
- cwd가 Vault일 때 안전한 sibling `.okc-work` project
- cwd 직계 `*.okc-project` 디렉터리

한 개면 바로 열고 여러 개면 **Workspace**에서 고릅니다. project가 없으면
AI 연결과 source 선택을 마친 뒤 안전한 `.okc-work/*.okc-project`를 만듭니다.
파이프나 CI처럼 비대화형인 실행은 TUI를 열지 않고 도움말과 종료 코드 `2`를
반환합니다.

## 2. AI 연결

미완료 V3 AI 작업이 있고 사용할 profile이 없으면 **AI Connection**이 먼저
열립니다.

1. `Left`/`Right`로 OpenAI, Anthropic, Gemini, Ollama,
   OpenAI-compatible 중 하나를 고릅니다. schema-3 command provider는 아직
   선택할 수 없습니다.
2. `Up`/`Down` 또는 `Tab`으로 profile, endpoint, 명시적 model ID를
   입력합니다. 표준 provider와 Ollama에는 기본 endpoint가 채워집니다.
3. credential mode에서 `Space`로 OS keychain과 환경변수 참조를 바꿉니다.
4. `Enter`를 누르면 고정 synthetic request로 generation/embedding capability를
   검사합니다. 이 검사를 통과해야 연결됨으로 처리됩니다.

keychain 입력은 언제나 고정 길이 `************`로 표시됩니다. 실제 token과
길이는 화면, `Debug`, 오류, recording, TOML, project에 들어가지 않습니다.
profile에는 고정 OKC service 아래 account 참조만 남습니다. keychain이
잠겼거나 지원되지 않으면 환경변수 이름을 사용하거나 설정을 취소해야 하며
평문 파일 fallback은 없습니다.

Anthropic profile은 embedding을 제공하지 않으므로 primary 연결 뒤 별도의
embedding profile 화면이 이어집니다.

## 3. Vault 선택

**Vaults**에는 cwd 자체와 직계 하위의 안전한 Vault 후보, 직계 `.zip`,
`.tar.zst`, `.tzst`가 정렬되어 표시됩니다.

| 키 | 동작 |
|---|---|
| `Up` / `Down` | 후보 이동 |
| `Space` | 최대 10개까지 선택/해제 |
| `E` | 현재 source ID 수정 |
| `M` | 수동 source 경로 입력 |
| `Enter` | 체크한 목록을 전체 active source set으로 저장 |

`.okc-work`, 기존 IntegratedVault, Compiled Vault, project 디렉터리, symlink는
후보에서 제외됩니다. 같은 source ID, 조상과 자손의 동시 선택, 10개 초과는
저장 전에 거부됩니다. 저장 경로는 cwd 기준 절대 canonical 경로로 고정되고,
기존 project에서 목록을 바꾸면 과거 run을 지우지 않은 채 새 run을 만들며
이전 승인은 stale 상태가 됩니다.

## 4. Preflight와 원격 공개 동의

**Preflight**에서 첫 `Enter`는 provider를 호출하지 않습니다. source를
검사하고 다음 정보를 보여줍니다.

- Markdown document/block/input byte 수
- 예상 token/request 범위
- sensitive finding 수(원문 대신 category/location/content hash만 기록)
- embedding/organizer/synthesis/critic profile과 local/remote 경계

sensitive 내용이 있으면 semantic route와 해당 cluster route는 local이어야
합니다. remote route가 필요한 다음 uncached 작업은 실행 직전에 확인 창을
열며 `Y`가 있어야만 전송합니다. `N`이나 `Esc`는 취소합니다. offline
compile/verify와 이미 완료된 task cache 자체는 provider를 호출하지 않습니다.

## 5. Taxonomy 검토

organizer proposal이 준비되면 **Taxonomy**로 이동합니다. 자동 승인은 없습니다.

| 키 | 동작 |
|---|---|
| `Up` / `Down` | cluster 이동 |
| `E` / `P` | 이름 / canonical path 수정 |
| `Left` / `Right` | 현재 cluster의 document 하나를 이웃 cluster로 이동 |
| `M` | 현재 cluster를 이전 cluster와 merge |
| `S` | document 하나를 새 cluster로 split |
| `X` | 수정 이유 입력 |
| `A` 또는 `Enter` | 전체 taxonomy 명시 승인 |

모든 document가 정확히 한 cluster에 있어야 승인됩니다. 수정한 proposal은
rationale이 없으면 거부됩니다. 승인은 reseal된 taxonomy hash에 결합됩니다.

## 6. Cluster 검토와 재생성

**Clusters**는 `Source / Evidence`, `Synthesis`, `Critic`의 세 pane으로
표시됩니다. `[`와 `]`로 검토할 cluster를 이동합니다.

- `major`/`critical` finding은 승인할 수 없습니다.
- omission과 `minor` finding은 `Up`/`Down`으로 하나씩 이동하고 `Space`로
  각각 확인해야 합니다.
- 확인할 항목마다 `W`로 개별 rationale도 입력해야 합니다.
- `A` 또는 `Enter`는 현재 revision만 명시 승인합니다.
- `R`로 feedback을 입력하고 `G`를 누르면 이전 proposal/critic hash와 feedback
  hash에 결합된 새 revision을 생성하고 critic을 다시 실행합니다. 이전 승인은
  즉시 stale이 됩니다.

모든 cluster가 승인되면 application service가 최신 integration plan을 자동
봉인합니다.

## 7. Build와 Verify

**Build**는 source 밖에서 처음 비어 있는 `IntegratedVault`,
`IntegratedVault-2`, … 경로를 제안합니다. `E`로 경로를 수정하고 `Enter`로
확정합니다. 기존 파일이나 디렉터리는 덮어쓰지 않습니다.

compile은 승인 plan과 recording만 읽는 provider-free 작업입니다. staging 뒤
atomic publication barrier에 들어가며, 그 이후에는 취소할 수 없다는 상태가
표시됩니다. publication 직후 별도 verify를 실행하고 둘 다 성공한 경우에만
**Verify** 성공 상태로 이동합니다. **Verify**에서 `Enter`를 누르면 같은
artifact를 다시 독립 검증합니다.

## 8. 작업, 취소, 재개

네트워크·검사·compile·verify는 bounded worker thread에서 실행되어 화면이
계속 반응합니다. 한 번에 하나의 변경 작업만 실행하며 Status 줄은 phase,
현재 cluster, 완료/전체 수를 표시합니다.

작업 중 `Esc` 또는 `Ctrl+C`를 누르면 취소 확인 창이 열립니다. `Y`는 취소
token을 core/provider 쪽으로 전달하고 `N`은 계속합니다. publication barrier
뒤에는 취소가 거부됩니다. 다시 실행하면 append-only journal의 완료 task는
호출하지 않고 첫 실패·취소 지점부터 재개합니다.

## 9. 공통 키와 표시 안전성

| 키 | 동작 |
|---|---|
| `Tab` / `Shift+Tab`, `Left` / `Right` | 입력 중이 아닐 때 화면 이동 |
| `?` | 도움말 표시 |
| `Esc` | 입력/확인 닫기, idle이면 종료 |
| `Ctrl+C` | idle이면 종료, 작업 중이면 취소 확인 |
| Settings의 `L` / `A` / `H` | 언어 / ASCII-safe / 고대비 전환 |

source/provider 문자열의 control 및 bidi 문자는 escape해서 렌더링합니다. 작은
터미널에서는 데이터 대신 필요한 최소 크기만 표시합니다. 정상 종료와 panic은
raw mode와 alternate screen 복원을 시도합니다.

## 다음 단계

- 동일 기능을 자동화하려면 [CLI 사용법](./cli.md)
- provider별 설정은 [AI Provider](./ai-provider.md)
- 구현 범위와 blocker는 [현재 구현 상태](./current-state.md)
- 오류별 대응은 [문제 해결](./troubleshooting.md)
