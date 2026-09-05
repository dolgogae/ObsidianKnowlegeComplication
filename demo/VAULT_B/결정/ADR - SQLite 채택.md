---
tags:
  - adr
  - storage
status: accepted
date: 2026-07-25
---

# SQLite 채택

## 상황

통합 작업은 여러 단계와 승인으로 이루어지고 provider 실패 뒤 재개가 필요하다.
여러 JSON 파일만으로 current pointer와 append-only 관계를 원자적으로 갱신하기
어렵다.

## 결정

application journal에 SQLite를 사용한다. foreign key를 켜고, source set 변경과
새 run pointer 갱신을 하나의 transaction으로 처리한다. immutable large payload는
DB에 반복 저장하지 않고 object store hash로 참조한다.

## 결과

migration과 crash recovery 테스트가 필요하다. DB 자체는 비밀 저장소가 아니다.

적용 설계: [[아키텍처/저장소 선택]] · 복구: [[운영/백업과 복구]]
