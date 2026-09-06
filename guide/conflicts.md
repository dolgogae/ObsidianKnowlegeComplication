---
title: 검토와 충돌
description: taxonomy, contradiction, omission과 critic finding을 안전하게 검토하는 방법
---

# 충돌을 추측하지 않고 검토하기

현재 OKC는 AI나 다수결이 어떤 주장이 참인지 결정하게 하지 않습니다.
Taxonomy는 문서를 정확히 한 cluster에 배치하고, synthesis는 서로 충돌하는
주장을 각 source/time/context evidence와 함께 보존해야 합니다. 모든 결과는
critic과 curator review를 통과합니다.

## Taxonomy 충돌

먼저 전체 배치를 확인합니다.

```sh
okc --project Team.okc-project review taxonomy show
okc --project Team.okc-project \
  review taxonomy export --out taxonomy.json
```

편집한 배열은 모든 현재 `DocumentId`를 정확히 한 번 포함해야 하고 canonical
path가 안전하고 고유해야 합니다. 누락, 중복, foreign ID, traversal path가
있으면 승인되지 않습니다.

```sh
okc --project Team.okc-project \
  review taxonomy approve \
  --edited-clusters taxonomy.json \
  --rationale "중복 주제를 합치고 팀 용어로 이름을 바꿈"
```

수정은 새 taxonomy hash를 만들며 이전 cluster proposal과 approval을 stale로
만듭니다.

## Synthesis와 contradiction

```sh
okc --project Team.okc-project integrate
okc --project Team.okc-project review cluster list
okc --project Team.okc-project review cluster show CLUSTER_ID
```

Cluster 출력에서 다음을 대조합니다.

- 모든 source block/frontmatter value가 exactly-one disposition인지;
- 각 non-empty section이 current block evidence를 인용하는지;
- `integrated` content가 section 또는 contradiction에 나타나는지;
- `preserved_verbatim` content가 출력에서 유지되는지;
- contradiction의 각 claim이 독립 evidence와 context를 갖는지;
- critic이 전체 source inventory와 현재 proposal을 비교했는지.

근거가 누락되거나 결론이 한 source를 임의로 지우면 feedback으로 새 revision을
만듭니다.

```sh
okc --project Team.okc-project \
  review cluster regenerate CLUSTER_ID \
  --feedback "양쪽 주장의 날짜와 source evidence를 모두 보존"
okc --project Team.okc-project integrate
```

## Omission과 critic finding

`critical` 또는 `major` finding은 승인할 수 없습니다. Proposal을 고치고 새
critic을 받아야 합니다. `minor` finding만 exact finding ID와 검토 이유를
waiver할 수 있습니다.

`omission_proposed` disposition도 자동으로 삭제되지 않습니다. 각 target에
정확한 key와 이유가 있어야 omission이 효력을 얻습니다.

```sh
okc --project Team.okc-project review cluster approve CLUSTER_ID \
  --omission-rationale 'DOCUMENT_ID:TARGET_ID=반복 헤더이며 근거와 대조함' \
  --minor-waiver 'FINDING_ID=표현상 경고를 검토하고 수용함'
```

Unknown, duplicate, missing, stale, 또는 다른 revision의 ID는 fail-closed로
거부됩니다. 자유 형식 replacement text/path를 approval로 주입할 수 없습니다.

## 변경 뒤에는 다시 검토

Source, route, policy, prompt/schema, taxonomy, proposal, critic, recording이
바뀌면 dependent approval은 stale입니다. 기존 JSON이나 rationale을 고쳐
권위를 되살리지 말고, 새 revision의 실제 ID/hash를 확인해 다시 승인하세요.

모든 cluster가 승인된 경우에만 `ApprovedIntegrationPlan`이 봉인되고 offline
compile이 가능해집니다.
