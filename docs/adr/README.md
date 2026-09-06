---
title: ADR Proposal Review Set
status: proposed
owners:
  - architect
  - release-maintainer
last_updated: 2026-09-06
decision_refs:
  - ADR-0027
  - ADR-0028
  - ADR-0029
  - ADR-0030
  - ADR-0031
source_refs:
  - HIST-CURRENT-PLAN
---

# ADR 설계안 검토 — 2026-09-06

## 현재 상태

기존 안정화 변경 48개 파일은
`f4c50ad71f61478316d5e68182807ef8612f4d2b`
(`fix: harden schema 3 compilation and local validation`)로 커밋하여
`origin/main`에 푸시했다. 아래 문서는 그 이후 작성한 **검토용 초안**이다.
`proposed`는 승인·구현 완료를 뜻하지 않으며, 기존 명세나 accepted ADR보다
우선하지 않는다. 문서 작성 요청은 설계안 수락, 신규 형식 도입, 성능 기준
완화, 외부 제공자 호출, 릴리스·서명·배포 승인이 아니다.

ADR-0001–0027은 각 문서의 Status와 ADR-0027에 기록된 개정/대체 관계를
따른다. 이 디렉터리에 있다는 이유만으로 ADR-0028–0031을 accepted로
취급해서는 안 된다. 현재 구현과 증거는 [CURRENT_STATE](../CURRENT_STATE.md),
잔여 질문은 [OPEN_QUESTIONS](../history/OPEN_QUESTIONS.md)에 있다.

## 설계안과 권고

| 초안 | 권고 방향 | 승인 전 남겨 둔 핵심 결정 |
|---|---|---|
| [ADR-0028 성능·bounded semantic 실행](0028-bounded-semantic-execution-and-performance.md) | disk-backed corpus/벡터/후보 저장, 고정 chunk/HNSW/union profile, evidence-complete 계층 작업, replay/live 예산 분리 | 수치 profile·엔진/산술 규칙·기준 장비·live token/cost 예산·20분 목표의 replay 해석 |
| [ADR-0029 수동 수정·민감정보 검토](0029-manual-amendment-and-sensitive-finding-review.md) | 기존 section ID/evidence 유지, human-origin revision 추가, critic 재실행과 재승인; scanner annotation은 local-only 유지 | 수정 허용 범위·human-origin 형식·journal migration·원격 우회 없는 annotation 의미 |
| [ADR-0030 Pack·대용량 형식·비 Markdown](0030-current-pack-and-extended-materialization.md) | A: 현재 Schema 3 디렉터리를 그대로 Pack; B: 기존 형식의 streaming/큰 control-file profile 검토; C: typed asset/Canvas/Base/수동 수정 evidence envelope | A/B/C 각각 승인, archive byte profile, 확대 한도와 검증 증거, 새 schema 필요 여부 및 Schema 3 보존/전환 정책 |
| [ADR-0031 취소·복구·파일시스템](0031-cancellation-recovery-and-filesystem-capabilities.md) | 실제 publish 직전 atomic 취소 경계, config intent 복구, handle 기반 경로, 명시적 orphan 처리 | core control API, private journal schema 5 후보, SQLite VFS/native capability와 지원 threat model |

숫자는 평가를 시작하기 위한 제안이며 달성된 성능이나 검증된 품질이 아니다.
특히 현재 측정은 10만 노트/25.6 MB 입력만으로 RSS 5.36 GB였다. QG-006은
통과하지 않았고, 모든 초안이 수락되더라도 구현·시험 없이 통과로 바뀌지 않는다.

## 의존성과 구현 순서

1. ADR-0031의 런타임 취소/복구/경로 경계를 검토한다. artifact bytes를 바꾸지
   않는 부분은 형식 확장과 분리할 수 있다.
2. ADR-0028의 내부 streaming과 작은 fixture의 byte parity를 구현·검증한다.
   기존 `PreparedCorpus` API는 작은 입력용으로 보존한다.
3. ADR-0030-A의 현재 Schema 3 Pack은 C의 비 Markdown 형식 결정과 별개로
   검토할 수 있다. 0030-B의 큰 control-file 한도는 streaming 검증 없이
   먼저 높이지 않는다.
4. ADR-0029의 local-only scanner 검토는 독립적으로 검토하되, 수동 수정의
   artifact export는 0030-C의 human-origin 표현·독립 검증이 필요하다.
5. 비 Markdown/확장 provenance의 정확한 형식과 기존 Schema 3 보존 정책을
   먼저 승인한 다음 해당 writer/reader를 구현한다.
6. native matrix, fuzz/kill, 전체 규모 replay/live, signing·supply-chain
   증거를 쌓는다. 패키지 공개는 별도 보호된 승인 절차를 따른다.

## 승인 체크리스트

- [ ] 승인할 ADR 번호와, ADR-0030의 경우 A/B/C 범위를 명시한다.
- [ ] 20분/2-GB 역사적 목표의 현재 의미와 기준 장비·fixture를 확정한다.
- [ ] semantic profile의 라이브러리/산술/순서 규칙과 golden vector를 확정한다.
- [ ] paid/live 제공자의 모델·호출·token/cost 한도를 별도로 승인한다.
- [ ] 수동 수정 origin과 scanner annotation이 기존 승인/원격 경계를 우회하지
      않는지 보안 검토한다.
- [ ] 더 큰 parser bound나 새 schema가 필요하면 정확한 규범 변경과
      migration/rollback, archive·compatibility 정책을 먼저 기록한다.
- [ ] SQLite/플랫폼 I/O 안전성과 private journal migration을 검토한다.
- [ ] 각 초안의 예정 테스트를 구현하고 증거를 code/test traceability에 연결한다.
- [ ] 승인된 내용을 같은 변경에서 명세·안정 알고리즘·API 계약에 반영한다.

현재 문서는 위 항목을 자동으로 체크하지 않는다. 미승인 초안은 구현 명령이
아니며, 기존 default compiler에 experimental logic을 추가할 근거가 아니다.

## 범위 밖

command-provider는 계속 없는 상태다. 사용자 실행 파일 감독·환경·출력·취소·
disclosure 정책이 별도로 승인되지 않았으므로 이 초안 묶음에 복구하지 않는다.
MCP/Obsidian plugin, registry, 외부 계정 연결, 실제 유료 제공자 실행,
인증서 접근, stable release publication도 이번 문서 작업의 범위가 아니다.

각 ADR에는 영향 requirement/algorithm/QG, 대안과 비용, 단계별 수락 조건,
필요한 명세 변경, 실패·rollback 및 예정 테스트가 포함되어 있다.
