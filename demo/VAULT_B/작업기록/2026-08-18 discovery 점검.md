---
tags:
  - atlas/worklog
  - discovery
date: 2026-08-18
---

# Discovery 점검

cwd와 직계 하위 Vault 후보를 정렬해 표시하는 흐름을 확인했다. symlink,
IntegratedVault, project directory는 후보에서 제외해야 한다. 상위 workspace와
하위 Vault를 동시에 source로 고르면 중첩으로 거부되는 것이 기대 동작이다.

관련 설계: [[아키텍처/시스템 개요]] · 다음 기록: [[작업기록/2026-08-19 provider 점검]]
