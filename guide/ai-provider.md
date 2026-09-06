---
title: AI Provider 연결
description: 현재 provider profile, credential, disclosure와 proposal approval 경계
---

# AI Provider 연결하기

현재 Schema 3 integration에는 embedding, organizer, synthesis, critic 역할이
필요합니다. Provider는 evidence에 묶인 proposal만 반환합니다. Local schema와
identity/evidence 검증, critic, curator approval을 모두 통과해야 output에
반영됩니다. 완료 recording을 사용하는 compile/verify/explain에는 live
provider가 필요 없습니다.

전체 실행 순서는 [AI 통합](./integration.md)을 따르세요.

## Profile 만들기

지원하는 HTTP adapter는 OpenAI, Anthropic, Gemini, Ollama,
OpenAI-compatible입니다. Endpoint와 model ID를 명시합니다.

```sh
okc provider add local \
  --kind ollama \
  --endpoint http://127.0.0.1:11434 \
  --model MODEL
okc provider test local
```

Hosted profile에는 secret 값 대신 환경변수 이름을 저장합니다.

```sh
export OPENAI_API_KEY='process-owned-secret'
okc provider add hosted \
  --kind open-ai \
  --endpoint https://api.openai.com/v1 \
  --model MODEL \
  --api-key-env OPENAI_API_KEY
```

CLI/TUI에서는 opaque OS-keychain account도 사용할 수 있습니다.

```sh
okc provider add hosted-keychain \
  --kind open-ai \
  --endpoint https://api.openai.com/v1 \
  --model MODEL \
  --os-keychain hosted-keychain
```

Keychain이 잠겼거나 지원되지 않으면 환경변수 참조를 선택하거나 취소합니다.
평문 파일 fallback은 없습니다. Python/Node.js profile은 keychain을 열지 않고
환경변수 이름만 허용합니다.

## 역할별 route

한 profile을 기본값으로 지정하거나 role override를 둡니다.

```sh
okc --project Team.okc-project project ai-route set hosted
okc --project Team.okc-project \
  project ai-route set embedding local-embedding
okc --project Team.okc-project \
  project ai-route set critic strict-critic
```

Anthropic profile은 native embedding을 선언하지 않으므로 별도 embedding
profile이 필요합니다. Capability test는 exact provider/model response identity,
strict structured generation, embedding support와 bounds를 검사합니다.

`command` kind는 예약 값일 뿐 현재 adapter가 없습니다. Profile test와
language adapter에서 fail-closed로 거부됩니다.

## 공개 전 preflight

```sh
okc --project Team.okc-project integrate
```

OKC는 source block을 먼저 scan하고 finding의 category, 위치, span, content
hash만 저장합니다. Matched secret text는 기록하지 않습니다. Effective finding이
있으면 embedding/organizer 및 관련 cluster synthesis/critic은 local route여야
합니다.

Remote cache miss는 비대화형 실행에서 두 명시적 동의를 요구합니다.

```sh
okc --project Team.okc-project integrate \
  --allow-remote-provider --yes
```

이 값은 한 호출에만 적용되며 project나 recording에 다음 호출의 권한으로
저장되지 않습니다. Resume/regeneration에서 새 remote miss가 생기면 다시
동의해야 합니다.

## Recording, validation, approval

Application service가 portable request schema를 local에서 검사하고 provider
response의 JSON, bounds, request hash, response identity, IDs, coverage,
evidence를 다시 검증합니다. Request/response는 immutable object와 journal에
content hash로 기록됩니다. 동일한 complete cache key만 resume할 수 있습니다.

Provider가 taxonomy나 synthesis를 반환해도 자동 승인되지 않습니다.

```sh
okc --project Team.okc-project review taxonomy show
okc --project Team.okc-project review taxonomy approve
okc --project Team.okc-project review cluster show CLUSTER_ID
okc --project Team.okc-project review cluster approve CLUSTER_ID
```

Omission/minor waiver는 exact ID별 curator rationale이 필요하고
major/critical finding은 regeneration으로 해결해야 합니다.

## Secret과 오류 안전성

Token 값과 길이는 project, SQLite, profile serialization, recording,
provenance, progress event, 오류, `Debug`, 화면에 들어가지 않습니다. Provider
job 시작 시 environment 값을 다시 읽고 redacted/zeroizing 타입으로 다룹니다.
URL credential과 credential-like option key도 거부됩니다.

Transport는 TLS verification, bounded response/deadline, cancellation과 제한된
retry를 적용합니다. 자동화는 human message를 파싱하지 말고 structured error
code/category를 사용하세요.

규범 상세는
[AI Provider 명세](https://github.com/dolgogae/okc/blob/main/docs/specs/ai-provider-and-augmentation.md)에
있습니다.
