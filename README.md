# sql-mcp

**시맨틱 레이어 DB**를 읽는 Rust **MCP** 서버.

- **MCP** (`server/`, `rmcp`): 테이블·컬럼 탐색, BM25/벡터 컬럼 검색, 샘플(+code), **SELECT만**
- Frontend / 관리 UI: **없음(취소)**
- 자격 증명은 기동 설정으로만 주입

계약: [ARCHITECTURE.md](ARCHITECTURE.md) · 계획: [ROADMAP.md](ROADMAP.md) · 워크플로: [AGENTS.md](AGENTS.md)

## Status

기획 단계. 바이너리 실행 방법은 아직 없다. D1/D2/D12/D14 확정 후 M1(`server/`)을 시작한다.

## Quickstart

구현 후 `server/DESIGN.md`의 `## Commands`와 이 절을 채운다.

```text
# (예정) 시맨틱 DB URL
export SEMANTIC_DATABASE_URL=...

# (예정) MCP
cargo run -p sql-mcp
```

## 문서 맵

| 문서 | 용도 |
|------|------|
| [AGENTS.md](AGENTS.md) | 수행 방법·문서 레이아웃 |
| [ARCHITECTURE.md](ARCHITECTURE.md) | 불변 계약·인터페이스 |
| [ROADMAP.md](ROADMAP.md) | 마일스톤·미결정 |
| `server/DESIGN.md` | MCP 내부 + Commands (코드 도입 시) |
