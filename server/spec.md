# NL2SQL 시멘틱 컴파일러 & MCP 서버 개발 명세서

시맨틱 컴파일 파이프라인·타입별 libSQL 카탈로그의 **상세 설계**다.  
컴포넌트 간 계약은 [ARCHITECTURE.md](../ARCHITECTURE.md), 마일스톤은 [ROADMAP.md](../ROADMAP.md), 서버 모듈/Commands는 [DESIGN.md](DESIGN.md).

## 0. 확정 전제 (계획과 동일)

1. **카탈로그는 타입별 `meta_*` 테이블**에 저장한다. `document(kind,name,body)` 단일 blob 정본은 쓰지 않는다. MCP `metadata_*` **입출력은 JSON**(`document.schema.json`)이며, 저장 시 타입별 행으로 분해·조회 시 JSON으로 재조립한다.
2. **타깃 질의는 시맨틱 SELECT만.** 자유 물리 SELECT 경로는 없다. 공개 툴 `execute_sql`의 입력은 시맨틱 SQL이고, 6단계 컴파일 후에만 물리 SQL이 생성·실행된다. (문서·다이어그램의 `execute_semantic_sql`은 **이명** — 공개 툴명을 바꾸지 않는다.)
3. **툴 표면 유지:** `execute_sql`, `sample_column`, `search_columns`, `metadata_*` 등 현행 이름. `sample_cube_data`는 `sample_column`의 이명이다.
4. **타깃 접속:** Postgres=`sqlx`, SQLite=`libsql`(로컬). JDBC/ODBC는 쓰지 않는다.
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

`execute_sql` → 파이프라인(필수) → 컴파일된 물리 SQL만 타깃에 전달. 클라이언트가 넘긴 문자열을 SELECT 가드만 하고 타깃에 넣는 경로는 없다.

## 2. 임베디드 libSQL 카탈로그 스키마

`metadata_*`로 받은 JSON(`document.schema.json`)을 검증·연결 검사한 뒤, **단일 Write 트랜잭션**으로 아래 타입별 테이블에 반영한다. CRUD 성공 시 FTS·embedding 인덱스를 같은 트랜잭션(또는 동일 성공 경로)에서 재구축한다.

```sql
-- 1. Table 계층
CREATE TABLE meta_tables (
    name TEXT PRIMARY KEY,
    description TEXT,
    db TEXT NOT NULL,
    schema TEXT NOT NULL,
    table_name TEXT NOT NULL,
    pk_json TEXT,                     -- JSON Array: ["id"]
    scope_filter TEXT,                -- NULL 가능
    default_filter TEXT,              -- NULL 가능
    role_json TEXT DEFAULT '"all"'    -- "all" 또는 JSON Array: ["role_a"]
);

CREATE TABLE meta_table_columns (
    table_name TEXT REFERENCES meta_tables(name),
    name TEXT NOT NULL,
    type TEXT NOT NULL,
    nullable BOOLEAN NOT NULL,
    as_code BOOLEAN DEFAULT 0,
    PRIMARY KEY (table_name, name)
);

CREATE TABLE meta_table_permission_filters (
    table_name TEXT REFERENCES meta_tables(name),
    role TEXT NOT NULL,
    expression TEXT NOT NULL
);

-- 2. View 계층 (join / union / minus / select)
CREATE TABLE meta_views (
    name TEXT PRIMARY KEY,
    description TEXT,
    combine TEXT CHECK (combine IN ('join', 'union', 'minus', 'select')),
    prefer TEXT CHECK (prefer IS NULL OR prefer = 'summary')
);

CREATE TABLE meta_view_sources (
    view_name TEXT REFERENCES meta_views(name),
    kind TEXT CHECK (kind IN ('table', 'view')),
    source_name TEXT NOT NULL,
    summary BOOLEAN DEFAULT 0
);

CREATE TABLE meta_view_joins (
    view_name TEXT REFERENCES meta_views(name),
    left_source TEXT NOT NULL,
    right_source TEXT NOT NULL,
    join_type TEXT CHECK (join_type IN ('inner', 'left', 'right', 'full')),
    on_conditions_json TEXT NOT NULL  -- JSON: [{"left_column": "a", "right_column": "b"}]
);

CREATE TABLE meta_view_columns (
    view_name TEXT REFERENCES meta_views(name),
    name TEXT NOT NULL,
    mappings_json TEXT NOT NULL,      -- JSON: [{"source": "t1", "column": "col1"}]
    PRIMARY KEY (view_name, name)
);

-- 3. Cube 계층
CREATE TABLE meta_cubes (
    name TEXT PRIMARY KEY,
    view_name TEXT REFERENCES meta_views(name),
    description TEXT NOT NULL
);

CREATE TABLE meta_cube_dimensions (
    cube_name TEXT REFERENCES meta_cubes(name),
    name TEXT NOT NULL,
    type TEXT CHECK (type IN ('code', 'date', 'time', 'datetime', 'string')),
    column_ref TEXT,
    expression TEXT,
    default_filter TEXT,
    encryption BOOLEAN NOT NULL,
    role_json TEXT DEFAULT '"all"',
    PRIMARY KEY (cube_name, name)
);

CREATE TABLE meta_cube_facts (
    cube_name TEXT REFERENCES meta_cubes(name),
    name TEXT NOT NULL,
    amount BOOLEAN NOT NULL,
    fx BOOLEAN NOT NULL,
    column_ref TEXT,
    expression TEXT,
    default_filter TEXT,
    encryption BOOLEAN NOT NULL,
    role_json TEXT DEFAULT '"all"',
    PRIMARY KEY (cube_name, name)
);

-- 4. Metric & Segment 계층
CREATE TABLE meta_metrics (
    name TEXT PRIMARY KEY,
    cube_name TEXT REFERENCES meta_cubes(name),
    description TEXT,
    expression TEXT NOT NULL,
    aggregation TEXT CHECK (aggregation IN ('group', 'rollup', 'cube', 'grouping_sets')),
    group_by_json TEXT,
    filter_expression TEXT,
    role_json TEXT DEFAULT '"all"'
);

CREATE TABLE meta_segments (
    name TEXT PRIMARY KEY,
    cube_name TEXT REFERENCES meta_cubes(name),
    description TEXT,
    filter_expression TEXT NOT NULL,
    role_json TEXT DEFAULT '"all"'
);

-- 5. keyword 검색용 FTS5 (순위: bm25()). vector embedding은 별도 side table.
CREATE VIRTUAL TABLE meta_search_fts USING fts5(
    cube_name,
    column_name,
    kind, -- 'dimension', 'fact', 'metric', …
    type_info,
    description,
    tokenize = 'unicode61'
);
```

`search_columns` 모드: `keyword`(FTS5/`bm25`) · `vector` · `hybrid`(RRF). 임베더 없으면 vector/hybrid → keyword 폴백(D14).

## 3. 시맨틱 SQL 컴파일 파이프라인 명세 (6단계)

`execute_sql`이 sqlparser-rs로 아래를 **순차** 실행한다. 자유 물리 SQL 우회는 없다.

### Step 1: 시맨틱 SQL의 SELECT 및 호출자 Role 점검

- 입력: `caller_role`, 시맨틱 SQL(`execute_sql`의 `sql`)
- 동작:
  - `Parser::parse_sql(&GenericDialect, sql)` 실행.
  - AST 최상위가 `Statement::Query`인지 확인하고, DDL/DML·세션 변경·파일 접근 등은 즉시 거절.
  - FROM의 Cube를 식별하고 `caller_role` 접근 가능 여부를 검증.
- 출력: 정제된 시맨틱 Query AST

### Step 2: 래핑(Metric, Segment) 전개 및 가산성(Additivity) 분석

- 동작:
  - Segment 치환: WHERE의 세그먼트 식별자 → `meta_segments.filter_expression`.
  - Metric 전개: SELECT/HAVING → `meta_metrics.expression`; `filter_expression`은 Metric Filter Context에 수집(WHERE AND 또는 `FILTER (WHERE …)` 준비).
  - 가산성: `COUNT(DISTINCT)` / `AVG` / `MEDIAN` 등이면 `is_semi_additive` — 요약 뷰 그레뉼러리티 불일치 방지.
- 출력: Dimension·Fact 기준 큐브 SQL AST + Metric/Segment 필터 목록

### Step 3: 요약 뷰(Summary View) 우선 라우팅 및 커버리지 검증

- 동작:
  - Cube의 View가 `combine: "select"`이고 `prefer: "summary"`이면 `summary: true` source를 우선.
  - ReqCols = Projection ∪ GROUP BY ∪ WHERE ∪ Metric Filter 컬럼.
  - 요약 뷰의 `meta_view_columns`가 ReqCols를 커버하는지 확인. 비가산 metric이면 요약 뷰 Group By 단위가 질의와 완전 일치할 때만 채택.
  - 조건을 만족하는 최적 요약 뷰를 고르고, 없으면 기본(원천) View.
- 출력: 선택된 단일 View (join 또는 union)

### Step 4: 테이블 가지치기 (Join 그래프 최소 경로 & Union Pruning)

- Join: 필요 컬럼의 \(T_{req}\) + `meta_view_joins` 그래프로 연결 최소 집합(Steiner / Shortest Path 근사) — bridge table 누락 금지.
- Union: WHERE와 각 table `scope_filter`가 disjoint이면 해당 UNION 브랜치 제외.
- 출력: 최적화된 물리 테이블 집합·결합 관계

### Step 5: 물리 Table / Column 치환

- AST의 Dimension/Fact/Metric을 `{db}.{schema}.{table}.{column}` 또는 물리 스칼라 `expression`으로 치환(Visitor/Folder — 누락 식별자 0).
- 출력: 물리 식별자 AST

### Step 6: 보안 격리 필터 주입(서브쿼리 푸시다운) & 마스킹

- 테이블별 `permission_filters`(role 매칭) · `scope_filter` · `default_filter`를 바깥 WHERE가 아니라  
  `FROM (SELECT * FROM db.schema.tbl WHERE [filters]) alias` 인라인 뷰로 주입 (Outer Join 왜곡·보안 유출 방지).
- Dimension/Fact `default_filter`는 메인 WHERE에 AND.
- `encryption: true`면 프로젝션을 `NULL` 또는 `'***'`로 래핑. 응답·`sample_column`에서도 평문 금지(D17).
- 출력: 타깃으로 보낼 최종 물리 SQL 문자열

## 4. 개발 시 주의사항 (Checklist)

- [ ] sqlparser AST 변환 시 Visitor/Folder로 식별자 누락 없이 순회할 것.
- [ ] LEFT/RIGHT Outer Join에서 table 필터는 메인 WHERE가 아니라 인라인 뷰로 격리할 것.
- [ ] `encryption: true`는 `sample_column`·최종 응답에서 평문 노출 금지(AST 마스킹 + 응답 마스킹).
- [ ] meta_* / FTS / embedding 재구축은 동시성 읽기와 충돌하지 않도록 **단일 Write 트랜잭션**으로 처리할 것.
- [ ] `execute_sql`에 자유 물리 SQL 우회 경로를 두지 말 것.
- [ ] JDBC/ODBC를 도입하지 말 것 — 타깃은 Postgres=`sqlx`, SQLite=`libsql`만.
