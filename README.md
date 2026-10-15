# sql-mcp

임베디드 **libSQL** 시맨틱 저장소를 품은 Rust **MCP** 서버(`rmcp`, **일반 HTTP**).

- **타깃 DB** (DW, **복수**): 테이블/컬럼 탐색, 샘플(+code), **SELECT만**
- **시맨틱 저장소** (프로세스 내 **libSQL**): **메타데이터 CRUD(JSON)**. 등록 스키마·예시는 [`semantic/`](semantic/DESIGN.md)
- Frontend / stdio / Streamable HTTP: **없음(취소)**

계약: [ARCHITECTURE.md](ARCHITECTURE.md) · 계획: [ROADMAP.md](ROADMAP.md) · 워크플로: [AGENTS.md](AGENTS.md)

## Status

M0–M3 골격: 시맨틱=**libSQL**, 타깃=**Postgres**, transport=**일반 HTTP**, 검색·컴파일·CI·배포 런북. 빌드·실행은 [server/DESIGN.md](server/DESIGN.md), 배포는 [deploy/SETUP.md](deploy/SETUP.md).

## Quickstart

```text
export SQL_MCP_SEMANTIC_PATH=./data/semantic.db
export SQL_MCP_CONFIG=./server/examples/config.example.toml
# config의 url_env 가 가리키는 타깃 DSN을 환경에 넣는다.

cargo run -p sql-mcp
# MCP: POST http://127.0.0.1:8080/mcp  (JSON 요청/응답)
```

## 문서 맵

| 문서 | 용도 |
|------|------|
| [AGENTS.md](AGENTS.md) | 수행 방법·문서 레이아웃 |
| [ARCHITECTURE.md](ARCHITECTURE.md) | 불변 계약·인터페이스 |
| [ROADMAP.md](ROADMAP.md) | 마일스톤·미결정 |
| [semantic/DESIGN.md](semantic/DESIGN.md) | 등록 JSON Schema·예시·libSQL 카탈로그 |
| `server/DESIGN.md` | MCP 내부 + Commands (코드 도입 시) |
