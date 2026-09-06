---
title: 현재 구현 상태
description: OKC 0.3.0 현재 Schema 3 개발 빌드에서 사용할 수 있는 범위
---

# 현재 무엇을 사용할 수 있나요?

이 가이드는 `okc 0.3.0` 개발 트리를 기준으로 합니다. Main에는 현재 Schema 3
구현만 있고, 로컬 macOS arm64 검증은 진행됐지만 공개 안정 릴리스는 아닙니다.

## 지금 가능한 흐름

- current project 생성, directory/archive source 등록, provider profile과 role
  route 구성
- 민감정보 preflight, embedding/candidate/organizer/synthesis/critic task와
  append-only resume
- taxonomy와 cluster CLI/TUI 검토, regeneration, omission/minor waiver, 승인
- 완전한 approved plan에서 provider-free Markdown directory compile
- Schema 3 directory verify와 한 canonical/stub path의 provenance explain
- cwd-first TUI, bounded worker, publication 이전 취소, OS keychain 또는
  환경변수 참조
- Python 3.11+와 Node.js 22.13+의 같은 project/integration/approval/compile
  workflow
- interop schema 2의 typed verification/explanation result, 최대 64개 progress
  event, same-project `PROJECT_BUSY`, structured error와 type declaration
- macOS arm64 wheel/sdist/npm tarball의 로컬 build/install/type smoke와
  cross-language artifact inventory golden

`legacy/` output directory는 current source redirect stub입니다. 이전 artifact를
읽는 compatibility directory가 아닙니다.

## 이전 schema 입력

Schema 1/2 compiler, reader, writer, project upgrade, migration, alias와 committed
fixture는 main에 없습니다. 인식 가능한 이전 marker와 Pack suffix에는
`ARTIFACT_SCHEMA_UNSUPPORTED`, `supported_schema = 3`, detected schema/family를
돌려줍니다. Mixed marker, symlink, malformed/oversized manifest와 unknown/corrupt
artifact는 fail-closed verification error입니다.

이전 source는 ADR-0027의 annotated archive tag에 보존되어 있지만 release나
current package가 아닙니다.

## 아직 제한되는 것

- deterministic block chunk/HNSW/full candidate union과 100k semantic benchmark
- hierarchical synthesis와 manual section amendment
- persisted sensitive exception, command-provider supervisor, broader provider
  conformance
- attachment/Canvas/Base carry-through, complete link rewrite, current OKCPack
- 100,000 notes/20 GB streaming/RSS gate
- complete fuzz/property, hostile TOCTOU, crash injection, PTY E2E
- 네 native target의 두 차례 same-commit remote CI와 package matrix
- macOS/Windows native signing/notarization, protected publication

현재 Markdown directory 흐름은 [Quickstart](./index.md),
[AI 통합](./integration.md), [CLI](./cli.md), [TUI](./tui.md),
[Python · Node.js](./python-node.md)에서 확인할 수 있습니다. Exact verification
evidence와 blocker는 저장소의
[`docs/CURRENT_STATE.md`](https://github.com/dolgogae/okc/blob/main/docs/CURRENT_STATE.md)가
권위 있는 기준입니다.
