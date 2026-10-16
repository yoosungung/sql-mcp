# NL2SQL 시멘틱 컴파일러 & MCP 서버 개발 명세서

## 1. 개요 및 아키텍처 목적

본 문서는 Rust(axum) 기반 HTTP MCP 서버와 내장 libSQL 시멘틱 카탈로그를 바탕으로, 자연어 질의 에이전트(DW Agent)의 가상 SQL(Semantic SQL)을 안전하고 최적화된 물리 DW SQL로 변환·실행하는 컴파일러 구현 명세서입니다.

```
[DW Query Agent]                [Coding Agent (Git)]
       │                                 │
       ▼ (execute_semantic_sql)          ▼ (JSON Schema 검증/동기화)
┌─────────────────────────────────────────────────────────────────┐
│ Rust HTTP MCP Server (axum + sqlparser-rs)                      │
│                                                                 │
│  [Embedded libSQL]                                              │
│   ├─ Git JSON 역정규화 카탈로그 (Tables, Views, Cubes, Metrics) │
│   └─ FTS5 자연어 컬럼/지표 검색 가상 테이블                     │
│                                                                 │
│  [Semantic SQL Compilation Pipeline (6 Steps)]                  │
│   1. AST Select 점검 & 인가 검증                                │
│   2. Metric/Segment 전개 & 필터/가산성(Additivity) 추출          │
│   3. View 라우팅 (요약 뷰 우선 선택 & 컬럼 커버리지 검증)       │
│   4. 테이블 가지치기 (Join 그래프 MST 추적 & Union Pruning)     │
│   5. 물리 Table/Column 치환                                     │
│   6. 보안/격리 필터 주입(푸시다운) & Encryption 마스킹          │
└─────────────────────────────────────────────────────────────────┘
                               │
                               ▼ (Connection Pool: JDBC/ODBC)
                     [Target Data Warehouse]
```

## 2. 임베디드 libSQL 카탈로그 스키마

Git 저장소 내 document.schema.json을 준수하는 JSON 파일들을 파싱하여 아래 libSQL 테이블에 동기화합니다.

``` SQL
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

-- 5. 자연어 탐색용 FTS5 가상 테이블
CREATE VIRTUAL TABLE meta_search_fts USING fts5(
    cube_name,
    column_name,
    kind, -- 'dimension', 'fact', 'metric'
    type_info,
    description,
    tokenize = 'unicode61'
);
```

## 3. 시맨틱 SQL 컴파일 파이프라인 명세 (6단계)

개발자는 sqlparser-rs를 활용하여 아래 단계를 순차적으로 실행하는 파이프라인을 작성해야 합니다.

### Step 1: 시맨틱 SQL의 SELECT 및 호출자 Role 점검

- 입력: caller_role, semantic_sql
- 동작:
  - sqlparser::parser::Parser::parse_sql(&GenericDialect, sql) 실행.
  - AST 최상위 노드가 Statement::Query 형태인지 확인하고, DDL/DML, 세션 변경문, 파일 시스템 접근문 발견 시 즉시 McpError::Forbidden 반환.
  - FROM 절의 대상 Cube를 식별하고, 호출자의 caller_role이 해당 Cube에 접근 가능한지 검증.
- 출력: 정제된 시맨틱 Query AST

### Step 2: 래핑(Metric, Segment) 전개 및 가산성(Additivity) 분석

- 동작:
  - Segment 치환: WHERE 절 내 세그먼트 식별자를 해당 meta_segments.filter_expression으로 치환.
  - Metric 전개:
    - meta_metrics.expression으로 SELECT/HAVING 절 치환.
    - meta_metrics.filter_expression이 존재할 경우, 이를 별도 컴파일 컨텍스트(Metric Filter Context)에 수집 (WHERE 절 AND 병합 또는 FILTER (WHERE ...) 절 생성 준비).
  - 가산성 플래그 설정: Metric의 집계 함수가 비가산적(COUNT(DISTINCT), AVG, MEDIAN)인지 판별하여, 요약 뷰 선택 시 집계 그레뉼러리티 불일치가 발생하지 않도록 플래그(is_semi_additive) 설정.
- 출력: Dimension과 Fact 기준으로 풀린 큐브 SQL AST + 추출된 Metric/Segment 필터 목록

### Step 3: 요약 뷰(Summary View) 우선 라우팅 및 커버리지 검증

- 동작:
  - 대상 Cube의 View가 combine: "select"이고 prefer: "summary"인 경우, 연결된 N개의 View 중 summary: true인 View 목록을 우선 추출.
  - 필수 컬럼 집합(Required Columns) 계산:
    - ReqCols=Projection Cols∪GROUP BY Cols∪WHERE Cols∪Metric Filter Cols
  - 커버리지 및 가산성 검증:
    - 요약 뷰의 meta_view_columns가 를 모두 포함하는지 확인.
    - 쿼리에 비가산적 메트릭이 포함된 경우, 요약 뷰의 사전 정의된 Group By 단위가 쿼리의 Group By 단위와 완전 일치하는지 검증 (불일치 시 해당 요약 뷰 탈락).
  - 조건을 만족하는 가장 세분화된(최적의) 요약 뷰를 선택. 만족하는 요약 뷰가 없으면 기본(원천) View 채택.
- 출력: 선택된 단일 View (join 또는 union 뷰)

### Step 4: 테이블 가지치기 (Join 그래프 최소 경로 & Union Pruning)

- Join View 처리 시:
  - 필요한 컬럼을 가진 물리 테이블 집합 $T_{req}$​ 도출.
  - meta_view_joins를 간선(Edge)으로 하는 그래프 구축.
  - $T_{req}$의 모든 노드를 연결하는 최소 신장 트리(Steiner Tree / Shortest Path)를 탐색하여 필요한 경유 테이블(Bridge Table)을 누락 없이 포함.
- Union View 처리 시:
  - WHERE 절의 조건식(예: 날짜 범위)과 각 Table의 scope_filter를 비교.
  - 조건이 겹치지 않는(Disjoint) 물리 테이블은 UNION ALL 브랜치에서 사전 제외.
- 출력: 최적화된 물리 테이블 집합 및 결합 관계 정의

### Step 5: 물리 Table / Column 치환

- 동작:
  - AST 상의 시맨틱 Dimension/Fact/Metric 별칭을 실제 물리 식별자인 {db}.{schema}.{table}.{column} 또는 물리 스칼라 표현식(expression)으로 1:1 치환.
- 출력: 물리 DW 식별자로 변환된 AST

### Step 6: 보안 격리 필터 주입(서브쿼리 푸시다운) & 마스킹 처리

- 동작:
  - 인라인 뷰(Derived Table) 푸시다운:
    - 테이블별 permission_filters (호출자 role 매칭식), scope_filter, default_filter를 바깥쪽 WHERE 절이 아닌 물리 테이블의 인라인 서브쿼리 내부로 주입하여 Outer Join 왜곡 및 옵티마이저 보안 유출 차단:
      - FROM (SELECT ∗ FROM db.schema.tbl WHERE [filters]) alias
  - Cube 레벨 필터 병합: Dimension/Fact에 정의된 default_filter를 메인 AST의 WHERE 절에 AND로 결합.
  - 마스킹(Encryption): encryption: true인 컬럼에 대해 호출자 권한이 없거나 마스킹 대상인 경우 프로젝션 표현식을 NULL 또는 '***'로 래핑.
- 출력: DW로 전달할 최종 물리 SQL 문자열

## 4. 개발 시 주의사항 (Checklist)

[ ] sqlparser-rs AST 조작 시 변환 누락된 식별자 노드가 없도록 Visitor 패턴 또는 재귀 순회(Folder 패턴)를 적용할 것.

[ ] LEFT/RIGHT Outer Join 발생 시 Table 레벨 필터를 메인 WHERE 절에 넣지 말고 반드시 인라인 뷰(Derived Table) 서브쿼리로 격리할 것.

[ ] encryption: true인 필드는 sample_cube_data 및 최종 응답 결과에서 평문이 노출되지 않도록 AST 레벨에서 마스킹 표현식으로 치환할 것.

[ ] libSQL 동기화 작업은 동시성 읽기 요청과 충돌하지 않도록 단일 Write 트랜잭션으로 처리할 것.
