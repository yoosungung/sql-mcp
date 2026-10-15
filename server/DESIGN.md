# server

MCP 서버(`sql-mcp`) 내부 설계. 컴포넌트 간 계약은 [ARCHITECTURE.md](../ARCHITECTURE.md).

## 역할

- **일반 HTTP**: `rmcp` `StreamableHttpService`를 **stateless + JSON response**로 기동. 세션 ID·장수 SSE는 쓰지 않는다(계약의 “세션형 Streamable HTTP 취소”와 동일 취지).
- **시맨틱**: 프로세스 내 `libsql` 0.9(`core`) · `document` 테이블 · `metadata_*`
- **타깃**: `sqlx` 0.9 · 첫 엔진 **Postgres** · SELECT only

## 모듈

| 모듈 | 책임 |
|------|------|
| `select_guard` | SQL 단일 SELECT(읽기 CTE 허용) AST 검사 |
| `semantic` | 스키마 검증 · 연결 검증 · libSQL CRUD |
| `target` | 타깃 레지스트리 · Postgres 클라이언트 |
| `tools` | MCP 툴 표면 |
| `http` | axum 리스너 `/mcp` |

## 검증

1. `document.schema.json`으로 문서 하나 검증
2. 반영 후 등록 집합 연결 검증(실패 시 저장소 불변)
3. `metadata_update`는 **전체 replace**(D15)

## 타깃 탐색

- `list_tables` / `describe_table`: 타깃 카탈로그(`information_schema`) 우선. `target` 생략 시 시맨틱 `table` 등록분.
- `search_columns`: M1 스텁 — 빈 `items`
- `sample_column`: `is_code`는 M1에서 `false`(D9 미정)

## 설정

| 키 | 의미 |
|----|------|
| `SQL_MCP_SEMANTIC_PATH` | libSQL 파일 경로(`:memory:` 허용) |
| `SQL_MCP_CONFIG` | TOML 설정 경로 |
| `SQL_MCP_LISTEN` | 바인드 주소(기본 `127.0.0.1:8080`) |
| `SQL_MCP_MAX_ROWS` / `SQL_MCP_SAMPLE_LIMIT` / `SQL_MCP_TIMEOUT_MS` | 상한 |

타깃 DSN은 설정 `url_env`가 가리키는 환경 변수로만 주입한다.

## Commands

```text
# 단위·통합(Postgres 없이 가능한 범위)
cargo test -p sql-mcp

# 실행
export SQL_MCP_SEMANTIC_PATH=./data/semantic.db
export SQL_MCP_CONFIG=./server/examples/config.example.toml
cargo run -p sql-mcp

# Postgres 연동 스모크(선택): 타깃 DSN을 config url_env에 맞춘 뒤
cargo test -p sql-mcp --test http_smoke -- --ignored
```
