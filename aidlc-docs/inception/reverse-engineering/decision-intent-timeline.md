# Decision & Intent Timeline (reconstructed from Codex sessions)

**What this is**: the one artifact the repo did *not* already hold — a reconstruction of the
**user's raw intent** across the project's life, built from Codex session history and mapped to the
ADRs / requirements those intents became. The repo's `DECISION_LOG.md` records *what was decided*;
this records *what the user actually asked for*, in their words, with citations.

**Method**: user turns were extracted from `~/.codex/sessions/2026/{08,09}/…/rollout-*.jsonl`
(real turns = `response_item` with `role == "user"`; injected developer/environment preambles
filtered out). Sessions whose `cwd` ends in `ObsidianKnowlegeComplication` are okc-core's own
history (the repo was later renamed to `okc-core`). Consolidated transcript: `/tmp/okc-core-user-intents.md`.

> **Provenance discipline** (per the okc-mcp lesson the user cares about): entries below are
> **user requests** quoted/paraphrased from sessions. Whether each was *implemented* is tracked in
> [`docs/TRACEABILITY.md`](../../../docs/TRACEABILITY.md) and [`docs/CURRENT_STATE.md`](../../../docs/CURRENT_STATE.md),
> not asserted here. ADR/REQ mappings are the drafter's attribution (verify against the ADR text).

## Citation keys

| Key | Date | Session (rollout id) |
|---|---|---|
| S1 | 2026-08-15 | `rollout-2026-08-15T14-57-13-01a003fe…` (genesis) |
| S2 | 2026-09-01 | `rollout-2026-09-01T21-17-19-01a05ce7…` (repo init) |
| S3 | 2026-09-01 | `rollout-2026-09-01T21-19-11-01a05ce8…` (OKC naming, TUI, multi-vault, MCP-neutral) |
| S4 | 2026-09-02 | `rollout-2026-09-02T21-41-55-01a06223…` (v2 dev·TUI·release impl) |
| S5 | 2026-09-02 | `rollout-2026-09-02T23-41-51-01a06291…` (AI-required pivot) |
| S6 | 2026-09-03 | `rollout-2026-09-03T20-59-12-01a06723…` (V3 AI-required impl + cwd TUI) |
| S7 | 2026-09-04/05 | `rollout-2026-09-05T00-42-25-01a06d15…` (cwd-based real-use TUI impl) |
| S8 | 2026-09-05 | `rollout-2026-09-05T10-13-47-01a06f20…` (demo vaults + run flow) |
| S9 | 2026-09-05 | `rollout-2026-09-05T10-20-17-01a06f26…` (library-ization plan) |
| S10 | 2026-09-05 | `rollout-2026-09-05T11-52-03-01a06f7a…` (Python·Node impl) |
| S11 | 2026-09-05 | `rollout-2026-09-05T11-56-56-01a06f7f…` (one-line install) |
| S12 | 2026-09-05 | `rollout-2026-09-05T12-01-30-01a06f83…` (v1/v2-in-source cleanup question) |
| S13 | 2026-09-06 | `rollout-2026-09-06T16-39-33-01a075a8…` (Schema 3 single-source impl) |
| S14 | 2026-09-06 | `rollout-2026-09-06T18-22-43-01a07606…` (contradiction audit + ADR drafts) |
| S15 | 2026-09-06 | `rollout-2026-09-06T22-40-47-01a076f3…` (Windows SDK smoke-test fix) |

## Timeline

### 1 — Genesis: frameworkify Vault integration, docs-first (S1, 2026-08-15)
- **Intent**: ingest a shared ChatGPT session on "brain merging" / neuroscience-flavored math,
  then *"Vault 통합하는 걸 라이브러리나 프레임워크화 해서 만들고 싶어"* — turn Vault integration into a
  library/framework. Asked: which language, how consumers use it, what else is needed. (S1)
- **Framework vs plugin**: *"프레임워크로 만드는게 낫겠어, 아님 code agent(codex, claude code)에 연결하는
  mcp나 plugin으로 만드는게 낫겠어?"* — deliberated framework vs MCP/plugin. (S1)
  → **framework-first** (ADR-0002).
- **Provider-neutral AI from day one**: *"ai는 openai만 국한된게 아니라 어느 llm도 잘 붙을 수 있게
  인터페이스같은게 있으면 좋겠어."* (S1) → **ADR-0004**, REQ-AI-001 (provider neutrality).
- **Founding rule — documentation-first**: *"세웠던 플랜과 앞선 gpt 세션들 그리고 뇌과학 알고리즘과 로직을
  포함한 모든 것들을 각 역할에 맞게 md파일로 이력을 남겨두고 … 세션을 refresh 하고 할거니까 … 가장 중요한건
  알고리즘, 로직이야."* — persist everything to Markdown so fresh sessions rely only on the docs;
  algorithms/logic are the most important to capture. (S1)
  → this is the origin of the docs-first mandate in [`AGENTS.md`](../../../AGENTS.md), `docs/algorithms/`,
  `docs/history/`, and ultimately of *this very exercise*.
- **Process asks**: git per-feature branch/commit/merge; an "Agent Team" split into SW-dev and QA
  roles. (S1) → reflected in `docs/roles/`.
- **Open source**: README explaining what the open-source project is. (S1) → ADR-0007.

### 2 — Repository created (S2, 2026-09-01)
- **Intent**: initialize git and push to `github.com/dolgogae/ObsidianKnowlegeComplication.git`.
  (S2) — later renamed to `okc-core.git`.

### 3 — Naming, TUI, multi-vault, and the core problem statement (S3, 2026-09-01)
- **Verify against requirements**: *"요구사항에 맞게 다 구현됐는지 확인해줘."* (S3)
- **Multi-vault**: *"N개의 vault가 있다면 어떻게 사용하는지 예시."* (S3) → REQ-SRC-*, ADR-0016.
- **TUI with in-app conflict resolution**: *"cli 말고 tui로 만들어서 충돌도 tui상에서 선택하도록."* (S3)
  → conflict UX; REQ-APP-*.
- **Product name = OKC**: *"이 앱은 ObisidianKnoweledgeComplication이라고 OKC라고 명명할꺼야"*; and
  *"cli도 vaultc 말고 okc로 해줘."* (S3) → **ADR-0019** (single `okc` app/CLI name).
- **Install like Claude Code**: *"claude code처럼 다운로드할 수 있는 방식이면 젤 좋을거 같아."* (S3)
  → **ADR-0020**.
- **THE problem framing — MCP-origin-neutral merge**: *"obsidian mcp는 각 개인이 로컬에서 자신의 경험을
  obsidian vault에 저장하는 역할 … 그 개개인의 vault를 병합하는 역할이 해당 프로젝트의 문제해결 … 각기 다른
  mcp를 통해 만들어진 vault여도 통합을 잘 하냐는 거지."* (S3)
  → **ADR-0016**, REQ-SRC-001 (origin neutrality). This is the canonical statement of what OKC is
  *for*. (Note: neutrality is REQ-SRC-001; REQ-MCP-001 is a *separate, future* MCP adapter surface.)
- Also probed the meaning of "provider" and where the passphrase/secret is used (S3) → later
  secret boundary, ADR-0025.

### 4 — v2 build-out: dev + TUI + release (S4, 2026-09-02)
- **Intent**: implement the "OKC v2 개발·TUI·배포 완성 계획" (a prior-agent plan treated as source of
  intent); update README; commit/push. (S4) → precursor format family, ADR-0015/0018 (later
  superseded by Schema 3 single-source, ADR-0027).

### 5 — The pivot: AI becomes *required* (S5, 2026-09-02)
- **Guide style**: wanted a `spring.io/quickstart`-style guide (UI + content), Markdown-based;
  asked where such guides normally live. (S5) → `guide/`.
- **Discovery of the gap**: *"여기 병합시킬때 AI 안써?"* then the decisive statement —
  *"내가 의도한건 AI가 위 로직을 기반으로 vault를 합치는거였는데, 그게 구현이 안된거네? 나는 AI가 필수로
  연결되어야한다고 생각하고 그 AI가 로직을 기반으로 완벽 통합이 되는게 목적."* — AI was always the
  intent; merge without AI is not the product; **AI must be required**. (S5)
  → **ADR-0022** (V3 AI-required), REQ-AI-004 (mandatory integration).
- **Provider unification**: *"AI는 결국 API로 호출해서 써야하는데 어떻게 공통화해서 편하게 사용할 수 있을까?"*
  and *"AI 인터페이스를 모두 동일하게 설계하기는 많이 어렵나?"* (S5) → ADR-0023, REQ-AI-001/003 (provider neutrality + recording).

### 6 — V3 implementation + cwd-first TUI (S6, 2026-09-03)
- **Intent**: implement the "OKC 0.3 V3 — AI 필수 Vault 통합" plan; record the conflict with the old
  "AI optional / no auto near-dup merge" spec in `OPEN_QUESTIONS` and ADR-0022–0024 **before** code.
  (S6) → ADR-0022/0023/0024, `docs/history/OPEN_QUESTIONS.md`.
- **Plain-language limitations**: asked to rewrite the "not a stable release…" warning in simpler
  terms. (S6) → `docs/CURRENT_STATE.md` limitations wording.
- **cwd-first TUI**: *"tui로도 통합 작업을 할 수 있게 … ai가 연결 안됐으면 연결 화면 … 해당 폴더에서 vault
  어느것들을 통합할건지 … claude code 같이 특정 폴더에서 실행하면 그 폴더 기준으로 okc의 통합기능을 실행."*
  (S6) → **ADR-0025**, REQ-APP-002.
- **Token entry like Claude Code**: *"TUI에서 환경변수는 맞는데 claude code 토큰처럼 입력받는걸로."* (S6)
  → keychain/secret boundary, ADR-0025.

### 7 — cwd-based real-use TUI, shared service (S7, 2026-09-04/05)
- **Intent**: `cd 폴더 && okc` runs the full flow in-TUI
  (AI connect → Vault select → sensitive-info check/consent → taxonomy review → cluster review →
  compile → verify), with V3 logic moved into `okc-app` as a **shared service** so CLI and TUI run
  the same code. (S7) → **ADR-0025**; `crates/okc-app`.

### 8 — Demo data & run flow (S8, 2026-09-05)
- **Intent**: confirm the run flow (install okc → cd folder → run TUI); create `demo/` with
  realistic, interlinked `VAULT_A/B/C` dummy data — *"obsidian vault가 어떻게 생긴지 확인해서 링크들이
  서로 어떤식으로 유기적으로 되어있어야하는지 확인해서."* (S8) → `demo/`.

### 9 — Library-ization: decide, then plan (S9, 2026-09-05)
- **Intent**: expose OKC as a library importable from other languages — *"라이브러리화해서 다른
  언어에서 import해서 사용."* Explicitly *plan only, no changes yet, and decide which languages
  first* — *"변경하지 말고 지금은 계획만 세우고 어떤 언어들 먼저 할지도 정해야지."* (S9)
  → **ADR-0026**, REQ-SDK-*. (Note the recurring user preference: decide/plan before implementing.)

### 10 — Python & Node bindings implemented (S10, 2026-09-05)
- **Intent**: implement per the plan — order `common Rust API → Python → Node`; Python dist
  `okc-compiler` / import `okc`; npm `okc-compiler`; CPython 3.11 `abi3`; first release requires
  feature parity across both languages. Update README/docs/guide; commit/push. Confirmed *"SDK도
  포함되어있지? guide랑 docs에."* (S10) → **ADR-0026**, REQ-SDK-001/002; `crates/okc-interop`,
  `bindings/python`, `bindings/node`.

### 11 — Frictionless install (S11, 2026-09-05)
- **Intent**: *"okc로 바로 tui 시작을 못하고 릴리즈된 폴더 절대 경로를 적어야 실행"* — install friction; then
  *"PATH 등록 하는거 말고 claude code같이 딱 설치해서 바로 okc로 쓸 수 있게는 못하나?"* (S11)
  → **ADR-0020/0021** (install/update, scoped updater exception).

### 12 — "v1/v2 in source looks messy" (S12, 2026-09-05)
- **Intent**: *"원래 오픈소스들이 v1,v2 이런식으로 남겨놓냐?"* … *"소스코드로 이렇게 남아있으니까
  지저분해보이는데"* — dislike of multiple format versions living in source. (S12)
  → motivates **ADR-0027** (single-source).

### 13 — Schema 3 single-source cleanup (S13, 2026-09-06)
- **Intent**: implement "현행 Schema 3 단일 소스 정리" — main keeps only Schema 3; V1/V2 preserved
  via git history + remote **archive tags**; keep Python/Node SDK but Schema-3-only. (S13)
  → **ADR-0027**; see `DECISION_LOG.md` for the executed decision.

### 14 — Contradiction audit + ADR drafts (S14, 2026-09-06)
- **Intent**: *"현재 프로젝트에 논리적 모순이나 리팩터링이 필요한 부분을 세세하게 점검하고 고쳐줘"*, finish the
  remainder, commit/push, and *"ADR 설계안 작성해"* (write ADR design drafts). (S14)
  → **ADR-0028–0031** (`proposed`): bounded semantic execution & performance (0028), manual
  amendment & sensitive-finding review (0029), current pack & extended materialization (0030),
  cancellation/recovery & filesystem capabilities (0031).

### 15 — Cross-platform SDK hardening (S15, 2026-09-06)
- **Intent**: fix a Windows Python-SDK smoke-test failure (`okc_compiler-0.3.0-cp311-abi3-win_amd64.whl`),
  then commit/push. (S15) → matches HEAD commit `7f87f7c` "make SDK smoke tests portable".

## Cross-cutting themes (user's own priorities, recurring)

1. **Documentation-first / session-resilient** — the reason this repo has DECISION_LOG, ADRs, and
   summaries at all (S1, reinforced by every "docs 업데이트/최신화" request: S4, S6, S10, S12).
2. **"Claude Code-like" ergonomics** — one-line install (S3, S11), cwd-as-workspace (S6, S7),
   token-prompt secret entry (S6). This is the clearest UX north star in the sessions.
3. **AI as a first-class requirement, provider-neutral** — from "any LLM" (S1) to "AI must be
   required" (S5) to V3 (S6).
4. **Decide/plan before implementing** — repeated explicitly (S3 "계획 완성", S9 "변경하지 말고
   계획만", S6 "코드보다 먼저 OPEN_QUESTIONS에 기록"). Consistent with the user's known sensitivity to
   agents running ahead of review.
5. **Cleanliness of the single source** — dislike of versioned cruft in `main` (S12 → S13/ADR-0027).

## Intent → ADR / REQ map (drafter's attribution — verify)

| Intent | Sessions | ADR(s) | REQ area |
|---|---|---|---|
| Framework-first, thin adapters | S1 | 0002 | — |
| Provider-neutral AI interface | S1, S5 | 0004, 0023 | REQ-AI-001/003 |
| Documentation-first mandate | S1 | (AGENTS.md) | — (no REQ) |
| Immutable sources, new output | S3, S4 | 0003, 0006 | REQ-SNP-*, REQ-PRV-001 |
| Multi-vault, MCP-origin-neutral merge | S3 | 0016 | REQ-SRC-001 |
| Single `okc` app + CLI name | S3 | 0019 | REQ-APP-001 |
| Claude Code-style install/update | S3, S11 | 0020, 0021 | REQ-REL-001 |
| AI required (V3) | S5, S6 | 0022 | REQ-AI-004 |
| Critic gate + human approval | S6 | 0024 | REQ-AI-002, REQ-INT-004/005 |
| cwd-workspace TUI + secret boundary | S6, S7 | 0025 | REQ-APP-002, REQ-SEC-003 |
| Python/Node library boundary | S9, S10 | 0026 | REQ-SDK-001/002 |
| Schema 3 single source, archive tags | S12, S13 | 0027 | REQ-CMP-003 |
| Perf / manual-amend / pack / recovery (drafts) | S14 | 0028–0031 (proposed) | REQ-PERF-001, REQ-MAT-001 |

> These rows were cross-checked against [`docs/TRACEABILITY.md`](../../../docs/TRACEABILITY.md) and
> `docs/adr/` by an adversarial verification pass (2026-09-07) and corrected — most importantly the
> `REQ-AI-*` mapping (REQ-AI-001 = provider neutrality, REQ-AI-002 = hostile-proposals/critic,
> REQ-AI-003 = recording/replay, REQ-AI-004 = mandatory integration) and REQ-MCP-001 (a future
> *adapter*, not merge neutrality — neutrality is REQ-SRC-001). `TRACEABILITY.md` remains authoritative.
