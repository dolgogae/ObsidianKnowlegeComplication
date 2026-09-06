---
title: OKC Quickstart
description: 현재 Schema 3 프로젝트를 준비하는 5분 가이드
outline: [2, 3]
sidebar: false
aside: false
---

<a class="qs-back" href="./cli">← 전체 가이드</a>

<div class="qs-hero">
  <p class="qs-eyebrow">OKC / 5분 Quickstart</p>
  <h1>원본을 건드리지 않고 시작합니다.</h1>
  <p class="qs-lede">
    개발 바이너리를 빌드하고, 현재 Schema 3 project에 immutable source를
    등록한 뒤 provider-free 단계와 AI review 경계를 확인합니다.
  </p>
  <span class="qs-badge">v0.3.0 development tree · current Schema 3</span>
</div>

::: warning 개발 버전입니다
공식 안정 설치 프로그램과 registry package는 아직 없습니다. 현재 출력은
Markdown directory이며 attachment/Canvas/Base/OKCPack은 지원하지 않습니다.
원본 Vault를 백업하고 결과를 유일한 복사본으로 사용하지 마세요.
:::

## 1. 빌드와 환경 확인

저장소 루트에서 고정 lockfile로 빌드합니다.

::: code-group

```sh [macOS / Linux]
cargo build --locked --release -p okc
./target/release/okc doctor
```

```powershell [Windows PowerShell]
cargo build --locked --release -p okc
.\target\release\okc.exe doctor
```

:::

`OKC 0.3.0`과 `format: okc schema 3`이 보이면 현재 바이너리입니다.

## 2. 프로젝트 만들기

Quickstart는 저장소의 작은 `tests/fixtures/basic_vault`를 읽습니다. Source는
수정하지 않고 새 project에 절대 경로로 등록됩니다.

::: code-group

```sh [macOS / Linux]
mkdir -p .quickstart
./target/release/okc project create .quickstart/Demo.okc-project \
  --name Demo --curator quickstart --language ko-KR
./target/release/okc --project .quickstart/Demo.okc-project \
  project source add demo "$PWD/tests/fixtures/basic_vault"
```

```powershell [Windows PowerShell]
New-Item -ItemType Directory -Force .quickstart | Out-Null
$fixture = (Resolve-Path tests/fixtures/basic_vault).Path
.\target\release\okc.exe project create .quickstart/Demo.okc-project `
  --name Demo --curator quickstart --language ko-KR
.\target\release\okc.exe --project .quickstart/Demo.okc-project `
  project source add demo $fixture
```

:::

Project와 source가 다른 경로에 있고 원본 파일의 내용/권한/수정 시각이 그대로
유지됩니다. `project upgrade`나 이전 approval import는 제공하지 않습니다.

## 3. 로컬 provider 연결

새 integration proposal에는 capability를 만족하는 provider가 필요합니다.
실행 중인 로컬 Ollama 예시는 다음과 같습니다.

```sh
./target/release/okc provider add local \
  --kind ollama \
  --endpoint http://127.0.0.1:11434 \
  --model YOUR_MODEL
./target/release/okc provider test local
./target/release/okc --project .quickstart/Demo.okc-project \
  project ai-route set local
```

Provider가 없다면 여기까지가 안전한 setup smoke입니다. Model ID를 임의로
자동 선택하지 마세요. Hosted provider를 쓸 때는 [AI Provider](./ai-provider.md)의
credential과 per-call consent 경계를 먼저 확인합니다.

## 4. 통합과 명시적 승인

```sh
./target/release/okc --project .quickstart/Demo.okc-project integrate
./target/release/okc --project .quickstart/Demo.okc-project \
  review taxonomy show
./target/release/okc --project .quickstart/Demo.okc-project \
  review taxonomy approve --rationale "전체 배치를 검토함"

./target/release/okc --project .quickstart/Demo.okc-project integrate
./target/release/okc --project .quickstart/Demo.okc-project \
  review cluster list
```

각 cluster의 source/evidence, synthesis, critic을 읽은 뒤 하나씩 승인합니다.
Omission과 minor finding에는 exact ID별 rationale이 필요하고 major/critical은
승인할 수 없습니다. 자세한 절차는 [AI 통합](./integration.md)에 있습니다.

## 5. Offline compile과 검증

모든 cluster가 승인되어 최신 plan이 봉인되면 provider를 끄고도 실행할 수
있습니다.

```sh
./target/release/okc --project .quickstart/Demo.okc-project \
  compile --output "$PWD/.quickstart/CompiledVault"
./target/release/okc verify .quickstart/CompiledVault
./target/release/okc explain \
  .quickstart/CompiledVault \
  knowledge/YOUR_APPROVED_PATH.md
```

Compile은 기존 경로를 덮어쓰지 않습니다. Verify는 manifest, approved plan,
모든 file byte/checksum과 provenance를 다시 계산합니다. Explain도 먼저 전체
verify를 통과한 뒤 한 output의 typed record를 반환합니다.

## 다음으로

<div class="guide-grid">
  <a class="guide-card" href="./integration">
    <strong>AI 통합 전체 흐름 →</strong>
    <p>Taxonomy, cluster, critic, omission과 waiver 승인을 진행합니다.</p>
  </a>
  <a class="guide-card" href="./cli">
    <strong>CLI 계약 →</strong>
    <p>현재 명령, JSON 출력, 종료 코드와 제거된 이전 형태를 확인합니다.</p>
  </a>
  <a class="guide-card" href="./tui">
    <strong>TUI 사용법 →</strong>
    <p>Cwd discovery부터 review, compile, verify까지 진행합니다.</p>
  </a>
  <a class="guide-card" href="./python-node">
    <strong>Python · Node.js →</strong>
    <p>Interop schema 2 typed result와 Job API를 사용합니다.</p>
  </a>
</div>
