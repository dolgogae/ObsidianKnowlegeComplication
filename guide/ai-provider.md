---
title: AI Provider 연결
description: 명시적 공개, recording, replay와 proposal approval 사용법
---

# AI Provider 연결하기

새 V3 Vault를 만들 때 AI integration은 필수입니다. provider는 파일을 직접
바꾸지 않고 evidence에 묶인 proposal만 반환하며, schema validation,
critic, 사람의 명시적 승인을 모두 통과해야 합니다. 완료 recording을
사용한 offline compile/verify에는 live provider가 필요 없습니다.

V3 profile/route 설정과 review 명령은 [V3 AI 통합](./v3-integration.md)을
따르세요.

## V3 credential 저장 경계

TUI에서는 OS keychain 또는 환경변수 참조를 선택할 수 있습니다. keychain
token은 고정 길이 mask로 입력하며 profile TOML에는 고정 OKC service 아래의
profile별 account 이름만 기록됩니다. token 값과 길이는 project, SQLite,
recording, 오류, `Debug`, 화면에 기록되지 않고 입력 buffer는 사용 뒤
zeroize됩니다. keychain이 잠겼거나 지원되지 않으면 환경변수 이름을
사용하거나 취소해야 하며 평문 파일로 대체하지 않습니다.

CLI profile은 기존 환경변수 방식과 `--os-keychain ACCOUNT` 참조를 모두
지원합니다. 실제 secret을 인자로 전달하지 않습니다.

```sh
okc provider add default --kind open-ai \
  --endpoint https://api.openai.com/v1 --model MODEL \
  --api-key-env OPENAI_API_KEY

okc provider add default --kind open-ai \
  --endpoint https://api.openai.com/v1 --model MODEL \
  --os-keychain default
```

아래 내용은 frozen V2의 선택적 NDJSON augmentation 호환 흐름입니다.

::: warning 아래 V2 Provider는 별도 실행 파일입니다
이 저장소는 특정 LLM provider를 번들하지 않습니다. `augment`를 사용하려면
OKC protocol V2 NDJSON을 구현한 실행 파일이 필요합니다.
:::

## 문서 선택하기

provider에 plan 전체가 자동 공개되지는 않습니다. `plan.json`에서 document
ID를 확인합니다.

```sh
jq -r '.workspace.documents | keys[]' okc-run/plan.json
```

선택한 문서만 보내려면 `--document-id`를 한 번 이상 사용합니다.

```sh
okc augment okc-run/plan.json \
  --provider-cmd ./my-okc-provider \
  --provider-arg model-name \
  --document-id doc_... \
  --out okc-run/augmentation.jsonl
```

모든 문서를 보내려면 의도를 명시해야 합니다.

```sh
okc augment okc-run/plan.json \
  --provider-cmd ./my-okc-provider \
  --all-documents \
  --out okc-run/augmentation.jsonl
```

현재 선택된 문서는 파싱된 모든 block을 전송합니다. block 단위 공개 선택은
아직 구현되지 않았습니다. provider는 shell 없이 직접 실행되며 stdout에는
protocol NDJSON만, 로그는 stderr에 써야 합니다.

## 기록 검증과 Replay

live exchange는 canonical JSONL recording으로 남습니다. 다음 작업은 provider,
네트워크 또는 출력 Vault를 사용하지 않습니다.

```sh
okc validate okc-run/plan.json \
  --augmentation okc-run/augmentation.jsonl

okc replay okc-run/plan.json \
  --augmentation okc-run/augmentation.jsonl \
  --out okc-run/replayed-augmentation.jsonl
```

유효한 replay 결과는 원래 recording과 byte-identical합니다.

## Proposal 승인하기

validation record의 `proposal_id`와 `proposal_content_hash`를 결정 문서의
`decisions` 배열에 넣습니다.

```json
{
  "plan_id": "plan_...",
  "proposal_id": "proposal-1",
  "proposal_content_hash": "content hash",
  "approved": true,
  "approver": "curator-id",
  "policy_version": "team-policy-v2"
}
```

```sh
okc approve okc-run/plan.json \
  --decisions okc-run/decisions.json \
  --proposals okc-run/replayed-augmentation.jsonl \
  --out okc-run/approved-plan.json
```

승인하지 않을 proposal은 결정 자체를 생략하거나 `approved: false`로 기록할
수 있습니다. Provider가 conflict를 설명하는 proposal을 내더라도 직접
target을 선택하거나 자기 proposal을 승인할 수는 없습니다.

## 원격 Provider 동의

원격 공개는 기본 거부됩니다. Plan을 만들 때 정책으로 허용하고 live call에서
일회성 동의를 다시 제공해야 합니다.

```toml
schema_version = 2

[augmentation]
allow_remote_providers = true
```

```sh
okc --policy remote-policy.toml plan \
  personal=/vaults/Personal \
  --out okc-run/remote-plan.json

okc augment okc-run/remote-plan.json \
  --provider-cmd ./my-remote-provider \
  --all-documents \
  --allow-remote-provider \
  --out okc-run/remote-augmentation.jsonl
```

둘 중 하나라도 빠지면 source projection을 provider에 쓰기 전에 실패합니다.
Replay는 새 source text를 공개하지 않으므로 동의 플래그를 받지 않습니다.

전체 wire schema와 hard limits는
[AI Provider 규범 명세](https://github.com/dolgogae/okc/blob/main/docs/specs/ai-provider-and-augmentation.md)를
참고하세요.
