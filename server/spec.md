# NL2SQL 시멘틱 컴파일러 & MCP 서버 개발 명세서

시맨틱 컴파일 파이프라인·타입별 libSQL 카탈로그의 **상세 설계**다.  
컴포넌트 간 계약은 [ARCHITECTURE.md](../ARCHITECTURE.md), 마일스톤은 [ROADMAP.md](../ROADMAP.md), 서버 모듈/Commands는 [DESIGN.md](DESIGN.md).

## 0. 확정 전제 (계획과 동일)

1. **카탈로그는 타입별 `meta_*` 테이블**에 저장한다. `document(kind,name,body)` 단일 blob 정본은 쓰지 않는다. MCP `metadata_*` **입출력은 JSON**(`document.schema.json`)이며, 저장 시 타입별 행으로 분해·조회 시 JSON으로 재조립한다.
2. **타깃 질의는 시맨틱 SELECT만.** 자유 물리 SELECT 경로는 없다. 공개 툴 `execute_sql`의 입력은 시맨틱 SQL이고, 6단계 컴파일 후에만 물리 SQL이 생성·실행된다. (문서·다이어그램의 `execute_semantic_sql`은 **이명** — 공개 툴명을 바꾸지 않는다.)
3. **툴 표면 유지:** `execute_sql`, `sample_column`, `search_columns`, `metadata_*` 등 현행 이름. `sample_cube_data`는 `sample_column`의 이명이다.
4. **타깃 접속:** Postgres·MySQL=`sqlx`, SQLite=`libsql`(로컬), Oracle=`oracledb`(thin). JDBC/ODBC/ADBC는 쓰지 않는다.
5. **검색:** keyword는 **FTS5**(순위는 `bm25()` — 별도 BM25 엔진이 아님). **vector + hybrid(RRF) 유지.** 구조화 FTS(`meta_search_fts`)와 embedding side table을 쓰며, FTS를 이중으로 두지 않는다.

## 1. 개요 및 아키텍처 목적

Rust(axum + `rmcp`) HTTP MCP 서버와 임베디드 libSQL 시맨틱 카탈로그로, DW Agent의 **시맨틱 SQL**을 안전·최적화된 **물리 DW SQL**로 변환·실행한다.

```
[DW Query Agent]                [Coding Agent]
       │                                 │
       ▼ (execute_sql)                   ▼ (metadata_* JSON)
┌─────────────────────────────────────────────────────────────────┐
│ Rust HTTP MCP Server (axum + rmcp + sqlparser-rs)               │
│                                                                 │
│  [Embedded libSQL]                                              │
│   ├─ 타입별 meta_* 카탈로그 (table/view/cube/metric/segment)    │
│   ├─ meta_search_fts (FTS5 keyword / bm25)                      │
│   └─ embedding side table (vector; hybrid = FTS ∪ vector RRF) │
│                                                                 │
│  [Semantic SQL Compilation Pipeline (6 Steps)]                  │
│   1. AST Select 점검 & 인가 검증                                │
│   2. Metric/Segment 전개 & 필터/가산성(Additivity) 추출          │
│   3. View 라우팅 (요약 뷰 우선 선택 & 컬럼 커버리지 검증)       │
│   4. 테이블 가지치기 (Join 그래프 최소 경로 & Union Pruning)    │
│   5. 물리 Table/Column 치환                                     │
│   6. 보안/격리 필터 주입(푸시다운) & Encryption 마스킹          │
└─────────────────────────────────────────────────────────────────┘
                               │
                               ▼ (sqlx pool)
                     [Target Data Warehouse]
```

`execute_sql` → 파이프라인(필수) → 컴파일된 물리 SQL만 타깃에 전달. 클라이언트가 넘긴 문자열을 SELECT 가드만 하고 타깃에 넣는 경로는 없다. `dry_run=true`이면 컴파일·인가까지 동일하게 수행한 뒤 물리 SQL만 반환하고 타깃 SELECT는 하지 않는다.

## 2. 임베디드 libSQL 카탈로그 스키마

`metadata_*`로 받은 JSON(`document.schema.json`)을 검증·연결 검사한 뒤, 변경된 텍스트에 대한 외부 임베딩만 증분 계산하여 **단일 Write 트랜잭션**으로 아래 타입별 테이블과 검색 인덱스(`column_index` / `column_index_fts`)에 함께 반영한다. 정본 DDL은 `semantic/libsql/catalog.sql`이다.

```sql
-- 1. Table 계층
CREATE TABLE meta_tables (
    name TEXT PRIMARY KEY,
    description TEXT,
    target TEXT NOT NULL,
    db TEXT NOT NULL,
    schema TEXT NOT NULL,
    table_name TEXT NOT NULL,
    pk_json TEXT,
    fk_json TEXT,
    scope_filter TEXT,
    default_filter TEXT,
    role_json TEXT NOT NULL DEFAULT '"all"'
) STRICT;

-- 2. View 계층 (join / union / minus / select)
CREATE TABLE meta_views (
    name TEXT PRIMARY KEY,
    description TEXT,
    target TEXT NOT NULL,
    combine TEXT NOT NULL CHECK (combine IN ('join', 'union', 'minus', 'select')),
    prefer TEXT CHECK (prefer IS NULL OR prefer = 'summary')
) STRICT;

-- 3. Cube 계층
CREATE TABLE meta_cubes (
    name TEXT PRIMARY KEY,
    target TEXT NOT NULL,
    view_name TEXT NOT NULL REFERENCES meta_views(name),
    description TEXT NOT NULL,
    role_json TEXT NOT NULL DEFAULT '"all"'
) STRICT;

-- 4. 검색 투영 (FTS5 keyword/bm25 + vector embedding + target 필터)
CREATE TABLE column_index (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    target TEXT NOT NULL DEFAULT 'dw',
    doc_kind TEXT NOT NULL CHECK (doc_kind IN ('table', 'view', 'cube', 'metric', 'segment')),
    doc_name TEXT NOT NULL,
    field_name TEXT NOT NULL,
    field_role TEXT NOT NULL CHECK (field_role IN ('column', 'dimension', 'fact', 'metric', 'segment', 'document')),
    text TEXT NOT NULL,
    embedding BLOB,
    UNIQUE (doc_kind, doc_name, field_name, field_role)
) STRICT;

CREATE VIRTUAL TABLE column_index_fts USING fts5(
    text,
    row_id UNINDEXED
);
```

`search_columns` 모드: `keyword`(FTS5/`bm25`, 음수 랭크를 양수 점수로 정규화) · `vector` · `hybrid`(RRF). `target` 인자가 주어지면 `column_index.target`으로 필터링한다. 임베더 없으면 vector/hybrid → keyword 폴백(D14).

## 3. 시맨틱 SQL 컴파일 파이프라인 명세 (6단계 + Step 7–8)

`execute_sql`이 sqlparser-rs로 아래를 **순차** 실행한다. 자유 물리 SQL 우회는 없다.

### Step 1: 시맨틱 SQL의 SELECT 및 전 계층 Role 점검

- 입력: `caller_role`(또는 설정 기본 role), 시맨틱 SQL(`execute_sql`의 `sql`)
- 동작:
  - `Parser::parse_sql(&GenericDialect, sql)` 실행.
  - AST 최상위가 `Statement::Query`인지 확인하고, DDL/DML·세션 변경·파일 접근 등은 즉시 거절.
  - FROM의 Cube(단일·JOIN·UNION)를 식별하고 `cube` · 참조된 `dimension`/`fact` · `metric` · `segment` · 물리 `table`의 `role` 접근 가능 여부를 모두 검증.
- 출력: 정제된 시맨틱 Query AST

### Step 2: 래핑(Metric, Segment, 일반 WHERE, HAVING/ORDER BY/LIMIT) 전개 및 가산성 분석

- 동작:
  - WHERE 전개: 세그먼트 식별자는 `meta_segments.filter_expression`으로 치환하고, 일반 dimension/fact 조건식은 AST 제한(`assert_filter_expr`) 및 식별자 검증(`assert_idents_known`) 후 보존한다.
  - Metric 전개: 단일·복수 metric을 지원한다. 단일 metric(또는 동일 필터)은 `filter_expression`을 WHERE AND로 두고, 복수 metric에서 개별 필터가 다르면 `CASE WHEN (filter) THEN expr END`로 집계 내부에 주입한다.
  - Group By 결정: SELECT/GROUP BY에 명시된 dimension 부분집합(생략 시 `metric.group_by` 기본값)을 사용하며, `aggregation`이 `rollup`/`cube`/`grouping_sets`이면 해당 그룹핑 절을 생성한다.
  - 가산성: `COUNT(DISTINCT)` / `AVG` / `MEDIAN` 등이면 `is_semi_additive` — 요약 뷰 그레뉼러리티 불일치 방지.
- 출력: Dimension·Fact 기준 큐브 SQL 계획 + 필터 목록 + `HAVING`/`ORDER BY`/`LIMIT`/`OFFSET`

### Step 3: 요약 뷰(Summary View) 우선 라우팅 및 커버리지 검증

- 동작:
  - Cube의 View가 `combine: "select"`이고 `prefer: "summary"`이면 `summary: true` source를 우선.
  - ReqCols = Projection ∪ GROUP BY ∪ WHERE ∪ Metric Filter 컬럼.
  - 요약 뷰의 `meta_view_columns`가 ReqCols를 커버하는지 확인. 비가산(`is_semi_additive`) metric이면 요약 뷰 컬럼 집합이 `ReqCols`와 **정확히 일치**할 때만 채택.
  - 조건을 만족하는 최적 요약 뷰를 고르고, 없으면 기본(원천) View.
- 출력: 선택된 단일 View (join, union, minus)

### Step 4: 테이블 가지치기 (Join 그래프 최소 경로 & Union Pruning)

- Join: 기준 테이블(`sources[0]`)과 필요 컬럼의 \(T_{req}\) + `meta_view_joins` 그래프로 연결 최소 집합(Steiner 근사)을 구하되, `INNER JOIN` 필터 의미와 `LEFT/RIGHT JOIN` 방향성이 뒤집히지 않도록 보존한다.
- Union: WHERE(metric/segment/사용자 WHERE 및 `:bind` 값 반영)와 각 table `scope_filter`가 disjoint이면 해당 UNION 브랜치 제외.
- 출력: 최적화된 물리 테이블 집합·결합 관계

### Step 5: 물리 Table / Column 치환 및 바인드(`:name`) 안전 치환

- AST의 Dimension/Fact/Metric을 `{schema}.{table}.{column}` 또는 물리 스칼라 `expression`으로 치환(VisitorMut — 누락 식별자 0).
- `params`가 제공된 경우 `:name` 플레이스홀더를 작은따옴표 이스케이프된 안전 리터럴로 치환한다.
- 출력: 물리 식별자 AST

### Step 6: 보안 격리 필터 주입(서브쿼리 푸시다운) & 마스킹

- 테이블별 `permission_filters`(호출자 role 매칭) · `scope_filter` · `default_filter`를 바깥 WHERE가 아니라  
  `FROM (SELECT * FROM schema.tbl WHERE [filters]) alias` 인라인 뷰로 주입 (Outer Join 왜곡·보안 유출 방지).
- `encryption: true`면 프로젝션·집계식을 `NULL`로 마스킹하고 필터 조건에서의 사용을 거부한다. 응답·`sample_column`에서도 평문 금지(D17).
- 출력: 집계·필터·HAVING/ORDER BY/LIMIT까지 반영된 물리 SQL (Step 7 입력)

### Step 7: 코드사전(`as_code`) 조인

- group_by/프로젝션 dim 중, dimension에 `as_code` 객체가 있으면 집계 서브쿼리 **바깥**에서 LEFT JOIN.
- ON: `q."{dim}" = code."{key_column}"`. 사전 테이블에도 permission/scope/default + `as_code.where`를 인라인 뷰로 적용.
- SELECT에 `code."{value_column}" AS "{dim}__value"` 추가(외부 `ORDER BY`/`LIMIT`/`OFFSET` 유지).
- 출력: 타깃으로 보낼 최종 물리 SQL 문자열(식별자 quote는 D21 프로필)

### Step 8 (실행 경계): DialectProfile emit · 행 상한 · 타임아웃 (D21)

- Step 1–7은 엔진 중립 **canonical** 물리 SQL(식별자 `"…"` · `date_trunc`).
- `pipeline` 말미: `DialectProfile::emit_physical_sql` = `rewrite_date_trunc` → `adapt_compiled_sql`(MySQL backticks).
  - `date_trunc('year'|'month'|'day'|'hour', expr)` → MySQL `DATE_FORMAT`/`DATE` · Oracle `TRUNC` · SQLite `datetime(…, 'start of …')` · Postgres 유지. 미지원 단위 → `validation`.
- 타깃 `execute_select`는 `wrap_row_limit` 및 `SQL_MCP_TIMEOUT_MS` 타임아웃을 적용하며, 실행 직전 미해석 `:param`이 남아 있으면 `validation`으로 거절한다. `sample_sql`은 인라인 보안 필터 + 프로필 quote + `rewrite_date_trunc`를 적용한다.

## 4. 개발 시 주의사항 (Checklist)

- [x] sqlparser AST 변환 시 Visitor/Folder로 식별자 누락 없이 순회할 것.
- [x] LEFT/RIGHT Outer Join에서 table 필터는 메인 WHERE가 아니라 인라인 뷰로 격리할 것.
- [x] `encryption: true`는 `sample_column`·최종 응답·집계/필터에서 평문 노출 금지(AST 마스킹 + 응답 마스킹).
- [x] meta_* / FTS / embedding 재구축은 동시성 읽기와 충돌하지 않도록 **단일 Write 트랜잭션**으로 처리할 것.
- [x] `execute_sql`에 자유 물리 SQL 우회 경로를 두지 말 것.
- [x] JDBC/ODBC/ADBC를 도입하지 말 것 — 타깃은 Postgres·MySQL=`sqlx`, SQLite=`libsql`, Oracle=`oracledb`만.
- [x] 물리 SQL quote·행상한·방언 함수는 `DialectProfile`만 통하고, 파이프라인 Step 1–6에 엔진 `if`를 흩뿌리지 말 것(D21).
- [x] `sqlparser` `to_string()`/`Display` 결과에 방언을 기대하지 말 것 — emit은 프로필이 담당.
