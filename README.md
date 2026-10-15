# sql-mcp

**시맨틱 레이어 DB**를 중심으로 한 MCP + 관리 UI.

- **MCP** (`server/`, Rust/`rmcp`): 시맨틱 DB에 대해 테이블·컬럼 탐색, 샘플(+code), **SELECT만**
- **Frontend** (`frontend/`): 시맨틱 레이어 DB **관리**
- 자격 증명은 기동 설정으로만 주입

계약: [ARCHITECTURE.md](ARCHITECTURE.md) · 계획: [ROADMAP.md](ROADMAP.md) · 워크플로: [AGENTS.md](AGENTS.md)

## Status

기획 단계. 바이너리·UI 실행 방법은 아직 없다. D1/D10/D11/D12 확정 후 M1(MCP) → M2(Frontend) 순으로 구현한다.

## Quickstart

구현 후 각 컴포넌트 `DESIGN.md`의 `## Commands`와 이 절을 채운다.

```text
# (예정) 시맨틱 DB URL
export SEMANTIC_DATABASE_URL=...

# (예정) MCP
cargo run -p sql-mcp

# (예정) Frontend — frontend/DESIGN.md 참고
```

## 문서 맵

| 문서 | 용도 |
|------|------|
| [AGENTS.md](AGENTS.md) | 수행 방법·문서 레이아웃 |
| [ARCHITECTURE.md](ARCHITECTURE.md) | 불변 계약·컴포넌트 간 인터페이스 |
| [ROADMAP.md](ROADMAP.md) | 마일스톤·미결정 |
| `server/DESIGN.md` | MCP 내부 + Commands (코드 도입 시) |
| `frontend/DESIGN.md` | 관리 UI 내부 + Commands (코드 도입 시) |
