# sql-mcp

임베디드 **libSQL** 시맨틱 저장소를 품은 Rust **MCP** 서버(`rmcp`, **일반 HTTP**).

- **타깃 DB** (DW, **복수**): 테이블/컬럼 탐색, 샘플(+code), **SELECT만**
- **시맨틱 저장소** (프로세스 내 **libSQL**): **메타데이터 CRUD(JSON)** 통로. **시맨틱 레이어 모델은 향후 정의**
- Frontend / stdio / Streamable HTTP: **없음(취소)**

계약: [ARCHITECTURE.md](ARCHITECTURE.md) · 계획: [ROADMAP.md](ROADMAP.md) · 워크플로: [AGENTS.md](AGENTS.md)

## Status

기획 단계. 시맨틱 저장소=**libSQL**, transport=**일반 HTTP**. 시맨틱 레이어 상세는 후속(D12). D2/D13 확정 후 M1 가능.

## Quickstart

구현 후 `server/DESIGN.md`의 `## Commands`와 이 절을 채운다.

```text
# (예정)
export SQL_MCP_SEMANTIC_PATH=./data/semantic.db
# targets: config (복수)
# listen: e.g. http://127.0.0.1:8080

cargo run -p sql-mcp
```

## 문서 맵

| 문서 | 용도 |
|------|------|
| [AGENTS.md](AGENTS.md) | 수행 방법·문서 레이아웃 |
| [ARCHITECTURE.md](ARCHITECTURE.md) | 불변 계약·인터페이스 |
| [ROADMAP.md](ROADMAP.md) | 마일스톤·미결정 |
| `server/DESIGN.md` | MCP 내부 + Commands (코드 도입 시) |
