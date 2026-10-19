# ROADMAP.md

수행 계획·순서·미결정 항목. 일정(일/주) 추정은 두지 않는다 — 마일스톤과 의존성으로만 나눈다.

## 목표

Rust MCP(`rmcp`) 서버: **임베디드 시맨틱 DB** + **복수 타깃 DB** 조회(SELECT) + **메타데이터 CRUD(JSON)**.  
Frontend / Admin API / **세션형 Streamable HTTP**는 **취소**. Transport는 **일반 HTTP**(기본) + **stdio**(로컬).

## 확정된 결정

| 항목 | 결정 |
|------|------|
| 시맨틱 DB | **MCP 프로세스에 임베드** (원격 시맨틱 서버 없음) |
| 시맨틱 엔진 | **libSQL** (임베디드) — 네이티브 벡터·FTS, 파일 하나로 M1 CRUD·이후 검색에 적합 |
| 시맨틱 등록 문서 | `table` · `view` · `cube` · `metric` · `segment`. 스키마·예시는 `semantic/` |
| 조회 대상 | **타깃 DB**, **복수** 허용 |
| 타깃 SQL | SELECT only |
| 메타 | MCP `metadata_*` CRUD, **JSON 바디**. 검증은 `semantic/schema/document.schema.json` |
| 컬럼 검색 | BM25/벡터/hybrid — 인덱스는 등록 JSON 투영(D12). 임베딩=외부 API, 없으면 강화 BM25(D14) |
| MCP 툴 | `list_databases`, `list_tables`, `describe_table`, `search_columns`, `sample_column`(cube+member), `execute_sql`(+as_code 조인), `metadata_*` — DW 표면 인자는 `database`(등록 문서 필드는 `target`) |
| Transport | **일반 HTTP**(기본) + **stdio**(로컬). 세션형 Streamable HTTP 취소 |
| Frontend / Admin API | 취소 |
| 개발 | TDD, 문서 우선 |
| metric·segment 식 (D16) | 새 DSL 없음 · `sqlparser` + AST 제한 |
| `encryption` (D17) | 응답 마스킹(암·복호화 아님) |
| 식 식별자 검사 (D18) | 저장=연결 그래프 · 컴파일=AST 식별자 |

## 시맨틱=libSQL 선택 이유

임베디드 + (장차) BM25/벡터 + 지금 당장 JSON 메타 CRUD에 **libSQL이 잘 맞음**.

- 확장(`sqlite-vec`) 없이 벡터·FTS 경로가 엔진에 있음
- 프로세스 내 파일 DB → `rmcp` 안에 심기 자연스러움
- 타입별 `meta_*` 카탈로그 + JSON CRUD 통로·검색 인덱스 투영이 가능
- 주의: hybrid는 앱 RRF. Turso 브랜드/API 이동은 있으나 M1 시맨틱은 `libsql` 핀(D2)
- 타깃 DB는 libSQL일 필요 없음(Postgres 등 별도, D13)

**임베딩 공급(D14):** 외부 OpenAI-호환 HTTP API. 벡터는 libSQL에 저장. API 없으면 **강화 BM25(keyword)만** — HashEmbedder·로컬 ONNX는 쓰지 않음.

## 미결정

| ID | 질문 | 옵션 / 비고 |
|----|------|-------------|
| D1 | 임베디드 시맨틱 엔진 | **결정: libSQL** |
| D2 | 클라이언트 | **결정:** 시맨틱=`libsql` 0.9 (`core`만) · 타깃=`sqlx` 0.9(+postgres,+mysql) · Oracle=`oracledb`(thin). **크레이트 분리**. Rust **1.94+** (`rust-toolchain.toml`) |
| D14 | 임베딩·검색 폴백 | **결정: 외부 API만 임베딩.** `SQL_MCP_EMBEDDING_URL`+`API_KEY` → vector/hybrid. **미설정 시 keyword(BM25)만**(vector/hybrid 요청도 keyword로 폴백). 오프라인 품질은 **BM25 강화**(한글 유지·문자 bigram·`as_code` 등 인덱스 텍스트). HashEmbedder=테스트 전용. 로컬 ONNX 비채택 |
| D7 | 배포 | **결정(M3): release 바이너리 + 컨테이너 이미지**. Kustomize 베이스는 `deploy/k8s/base/` |
| D9 | 코드사전 | **결정:** dimension `as_code` = `{table,key_column,value_column,where?}`. 집계 후 dim=key LEFT JOIN → `{dim}__value`. `sample_column`은 cube+member(사전 있으면 사전 샘플). 에이전트 기입·MCP 저장만 |
| D12 | 등록 스키마 ↔ 저장·검색 | **결정: JSON 교환=`document.schema.json` · 저장=타입별 `meta_*` · 검색 투영=`column_index`+FTS5(`bm25`)+embedding(hybrid RRF). 규칙은 `semantic/DESIGN.md`·`server/spec.md`** |
| D13 | 타깃 엔진 | **결정: Postgres(`sqlx`) · MySQL(`sqlx`) · SQLite(`libsql` 로컬) · Oracle(`oracledb` thin)**. sqlx에 `sqlite` feature를 넣지 않는다(번들 SQLite 이중 링크 방지). ADBC·JDBC/ODBC 비채택. Exadata=일반 Oracle. 테스트 타깃은 BIRD Mini-Dev SQLite (`target_db/`) |
| D15 | `metadata_update` | **결정: 전체 replace** (merge-patch 아님). `body`는 문서 JSON 전체 |
| D16 | metric·segment 식 문법 | **결정: 새 DSL 없음.** `sqlparser`로 파싱한 **신뢰된 SQL 조각** + AST 제한. metric `expression` = 스칼라/집계식만 · segment·filter = 단일 boolean 식만. 서브쿼리·다중 문장·쓰기·CTE 거부. 식별자: table 필터=물리 컬럼 · cube/metric/segment=dimension·fact 이름 |
| D17 | `encryption: true` | **결정: 암·복호화 아님 → 응답 마스킹.** SELECT/샘플/로그에서 해당 셀을 `null`(또는 `"***"`)로 치환. AES·토큰화는 타깃 DW 책임 |
| D18 | expression/filter 컬럼 검사 시점 | **결정: 이중 검사.** `metadata_*` = 연결 그래프 · 컴파일=AST 식별자. `execute_sql`은 시맨틱 컴파일 필수 |
| D19 | `execute_sql` 입력 | **결정: 시맨틱 SELECT만.** 툴명 유지. 자유 물리 SELECT 경로 없음 |
| D20 | table/view/cube `target` | **결정:** 등록 JSON에 MCP `target` id 필수. leaf table·view·cube 동일 target 강제. `list_tables`/`describe_table`=cube 표면. 같은 target cube JOIN/UNION 허용(다른 target 거절) |
| D21 | 타깃 SQL 방언(dialect) | **결정: Cube식 DialectProfile emit.** 컴파일은 엔진 중립(canonical ANSI `"…"` · `date_trunc`). emit: `date_trunc`→엔진별 식 · quote adapt · `wrap_row_limit`. **비채택:** 자유 SQL 트랜스파일 · `sqlparser` Display 방언 위임 · **실행 bind 스타일 rewrite**(ad-hoc OLAP만, 드라이버 prepared bind 경로 없음). 상세 [ARCHITECTURE.md](ARCHITECTURE.md) · [server/DESIGN.md](server/DESIGN.md) · [server/spec.md](server/spec.md) |
| D22 | 물리 vs 시맨틱 식별자 | **결정: 부분 완화.** `$defs/name`(시맨틱) 유지 · `$defs/physical_ident`(물리 db/schema/table/컬럼·mapping·join on·as_code key/value) 분리. view 컬럼·cube 멤버는 시맨틱 `name`만. 물리명에 공백·기호가 있으면 view 별칭 + `mappings.column`에 물리 유지. `quote_ident`는 동일 허용 집합. |
| D23 | 시맨틱 SELECT 모드 | **결정: 그레인 기반 2-모드(Grouped vs Ungrouped).** **(1) Grouped(집계):** 명명 metric · ad-hoc 집계(`sum/count/avg/min/max`) · metric 파생 연산 **동일 SELECT 혼용 허용** + 차원 스칼라 식(`date_trunc` 등) + 차원-only `GROUP BY`(+ `HAVING`/`ORDER BY` 집계식) 허용. grain = 명시 `GROUP BY`(별칭·위치·식 포함) / 프로젝션 차원 / 명명 metric `group_by` 또는 전역 집계. 명명 metric은 활성 차원이 자신의 `group_by`에 속하는지 개별 검증, ad-hoc은 전 차원 허용. 같은 `target` cube JOIN 허용(metric 없는 JOIN 측은 참조 차원 `GROUP BY` 서브쿼리로 팬아웃 방지). **(2) Ungrouped(행 상세):** metric·집계·`GROUP BY`·`HAVING` 없는 dim/fact(+비집계 스칼라 식) 프로젝션. `SELECT DISTINCT` 보존. 비-`DISTINCT` 상세 조회는 요약 뷰(`summary: true`) 우회(원천 뷰 선택) 및 다대다 팬아웃(Chasm Trap) 방지를 위해 cube JOIN 거절. 전 모드/절에서 fact·dim·metric·segment `role` 및 `encryption` 직접 검사. |
| D24 | metric `group_by` 의미 | **제안(미합의): 명시적 슬라이스 허용목록(allowlist)만.** 필드명 `group_by` 유지. 질의 grain은 **항상** 호출자 SQL(`GROUP BY` / 프로젝션 차원)이 정하고, 메타 `group_by`는 “이 metric과 함께 쓸 수 있는 차원 ⊆ 목록” 검증만 한다. 축 미지정 시 **전역 집계**(빈 `GROUP BY`) — 목록 전체를 기본 grain으로 쓰지 않는다. Cube multi-stage `group_by`(고정 grain) · MetricFlow linkable 유도 · 필수축(`required_group_by`)과 구분. 상세는 아래 «후속 — metric group_by = 슬라이스 allowlist (D24)». |

(제거: D5 세션형 Streamable HTTP, D10/D11 Frontend. stdio는 로컬용으로 복원)

## 마일스톤

### M0 — 문서·계약

- [x] MCP-only · 임베디드 libSQL · 복수 타깃 · 메타 CRUD(JSON) · **일반 HTTP** · **stdio(로컬)**
- [x] 시맨틱 등록 JSON Schema·예시·카탈로그 DDL (`semantic/`)
- [x] D2·D13·D15 합의 (D14는 M2)

### M1 — 타깃 조회 + 메타 CRUD (일반 HTTP)

- [x] `server/` + **libSQL** 임베디드 + HTTP 리스너 + `DESIGN.md` / Commands
- [x] `metadata_*` CRUD — `document.schema.json` 검증 + `document` 테이블 저장 · update=전체 replace(D15)
- [x] `list_databases`(구 `list_targets`) · 타깃 SELECT 가드 · `list_tables` / `describe_table` / `execute_sql` / `sample_column` · 엔진 Postgres·SQLite(D13) · DW 인자 `database`
- [x] `search_columns`는 스텁 또는 no-op 허용 — 인덱스 본구현은 M2
- [x] HTTP 클라이언트로 스모크 (`cargo test -p sql-mcp`)

**완료 기준:** 타깃 SELECT·탐색 + 메타 JSON CRUD가 **일반 HTTP**로 동작. (Postgres 실연결 스모크는 `http_smoke` ignore 테스트·DSN 환경 선택)

### M2 — 검색·컴파일

- [x] `search_columns` BM25(FTS5) / 벡터 / hybrid(RRF) · D14(API 또는 강화 BM25) · D9 휴리스틱
- [x] 메타 CRUD 시 `column_index` 재구축
- [x] table→물리 SQL 컴파일(필터 AND) + SELECT AST 가드 유지
- [x] CI (`.github/workflows/ci.yml`)
- [x] OpenAI-호환 임베딩 HTTP 클라이언트 배선 (`SQL_MCP_EMBEDDING_*`, 미설정 시 BM25만 · D14)
- [x] cube/metric/segment → 물리 SQL 컴파일 (`compile_metric_select`)
- [x] D16 식 AST 제한 · D18 컴파일 식별자 검사 · D17 encryption 마스킹
- [x] 타깃 엔진 SQLite (`libsql`) · BIRD Mini-Dev → `target_db/`

### M3 — 배포·하드닝

- [x] (D7) release 바이너리 + 컨테이너 · `deploy/SETUP.md` · `deploy/k8s/base/`
- [x] `/health` 프로브 · tracing 로그
- [x] D16–D18 본구현 (`expr_guard` · `mask`)
- [x] MCP Resource로 `document.schema.json` 제공 (`resources/list` · `resources/read`)
- [x] Agent Skills 정본 `skills/` + MCP Resource 노출(`https://sql-mcp.local/skills/…`); Cursor용 `.cursor/skills` 심볼릭
- [x] MCP Skills extension: `skills/list` · `skills/get` + capabilities `io.modelcontextprotocol/skills` (`directoryRead` 미구현)
- [x] 타깃 MySQL(`sqlx`) · Oracle(`oracledb` thin) (D13; ADBC 비채택)
- [x] 운영 하드닝(전 계층 RBAC·타임아웃·읽기 전용 메타 모드·에러 마스킹)

### M4 — 시맨틱 컴파일러 · 타입별 카탈로그

- [x] 타입별 `meta_*` 저장 + `metadata_*` JSON 분해/재조립
- [x] `execute_sql` = 시맨틱 SQL → `pipeline` → 물리 SELECT (자유 물리 SELECT 삭제, D19)
- [x] keyword = FTS5/`bm25` · vector/hybrid 유지 (D12)
- [x] 요약 뷰 라우팅 (`combine=select` + `prefer=summary`) · join Steiner 근사 pruning · 인라인 뷰 필터 푸시다운 · encryption 프로젝션 NULL
- [x] union/minus 컴파일 · union pruning(`1=0` scope · 미매핑 source 제거)
- [x] metric/view 식별자 치환을 sqlparser `VisitorMut` AST 경로로 이관 (`ast_rewrite`)
- [x] metric SELECT AST 조립(`ast_select`) · 날짜 리터럴 구간 union disjoint(`date_range`)
- [x] bind-param 구간 비교(`params` → `:name` 치환) · OR(DNF) 날짜식 · `execute_sql.params`

### 후속 — 동일 target cube JOIN/UNION

- [x] semantic SQL에서 같은 `target`인 cube끼리 JOIN / UNION(·EXCEPT/INTERSECT) (다른 target 거절; Grouped·`DISTINCT` JOIN 허용, 비-`DISTINCT` Ungrouped JOIN은 팬아웃 방지로 거절)

### 후속 — execute_sql dry_run

- [x] `execute_sql.dry_run`: 시맨틱 컴파일·인가만 하고 타깃 SELECT 생략, `{ sql, dry_run: true }` 반환

### 후속 — 타깃 DialectProfile (D21)

- [x] `DialectProfile`: `quote_ident` · `adapt_compiled_sql` · `wrap_row_limit` — `TargetConfig.engine`으로 선택
- [x] `pipeline`/`sample`/`tools`에 프로필 주입 · 타깃 LIMIT은 `wrap_row_limit`
- [x] Postgres·SQLite·MySQL·Oracle 골든 테스트(quote · MySQL adapt · `LIMIT`/`FETCH`)
- [x] `date_trunc` rewrite (`year`/`month`/`day`/`hour`) · `emit_physical_sql`
- [x] 실행 bind rewrite **비채택**(ad-hoc OLAP)

### M5 — 컴파일러 완결성 · 전 계층 보안(RBAC) · 카탈로그/엔진 하드닝

- [x] 시맨틱 SELECT 전체 절 지원: 복수 metric · 선택 dimension 동적 `GROUP BY` · `ROLLUP`/`CUBE`/`GROUPING SETS` · 일반 `WHERE` 조건식 보존 · `HAVING` / `ORDER BY` / `LIMIT` / `OFFSET`
- [x] `:name` 바인드 파라미터 안전 리터럴 치환(쿼트 이스케이프) 및 실행 시 미해석 `:param` 거절
- [x] Cube JOIN `ON` 식 AST 검증·치환 및 Join Pruning 기준 테이블(`sources[0]`) 고정·Outer Join 방향 보존
- [x] 반가산(Semi-additive) metric 요약 뷰 그레뉼러리티 일치 검증 수정
- [x] 전 계층(`cube`·`table`·`dimension`·`fact`·`metric`·`segment`) `role` 접근 제어 + `permission_filters` Fail-Closed + `sample_column` 인라인 보안 필터 + 큐브 스코프 `encryption` 마스킹·수식/필터 차단
- [x] FTS5 `bm25()` 음수 랭크 점수 정규화 · `column_index.target` 및 `search_columns(target)` 필터링
- [x] 인메모리 카탈로그 스냅샷 캐시 + 단일 Write 트랜잭션 내 증분 임베딩 갱신
- [x] `SQL_MCP_TIMEOUT_MS` 타깃 쿼리 타임아웃 강제 · Postgres/MySQL `NUMERIC`/`DECIMAL`/`DATE`/`TIMESTAMP` 디코딩 · DSN 마스킹 상세 에러 · `SQL_MCP_MODE`

### 후속 — 그레인 기반 2-모드 시맨틱 SELECT 개편 (D23)

- [x] **Grouped 모드 통합:** 명명 metric · ad-hoc 집계 · metric 파생 연산 **동일 SELECT 혼용 허용** (단일 큐브 및 Cube JOIN 모두)
- [x] **차원 스칼라 식 & 차원-only GROUP BY:** `date_trunc` 등 비집계 스칼라 식 프로젝션/그룹핑(별칭·`GROUP BY 1` 위치 참조 포함) 및 metric 없는 차원 `GROUP BY`(+ `HAVING`/`ORDER BY` 집계식) 지원
- [x] **`HAVING` · `ORDER BY` 확장:** `HAVING` 내 집계 함수(`COUNT(*)`, `SUM(...)` 등) 허용(`assert_having_expr`) · `ORDER BY`에 별칭·위치·멤버·스칼라 식·집계식 직접 허용
- [x] **Ungrouped 상세 모드 가드:** 비집계 스칼라 식(`a * b`, `a / b` 등) 및 `SELECT DISTINCT` 지원 · 비-`DISTINCT` 상세 조회 시 요약 뷰(`summary: true`) 우회(원천 뷰 라우팅) · 다대다 팬아웃(Chasm Trap) 방지를 위해 비-`DISTINCT` Ungrouped Cube JOIN 거절(Grouped Cube JOIN에서 metric 없는 측은 참조 차원 `GROUP BY` 서브쿼리로 팬아웃 방지)
- [x] 계약·파이프라인 문서(`ARCHITECTURE.md` · `server/spec.md` · `server/DESIGN.md`)와 TDD

### 후속 — metric `group_by` = 슬라이스 allowlist (D24)

#### 현황(문제)

지금 `metric.group_by`는 **두 역할**을 겸한다.

1. **Allowlist:** 활성 차원 ⊆ `group_by` (컴파일러 `dimension X not in metric group_by`)
2. **Default grain:** 질의에 `GROUP BY`/프로젝션 차원이 없고 명명 metric만 있으면, 복수 metric `group_by`의 **교집합 전체**를 유효 `GROUP BY`로 씀 (`compile/mod.rs` `active_gb`)

문서도 갈라져 있다: skills/contract는 “허용목록+기본축”, `ARCHITECTURE.md`·`document.schema.json`은 “GROUP BY에 쓰는 view 컬럼”. Cube의 동명 `group_by`는 multi-stage **고정 grain**이라 혼동 여지가 크다.

#### 목표

`group_by`를 **명시적 슬라이스 허용목록**으로 단일화한다. “이 metric을 어떤 차원으로 자를 수 있는가”만 메타에 두고, **실제로 어떻게 자를지(grain)는 시맨틱 SQL이 명시**한다.

#### 옵션

| 옵션 | 요지 | 장점 | 단점 |
|------|------|------|------|
| **A (권장)** | 필드명 `group_by` 유지. 의미=allowlist only. 축 미지정 → 전역 집계. 기본 grain 필드 없음 | 스키마 변경 최소 · Cube 고정-grain과 이름만 같고 역할이 문서상 분리됨 · 질의 의 grain과 일치 | bare `SELECT metric` 행동 변경(브레이킹) · 기존 메타가 “기본축”으로 쓰이던 기대 깨짐 |
| B | A + 선택 필드 `default_group_by` (기본축 분리) | 브레이킹을 선택적으로 흡수 | 필드 2개 · 작성 부담 · skills/스키마/검증 복잡도↑ |
| C | 필드 개명(`slice_by` / `allowed_dimensions`) + allowlist only | Cube `group_by`와 이름 충돌 제거 | 전 메타·예시·에이전트 JSON 대량 rename · 이득 대비 침습 큼 |
| D (비채택) | allowlist 제거, MetricFlow식 linkable만 유도 | 작성 부담↓ | 그레인 가드 약화 · 현 컴파일러/등록 모델과 거리 큼 |

**권장: A.** 필수축이 필요해지면 이후 `required_group_by`(MetricFlow #2107류)를 **별도**로 검토 — allowlist와 반대 방향(must vs may)이므로 D24에 섞지 않는다.

#### 제안 계약(합의 후 ARCHITECTURE §1/§4에 반영)

- `aggregation=group` ⇒ `group_by: string[]` **필수**, 각 원소 ⊆ 연결 view 컬럼(현 links 검증 유지).
- 의미: 이 metric과 함께 `GROUP BY`/차원 프로젝션에 쓸 수 있는 **허용 차원 집합**. 질의 활성 차원의 식별자 ⊆ 목록(교집합이 아니라 **각** named metric 목록에 속해야 함 — 현 검증 유지).
- **Default grain 제거:** 명시 `GROUP BY`도 프로젝션 차원도 없으면 `active_gb = []`(전역 집계). 복수 metric이어도 교집합을 자동 GROUP BY로 펼치지 않는다.
- ad-hoc 집계: 계속 전 차원 허용(또는 cube dim 전체 allowlist 가상 — 현 `materialize_adhoc_metrics`와 동등). D24는 **명명 metric** 메타 의미만 바꾼다.
- `rollup`/`cube`/`grouping_sets`: `group_by` 필드 금지(현 스키마) 유지. 슬라이스 목록과 SQL 그룹핑 확장자는 별개.
- 빈 배열: 스키마 `minItems: 1` 유지(전역-only metric은 축을 쓰지 않는 질의만 허용하려면 별도 합의; 기본은 “최소 1개 슬라이스 가능 차원” 작성 강제).

#### 구현 순서 (TDD · 합의 후)

1. **문서(계약 먼저):** `ARCHITECTURE.md` metric `group_by` 문단 · D23 grain 문장의 “명명 metric `group_by` 또는 전역 집계” → “명시 grain 없으면 전역 집계; `group_by`는 allowlist” · `document.schema.json` description · `server/spec.md` Step 2 Group By 결정 · skills `semantic-cube-metric-segment` / `semantic-from-qa` / `semantic-register` contract · 예시 JSON은 목록을 “슬라이스 가능 차원 전체”로 재기술(값 자체는 넓게 유지 가능).
2. **실패 테스트:** `SELECT <named_metric> FROM cube`(+group_by에 여러 차원)가 **더 이상** 그 목록으로 `GROUP BY`하지 않고 전역 집계 SQL을 내는지 · `SELECT dim, metric … GROUP BY dim`에서 dim∉목록이면 기존처럼 거절 · dim∈목록이면 통과.
3. **구현:** `compile/mod.rs`의 `else if !named_metric_gbs.is_empty() && !has_adhoc_metrics { common_gb… }` 분기를 제거(또는 빈 vec). allowlist 검증 루프는 유지. ad-hoc ephemeral metric의 `group_by: allowed_gb`는 “가상 allowlist”로 두되 default grain 경로에 의존하지 않게 정리.
4. **회귀:** pipeline/compile 골든 · BIRD/client 예시 질의 중 bare metric 의존분 점검 · `describe_table`이 `group_by`를 그대로 노출하는지는 스키마 설명만 갱신.
5. **메타 마이그레이션 가이드(문서):** 기존 JSON의 `group_by` 배열은 **삭제하지 말고** 허용 슬라이스로 재해석. “기본 리포트 축”이 필요하면 질의에 `GROUP BY`/`SELECT dim, metric`을 명시. 좁은 `group_by`(1~2개)로 작성된 범용 metric은 슬라이스 실패가 나므로 skills 가이드대로 그레인 호환 차원 전체를 채우도록 재등록.

#### 완료 체크리스트

- [ ] D24 합의(권장 A 확정 또는 B/C 채택) 후 위 표를 **결정:** 으로 갱신
- [ ] 계약·스키마·skills·spec 문구 정합
- [ ] 컴파일러 default-grain 제거 + allowlist 검증 유지 (TDD)
- [ ] 예시/타깃 메타·에이전트 가이드에 bare metric = 전역 집계 명시
- [ ] Cube/MetricFlow와의 의미 차이를 `ARCHITECTURE.md` 또는 skills에 한 줄 교차 참조(동명 `group_by` 혼동 방지)

## 구현 시 작업 규칙

1. 계획 → 이 파일 · 계약 → `ARCHITECTURE.md` · 등록 스키마 → `semantic/` · 서버 내부 → `server/DESIGN.md`
2. 스켈레톤 → 테스트 → 구현
