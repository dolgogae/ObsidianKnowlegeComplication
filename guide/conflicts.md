---
title: 충돌 검토
description: plan 종료 코드 4와 LINK_AMBIGUITY 결정을 처리하는 방법
---

# 충돌을 검토하고 결정하기

OKC는 모호한 링크를 발견 순서대로 임의 선택하지 않습니다. 선택 가능한
대상들을 `plan.json`에 밀봉하고, 사용자가 그 후보 중 하나를 선택하거나
원본 표현을 유지하도록 요구합니다.

## 종료 코드 4는 무엇인가요?

다음 메시지는 plan 생성 실패가 아닙니다.

```text
okc: wrote `okc-run/plan.json` with 1 required conflict(s) awaiting a decision
```

`plan.json`은 만들어졌고 검토 단계에 진입했습니다. `jq`를 사용하면 미해결
필수 충돌만 볼 수 있습니다.

```sh
jq '[.conflicts[] | select(.required and .resolution == "unresolved")]' \
  okc-run/plan.json
```

각 conflict에서 다음 값을 확인합니다.

- 최상위 `plan_id`
- conflict의 `conflict_id`
- conflict의 `content_hash`
- `subject`와 밀봉된 후보 IDs

## 결정 문서의 바깥 구조

```json
{
  "schema_version": 2,
  "plan_id": "plan_...",
  "decisions": [],
  "conflicts": []
}
```

AI proposal decision은 `decisions`, link ambiguity decision은 `conflicts`에
들어갑니다. 모든 값은 현재 plan에서 복사해야 합니다.

## 원본 링크 보존

의도적으로 원본의 모호한 링크를 남기려면 다음 객체를 `conflicts` 배열에
넣습니다.

```json
{
  "plan_id": "plan_...",
  "conflict_id": "conflict_...",
  "conflict_content_hash": "plan의 content_hash",
  "action": { "type": "waive_preserve_original" },
  "decided_by": "curator-id",
  "policy_version": "team-policy-v2",
  "rationale": "두 대상 모두 유지하고 원본 표현을 보존"
}
```

## Markdown 대상 선택

`target_document_id`는 해당 conflict의 `documents` 후보 중 정확히 하나여야
합니다.

```json
{
  "type": "select_markdown_target",
  "target_document_id": "doc_..."
}
```

OKC는 target만 바꾸고 embed 여부, display text, heading, block suffix는
보존합니다. 사용자가 임의의 replacement text나 경로를 넣을 수는 없습니다.

## Canvas 대상 선택

Canvas reference의 밀봉된 후보를 그대로 사용합니다.

```json
{
  "type": "select_canvas_target",
  "target": { "kind": "document", "id": "doc_..." }
}
```

실제 후보에 따라 `kind`는 `document`, `asset`, `canvas`, `base`가 될 수
있습니다. Canvas의 알려지지 않은 JSON 필드는 target 선택 후에도 유지됩니다.

## 승인하기

모든 필수 conflict에 한 번씩 결정한 후 실행합니다.

```sh
okc approve okc-run/plan.json \
  --decisions okc-run/decisions.json \
  --out okc-run/approved-plan.json
```

다음 경우 fail-closed로 거부됩니다.

- 다른 plan의 ID나 content hash 사용
- 같은 conflict에 중복 결정
- 후보 집합 밖의 target 선택
- 필요한 conflict decision 누락
- 자유 형식 replacement 경로 사용

Source나 정책이 바뀌었다면 예전 결정을 고쳐 재사용하지 말고 새 plan과 새
결정을 만드세요.
