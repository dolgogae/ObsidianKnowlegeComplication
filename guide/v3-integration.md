---
title: V3 AI 통합
description: provider를 설정하고 taxonomy와 cluster를 승인해 V3 Vault를 만드는 개발 흐름
---

# AI가 제안하고, 사람이 승인하는 V3 통합

V3의 새 Vault는 모든 Markdown을 AI가 분류·합성하고 critic이 다시 점검한
기록을 요구합니다. live provider는 제안을 만들 때만 필요합니다. 승인된
recording과 integration plan이 있으면 compile, verify, explain은 offline으로
동일하게 동작합니다.

::: warning 지금 버전은 시험용입니다
현재는 AI가 정리한 Markdown 문서와 원래 문서로 돌아가는 링크 파일만 새
Vault에 만듭니다. 첨부 파일, Canvas, Base 파일은 아직 옮기지 않으며, 큰
Vault를 빠르게 처리하는 기능과 결과 수정, `.okcpack` 만들기, 터미널 화면의
실제 작업 기능도 준비 중입니다. 원본 Vault는 반드시 그대로 보관하고, 이
버전이 만든 결과를 유일한 백업으로 사용하지 마세요.
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

V2 프로젝트라면 원본을 수정하지 않는 upgrade를 사용합니다.

```sh
okc project upgrade Old.okc-project --out Team.okc-project
```

복사되는 것은 source binding뿐입니다. V2 plan, proposal, approval은 V3
authority로 재사용되지 않습니다.

## 2. provider profile 설정하기

로컬 Ollama 예시는 다음과 같습니다. model ID는 자동 선택하지 않습니다.

```sh
okc provider add default \
  --kind ollama \
  --endpoint http://127.0.0.1:11434 \
  --model YOUR_MODEL
okc provider test default
```

Anthropic은 native embedding을 선언하지 않으므로 별도 embedding profile을
지정해야 합니다. API key 값은 TOML이 아니라 환경변수에 두고 profile에는
`--api-key-env ENV_NAME`만 저장합니다.

```sh
okc --project Team.okc-project project ai-route set embedding local_embedding
okc --project Team.okc-project project ai-route set critic strict_critic
```

## 3. preflight와 taxonomy 제안 실행하기

```sh
okc --project Team.okc-project integrate
okc --project Team.okc-project integration status --format json
okc --project Team.okc-project review taxonomy show
```

민감정보 finding은 matched text가 아니라 category, 위치, span, hash만
기록합니다. finding이 있으면 embedding/organizer와 해당 cluster 작업은
local profile이어야 합니다. remote 비대화형 실행은 두 플래그가 모두
필요합니다.

```sh
okc --project Team.okc-project integrate \
  --allow-remote-provider --yes
```

taxonomy를 직접 고치려면 전체 cluster 배열을 export해 수정한 뒤 다시
seal하며 승인합니다.

```sh
okc --project Team.okc-project review taxonomy export --out taxonomy.json
okc --project Team.okc-project review taxonomy approve \
  --edited-clusters taxonomy.json \
  --rationale "폴더 구조를 팀 용어에 맞춤"
```

## 4. synthesis와 critic 검토하기

taxonomy 승인 뒤 `integrate`를 다시 실행하면 모든 cluster(싱글턴 포함)의
synthesis와 critic task를 만들거나 완료 task를 재사용합니다.

```sh
okc --project Team.okc-project integrate
okc --project Team.okc-project review cluster list
okc --project Team.okc-project review cluster show CLUSTER_ID
okc --project Team.okc-project review cluster approve CLUSTER_ID
```

`major`/`critical` finding은 승인할 수 없습니다. feedback을 hash-bound 새
revision으로 기록한 뒤 integration을 다시 실행해 critic도 다시 거쳐야 합니다.

```sh
okc --project Team.okc-project review cluster regenerate CLUSTER_ID \
  --feedback "근거가 없는 요약을 제거하고 누락된 source block을 반영"
okc --project Team.okc-project integrate
```

omission 또는 `minor` finding은 각각의 exact ID와 이유가 필요합니다. 다음
값은 `review cluster show` 출력의 실제 ID로 바꿉니다.

```sh
okc --project Team.okc-project review cluster approve CLUSTER_ID \
  --omission-rationale 'DOCUMENT_ID:TARGET_ID=원문과 대조해 의도적으로 제외' \
  --minor-waiver 'FINDING_ID=critic 근거를 검토한 curator waiver'
```

모든 cluster를 승인한 뒤 `integrate`를 다시 실행하면 sealed plan의
`objects/<hash>` 경로를 출력합니다.

## 5. offline compile, verify, explain

```sh
okc --project Team.okc-project compile \
  --integration-plan Team.okc-project/objects/PLAN_OBJECT_HASH \
  --output CompiledVault
okc verify CompiledVault
okc explain CompiledVault knowledge/TAXONOMY/NOTE.md --format json
```

compile은 provider profile이나 API key가 없어도 됩니다. recording,
taxonomy approval, 모든 cluster proposal/critic/approval, omission과 waiver
closure 중 하나라도 빠지거나 stale이면 출력 전 실패합니다. 기존 출력은
덮어쓰지 않습니다.

다음은 [CLI 전체 명령](./cli.md)과 [현재 구현 상태](./current-state.md)를
확인하세요.
