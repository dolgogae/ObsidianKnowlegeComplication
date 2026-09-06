---
title: Stabilization Follow-up — 2026-09-06
status: historical
owners:
  - core-rust-engineer
  - qa-security-engineer
  - release-maintainer
last_updated: 2026-09-06
decision_refs:
  - ADR-0014
  - ADR-0024
  - ADR-0026
  - ADR-0027
source_refs:
  - HIST-CURRENT-PLAN
---

# 잔여 안정화 작업 — 2026-09-06

## 결과와 경계

[직전 점검](2026-09-06-correctness-audit.md)의 변경을 보존한 상태에서 추가
결함을 수정하고 Rust 회귀 테스트 20개, POSIX TUI 테스트, 합성 corpus 측정
도구를 추가했다. Rust 130개 테스트가 macOS arm64와 에뮬레이션 Linux에서
통과했다. Python wheel/sdist와 Node 패키지를 저장소 밖에 새로 설치해
검증했다. 기존 Schema 3, ID/hash domain, SQLite schema 4, interop schema 2,
의존성과 출력 golden을 유지했다. 과거 보고서의 pip 실패 기록은 수정하지 않았다.

전체 제품·로드맵이나 QG-001–008이 완료된 것은 아니다. 10만 노트의 입력
단계만으로 최대 RSS 5.36 GB가 관측됐다. 20 GB 전체 semantic workload,
장시간 취소/프로세스 중단, 전 플랫폼 안전성, 새 출력 형식 및 서명·배포는
미완료다. [현재 상태](../../CURRENT_STATE.md)와
[설계 질문](../OPEN_QUESTIONS.md)을 함께 읽어야 한다.

## 추가 수정

| 항목 | 수정과 직접 검증 | 남은 한계 |
|---|---|---|
| F01 관리 파일 hardlink | Unix manifest/object/SQLite DB·sidecar의 link count가 1인지 확인; 외부 파일의 bytes/mode/mtime 보존을 회귀 테스트 | 검사 이후의 경합과 Windows 대응은 별도 |
| F02 source 핸들 | Linux/macOS에서 root 핸들을 고정하고 각 content 경로를 descriptor-relative/no-follow로 열기; archive leaf의 원래 inode와 열린 핸들 비교 | 디렉터리 열거는 아직 경로 기반; mutable state/output ancestor 고정은 미완료 |
| F03 ignore 결정성 | parent/global/source `.ignore`가 corpus membership을 바꾸지 않도록 ambient ignore 비활성화; sealed policy만 적용 | 기존 approved plan bytes는 수정하지 않음 |
| F04 실패 순서 | journal invalidation 성공 후 manifest 교체, 성공 후에만 메모리 갱신; DB abort trigger/manifest 디렉터리 교체로 실패 주입 | 보수적 무효화이지 두 저장소의 atomic commit/recovery는 아님 |
| F05 게시 수명 | stage 내부 디렉터리 bottom-up sync, 실제 atomic no-replace까지 guard 유지, 성공 직후 guard 해제 | 전체 crash/파일시스템/ancestor 경합 행렬은 미완료 |
| F06 실패 정리 | precommit 실패 시 명시적 cleanup; 실패하면 stage 경로와 원인 둘 다 반환; postcommit 실패 시 검증된 출력 보존 | interop는 기존 `PATH_UNSAFE`/`output` 코드와 상세 필드 사용 |
| F07 메모리 중복 | public corpus projection 전에 사용이 끝난 private inspection 해제 | 전체 입력 스트리밍·semantic 메모리 문제를 해결한 것은 아님 |
| F08 wheel 재현성 | `SOURCE_DATE_EPOCH`으로 SBOM 시각 고정 및 임의 UUID 제거; SBOM을 유지한 반복 wheel 바이트 비교를 CI에 추가 | cached native output 반복이며 독립 clean-build 증거는 아님 |

Hardlink, manifest 실패 순서, ambient ignore는 먼저 실패하는 테스트로
재현했다. Publisher 테스트는 실제 publish 직전 barrier에서 외부
file/directory/live symlink/dangling symlink가 먼저 생성되는 경우와 서로 다른
승인 계획 2개의 동시 게시를 실행한다. 정확히 하나의 섞이지 않은 출력만
검증을 통과한다. fault checkpoint는 core 내부 테스트용이며 public API나
환경변수로 노출하지 않았다.

`adversarial_contract.rs`는 JSON 중복/escaped key, 고정 byte 변이·truncation,
ZIP header 변이, source 생성/입력 순서·절대 root 변경을 반복 검사한다.
이는 재현 가능한 mutation/property smoke이며 coverage-guided fuzz 캠페인이
아니다. 개별 requirement와 테스트 이름은 [TRACEABILITY](../../TRACEABILITY.md)에 있다.

## Rust·문서 명령

호스트: macOS 26.2 arm64, Rust 1.97.1, RAM 32 GiB. 아래 명령을 저장소
root에서 실행했다. 네트워크/loopback 바인딩 및 호스트 측정 권한이 필요한
테스트는 sandbox 실패와 구분해 승인된 실행으로 재검증했다.

```sh
export PATH="/opt/homebrew/opt/rustup/bin:$PATH"
cargo check --locked --workspace --all-targets --all-features
cargo test --locked --workspace --all-features --no-fail-fast
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
cargo clippy --locked -p okc-core --no-default-features --lib -- -D warnings
cargo clippy --locked -p okc-core --no-default-features --features archives --lib -- -D warnings
cargo clippy --locked -p okc-core --no-default-features --features sqlite --lib -- -D warnings
cargo test --locked -p okc-core --test documentation_contract
npm run docs:build --prefix guide
git diff --check
```

모두 통과. Rust 테스트는 130개이며 모든 doc-test target도 통과했다.
`cargo audit`는 로컬에 설치되어 있지 않아 새 취약점 감사 성공을 주장하지 않는다.

### Linux 실행

Docker daemon은 Linux arm64이며 아래 image는 x86_64 GNU로 에뮬레이션했다.
image ID는 `sha256:2775a09d208ff0d7c1f50490c45b62db929e87ba1dcbc3f2132ac71a704bcdd3`.
host toolchain은 `1.97.1-x86_64-unknown-linux-gnu`이다. 저장소와 registry는
읽기 전용, build output은 작업 전용 임시 디렉터리, network는 차단했다.

```sh
docker run --rm --platform linux/amd64 --network none \
  --mount type=bind,source=/Users/sihun/workspace/projects/ObsidianKnowlegeComplication,target=/repo,readonly \
  --mount type=bind,source=/Users/sihun/.cargo/registry,target=/usr/local/cargo/registry,readonly \
  --mount type=bind,source=/private/tmp/okc-remaining.G9h3fx/linux-target,target=/target \
  --env CARGO_TARGET_DIR=/target --env CARGO_BUILD_JOBS=4 \
  --env RUSTUP_TOOLCHAIN=1.97.1-x86_64-unknown-linux-gnu \
  --env PYO3_NO_PYTHON=1 --workdir /repo rust:1.97.1-slim-bookworm \
  cargo test --offline --locked --workspace --all-features --no-fail-fast --quiet
```

130개와 모든 doc-test target 통과. 실제 Linux syscall 경로 검증이지만
native remote runner, Linux Python/Node 패키지 행렬의 증거는 아니다.

## Python·Node 설치 및 패키지

Python 3.13.15의 pip 실패는 Homebrew `pyexpat`가 시스템 Expat에서
`_XML_SetAllocTrackerActivationThreshold`를 찾지 못한 loader 불일치였다.
명령별 `DYLD_LIBRARY_PATH=/opt/homebrew/opt/expat/lib`로 Expat 2.8.3을
선택했다. 시스템 파일 수정이나 가짜 OS 버전 주입을 사용하지 않았다.

작업 임시 root는 `/private/tmp/okc-remaining.G9h3fx`이다.
Maturin 1.15.0/pytest 8.4.2는
`/private/tmp/okc-sdk-smoke.TDLo5i/python-tools`, mypy 1.18.2는
`/private/tmp/okc-sdk-final.fMdL93/python-tools`에 있다.
아래 변수들은 이 검증 경로를 짧게 표시한 것이다.

```sh
export PATH="/opt/homebrew/opt/rustup/bin:/private/tmp/okc-sdk-smoke.TDLo5i/python-tools/bin:$PATH"
export DYLD_LIBRARY_PATH=/opt/homebrew/opt/expat/lib
okc_probe_root=/private/tmp/okc-remaining.G9h3fx
okc_test_tools=/private/tmp/okc-sdk-smoke.TDLo5i/python-tools
okc_repo=/Users/sihun/workspace/projects/ObsidianKnowlegeComplication

PYO3_PYTHON="$okc_probe_root/python-final-wheel/bin/python" maturin build --manifest-path bindings/python/Cargo.toml --release --locked --out "$okc_probe_root/final-dist"
maturin sdist --manifest-path bindings/python/Cargo.toml --out "$okc_probe_root/final-dist"
/opt/homebrew/bin/python3.13 -m venv "$okc_probe_root/python-release-wheel"
/opt/homebrew/bin/python3.13 -m venv "$okc_probe_root/python-release-sdist"
"$okc_probe_root/python-release-wheel/bin/python" -m pip install --no-index --no-deps --disable-pip-version-check "$okc_probe_root/final-dist/okc_compiler-0.3.0-cp311-abi3-macosx_11_0_arm64.whl"
PYTHONPATH="$okc_test_tools" PYO3_PYTHON="$okc_probe_root/python-release-sdist/bin/python" CARGO_NET_OFFLINE=true \
  "$okc_probe_root/python-release-sdist/bin/python" -m pip install --no-index --no-build-isolation --no-deps --disable-pip-version-check "$okc_probe_root/final-dist/okc_compiler-0.3.0.tar.gz"
```

저장소 밖 임시 root에서 각 venv에 대해 다음 검증을 실행했다.
sdist는 압축 해제된 source에서 native extension을 다시 빌드해 설치했다.

```sh
PYTHONPATH="$okc_test_tools" "$okc_probe_root/python-release-wheel/bin/python" -m pytest --import-mode=importlib "$okc_repo/bindings/python/tests" -q
PYTHONPATH="$okc_test_tools" "$okc_probe_root/python-release-sdist/bin/python" -m pytest --import-mode=importlib "$okc_repo/bindings/python/tests" -q
PYTHONPATH=/private/tmp/okc-sdk-final.fMdL93/python-tools "$okc_probe_root/python-release-wheel/bin/python" -m mypy --strict "$okc_repo/bindings/python/tests/typing_contract.py"
"$okc_probe_root/python-release-wheel/bin/python" -I -c 'import okc; print(okc.__file__); assert okc.INTEROP_SCHEMA_VERSION == 2; print(okc.OkcClient().api_info())'
"$okc_probe_root/python-release-sdist/bin/python" -I -c 'import okc; print(okc.__file__); assert okc.INTEROP_SCHEMA_VERSION == 2; print(okc.OkcClient().api_info())'
```

각 설치본 12개 테스트, strict mypy, 격리 import 모두 통과. `okc.__file__`이
각 venv의 site-packages임을 확인했다. 합성 서버가 필요한 두 테스트는
sandbox의 `EPERM` 실패 후 loopback 권한을 받아 통과했다.

Node v24.13.1/npm 11.8.0에서 실행:

```sh
npm run build --prefix bindings/node
npm test --prefix bindings/node
npm run typecheck --prefix bindings/node
cp bindings/node/okc-compiler.darwin-arm64.node bindings/node/npm/darwin-arm64/
npm pack ./bindings/node --pack-destination /private/tmp/okc-remaining.G9h3fx/final-dist
npm pack ./bindings/node/npm/darwin-arm64 --pack-destination /private/tmp/okc-remaining.G9h3fx/final-dist
npm install --prefix /private/tmp/okc-remaining.G9h3fx/node-release-install --ignore-scripts --package-lock=false --offline --no-audit --no-fund /private/tmp/okc-remaining.G9h3fx/final-dist/okc-compiler-0.3.0.tgz /private/tmp/okc-remaining.G9h3fx/final-dist/okc-compiler-darwin-arm64-0.3.0.tgz
```

13개 테스트와 strict TypeScript 통과. clean install의 CJS/ESM import,
API info/schema, create/open/manifest가 통과했으며 `require.resolve`로
저장소 fallback이 아님을 확인했다. root 6개 파일, platform 3개 파일이다.
위 `cp`는 ignored native build output 배치이며 관리 source 파일을 덮어쓰지 않는다.

### SBOM, checksums, 반복 패키징

`npm sbom --prefix bindings/node --omit=dev --sbom-format cyclonedx` 결과를
`final-dist/okc-node.cyclonedx.json`에 저장했다. 기존 helper 실행:

```sh
DYLD_LIBRARY_PATH=/opt/homebrew/opt/expat/lib /opt/homebrew/bin/python3.13 bindings/build_artifact_manifest.py /private/tmp/okc-remaining.G9h3fx/final-dist
```

wheel의 embedded CycloneDX, sdist의 Cargo.lock, Node SBOM을 검증했다.
`final-dist`에서 `shasum -a 256 -c SHA256SUMS`를 실행해 5개 artifact 모두
통과했다. 다른 cwd에서 시작한 첫 checksum 시도는 경로를 찾지 못해 실패했으며,
이는 재검증 성공과 별개로 남긴 실행 오류다.

통제하지 않은 wheel 2회는 다르지만 sdist 2회는 같았다. wheel native
payload는 같은 SHA-256
`4958c272f0a7d74f4be743d42da189ae0e464ccc9e36a991873bd68471c4aede`였고
embedded SBOM의 UUID/생성 시각과 그에 의존하는 package record가 달랐다.
다음 명령을 `epoch-dist`, `epoch-repeat-dist` 각각에 실행하고 `cmp`로
wheel 전체가 같음을 확인했다.

```sh
SOURCE_DATE_EPOCH=1788686513 DYLD_LIBRARY_PATH=/opt/homebrew/opt/expat/lib \
  PYO3_PYTHON=/private/tmp/okc-remaining.G9h3fx/python-release-wheel/bin/python \
  /private/tmp/okc-sdk-smoke.TDLo5i/python-tools/bin/maturin build \
  --manifest-path bindings/python/Cargo.toml --release --locked \
  --out /private/tmp/okc-remaining.G9h3fx/epoch-dist
cmp /private/tmp/okc-remaining.G9h3fx/epoch-dist/okc_compiler-0.3.0-cp311-abi3-macosx_11_0_arm64.whl /private/tmp/okc-remaining.G9h3fx/epoch-repeat-dist/okc_compiler-0.3.0-cp311-abi3-macosx_11_0_arm64.whl
```

epoch는 당시 HEAD의 커밋 시각이다. 작업 트리가 커밋된 릴리스라는 뜻이
아니다. native build cache는 공유했다. 일치한 wheel SHA-256은
`55713d1b3617082420dc584c23b96036570f005e728a45de8d7df4c6b0ed4abb`이다.
이 wheel도 별도 `python-repro-wheel`
venv에 `pip --no-index --no-deps` 설치해 동일한 Python 12개 테스트를 통과했다.
SDK CI에는 커밋 시각 설정과 반복 비교를 추가했지만 remote 실행은 하지 않았다.

## TUI PTY

```sh
DYLD_LIBRARY_PATH=/opt/homebrew/opt/expat/lib /opt/homebrew/bin/python3.13 tests/tui_pty_smoke.py target/debug/okc
```

임시 프로젝트와 임시 provider config만 사용했다. preflight는 호출 0회,
embedding/organizer/synthesis/critic는 각 1회, 명시적 taxonomy/cluster 승인,
compile, 독립 verify, 종료 후 Verified 상태 재시작까지 통과했다.
compile/verify/restart에서 추가 provider 호출이 없고 source SHA/mtime 및
terminal mode가 유지됨을 검증했다. 결과:

```json
{"pty_workflow":"passed","provider":"synthetic-loopback","provider_calls":4,"source_unchanged":true,"terminal_restored":true,"verified_restart":true}
```

ANSI를 단순 제거하면 differential redraw를 잘못 읽고, 자식 종료를 기다릴 때
PTY를 읽지 않으면 harness 자체가 writer를 막았다. 작은 화면 상태 모델과
종료 중 drain으로 테스트 도구를 수정했다. 이는 실제 vendor 응답, 장시간
취소/kill, Windows ConPTY 검증을 대신하지 않는다. POSIX CI step을 추가했다.

## 입력 성능 측정

```sh
export PATH="/opt/homebrew/opt/rustup/bin:$PATH"
cargo build --locked --release -p okc-core --example corpus_probe
/usr/bin/time -l target/release/examples/corpus_probe 10 10000 256
/usr/bin/time -l target/release/examples/corpus_probe 10 100000 256
```

| 측정 | 10,000 notes | 100,000 notes |
|---|---:|---:|
| source Vault 수 | 10 | 10 |
| accepted bytes | 2,560,000 | 25,600,000 |
| parsed blocks | 20,000 | 200,000 |
| corpus build | 9.695 s | 174.569 s |
| 전체 wall time | 10.63 s | 188.09 s |
| maximum RSS | 579,567,616 bytes | 5,360,336,896 bytes |
| provider calls | 0 | 0 |
| semantic candidates | 측정 안 함 | 측정 안 함 |

초기 `10 1000 256` 확인에서는 probe는 실행됐으나 `/usr/bin/time`의 sandbox
sysctl 권한이 거부되어 RSS 증거로 쓰지 않았다. 위 표의 두 측정은 승인된
호스트 계측으로 실행했다.
입력은 반복 ASCII와 unique title뿐이며 전부 임시로 생성·정리했다.
동시 개발 작업을 통제한 기준 benchmark가 아니며 ambient-ignore 수정 전
probe binary였지만 fixture에는 ignore 파일이 없었다.

100k corpus hash:
`ce014f00d87032e6176f67010f89d5c1f4aedb4842e0ea74ae4a234959e101ee`.
이 결과는 역사적 V1 2-GB 메모리 목표를 초과한다. 현재 mandatory-AI 계약의
수치 예산/기준 장비 채택, streaming/HNSW/candidate union, provider token/cost,
20-GB 전체 작업은 따로 완료해야 한다. 도구 자체도 `qg_006_pass: false`를 출력한다.

## 인계

CURRENT_STATE, TRACEABILITY, 관련 명세/안정 알고리즘, RELEASE,
OPEN_QUESTIONS와 append-only DECISION_LOG를 갱신했다. 기존 accepted ADR의
불변식을 약화하거나 임의의 새 설계 결정을 승인하지 않았다. historical
transcript, commit/tag, remote workflow 실행, 서명, 패키지 배포는 변경하지 않았다.

공통 출력 inventory SHA-256은 그대로다:

```text
452ca0671e806a93b4f36f218cf9e62da899f6404c74705c2cf0ca14e413c7e5
```
