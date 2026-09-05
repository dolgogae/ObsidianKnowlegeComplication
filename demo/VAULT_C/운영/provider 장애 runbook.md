---
aliases:
  - AI 연결 장애 대응
tags:
  - atlas/operations
  - reliability/provider
status: active
---

# Provider 장애 runbook

## 연결 테스트 실패

endpoint와 명시적 model ID, credential reference 존재 여부를 확인한다. 실제
credential 값을 로그나 project note에 복사하지 않는다.

## 작업 중 timeout

partial response를 proposal로 승인하지 않는다. journal의 failed task와 마지막
complete task를 확인하고 같은 source identity에서 재개한다.

## 응답 schema 오류

원문을 수동으로 고쳐 정상 response처럼 저장하지 않는다. provider 설정 또는
prompt compatibility를 수정하면 새 task identity로 다시 실행한다.

상위 절차: [[운영/장애 대응 절차]] · 변경 승인: [[운영/변경 관리]]
