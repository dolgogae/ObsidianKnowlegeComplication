---
title: Current Framework Planning Session Record
status: historical
owners:
  - release-maintainer
last_updated: 2026-08-16
decision_refs:
  - ADR-0001
  - ADR-0002
  - ADR-0004
source_refs:
  - HIST-CURRENT-PLAN
---

# Current Framework Planning Session Record

Historical source material. Not normative. Use current specifications and accepted ADRs for implementation.

This is a recoverable record from the active conversation context, not a byte-perfect export. User prompts are retained verbatim where available; assistant material is summarized because the full earlier response bodies were not present in the refreshed execution context. No hidden reasoning or system instructions are reconstructed.

## User prompts, in order

1. `지금 수식적인 표현들이 뇌를 합치면서 뇌과학을 사용해서 그렇지?`
2. `지금 딱 Vault 통합하는 걸 라이브러리나 프레임워크화 해서 만들고 싶어. 1. 어떤 언어로 만들지 2. 프레임워크, 라이브러리는 어떤식으로 쓰게 할지. 3. 기타 필요한것들`
3. The same framework question was repeated.
4. `정확히 뭘 물어보는지 모르겠어서 좀더 풀어서 설명해줘`
5. `이걸 너가 보기엔 프레임워크로 만드는게 낫겠어. 아님 code agent(codex, claude code)에 연결하는 mcp나 plugin으로 만드는게 낫겠어?`
6. `1. sdk로 만들때 이 프레임워크를 어떻게 사용할지 예시를 나한테 설명해줬으면 좋겠어. 그리고 프레임워크의 역할이 어떻게 되는지도 2. ai는 openai만 국한된게 아니라 어느 llm도 잘 붙을 수 있게 인터페이스같은게 있으면 좋겠어`
7. `여기서 세웠던 플랜과 앞선 gpt 세션들 그리고 뇌과학 알고리즘과 로직을 포함한 모든 것들을 각 역할에 맞게 md파일로이력을 남겨두고 코딩작업을 할때 해당 md 파일들만 참고해서 만들 수 있도록 하자. 세션을 refresh 하고 할거니까 중요한 모든 정보들을 md 파일로 남겨둬야해. 가장 중요한건 알고리즘, 로직이야.`
8. `진행`
9. `지금 진행해줘`

## Recoverable assistant decisions

- The formulas are present because the design used brain-inspired memory analogies; they are engineering scoring/retrieval models rather than a literal act of combining brains.
- Framework-first is preferable. MCP and Obsidian plugin surfaces should wrap the same compiler rather than contain its logic.
- Rust was selected for the deterministic cross-platform core and CLI; Python remains suitable for model experiments/providers and TypeScript for an Obsidian plugin.
- SDK use should expose explicit inspect → plan → optional augment → validate → approve → compile → verify → explain phases.
- AI must be optional and provider-neutral through Rust traits and a versioned NDJSON subprocess protocol. AI proposes evidence-bound changes and never writes output directly.
- Markdown was chosen as the durable handoff format, with root agent instructions, role guides, normative specs, stable/experimental algorithm separation, ADRs, summaries, immutable transcripts, sources, open questions, and traceability.
