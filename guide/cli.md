---
title: CLI 사용법
description: 현재 Schema 3 프로젝트를 검토하고 컴파일하는 OKC CLI 가이드
---

# CLI로 Vault 통합하기

OKC CLI는 project → provider → integration → review → compile → verify/
explain 흐름을 사용합니다. 원본 Vault는 수정하지 않으며 결과는 source 밖의
새 디렉터리에만 게시합니다. 처음이라면 [Quickstart](./index.md)와
[AI 통합](./integration.md)을 먼저 읽으세요.

## 명령 한눈에 보기

| 명령 | 하는 일 | 중요한 경계 |
|---|---|---|
| `project create` | 새 Schema 3 project 생성 | 기존 경로를 덮어쓰지 않음 |
| `project source add` | directory/archive source 추가 | source ID와 immutable bytes 결합 |
| `project ai-route set` | 기본 또는 role별 profile 지정 | 변경 시 dependent approval stale |
| `provider add/list/show/test/remove` | 전역 provider profile 관리 | secret 값이 아닌 참조만 저장 |
| `integrate` | preflight와 provider task 실행·재개 | remote call은 두 consent flag 필요 |
| `integration status` | run/task 상태 조회 | human 또는 JSON |
| `review taxonomy ...` | taxonomy 조회·export·수정 승인 | 전체 exactly-once coverage reseal |
| `review cluster ...` | synthesis/critic 조회·승인·재생성 | major/critical 승인 불가 |
| `compile` | approved plan을 새 디렉터리에 materialize | provider-free, no-clobber |
| `verify DIRECTORY` | Schema 3 디렉터리 독립 검증 | retired schema는 unsupported |
| `explain DIRECTORY OUTPUT_PATH` | 한 output의 provenance 반환 | 먼저 전체 verify 수행 |
| `tui` | 같은 service의 터미널 UI | no-argument TTY 기본값 |
| `doctor` | 버전, target, TTY, project 진단 | 읽기 전용 |
| `update` | receipt 기반 업데이트 | 확인 필요, source build 거부 |

실행 중인 바이너리의 help가 가장 정확합니다.

```sh
okc --help
okc project --help
okc compile --help
okc explain --help
```

전역 옵션은 `--project PATH` 하나입니다. Policy와 build workspace는 현재
프로젝트/코어 계약이 관리하며 CLI 전역 옵션으로 바꾸지 않습니다.

## 프로젝트와 source

```sh
okc project create /work/Team.okc-project \
  --name Team --curator alice --language ko-KR

okc --project /work/Team.okc-project \
  project source add personal /vaults/Personal --owner Alice
okc --project /work/Team.okc-project \
  project source add team /vaults/Team.zip
```

지원 source는 디렉터리, `.zip`, `.tar.zst`, `.tzst`입니다. ID는 ASCII
영숫자, `-`, `_`, `.`만 사용하며 한 source set에서 고유해야 합니다. 동일한
전체 Vault content를 다른 ID로 중복 등록하면 실패합니다. Source를 바꾸면
새 run identity가 생기고 이전 approval은 stale이 됩니다.

CLI/TUI는 bounded cwd discovery도 지원합니다. 프로젝트가 하나뿐인 작업
폴더에서는 `--project`를 생략할 수 있지만, 비대화형 환경에서 0개 또는 여러
개가 보이면 명시적인 경로를 요구합니다.

## Provider와 integration

```sh
okc provider add local \
  --kind ollama \
  --endpoint http://127.0.0.1:11434 \
  --model MODEL
okc provider test local
okc --project /work/Team.okc-project project ai-route set local

okc --project /work/Team.okc-project integrate
okc --project /work/Team.okc-project integration status --format json
```

Remote cache miss는 source 공개 전에 두 플래그를 요구합니다.

```sh
okc --project /work/Team.okc-project integrate \
  --allow-remote-provider --yes
```

이 동의는 한 호출에만 적용되고 project에 저장되지 않습니다. Sensitive
finding이 있는 semantic/cluster work는 local route를 사용해야 합니다.

## Taxonomy와 cluster review

```sh
okc --project /work/Team.okc-project review taxonomy show
okc --project /work/Team.okc-project review taxonomy approve \
  --rationale "모든 문서 배치를 확인함"

okc --project /work/Team.okc-project integrate
okc --project /work/Team.okc-project review cluster list
okc --project /work/Team.okc-project review cluster show CLUSTER_ID
okc --project /work/Team.okc-project review cluster approve CLUSTER_ID
```

Taxonomy 편집은 전체 cluster 배열을 export하고 다시 import합니다.

```sh
okc --project /work/Team.okc-project \
  review taxonomy export --out taxonomy.json
okc --project /work/Team.okc-project \
  review taxonomy approve --edited-clusters taxonomy.json \
  --rationale "팀 정보 구조에 맞춤"
```

`review cluster show`에 omission 또는 minor finding이 있으면 각각 exact key와
rationale을 지정합니다. Major/critical finding은 waiver할 수 없고 feedback으로
새 revision을 생성해야 합니다.

```sh
okc --project /work/Team.okc-project \
  review cluster regenerate CLUSTER_ID \
  --feedback "근거 누락을 고쳐 다시 작성"

okc --project /work/Team.okc-project review cluster approve CLUSTER_ID \
  --omission-rationale 'DOCUMENT_ID:TARGET_ID=검토한 제외 사유' \
  --minor-waiver 'FINDING_ID=검토한 waiver 사유'
```

## Compile, verify, explain

선택된 프로젝트의 최신 approved plan을 사용합니다.

```sh
okc --project /work/Team.okc-project \
  compile --output /work/CompiledVault --format json
okc verify /work/CompiledVault --format json
okc explain /work/CompiledVault knowledge/topic.md --format json
```

또는 완전한 approved plan object를 명시합니다.

```sh
okc compile \
  --integration-plan /work/approved-integration-plan.json \
  --output /work/CompiledVault
```

Output은 존재하지 않는 디렉터리여야 합니다. Compile은 provider를 호출하지
않고 sibling stage를 검증한 다음 no-replace로 게시합니다. Explain의
`OUTPUT_PATH`는 `/` 구분 안전 상대 경로이며, 한 번에 한 typed provenance
record를 반환합니다. Pack 파일은 입력이나 출력으로 지원하지 않습니다.

## 제공하지 않는 이전 명령 형태

현재 help와 parser에는 inspect, plan, augment, replay, validate, top-level
approve, project upgrade, `--policy`, `--workspace`, positional approved plan,
`--pack`, Pack verify/explain, package query, pagination, cursor가 없습니다.
이전 schema의 marker나 suffix를 현재 명령에 전달해도 읽지 않으며 명시적인
unsupported-schema 오류를 반환합니다.

## 자동화와 종료 코드

Human 문구는 안정 API가 아닙니다. 가능한 곳에서 `--format json`과 language
binding의 structured error를 사용하세요.

| 코드 | 의미 |
|---:|---|
| `0` | 성공 |
| `2` | 인자/옵션 사용 오류 |
| `3` | source, path, project 입력 오류 |
| `4` | missing/stale decision 또는 approval |
| `5` | provider/disclosure 오류 |
| `6` | output/publication 오류 |
| `7` | verification/explanation 또는 retired schema |
| `70` | 내부 invariant 오류 |

## 다음 가이드

- [AI 통합](./integration.md)
- [검토와 충돌](./conflicts.md)
- [AI Provider](./ai-provider.md)
- [문제 해결](./troubleshooting.md)
