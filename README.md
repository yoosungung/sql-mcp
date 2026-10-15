# sql-mcp

AI 클라이언트(Cursor, Claude Desktop 등)가 SQL 데이터베이스를 안전하게 탐색·질의할 수 있게 하는 **Model Context Protocol (MCP)** 서버.

- 서버 언어: **Rust** (`rmcp`)
- MCP 능력: **테이블 목록/상세**, **컬럼 검색**, **컬럼 샘플(+code)**, **SELECT SQL만**
- 자격 증명은 기동 설정으로만 주입 · 행/샘플 상한·타임아웃

계약·인터페이스: [ARCHITECTURE.md](ARCHITECTURE.md)  
계획·미결정: [ROADMAP.md](ROADMAP.md)  
에이전트/기여자 워크플로: [AGENTS.md](AGENTS.md)

## Status

기획 단계. 바이너리·로컬 실행 방법은 아직 없다. M0에서 엔진·sqlx 여부를 확정한 뒤 M1에서 `server/` 구현을 시작한다.

## Quickstart

구현 후 `server/DESIGN.md`의 `## Commands`와 이 절을 채운다.

```text
# (예정) 빌드
cargo build -p sql-mcp

# (예정) stdio MCP
DATABASE_URL=sqlite:./dev.db SQL_MCP_READONLY=true cargo run -p sql-mcp
```

## 문서 맵

| 문서 | 용도 |
|------|------|
| [AGENTS.md](AGENTS.md) | 수행 방법·문서 레이아웃 |
| [ARCHITECTURE.md](ARCHITECTURE.md) | 불변 계약·MCP/컴포넌트 인터페이스 |
| [ROADMAP.md](ROADMAP.md) | 마일스톤·미결정 항목 |
| `server/DESIGN.md` | 서버 내부 설계 + Commands (코드 도입 시) |
