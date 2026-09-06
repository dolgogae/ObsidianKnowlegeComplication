---
title: AI 통합
description: provider를 설정하고 taxonomy와 cluster를 승인해 현재 Vault를 만드는 개발 흐름
---

# AI가 제안하고, 사람이 승인하는 통합

현재 Schema 3 Vault는 모든 Markdown을 AI가 분류·합성하고 critic이 다시
점검한 기록을 요구합니다. Live provider는 제안을 만들 때만 필요합니다.
승인된 recording과 integration plan이 있으면 compile, verify, explain은
provider 없이 결정적으로 동작합니다.

::: warning 개발 빌드 범위
현재는 canonical Markdown과 원래 문서 경로의 redirect stub만 만듭니다.
Attachment, Canvas, Base, 전체 링크 재작성, OKCPack, 대규모 HNSW 경로는 아직
준비 중입니다. 원본 Vault를 그대로 보관하고 결과를 유일한 백업으로 쓰지
마세요.
:::

## 1. 프로젝트와 source 만들기

::: code-group

```sh [macOS / Linux]
okc project create Team.okc-project \
  --name Team --curator alice --language ko-KR
okc --project Team.okc-project \
  project source add personal /vaults/Personal
```

```powershell [Windows PowerShell]
okc.exe project create Team.okc-project `
  --name Team --curator alice --language ko-KR
okc.exe --project Team.okc-project `
  project source add personal C:\Vaults\Personal
```

:::

디렉터리, `.zip`, `.tar.zst`, `.tzst`를 source로 등록할 수 있습니다.
Source ID는 프로젝트 안에서 고유해야 하고, 동일한 전체 Vault 내용을 다른
ID로 중복 등록할 수 없습니다. 이전 schema의 프로젝트를 upgrade하거나
approval을 가져오는 기능은 없습니다.

## 2. provider profile 설정하기

로컬 Ollama 예시는 다음과 같습니다. Model ID는 자동 선택하지 않습니다.

```sh
okc provider add default \
  --kind ollama \
  --endpoint http://127.0.0.1:11434 \
  --model YOUR_MODEL
okc provider test default
okc --project Team.okc-project project ai-route set default
```

Anthropic처럼 embedding을 제공하지 않는 profile을 주 경로로 쓸 때는 별도
embedding profile을 지정합니다. API key 값은 저장하지 않고 profile에는
`--api-key-env ENV_NAME` 또는 CLI/TUI의 `--os-keychain ACCOUNT` 참조만 둡니다.

```sh
okc --project Team.okc-project \
  project ai-route set embedding local-embedding
okc --project Team.okc-project \
  project ai-route set critic strict-critic
```

## 3. preflight와 taxonomy 제안

```sh
okc --project Team.okc-project integrate
okc --project Team.okc-project integration status --format json
okc --project Team.okc-project review taxonomy show
```

민감정보 finding에는 matched text가 아니라 category, 위치, span, content hash만
기록됩니다. Finding이 있으면 semantic work와 관련 cluster 작업은 local
profile을 사용해야 합니다. Remote 비대화형 실행은 두 플래그를 모두
요구합니다.

```sh
okc --project Team.okc-project integrate \
  --allow-remote-provider --yes
```

Taxonomy를 바꾸려면 전체 cluster 배열을 export해 수정하고 다시 seal하면서
승인합니다.

```sh
okc --project Team.okc-project \
  review taxonomy export --out taxonomy.json
okc --project Team.okc-project \
  review taxonomy approve \
  --edited-clusters taxonomy.json \
  --rationale "폴더 구조를 팀 용어에 맞춤"
```

수정하지 않는 경우에도 `review taxonomy approve`로 명시적으로 승인합니다.

## 4. synthesis와 critic 검토

Taxonomy 승인 뒤 `integrate`를 다시 실행하면 모든 cluster의 synthesis와
critic task를 만들거나 동일 cache key의 완료 task를 재사용합니다.

```sh
okc --project Team.okc-project integrate
okc --project Team.okc-project review cluster list
okc --project Team.okc-project review cluster show CLUSTER_ID
okc --project Team.okc-project review cluster approve CLUSTER_ID
```

`major`/`critical` finding은 승인할 수 없습니다. Feedback을 hash-bound 새
revision으로 남긴 뒤 integration을 다시 실행해 새 critic 결과를 받습니다.

```sh
okc --project Team.okc-project \
  review cluster regenerate CLUSTER_ID \
  --feedback "근거 없는 요약을 제거하고 누락된 source block을 반영"
okc --project Team.okc-project integrate
```

Omission 또는 `minor` finding은 각각의 exact ID와 이유가 필요합니다. 아래
값은 `review cluster show`의 실제 ID로 바꿉니다.

```sh
okc --project Team.okc-project review cluster approve CLUSTER_ID \
  --omission-rationale 'DOCUMENT_ID:TARGET_ID=원문과 대조해 의도적으로 제외' \
  --minor-waiver 'FINDING_ID=critic 근거를 검토한 curator waiver'
```

모든 cluster가 승인되면 최신 `ApprovedIntegrationPlan`이 프로젝트 immutable
object로 봉인됩니다.

## 5. offline compile, verify, explain

```sh
okc --project Team.okc-project compile --output CompiledVault
okc verify CompiledVault
okc explain CompiledVault knowledge/TAXONOMY/NOTE.md --format json
```

명시적 plan object를 사용해야 하는 자동화는 다음 형태를 쓸 수 있습니다.

```sh
okc compile \
  --integration-plan /absolute/path/to/approved-plan.json \
  --output /absolute/path/to/CompiledVault
```

Compile에는 provider profile이나 API key가 필요하지 않습니다. Recording,
taxonomy approval, cluster proposal/critic/approval, omission/waiver 중 하나라도
빠지거나 stale이면 출력 전에 실패합니다. 기존 출력은 덮어쓰지 않습니다.

다음은 [CLI 전체 명령](./cli.md), [Python · Node.js](./python-node.md),
[현재 구현 상태](./current-state.md)를 확인하세요.
