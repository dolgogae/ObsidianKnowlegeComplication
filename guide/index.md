---
title: OKC Quickstart
description: 샘플 Obsidian Vault를 컴파일하고 검증하는 5분 가이드
outline: [2, 3]
sidebar: false
aside: false
---

<a class="qs-back" href="./cli">← 전체 가이드</a>

<div class="qs-hero">
  <p class="qs-eyebrow">OKC / 5분 Quickstart</p>
  <h1>Vault를 컴파일해 봅시다.</h1>
  <p class="qs-lede">
    저장소에 포함된 샘플을 변경하지 않고 읽어, 검토 가능한 계획과
    독립적으로 검증되는 새 Obsidian Vault를 만듭니다.
  </p>
  <span class="qs-badge">v0.3.0 development tree · frozen V2 회귀 흐름</span>
</div>

<div class="qs-meta">
  <div class="qs-card">
    <strong>이번에 만들 것</strong>
    <p>Compiled Vault 하나와 선택적 배포 파일 <code>.okcpack</code></p>
  </div>
  <div class="qs-card">
    <strong>걸리는 시간</strong>
    <p>이미 Rust가 준비되어 있다면 약 5분</p>
  </div>
  <div class="qs-card">
    <strong>필요한 것</strong>
    <p>Rustup, 터미널, 이 저장소의 source checkout</p>
  </div>
  <div class="qs-card">
    <strong>바뀌지 않는 것</strong>
    <p>입력 Vault의 파일, 권한, 수정 시각은 그대로 유지됩니다.</p>
  </div>
</div>

::: warning 개발 버전입니다
공식 안정 설치 프로그램은 아직 없습니다. 이 가이드는 저장소에 고정된
Rust 1.97.1과 `Cargo.lock`으로 개발 바이너리를 직접 빌드합니다. 새 V3
Vault를 만들려면 먼저 [V3 AI 통합 가이드](./v3-integration.md)를 읽으세요.
:::

<p class="step-kicker">Step 1 / 5 · 준비</p>

## OKC 빌드하기

저장소 루트에서 release 바이너리를 빌드하고 현재 환경을 확인합니다.

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

출력에 `OKC 0.3.0`, `format: okc schema 3`, 현재 target이 보이면 준비가
끝났습니다.

::: details 빌드 없이 실행하고 싶다면
`cargo run --locked -p okc -- doctor`처럼 `--` 뒤에 OKC 인자를 넣을 수
있습니다. 아래 단계에서는 읽기 쉽게 release 바이너리 경로를 사용합니다.
:::

<p class="step-kicker">Step 2 / 5 · 검사와 계획</p>

## 샘플 Vault 계획하기

이번 Quickstart는 저장소의 `tests/fixtures/basic_vault`를 입력으로 씁니다.
Markdown, Canvas, attachment, opaque Base가 들어 있지만 필수 충돌은 없는
작은 Vault입니다.

::: code-group

```sh [macOS / Linux]
mkdir -p .quickstart
./target/release/okc inspect \
  quickstart=tests/fixtures/basic_vault \
  --workspace .quickstart/build.sqlite

./target/release/okc plan \
  quickstart=tests/fixtures/basic_vault \
  --workspace .quickstart/build.sqlite \
  --out .quickstart/plan.json
```

```powershell [Windows PowerShell]
New-Item -ItemType Directory -Force .quickstart | Out-Null
.\target\release\okc.exe inspect `
  quickstart=tests/fixtures/basic_vault `
  --workspace .quickstart/build.sqlite

.\target\release\okc.exe plan `
  quickstart=tests/fixtures/basic_vault `
  --workspace .quickstart/build.sqlite `
  --out .quickstart/plan.json
```

:::

계획 요약의 마지막 부분은 다음과 비슷합니다.

```text
operations: 5
conflicts: 0
required unresolved conflicts: 0
path: .quickstart/plan.json
```

`plan.json`은 아직 아무 Vault도 만들지 않습니다. 무엇을 어디에 쓸지,
어떤 링크를 바꿀지, 어떤 충돌에 사람 결정이 필요한지를 먼저 밀봉한
검토 문서입니다.

<p class="step-kicker">Step 3 / 5 · 승인</p>

## 빈 결정 문서로 승인하기

이 샘플에는 필수 충돌과 AI 제안이 없지만, OKC는 “검토 단계를 생략했다”는
암묵적 상태를 허용하지 않습니다. `plan.json`을 열어 최상위 `plan_id`를
복사한 다음 `.quickstart/decisions.json`을 만듭니다.

```json
{
  "schema_version": 2,
  "plan_id": "plan.json에서 복사한 plan_... 값",
  "decisions": [],
  "conflicts": []
}
```

이제 승인된 불변 계획을 만듭니다.

::: code-group

```sh [macOS / Linux]
./target/release/okc approve .quickstart/plan.json \
  --decisions .quickstart/decisions.json \
  --out .quickstart/approved-plan.json
```

```powershell [Windows PowerShell]
.\target\release\okc.exe approve .quickstart/plan.json `
  --decisions .quickstart/decisions.json `
  --out .quickstart/approved-plan.json
```

:::

::: tip 왜 승인 파일이 따로 있나요?
계획 자체를 고치면 무엇을 검토했는지 알 수 없습니다. OKC는 원래 계획을
그대로 두고, plan ID와 content hash에 묶인 결정만 별도로 결합합니다.
:::

<p class="step-kicker">Step 4 / 5 · 컴파일</p>

## 새 Vault와 Pack 만들기

승인된 계획을 새 디렉터리에 materialize하고, 같은 내용의 deterministic
`.okcpack`도 만듭니다.

::: code-group

```sh [macOS / Linux]
./target/release/okc compile .quickstart/approved-plan.json \
  --output "$PWD/.quickstart/CompiledVault" \
  --pack "$PWD/.quickstart/CompiledVault.okcpack"
```

```powershell [Windows PowerShell]
$quickstart = (Resolve-Path .quickstart).Path
.\target\release\okc.exe compile .quickstart/approved-plan.json `
  --output (Join-Path $quickstart "CompiledVault") `
  --pack (Join-Path $quickstart "CompiledVault.okcpack")
```

:::

OKC는 기존 출력 경로를 덮어쓰지 않습니다. 같은 명령을 다시 실행하면
기존 결과를 보존하고 종료 코드 `6`으로 실패합니다.

상대 경로와 절대 경로 모두 같은 sibling publication 검사를 통과합니다.
예시는 결과 위치를 분명하게 보이기 위해 절대 경로를 사용합니다.

<p class="step-kicker">Step 5 / 5 · 성공 확인</p>

## 독립적으로 검증하기

컴파일 성공 메시지만 신뢰하지 않고 Vault와 Pack을 각각 다시 검증합니다.

::: code-group

```sh [macOS / Linux]
./target/release/okc verify .quickstart/CompiledVault
./target/release/okc verify .quickstart/CompiledVault.okcpack
```

```powershell [Windows PowerShell]
.\target\release\okc.exe verify .quickstart/CompiledVault
.\target\release\okc.exe verify .quickstart/CompiledVault.okcpack
```

:::

두 명령 모두 `valid: true`를 출력하면 첫 컴파일이 완료된 것입니다.

<div class="result-tree">

```text
.quickstart/
├── plan.json
├── decisions.json
├── approved-plan.json
├── CompiledVault.okcpack
└── CompiledVault/
    ├── knowledge/
    ├── attachments/
    ├── canvases/
    ├── views/
    └── .okc/          # manifest, checksums, plan, provenance
```

</div>

마지막으로 출력 하나가 어디에서 왔는지 확인해 봅니다.

```sh
./target/release/okc explain \
  .quickstart/CompiledVault \
  knowledge/Topic.md
```

결과는 `knowledge/Topic.md`에서 copy operation과 원본 snapshot/file로
이어지는 provenance 레코드를 보여줍니다.

## 다음으로 해볼 것

<div class="guide-grid">
  <a class="guide-card" href="./cli">
    <strong>내 Vault를 CLI로 컴파일하기 →</strong>
    <p>여러 디렉터리와 ZIP, 출력 형식, 자동화용 종료 코드를 알아봅니다.</p>
  </a>
  <a class="guide-card" href="./conflicts">
    <strong>충돌을 검토하고 결정하기 →</strong>
    <p>종료 코드 4와 LINK_AMBIGUITY를 안전하게 처리합니다.</p>
  </a>
  <a class="guide-card" href="./tui">
    <strong>TUI 둘러보기 →</strong>
    <p>현재 폴더에서 AI 연결부터 Vault 선택, 검토, compile·verify까지 진행합니다.</p>
  </a>
  <a class="guide-card" href="./ai-provider">
    <strong>AI Provider 연결하기 →</strong>
    <p>명시적 문서 공개, 기록, replay, 개별 승인의 경계를 확인합니다.</p>
  </a>
</div>
