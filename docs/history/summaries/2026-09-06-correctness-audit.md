---
title: Correctness and Refactoring Audit — 2026-09-06
status: historical
owners:
  - core-rust-engineer
  - qa-security-engineer
last_updated: 2026-09-06
decision_refs:
  - ADR-0024
  - ADR-0026
  - ADR-0027
source_refs:
  - HIST-CURRENT-PLAN
---

# 논리·안전성·리팩터링 점검 — 2026-09-06

## 범위와 판정

현재 Schema 3 경로를 대상으로 명세·안정 알고리즘·ADR, Rust 7개 패키지,
CLI/TUI 공유 서비스, Python/Node.js 바인딩을 점검했다. 시작 시 작업 트리는
깨끗했고, 기존 Rust 86개 테스트는 통과했다. 테스트 통과만으로 발견되지 않는
승인 상태, 경로, 입력 경계, 큐 포화 문제를 집중적으로 확인했다.

아래는 수정한 결함이다. 주요 결함은 실패하는 회귀 테스트를 먼저 실행해
재현했으며, 나머지는 코드 검토 후 회귀 테스트 또는 빌드 검증을 추가했다.
전수 증명이나 외부 보안 인증, 안정 릴리스 판정은 아니다.

## 문서 모순 해결

구현을 일시 중단하고 [OPEN_QUESTIONS](../OPEN_QUESTIONS.md)에 네 가지
충돌을 기록했다. 기존 승인된 [ADR-0027](../../adr/0027-current-schema-single-source.md)
및 상위 명세에 따라 정리한 뒤 구현을 재개했다.

| 모순 | 정리한 현재 계약 |
|---|---|
| AI 명세에는 예약된 `command` 종류, 다른 명세에는 제거됨 | 알 수 없는 종류로 디코딩 단계에서 거부; 신규 도입은 별도 결정 필요 |
| ALG-PRV-001이 제거된 provenance graph·가상 루트·cursor를 요구 | Schema 3 출력별 provenance와 단일 출력 경로 설명만 현재 계약 |
| ALG-CNF-001이 과거 공개 결정/출력 권한을 현재처럼 설명 | 내부 분석과 최종 `ApprovedIntegrationPlan` 권한을 분리 |
| ALG-INT-001이 비 Markdown·Pack 출력을 구현된 것처럼 나열 | 현재 Markdown 출력과 미구현 미래 단계를 명시적으로 구분 |

NRM/DED 알고리즘과 Rust 역할 문서도 현재 내부 분석 범위로 정리했다.
SNP의 원본 재검사 설명은 새 corpus 구성에 적용되며, 오프라인 compile이
원본 Vault를 다시 여는 것으로 오해되지 않도록 수정했다. 과거 ADR 본문과
transcript는 고치지 않았다. 새로운 스키마나 약화된 불변식을 도입하지 않았다.

## 수정한 결함

| 항목 | 원인과 영향 | 수정 |
|---|---|---|
| A01 승인 상태 | 언어·라우트·taxonomy 변경 및 재생성 대기 중에도 저장된 계획이 최신으로 반환됨 | downstream 작업이 있는 설정 변경은 새 run을 추가; 최신 승인과 재생성 요청으로 계획/검증 출력의 유효성 재검사 |
| A02 산출물 검증 | 승인되지 않은 파일도 manifest/checksum을 함께 갱신하면 통과 | 허용 목록을 승인 계획에서 재생성; 정확한 전체 파일·manifest·checksum 바이트 비교 |
| A03 Markdown 본문 | 본문에 식별자용 제어문자 검증을 적용해 정상 줄바꿈도 거부 | 본문 검증 분리: LF·tab 허용, 다른 제어문자·빈 본문·16 MiB 초과 거부 |
| A04 출력 경로 | ASCII 소문자 비교만으로 Unicode·NFC·파일/디렉터리·공유 prefix 충돌을 놓침 | 기존 portable component 규칙과 pinned full case folding/NFC 재사용; 원래 source 철자 보존 |
| A05 데이터 모호성 | SourceId 역직렬화가 생성자 검증 우회; 같은 metadata slot에 다른 값 허용 | 동일 ID 검증과 unknown field 거부; `(key, value_index)` 유일성 보장 |
| A06 JSON 중복 키 | JSON map 변환이 중첩 중복 키를 마지막 값으로 덮어씀 | 공통 strict decoder로 Canvas, provider envelope/생성 JSON, embedded plan 검증 통일 |
| A07 민감정보 | PEM 시작 마커를 놓치고 email 시작 offset이 부정확; metadata-only 문서가 스캔에서 빠짐 | 마커·UTF-8 범위 수정, frontmatter key/value 스캔 및 영향 문서별 로컬 라우팅 |
| A08 로컬 경계 | `127.`로 시작하는 DNS 이름을 loopback으로 오인 | 실제 IP 파싱과 정확한 localhost 이름으로 분류 |
| A09 캐시 | endpoint/kind/limits 및 organizer candidates가 키에 미반영; task hash 정렬을 최신 순서로 오인 | 의미 입력을 키에 포함; stage/request 불일치 거부; journal event 순서 사용 |
| A10 워커 종료 | 64개 progress 큐가 차면 Finished 전송과 Drop/join이 상호 대기 | 완료 결과를 별도 슬롯에 보존; 새 작업 전 이전 완료 소비 요구 |
| A11 SDK 스케줄러 | 대기 큐가 무제한이고 서로 다른 client끼리 같은 프로젝트를 예약 가능 | 대기 64개 제한·retryable ResourceLimit; 프로세스 공유 예약과 완료 통지 전 해제 |
| A12 Node.js 데이터 | 재귀 camelCase 변환이 `release_date`와 `releaseDate`를 합쳐 값 유실 | 계약 필드만 변환; 임의 options/metadata value map은 원형 보존 |
| A13 원본/작업 DB | source 안의 workspace 경로를 허용해 Vault에 SQLite 파일 생성 | 저장 전 실제 기존 ancestor를 해석해 source 중첩 거부; DB/sidecar 링크 거부 |
| A14 프로젝트 링크 | manifest·DB·objects·workspace 링크를 따라 외부 파일을 읽거나 쓸 수 있음 | project open 이전 구조 검증; DB sidecar 및 object 접근 시 타입/링크 재검사 |
| A15 전체 timeout | 매 retry에 원래 timeout을 다시 주고 Retry-After로 한도 초과 가능 | 재시도와 대기를 포함하는 총 deadline 및 잔여 예산 적용 |
| A16 빌드/상대 경로 | SQLite 기능을 끄면 라이브러리 빌드 오류; 단일 이름 출력의 빈 parent 처리 오류 | optional module/import/helper feature gate 정리; Unix 출판 parent를 `.`로 처리 |

컴파일 서비스는 계획 선택부터 검증 결과 기록까지 writer lock을 유지한다.
거부된 profile 이름이 메모리의 라우트 설정에 남는 문제도 수정해 후보 값을
검증한 뒤에만 기존 설정을 바꾸도록 했다.
공개 application compile에서도 source/output 중첩을 검사한다. 중복되던
preflight 기록, JSON decoder, checksum 생성, manifest 한도를 공통화했다.
SDK progress 상태 전이도 같은 잠금 안에서 기록하며 완료 뒤 상태를 되돌리지 않는다.

## 호환성과 데이터 보호

- 기존 프로젝트 Schema 3, journal schema 4, interop schema 2를 유지했다.
- 과거 run·승인·objects를 삭제하거나 다시 쓰지 않는다.
- 민감정보 scanner는 버전이 있는 내부 입력이므로 `okc-sensitive-v3-2` 및
  `sensitive-findings-v2`로 새 preflight를 기록한다. 이전 검사 결과를 조용히
  재해석하지 않는다. metadata 범위는 compact JSON `{key, value}`의 UTF-8
  offset이며 findings 자체에는 매칭한 비밀 문자열을 넣지 않는다.
- 기존 저장 파일을 일괄 변환하지 않는다. 이제 거부하는 입력은 모호한 키,
  위험한 경로, 오래된 승인, 비계획 파일 등 현재 명세상 유효하지 않은 입력이다.
- `verify`의 계획 read 한도는 512 MiB, manifest는 1 MiB다. 다른 파일은
  계획에서 재생성한 정확한 길이로 제한한다. 대규모 streaming 구현은 아니다.
- Rust/Python/Node.js의 기존 fixture 전체 산출물 inventory SHA-256은 유지한다:

```text
452ca0671e806a93b4f36f218cf9e62da899f6404c74705c2cf0ca14e413c7e5
```

## 검증 기록

현재 macOS arm64 / Rust 1.97.1에서 실행했다. 필수 Rust 명령은 toolchain을
찾기 위해 `export PATH="/opt/homebrew/opt/rustup/bin:$PATH"` 후 실행했다.

| 명령 (Python 도구/입력 경로는 아래에 상세 기록) | 결과 |
|---|---|
| `cargo check --locked --workspace --all-targets --all-features` | 통과 |
| `cargo test --locked --workspace --all-features --no-fail-fast` | 110개 통과; 모든 doc-test target 통과 |
| `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` | 통과 |
| `cargo clippy --locked -p okc-core --no-default-features --lib -- -D warnings` | 통과 |
| `cargo clippy --locked -p okc-core --no-default-features --features archives --lib -- -D warnings` | 통과 |
| `cargo clippy --locked -p okc-core --no-default-features --features sqlite --lib -- -D warnings` | 통과 |
| `npm run build --prefix bindings/node` | 새 release native addon 빌드 통과; 프로세스 조회용 sandbox 권한 승인 후 재실행 |
| `npm test --prefix bindings/node` | 13개 통과; loopback 전체 승인/출력/golden 포함 |
| `npm run typecheck --prefix bindings/node` | strict TypeScript 통과 |
| `npm pack --dry-run` (`bindings/node`에서 실행) | 루트 패키지 의도된 6개 파일만 포함 |
| `maturin build --manifest-path bindings/python/Cargo.toml --release --locked --out /private/tmp/okc-audit-sdk.yBo2Mr/wheels` | 새 CPython 3.11+ abi3 macOS arm64 wheel 빌드 통과 |
| `python -m pytest bindings/python/tests -q` | 새 wheel을 사용해 12개 통과; loopback 전체 승인/출력/golden 포함 |
| `python -m mypy --strict --cache-dir /private/tmp/okc-audit-sdk.yBo2Mr/mypy-cache bindings/python/tests/typing_contract.py` | 1개 typing contract 통과; 새 wheel의 stubs 사용 |

Python에서는 기존 임시 도구의 maturin 1.15.0, pytest 8.4.2, mypy 및
CPython 3.13.15를 사용했다. 새 wheel만 별도의 `python-install` 디렉터리에
풀고 `PYTHONPATH` 선두에 두어 저장소 밖에서 테스트했다. 최종 재빌드한 wheel은
별도의 `python-final` 디렉터리에서 다시 검사했다. pip는 OS 버전
조회 오류에 이어 `pip._internal.operations.install.wheel` import 오류가 발생해
이번 clean pip-install smoke는 통과로 집계하지 않는다. API 테스트의 로컬
서버 bind 제한은 별도 권한 승인 후 재실행하여 통과했다.
최종 Node.js loopback 테스트에도 같은 권한 승인이 필요했으며 재실행은
13개 모두 통과했다. 실제 원격 제공자는 호출하지 않았다.

최종 Python 검증의 정확한 실행 환경과 명령은 다음과 같다. 테스트와 mypy의
작업 디렉터리는 `/private/tmp/okc-audit-sdk.yBo2Mr`이다.

```sh
PYO3_PYTHON=/private/tmp/okc-python-venv.RWFDEI/bin/python /private/tmp/okc-sdk-smoke.TDLo5i/python-tools/bin/maturin build --manifest-path bindings/python/Cargo.toml --release --locked --out /private/tmp/okc-audit-sdk.yBo2Mr/wheels
unzip -q /private/tmp/okc-audit-sdk.yBo2Mr/wheels/okc_compiler-0.3.0-cp311-abi3-macosx_11_0_arm64.whl -d /private/tmp/okc-audit-sdk.yBo2Mr/python-final
PYTHONPATH=/private/tmp/okc-audit-sdk.yBo2Mr/python-final:/private/tmp/okc-sdk-smoke.TDLo5i/python-tools /private/tmp/okc-python-venv.RWFDEI/bin/python -m pytest /Users/sihun/workspace/projects/ObsidianKnowlegeComplication/bindings/python/tests -q
PYTHONPATH=/private/tmp/okc-audit-sdk.yBo2Mr/python-final:/private/tmp/okc-sdk-final.fMdL93/python-tools:/private/tmp/okc-sdk-smoke.TDLo5i/python-tools /private/tmp/okc-python-venv.RWFDEI/bin/python -m mypy --strict --cache-dir /private/tmp/okc-audit-sdk.yBo2Mr/mypy-final-cache /Users/sihun/workspace/projects/ObsidianKnowlegeComplication/bindings/python/tests/typing_contract.py
```

최종 Python API, formatting, 문서 링크/가이드 및 tree 명령 결과는
[CURRENT_STATE](../../CURRENT_STATE.md)의 이번 audit evidence에 기록한다.
요구사항별 실제 회귀 테스트 이름은 [TRACEABILITY](../../TRACEABILITY.md)에 있다.

## 남은 범위

다음은 이번 수정으로 해결했다고 주장하지 않는다:

- 모든 OS·filesystem에서 descriptor-relative no-follow, hardlink/ancestor
  교체 경쟁, Windows reparse point, 크래시 시 manifest/SQLite 원자성;
- provider-backed TUI PTY, 장시간 cancellation, fuzz/property 전수 실행;
- 실제 원격 제공자 conformance와 10 Vault/100,000 notes/20 GB 성능;
- 비 Markdown materialization, 완전한 link rewrite, 현재 Pack, 수동 section
  수정, 영속 민감정보 예외 승인;
- 원격 4-platform native package matrix, 새 sdist/clean install, 재현 빌드,
  서명·공증·배포 게이트.

원본 Vault나 기존 사용자 산출물을 수정하지 않았으며 커밋·push·게시하지 않았다.
상세 릴리스 차단 사항은 [CURRENT_STATE](../../CURRENT_STATE.md)가 권위다.
