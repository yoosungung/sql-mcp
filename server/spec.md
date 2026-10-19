# NL2SQL 시멘틱 컴파일러 & MCP 서버 개발 명세서

시맨틱 컴파일 파이프라인·타입별 libSQL 카탈로그의 **상세 설계**다.  
컴포넌트 간 계약은 [ARCHITECTURE.md](../ARCHITECTURE.md), 마일스톤은 [ROADMAP.md](../ROADMAP.md), 서버 모듈/Commands는 [DESIGN.md](DESIGN.md).

## 0. 확정 전제 (계획과 동일)

1. **카탈로그는 타입별 `meta_*` 테이블**에 저장한다. `document(kind,name,body)` 단일 blob 정본은 쓰지 않는다. MCP `metadata_*` **입출력은 JSON**(`document.schema.json`)이며, 저장 시 타입별 행으로 분해·조회 시 JSON으로 재조립한다.
2. **타깃 질의는 시맨틱 SELECT만.** 자유 물리 SELECT 경로는 없다(D19). 공개 툴 `execute_sql`의 입력은 시맨틱 SQL이고, 6단계 컴파일 후에만 물리 SQL이 생성·실행된다. (문서·다이어그램의 `execute_semantic_sql`은 **이명** — 공개 툴명을 바꾸지 않는다.) **D23:** 시맨틱 SELECT는 그레인 기반 2-모드로 동작한다 — **(1) Grouped(집계) 모드:** 명명 metric · ad-hoc 집계(`sum/count/avg/min/max`) · metric 파생 연산 **동일 SELECT 혼용 허용**, 차원 스칼라 식(`date_trunc` 등) 및 차원-only `GROUP BY`/`HAVING` 허용, `HAVING`/`ORDER BY` 집계식 허용, 동일 `target` cube JOIN 허용(metric 없는 JOIN 측은 참조 차원 `GROUP BY` 서브쿼리로 다대다 팬아웃 방지). **(2) Ungrouped(행 상세) 모드:** metric·집계·`GROUP BY`·`HAVING` 없는 dim·fact(+비집계 스칼라 식) 프로젝션, `SELECT DISTINCT` 지원, 비-`DISTINCT` 상세 조회 시 요약 뷰(`summary: true`) 우회(원천 뷰 선택) 및 비-`DISTINCT` cube JOIN 거절.
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
  - WHERE 전개: 호출자 ad-hoc WHERE 절의 직접 서브쿼리는 거절(`assert_no_subquery_in_ast`)한 뒤, 세그먼트 식별자는 `meta_segments.filter_expression`(읽기 전용 서브쿼리 허용, `assert_segment_filter_expr`)으로 치환하고, 일반 dimension/fact 조건식은 AST 제한 및 식별자 검증(`assert_idents_known`) 후 보존한다.
  - Metric 및 집계·윈도우 전개(D23): 명명 metric(등록된 `metric.expression`에는 읽기 전용 서브쿼리 및 윈도우 함수 허용, `assert_metadata_metric_expr`), ad-hoc 집계(`sum/count/avg/min/max`), 윈도우 함수(`ROW_NUMBER`/`RANK`/`DENSE_RANK`/`NTILE`/`LAG`/`LEAD`/`FIRST_VALUE`/`LAST_VALUE`/`NTH_VALUE` 및 `SUM`/`COUNT`/`AVG`/`MIN`/`MAX` `OVER (...)`), metric 간 파생 연산을 동일 SELECT에서 함께 지원한다. 단일 metric(또는 모든 활성 metric의 필터가 동일)이면 `filter_expression`을 WHERE AND로 두고, 복수 metric에서 개별 필터(또는 무필터 ad-hoc 혼합)가 다르면 `CASE WHEN (filter) THEN expr END`로 집계 내부에 주입한다.
  - Group By 결정: 명시된 `GROUP BY`(차원 식별자·프로젝션 별칭·위치 `1..N`·차원 스칼라 식; 윈도우 함수는 `GROUP BY`에서 거절) → 없으면 프로젝션된 dimension(및 비-윈도우 차원 스칼라 식) → 둘 다 없으면 명명 metric 전용 질의는 `metric.group_by` **기본 grain**(복수 metric이면 교집합), ad-hoc 포함 질의는 전역 집계를 사용한다(D24). `group_by`는 허용목록이 아니며, 질의 축이 목록 밖이어도 cube 멤버·`role`·`encryption`만 통과하면 허용한다. `aggregation`이 `rollup`/`cube`/`grouping_sets`이면 해당 그룹핑 절을 생성한다. 프로젝션에 metric 없이 차원만 두고 `GROUP BY`/`HAVING`/집계 `ORDER BY`를 쓴 질의도 Grouped 모드로 컴파일한다.
  - 가산성: `COUNT(DISTINCT)` / `AVG` / `MEDIAN` 등이면 `is_semi_additive` — 요약 뷰 그레뉼러리티 불일치 방지.
- 출력: Dimension·Fact 기준 큐브 SQL 계획 + 필터 목록 + `DISTINCT`/`HAVING`/`ORDER BY`/`LIMIT`/`OFFSET`

### Step 3: 요약 뷰(Summary View) 우선 라우팅 및 커버리지 검증

- 동작:
  - Cube의 View가 `combine: "select"`이고 `prefer: "summary"`일 때, **Grouped 모드(또는 `SELECT DISTINCT`)**이면 `summary: true` source를 우선하고, **비-`DISTINCT` Ungrouped 상세 행 조회(D23)**이면 사전 집계 왜곡을 막기 위해 `summary: false` 원천 뷰를 우선한다.
  - ReqCols = Projection ∪ GROUP BY ∪ WHERE ∪ HAVING ∪ ORDER BY ∪ Metric Filter 컬럼.
  - 후보 뷰의 `meta_view_columns`가 ReqCols를 커버하는지 확인. 비가산(`is_semi_additive`) metric이면 요약 뷰 컬럼 집합이 `ReqCols`와 **정확히 일치**할 때만 채택.
  - 조건을 만족하는 최적 뷰를 고르고, 없으면 기본(원천) View.
- 출력: 선택된 단일 View (join, union, minus)

### Step 4: 테이블 가지치기 (Join 그래프 최소 경로 & Union Pruning)

- Join: 필요 컬럼의 \(T_{req}\)(단일 매핑 컬럼 우선 수집 후 다중 매핑 컬럼은 미포함 시에만 추가) + `meta_view_joins` 그래프로 최단 경로 연결 최소 집합(Steiner 근사)을 구한다. 모든 조인이 `inner`이고 \(T_{req}\)가 비어 있지 않으면 불필요한 루트(`sources[0]`)를 강제 포함하지 않으며, 외조인(`left`/`right`/`full`)이 있으면 방향성이 뒤집히지 않도록 루트 기준 순서와 방향을 보존한다. 순환 조인 그래프에서는 스패닝 트리 간선만 방출한다.
- Union: WHERE(metric/segment/사용자 WHERE 및 `:bind` 값 반영)와 각 table `scope_filter`가 disjoint이면 해당 UNION 브랜치 제외.
- 출력: 최적화된 물리 테이블 집합·결합 관계

### Step 5: 물리 Table / Column 치환 및 바인드(`:name`) 안전 치환

- 등록된 `metric.expression` 및 `segment.filter.expression` 내부의 시맨틱 서브쿼리를 재귀 컴파일(`expand_semantic_subqueries`)하고, 외부 스코프 AST의 Dimension/Fact/Metric(윈도우 `OVER (PARTITION BY … ORDER BY …)` 내부 포함)을 `{schema}.{table}.{column}` 또는 물리 스칼라 `expression`으로 치환(VisitorMut — 서브쿼리 내부 스코프 보호, 누락 식별자 0). 다중 매핑(`mappings` 복수) 뷰 컬럼은 Step 4에서 선택된 물리 테이블 집합에 속한 매핑을 우선 선택한다.
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
  - `date_trunc('year'|'month'|'day'|'hour', expr)` → MySQL `DATE_FORMAT`/`DATE` · Oracle `TRUNC` · SQLite `datetime(…, 'start of …')`(단, SQLite에서 `'YYYY'`/`YYYY`/`'YYYY-MM'`/`'YYYYMM'`/`'YYYY-MM-DD'` 리터럴과 직접 비교·`BETWEEN`·`IN`될 때는 리터럴 포맷에 맞춰 `SUBSTR`/`strftime`/`date`로 자동 적응) · Postgres 유지. 미지원 단위 → `validation`.
  - 편의 스칼라 함수(`DIVIDE(a, b)`, `YEAR(d)`, `MONTH(d)`, `DAY(d)`, `SUBSTR`/`SUBSTRING`, `LENGTH`, `REPLACE`, `INSTR`, `IFNULL`, `IIF`, `STRFTIME`)를 허용하며, SQLite 타깃에서는 정수 나눗셈 절삭을 막기 위해 `/` 좌변을 `CAST(… AS REAL)`로 승격한다.
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
