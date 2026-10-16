# server

MCP 서버(`sql-mcp`) 내부 설계. 컴포넌트 간 계약은 [ARCHITECTURE.md](../ARCHITECTURE.md).

## 역할

- **일반 HTTP**(기본): `rmcp` `StreamableHttpService`를 **stateless + JSON response**로 기동. 세션 ID·장수 SSE는 쓰지 않는다.
- **stdio**(로컬): `--stdio` 또는 `SQL_MCP_TRANSPORT=stdio` → `rmcp` `stdio()`. 로그는 stderr. 동일 `SqlMcp` 핸들러.
- **시맨틱**: 프로세스 내 `libsql` 0.9(`core`) · 타입별 `meta_*` + `column_index`/FTS5 · `metadata_*` JSON
- **컴파일**: `pipeline` (시맨틱 SQL → 물리 SELECT). 상세 [spec.md](spec.md)
- **타깃**: 엔진 **Postgres**(`sqlx` 0.9) · **SQLite**(`libsql` 로컬, 시맨틱과 동일 C 라이브러리) · SELECT only
- **로컬 테스트 DB**: BIRD Mini-Dev SQLite → 저장소 루트 `target_db/` ([target_db/README.md](../target_db/README.md))

## 모듈

| 모듈 | 책임 |
|------|------|
| `select_guard` | SQL 단일 SELECT(읽기 CTE 허용) AST 검사 |
| `expr_guard` | D16 식 AST 제한 · D18 컴파일 시 식별자 검사 |
| `mask` | D17 `encryption` 응답 마스킹(null) |
| `compile` | table/metric → 물리 SQL: 요약뷰·join/union pruning·필터 푸시다운·encryption NULL |
| `ast_rewrite` | sqlparser VisitorMut로 식 식별자 → 물리 식 치환 |
| `ast_select` | SELECT/WHERE/GROUP BY AST 조립·파서 정규화 |
| `date_range` | 날짜 구간 추출(리터럴·`:bind`·OR/DNF)·union disjoint 판정 |
| `pipeline` | 시맨틱 SQL AST → `compile_*` → 물리 SELECT (`execute_sql` 유일한 실행 경로) |
| `semantic/typed` | JSON ↔ 타입별 `meta_*` 분해/재조립 |
| `semantic` | 스키마·연결 검증 · libSQL CRUD · 검색 인덱스 재구축 |
| `search` | BM25(FTS5) · 벡터 · hybrid(RRF) |
| `embed` | D14: OpenAI-호환 HTTP만. 없으면 None → BM25만. `HashEmbedder`는 테스트 전용 |
| `sample` | cube+member → 물리/as_code 사전 샘플 해석 |
| `code_detect` | (레거시) 텍스트 코드 휴리스틱 — 현재 sample_column 미사용 |
| `target` | 타깃 레지스트리 · Postgres(`sqlx`) / SQLite(`libsql`) |
| `tools` | MCP 툴 표면 · Resource(schema + `skills/`) |
| `skill_resources` | `skills/` embed · Resource URI/MIME · list/read |
| `http` | axum 리스너 `/mcp` (일반 HTTP) |
| `stdio` | 로컬 stdin/stdout MCP (`--stdio`) |

## 검증

1. `document.schema.json`으로 문서 하나 검증
2. 반영 후 등록 집합 연결 검증(실패 시 저장소 불변)
3. `metadata_update`는 **전체 replace**(D15)
4. CRUD 성공 후 `column_index` 재구축

## 타깃 탐색

- `list_tables` / `describe_table`: 타깃 카탈로그(`information_schema`) 우선. `target` 생략 시 시맨틱 `table` 등록분.
- `search_columns`: `keyword` | `vector` | `hybrid`(기본). 임베더 없으면 vector/hybrid→keyword.
- `sample_column`: `cube`+`member` → mappings 또는 dimension `as_code` 해석 후 타깃 샘플. encryption 마스킹(D17, `masked: true`)
- `execute_sql`: **시맨틱 SQL만** 수용 → `pipeline` 컴파일(Step7 코드사전 조인) → 타깃 실행. `caller_role`·`params`. 응답에 물리 `sql` 포함. encryption 마스킹(D17).

## 설정

| 키 | 의미 |
|----|------|
| `SQL_MCP_SEMANTIC_PATH` | libSQL 파일 경로(`:memory:` 허용) |
| `SQL_MCP_CONFIG` | TOML 설정 경로 |
| `SQL_MCP_LISTEN` | 바인드 주소(기본 `127.0.0.1:8080`, HTTP 모드) |
| `SQL_MCP_TRANSPORT` | `stdio`면 stdio 모드(`--stdio`와 동일). 미설정·기타는 HTTP |
| `SQL_MCP_MAX_ROWS` / `SQL_MCP_SAMPLE_LIMIT` / `SQL_MCP_TIMEOUT_MS` | 상한 |
| `SQL_MCP_EMBEDDING_URL` / `SQL_MCP_EMBEDDING_API_KEY` | 둘 다 있으면 임베딩·vector/hybrid. 없으면 keyword(BM25)만(D14) |
| `SQL_MCP_EMBEDDING_MODEL` | 기본 `text-embedding-3-small` |

타깃 DSN은 설정 `url_env`가 가리키는 환경 변수로만 주입한다.

## Commands

```text
cargo test -p sql-mcp
cargo build -p sql-mcp --release

export SQL_MCP_SEMANTIC_PATH=./data/semantic.db
export SQL_MCP_CONFIG=./server/examples/config.example.toml
cargo run -p sql-mcp
# GET /health · POST /mcp

# 로컬 stdio (Cursor mcp.json 예: examples/mcp.stdio.example.json)
cargo run -p sql-mcp -- --stdio

# Postgres 연동 스모크(선택)
cargo test -p sql-mcp --test http_smoke -- --ignored

# BIRD SQLite 타깃 (target_db/ 준비 후 — ../target_db/README.md)
export SQL_MCP_CONFIG=./server/examples/config.bird.toml
cargo run -p sql-mcp

# Cursor/VS Code 디버그: Run and Debug → Debug sql-mcp (BIRD) (.vscode/launch.json, CodeLLDB)

# 컨테이너·K8s: ../deploy/SETUP.md
```

동일 `SQL_MCP_SEMANTIC_PATH`를 HTTP·stdio 프로세스가 **동시에** 열지 말 것(libSQL 파일 락).

