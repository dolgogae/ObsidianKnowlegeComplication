---
aliases:
  - Atlas
  - OKC pilot
tags:
  - atlas
  - engineering
status: accepted
---

# Atlas 기술 범위

Atlas는 여러 Obsidian Vault snapshot을 canonical model로 읽고, AI가 제안한
taxonomy와 synthesis를 schema 검증한 뒤 curator 승인으로 봉인하는 로컬 우선
컴파일러다.

## 현재 범위

- Rust 단일 실행 파일과 terminal UI
- Markdown document와 block inventory
- provider profile, recording, 재개 가능한 task journal
- taxonomy와 cluster revision 승인
- provider-free directory compile과 verify

## 제외 범위

현재 개발 slice는 V3 pack, attachment, Canvas, Base materialization을 완료하지
않았다. 검색의 대규모 approximate index도 성능 증거가 없으므로 파일럿 약속에
포함하지 않는다. [[아키텍처/시스템 개요]]는 이 범위를 component 경계로 풀고,
[[프로젝트/마일스톤]]은 검증 순서를 정한다.
