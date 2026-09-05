---
aliases:
  - 실행일 가이드
tags:
  - atlas/field-guide
  - operations/runbook
status: active
---

# Atlas 운영 가이드

## 실행 전

[[고객/온보딩 체크리스트]]의 권한과 source 선택을 확인한다. provider route는
[[거버넌스/데이터 분류]]와 일치해야 하며, output 경로가 source 밖에 있고 아직
존재하지 않는지 확인한다.

## 실행 중

preflight를 읽은 뒤 remote disclosure가 있으면 명시적으로 동의한다. taxonomy의
모든 문서를 검토하고 cluster별 source, synthesis, critic을 대조한다.

## 실행 후

compile 직후와 별도 명령으로 verify한다. 결과 공유 전 [[운영/출시 준비도]]와
[[거버넌스/승인 역할]]을 확인한다. 오류는 [[운영/장애 대응 절차]]로 이동한다.
