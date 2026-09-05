# OKC TUI 데모 작업공간

이 폴더에는 같은 Atlas 프로젝트를 서로 다른 관점에서 기록한 세 개의
Obsidian Vault가 있습니다.

- VAULT_A: 제품, 사용자 조사, 요구사항, 제품 회의
- VAULT_B: 아키텍처, 구현 결정, 품질, 배포와 운영
- VAULT_C: 고객 도입, 지원, 거버넌스, 평가와 현장 운영

세 Vault에는 정확히 같은 문서, 비슷하지만 표현이 다른 문서, 서로 보완하는
문서, 날짜와 정책이 충돌하는 문서가 섞여 있습니다. 실제 비밀, 개인 연락처,
고객 식별 정보는 포함하지 않았습니다.

현재 데이터 규모는 Markdown 112개와 내부 링크 434개입니다.

| Vault | 관점 | 노트 | 내부 링크 |
|---|---|---:|---:|
| VAULT_A | 제품과 사용자 조사 | 30 | 118 |
| VAULT_B | 엔지니어링과 품질 | 38 | 136 |
| VAULT_C | 고객 운영과 거버넌스 | 44 | 180 |

각 Vault는 루트의 `.obsidian` 설정 폴더, 전체 MOC, 영역별 MOC, 주제 노트,
회의·작업·daily note로 구성됩니다. 링크는 Vault 내부에서 실제로 해석되도록
만들었으며 다음 Obsidian 기능을 포함합니다.

- Vault-root 기준 wikilink와 custom display text
- aliases와 계층형 tags properties
- heading link와 human-readable block reference
- note, heading, block embed
- MOC에서 주제로, 주제에서 근거·결정·후속 작업으로 이어지는 backlinks

Obsidian 내부 링크는 Vault 범위 안에서 해석되므로 VAULT_A에서 VAULT_B의 파일을
직접 가리키는 링크는 만들지 않았습니다. 대신 같은 개념에 유사한 title, aliases,
tags와 본문 표현을 사용해 OKC가 Vault 사이의 의미 관계와 충돌을 찾게 했습니다.

## 실행

저장소에서 release binary를 먼저 빌드한 뒤 실행합니다.

    cargo build --locked --release -p okc
    cd demo
    ../target/release/okc

이미 이 저장소에 빌드된 0.3.0 debug binary를 사용하려면 다음처럼 실행합니다.

    cd demo
    ../target/debug/okc

Vault 선택 화면에서 VAULT_A, VAULT_B, VAULT_C만 선택하세요. 현재 탐색 규칙상
하위 Markdown을 포함한 상위 demo 폴더도 후보로 보일 수 있지만, 상위 폴더와
하위 Vault를 동시에 선택하면 안 됩니다.

빌드 출력은 source 밖의 새 경로를 사용하세요. 기본 제안인 IntegratedVault는
이 작업공간 바로 아래에 생성되며, 기존 경로를 덮어쓰지 않습니다.

세 Vault를 모두 선택하면 112개 문서를 대상으로 AI integration이 실행되므로
provider와 model에 따라 시간과 호출 비용이 생길 수 있습니다. 이것이 의도한
대용량 demo 경로입니다.

## 데이터 검사 결과

작성 후 Vault별 wikilink graph를 검사했습니다. 세 Vault 모두 깨진 링크와 고립
노트가 없고, 각 graph가 하나의 연결 component를 이룹니다. OKC 0.3.0의 inspect도
세 snapshot과 Markdown document 112개를 정상적으로 읽었습니다.
