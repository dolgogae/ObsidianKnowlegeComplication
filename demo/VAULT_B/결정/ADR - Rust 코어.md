---
tags:
  - adr
  - rust
status: accepted
date: 2026-07-26
---

# Rust 코어 채택

## 결정

snapshot, parser, identity, validation, materialization, verification을 Rust
library에 둔다. CLI와 TUI는 같은 application service를 호출하는 얇은 adapter로
유지한다.

## 이유

하나의 구현으로 경로 검증과 결정적 serialization을 공유할 수 있고, 잘못된
provider output을 typed boundary에서 거부하기 쉽다. UI별로 compile 정책을
복제하지 않는다.

## 제약

unsafe 사용을 최소화하고 플랫폼별 atomic publication은 검토된 wrapper를 통해
구현한다. 성능 주장은 실제 workload 증거가 있을 때만 문서화한다.

적용 구조: [[아키텍처/시스템 개요]] · 검증: [[품질/테스트 전략]]
