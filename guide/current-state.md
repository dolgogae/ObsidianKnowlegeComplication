---
title: 현재 구현 상태
description: OKC 0.3.0 V3 개발 빌드에서 지금 사용할 수 있는 범위
---

# 현재 무엇을 사용할 수 있나요?

이 가이드는 `okc 0.3.0` 개발 트리를 기준으로 합니다. 로컬 macOS arm64
테스트는 통과했지만 공개 안정 릴리스는 아닙니다.

## 지금 가능한 V3 개발 흐름

- schema-3 project 생성/V2 source-binding upgrade, provider profile과 AI route
- sensitive preflight, embedding/candidate/organizer/synthesis/critic task resume
- taxonomy 및 cluster CLI 검토/승인, provider-free V3 directory compile
- V3 directory verify와 canonical/stub provenance explain
- `cd 원하는-폴더 && okc` TUI에서 provider/Vault/preflight/taxonomy/cluster/
  compile/verify 실행, worker 취소와 재개
- OS keychain 또는 환경변수 참조 기반 provider credential
- V3 Markdown block/frontmatter disposition, evidence, contradiction, critic,
  omission/waiver, approval, recording closure 검증

## frozen V2 회귀 경로에서 가능한 것

- inspect, plan, augment, validate, replay, approve, compile, verify, explain
- 디렉터리, ZIP, `tar.zst`, `.tzst` source와 Markdown, Canvas, attachment,
  opaque Base 처리
- exact duplicate, near-duplicate candidate, Markdown/Canvas link conflict
- deterministic Compiled Vault와 `.okcpack`, typed provenance 검증·설명

이 writer 명령은 현재 회귀 테스트를 위해 개발 바이너리에 남아 있습니다.
ADR-0022가 정한 안정 V3 공개 경계에서는 V1/V2가 `verify`와 `explain`만
제공해야 하므로, writer surface 제거 자체도 release blocker입니다.

## 공통 UI 기반

- TUI의 10개 V3 화면, cwd project/Vault 탐색과 새 project 생성, taxonomy
  merge/split/document 이동, cluster 3-pane 검토와 feedback 재생성
- bounded worker, publication barrier 이전 취소, 키보드 탐색, 한국어 라벨,
  ASCII·고대비 모드와 terminal restoration

## 아직 제한되는 것

- 100,000 notes/20 GB reference performance gate
- complete fuzz/property 및 PTY end-to-end coverage
- 네 개 지원 target의 두 차례 동일-commit remote CI 증거
- macOS/Windows native signing과 notarization
- V1 project reconstruction
- V3 deterministic block chunk/HNSW/candidate union과 100k semantic benchmark
- manual section amendment, sensitive exception UI
- V3 attachment/Canvas/Base/link rewrite와 deterministic OKCPack
- schema-3 command provider supervisor
- 개발 CLI의 V2 writer surface 제거와 공개 legacy read-only 경계 완성

현재 Markdown-only V3 directory 흐름은 [TUI](./tui.md) 또는
[CLI](./cli.md)에서 사용할 수 있습니다. 전체 규범 상태는 저장소의
[`docs/CURRENT_STATE.md`](https://github.com/dolgogae/okc/blob/main/docs/CURRENT_STATE.md)가
권위 있는 기준입니다.
