# server

MCP 서버(`sql-mcp`) 내부 설계. 컴포넌트 간 계약은 [ARCHITECTURE.md](../ARCHITECTURE.md).

## 역할

- **일반 HTTP**(기본): `rmcp` `StreamableHttpService`를 **stateless + JSON response**로 기동. 세션 ID·장수 SSE는 쓰지 않는다.
- **stdio**(로컬): `--stdio` 또는 `SQL_MCP_TRANSPORT=stdio` → `rmcp` `stdio()`. 로그는 stderr. 동일 `SqlMcp` 핸들러.
- **CLI UX**: `-h`/`--help`는 stdout 도움말 후 exit 0(설정·DB를 열지 않음). 알 수 없는 옵션은 stderr+도움말 후 exit 2. 기동 실패(설정 경로·semantic DB·listen bind)는 `sql-mcp: …` + 경로/env 힌트.
- **시맨틱**: 프로세스 내 `libsql` 0.9(`core`) · 타입별 `meta_*` + `column_index`/FTS5 · HTTP `/api/meta` JSON
- **컴파일**: `pipeline` (시맨틱 SQL → 물리 SELECT). 상세 [spec.md](spec.md)
- **방언(D21)**: 컴파일은 엔진 중립. 물리 emit·행상한·(선택) bind/날짜 rewrite만 `DialectProfile`(`TargetConfig.engine`)
- **타깃**: 엔진 **Postgres**·**MySQL**(`sqlx` 0.9) · **SQLite**(`libsql` 로컬) · **Oracle**(`oracledb` thin, Instant Client 없음) · SELECT only · ADBC/JDBC/ODBC 비채택
- **로컬 테스트 DB**: BIRD Mini-Dev SQLite → 저장소 루트 `target_db/` ([target_db/README.md](../target_db/README.md))

## 모듈

| 모듈 | 책임 |
|------|------|
| `select_guard` | SQL 단일 SELECT(읽기 CTE 허용) AST 검사 |
| `expr_guard` | D16 식 AST 제한 · D18 컴파일 시 식별자 검사 |
| `mask` | D17 `encryption` 응답 마스킹(null) |
| `dialect` | **D21** `DialectProfile`: `quote_ident` · `rewrite_date_trunc` · `adapt_compiled_sql` · `wrap_row_limit` · `emit_physical_sql` |
| `compile/` | table/metric/cube → 물리 SQL. 공개 API는 `crate::compile::*` 유지 |
| `compile/mod` | 공개 타입·엔트리(`compile_cube_query` 등)·RBAC/바인드 헬퍼 |
| `compile/view_prune` | Step 3–4: 요약뷰 라우팅·join Steiner prune·union prune·인라인 보안 뷰 |
| `compile/rewrite` | Step 5–7: 멤버→물리 맵·서브쿼리 전개·HAVING/ORDER BY·filter inject·as_code JOIN |
| `ast_rewrite` | sqlparser VisitorMut로 식 식별자 → 물리 식 치환 |
| `ast_select` | SELECT/WHERE/GROUP BY AST 조립·파서 정규화 |
| `date_range` | 날짜 구간 추출(리터럴·`:bind`·OR/DNF)·union disjoint 판정 |
| `pipeline/` | 시맨틱 SQL AST → `compile_*` → 물리 SELECT. 공개 API는 `crate::pipeline::*` 유지 |
| `pipeline/mod` | 엔트리·단일 큐브·집합 연산(UNION/INTERSECT/EXCEPT) 오케스트레이션. FROM `relation_alias_cube`는 첫 식별자를 큐브로 정규화(`cube`·`cube.phys`·`AS alias`) |
| `pipeline/existence` | **D27** 리터럴-only 프로젝션 → existence dim rewrite (`SELECT *` 불허) |
| `pipeline/join` | 다중 큐브 JOIN · ON 검증/리라이트. **D29** 뷰 안 `one_to_many` 팬아웃·캐즘은 `compile` `assert_no_fanout_or_chasm` (`SQL_MCP_GRAIN_GUARD`, 기본 끔). 합계 재작성 없음 |
| `pipeline/clauses` | SELECT 프로젝션 분류·adhoc 집계·WHERE 세그먼트·GROUP BY/HAVING/ORDER BY/LIMIT. **D28** 비가산 fact `SUM`/`AVG`/`COUNT(col)` 가드는 `compile/rewrite` |
| `semantic/typed` | JSON ↔ 타입별 `meta_*` 분해/재조립 |
| `semantic` | 스키마·연결 검증 · libSQL CRUD · 검색 인덱스 재구축 |
| `search` | BM25(FTS5) · 벡터 · hybrid(RRF) |
| `embed` | D14: OpenAI-호환 HTTP만. 없으면 None → BM25만. `HashEmbedder`는 테스트 전용 |
| `sample` | cube+member → 물리/as_code 사전 샘플 해석 |
| `code_detect` | (레거시) 텍스트 코드 휴리스틱 — 현재 sample_column 미사용 |
| `target` | 타깃 레지스트리 · Postgres·MySQL(`sqlx`) / SQLite(`libsql`) / Oracle(`oracledb`) |
| `tools` | MCP 툴 표면 · Resource(schema + `skills/`) |
| `skill_resources` | `skills/` embed · Resource URI/MIME · list/read |
| `api` | 작성 HTTP `/api` (메타 CRUD · 물리 tables/columns/sample · `/api/databases`) |
| `http` | axum `/health` · `/api` · `/mcp` · `/ui` (`semantic-ui/dist` 정적) |
| `ui_build` | `semantic-ui/dist` 신선도 판정. `build.rs`가 `npm run build` 선행 |
| `stdio` | 로컬 stdin/stdout MCP (`--stdio`) |
| `cli` | 바이너리 argv: `-h`/`--help` · 알 수 없는 플래그 · 기동 실패 시 경로/힌트(stderr) |

## 검증

1. `document.schema.json`으로 문서 하나 검증. 실패 메시지는 JSON Pointer(예: `/dimensions/0/type`)와 키워드 이유이며, 문서 전체 덤프가 아니다.
2. 반영 후 등록 집합 연결 검증(실패 시 저장소 불변)
3. `PUT /api/meta/{kind}/{name}`은 **전체 replace**(D15)
4. CRUD 시 변경된 텍스트만 증분 임베딩한 뒤 **단일 Write 트랜잭션**으로 `meta_*`와 `column_index`/`column_index_fts`를 함께 갱신하고, 인메모리 카탈로그 캐시를 갱신

## DialectProfile (D21)

Cube.js `*Query` 어댑터와 같은 역할: **생성기 쪽 방언**, 자유 SQL 트랜스파일이 아니다. 모듈 `dialect`.

| 원칙 | 내용 |
|------|------|
| 중립 컴파일 | Step 1–7은 **canonical** `"…"` + `date_trunc` + D25 날짜 편의 함수 — `engine`에 의존하지 않음 |
| emit만 분기 | `emit_physical_sql` = `date_trunc`/날짜 편의 rewrite + quote adapt · 샘플 quote · `wrap_row_limit` |
| `date_trunc` | 단위 `year`/`month`/`day`/`hour`. MySQL·Oracle·SQLite로 치환, PG는 유지. 그 외 단위 → `validation` |
| D25 날짜 | `CURRENT_DATE`/`CURRENT_TIMESTAMP` identity · `NOW()`→Oracle `CURRENT_TIMESTAMP` · `DATEDIFF(end,start)` 일수 4방언 · `YEAR`/`MONTH`/`DAY`→PG/Oracle `EXTRACT` · `EXTRACT(YEAR\|MONTH\|DAY\|HOUR FROM x)`→SQLite `strftime` |
| 비채택 | SQLGlot 전체 트랜스파일 · 실행 bind rewrite · ADBC/JDBC |

`execute_sql` / `sample_column`은 `TargetConfig.engine` → `DialectProfile::for_engine`.

## 타깃 탐색 및 보안

- DW 조회 툴 `description`·서버 `instructions`는 **에이전트 선택 가이드**(언제 쓰고/쓰지 않는지·다음 스텝). 구현 용어(cube/물리 SQL)는 툴 설명에 넣지 않는다.
- `list_databases`: 설정 타깃 목록. 출력 `name`(=`TargetConfig.id`·문서 `target`)·`description`·`engine`(시크릿 제외).
- `list_tables` / `describe_table`: 시맨틱 **cube** 조회(툴 이름 유지). `list_tables(database?)`는 DW용 요약 `{ database, name, description }`만(`database`=`cube.target`). `describe_table(table, database?, caller_role?)`는 DW 상세: `database`/`name`/`description` + role-필터된 `dimensions`/`facts`/`metrics`/`segments`(멤버는 `name`·`description`·`type`·`encryption`·선택 `as_code`; metric은 `expression`·`aggregation`·`group_by`·`filter` 식 문자열; segment는 `filter` 식 문자열). `role`·`kind`·`view`·물리 `column`·metric/segment `cube`는 숨김. 물리 `information_schema` 경로 없음.
- `search_columns`: `keyword` | `vector` | `hybrid`(기본). DW 응답 `{ database, table, kind, name, description, score }` (`kind`=`table`\|`dimension`\|`fact`\|`metric`\|`segment`). `database`·`caller_role` 필터. 임베더 없으면 vector/hybrid→keyword.
- `sample_column` / `execute_sql`: 해석된 **cube.target**이 툴 인자 `database`와 같아야 한다(SQL에 cube가 여러 개면 **전부** 동일 target이고 인자와도 일치). `execute_sql`은 동일 target cube **JOIN/UNION**·복수 metric·동적 `GROUP BY`(`ROLLUP`/`CUBE`/`GROUPING SETS`)·`SELECT DISTINCT`·일반 `WHERE`·`HAVING`·`ORDER BY`·`LIMIT`/`OFFSET`·`:name` 바인드 치환을 허용한다. **D23:** 그레인 기반 2-모드 — **(1) Grouped(집계):** 명명 metric·ad-hoc 집계·metric 파생 연산 **혼용 허용**, 차원 스칼라 식(`date_trunc` 등) 및 차원-only `GROUP BY`/`HAVING` 허용, `HAVING`/`ORDER BY` 집계식 허용, 동일 `target` cube JOIN 허용(metric 없는 측은 참조 차원 `GROUP BY` 서브쿼리로 팬아웃 방지) / **(2) Ungrouped(행 상세):** metric·`GROUP BY`·`HAVING` 없는 dim/fact(+비집계 스칼라 식) 조회, 비-`DISTINCT` 조회 시 요약 뷰 우회(원천 뷰만) 및 비-`DISTINCT` cube JOIN 거절. 실행 응답은 `{ columns, rows, row_count, execution_time_ms }`(+`max_rows` 도달 시 `truncated: true`); 물리 `sql`은 `dry_run=true`일 때만 `{ sql, dry_run: true }`. `sample_column(database, table, name)`은 mappings 또는 dimension `as_code` 해석 후 인라인 보안 필터로 샘플하고 DW 응답 `{ database, table, name, as_code, masked, values }`만 반환(물리 `resolved`/`sql` 숨김). 큐브 스코프 `encryption` 마스킹(D17) 및 전 계층 `role` 검증.
- Skills extension: capabilities에 `io.modelcontextprotocol/skills` (빈 설정). `skills/list` · `skills/get` → `skills_ext` (Resource URI·내용과 동일, SHA-256 digest). `resources/directory/read` 없음.

## 설정

| 키 | 의미 |
|----|------|
| `SQL_MCP_SEMANTIC_PATH` | libSQL 파일 경로(`:memory:` 허용) |
| `SQL_MCP_CONFIG` | TOML 설정 경로 |
| `SQL_MCP_LISTEN` | 바인드 주소(기본 `127.0.0.1:8080`, HTTP 모드) |
| `SQL_MCP_TRANSPORT` | `stdio`면 stdio 모드(`--stdio`와 동일). 미설정·기타는 HTTP |
| `SQL_MCP_MAX_ROWS` / `SQL_MCP_SAMPLE_LIMIT` / `SQL_MCP_TIMEOUT_MS` | 상한 및 쿼리 타임아웃 |
| `SQL_MCP_GRAIN_GUARD` | 쉼표로 구분한 cube `target`. 그 타깃만 D29 팬아웃·캐즘 거절. 미설정이면 꺼짐 |
| `SQL_MCP_DEFAULT_ROLE` | `caller_role` 미지정 시 기본 적용할 역할 |
| `SQL_MCP_EMBEDDING_URL` / `SQL_MCP_EMBEDDING_API_KEY` | 둘 다 있으면 임베딩·vector/hybrid. 없으면 keyword(BM25)만(D14) |
| `SQL_MCP_EMBEDDING_MODEL` | 기본 `text-embedding-3-small` |

타깃 DSN은 설정 `url_env`가 가리키는 환경 변수로만 주입한다.

## Commands

```text
# cargo build/run이 semantic-ui `npm run build`를 선행한다(Node 22+). dist가 소스보다 새면 건너뛴다.
# rust-analyzer만 SQL_MCP_SKIP_UI_BUILD=1. CI/CD는 Node 22로 UI를 먼저 빌드한다.
cargo test -p sql-mcp
cargo build -p sql-mcp --release

export SQL_MCP_SEMANTIC_PATH=./data/semantic.db
export SQL_MCP_CONFIG=./server/examples/config.example.toml
cargo run -p sql-mcp
# GET /health · /api · POST /mcp · GET /ui/  (semantic-ui/dist)
cargo run -p sql-mcp -- --help

# 로컬 stdio (Cursor mcp.json 예: examples/mcp.stdio.example.json)
cargo run -p sql-mcp -- --stdio

# Postgres / MySQL / Oracle 연동 스모크(선택, DSN 환경 필요)
# SQL_MCP_TARGET_DW_URL · SQL_MCP_TARGET_MYSQL_URL · SQL_MCP_TARGET_ORACLE_URL
cargo test -p sql-mcp --test http_smoke -- --ignored

# BIRD SQLite 타깃 (target_db/ 준비 후 — ../target_db/README.md)
export SQL_MCP_CONFIG=./server/examples/config.bird.toml
cargo run -p sql-mcp

# Cursor/VS Code 디버그: Run and Debug → Debug sql-mcp (BIRD)
# preLaunchTask `build semantic-ui` 후 CodeLLDB (.vscode/launch.json)
# GUI 태스크는 zshrc를 안 읽으므로 PATH에 ~/.local/bin·Homebrew를 넣는다.

# 컨테이너·K8s: ../deploy/SETUP.md
```

동일 `SQL_MCP_SEMANTIC_PATH`를 HTTP·stdio 프로세스가 **동시에** 열지 말 것(libSQL 파일 락).

