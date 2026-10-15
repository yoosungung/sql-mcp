# sql-mcp

임베디드 시맨틱 DB를 품은 Rust **MCP** 서버(`rmcp`, **stdio**).

- **타깃 DB** (DW, **복수**): 테이블/컬럼 탐색, 샘플(+code), **SELECT만**
- **시맨틱 DB** (프로세스 내): BM25/벡터 컬럼 검색 + **메타데이터 CRUD(JSON)**
- Frontend / Streamable HTTP: **없음(취소)**

계약: [ARCHITECTURE.md](ARCHITECTURE.md) · 계획: [ROADMAP.md](ROADMAP.md) · 워크플로: [AGENTS.md](AGENTS.md)

## Status

기획 단계. D1/D2/D12/D14 확정 후 M1(`server/`) 시작.

## Quickstart

구현 후 `server/DESIGN.md`의 `## Commands`와 이 절을 채운다.

```text
# (예정) 임베디드 시맨틱 DB 경로 + 타깃 설정
export SQL_MCP_SEMANTIC_PATH=./data/semantic.db
# targets: config file / env (복수)

cargo run -p sql-mcp   # stdio MCP
```

## 문서 맵

| 문서 | 용도 |
|------|------|
| [AGENTS.md](AGENTS.md) | 수행 방법·문서 레이아웃 |
| [ARCHITECTURE.md](ARCHITECTURE.md) | 불변 계약·인터페이스 |
| [ROADMAP.md](ROADMAP.md) | 마일스톤·미결정 |
| `server/DESIGN.md` | MCP 내부 + Commands (코드 도입 시) |
