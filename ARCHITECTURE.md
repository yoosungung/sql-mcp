# ARCHITECTURE.md

컴포넌트 *간* 계약과 인터페이스 형태. 내부 구현은 `server/DESIGN.md`(코드 도입 시)에 둔다. 등록 JSON 필드 정본은 `semantic/schema/document.schema.json`이다.

## 1. 계약사항 (불변 규칙)

1. **임베디드 시맨틱 DB (libSQL).** 시맨틱 저장소는 **MCP 서버(`rmcp`/process) 안에 libSQL로 임베드**한다(별도 원격 시맨틱 서버 없음). 파일 경로 등은 기동 설정. (장차) BM25/벡터 인덱스는 이 DB에 둔다.
2. **시맨틱 등록 문서.** 코딩 에이전트가 등록하는 JSON은 `table`, `view`, `cube`, `metric`, `segment`다. 문서 하나의 정본은 `semantic/schema/document.schema.json`이다. 코딩 에이전트는 그 스키마와 이 계약의 연결 규칙을 보고 검증한다. MCP `metadata_*`도 같은 스키마로 검증한다.
3. **타깃 DB (복수).** 실제 DW/조회 대상은 **타깃 DB**라 부른다. **복수 등록**을 허용한다. `execute_sql`·샘플 등 데이터 조회는 타깃 DB로 간다. `table` · `view` · `cube` 등록 문서에 MCP 연결 id인 **`target`**을 둔다(설정 `TargetConfig.id`와 동일). view·cube가 닿는 모든 leaf table의 `target`은 서로·문서와 **같아야** 한다. table의 `db`는 그 타깃 *안*의 물리 DB 이름이다. 같은 `target`인 cube끼리만 semantic **JOIN / UNION**(·EXCEPT/INTERSECT)을 허용한다. target이 다르면 거절한다. view 안 table join/union은 기존과 같다. **DW 에이전트 MCP 표면**에서는 이 연결을 **`database`**라 부르며(`list_databases`의 `name` = `TargetConfig.id` = 문서 `target`), 조회 툴 인자는 `database`다.
4. **언어.** MCP 서버는 **Rust** + [`rmcp`](https://crates.io/crates/rmcp) (MCP spec `2026-07-28`).
5. **범위.** **MCP 서버만.** Frontend·Admin UI·Admin API·**세션형 Streamable HTTP**는 **취소(범위 밖)**. Transport는 **일반 HTTP**(배포·원격)와 **stdio**(로컬 자식 프로세스). 둘 다 동일 툴 표면.
6. **타깃 SQL = 시맨틱 SELECT만.** `execute_sql` 입력은 **시맨틱 SQL**(cube FROM · metric/dimension/segment/fact)이며, 서버가 6단계 파이프라인으로 물리 SELECT를 만든 뒤에만 타깃에 실행한다. **자유 물리 SELECT 경로는 없다(D19).** FROM은 큐브 식별자만 쓰되, 에이전트 물리 SQL 습관(`FROM cube.physical` · `FROM cube AS T1`)은 **첫 식별자를 큐브 이름으로 정규화**해 동일 큐브로 라우팅한다(후행 물리 세그먼트는 무시; 별칭은 멤버 한정자로 유지). 시맨틱 SELECT는 **그레인 기반 2-모드(D23)**로 동작한다 — **(1) Grouped(집계) 모드:** 명명 metric·ad-hoc 집계(`sum/count/avg/min/max`)·metric 간 파생 연산·윈도우 함수(`OVER (PARTITION BY … ORDER BY …)`)를 **한 SELECT에서 자유롭게 혼용**할 수 있으며, `date_trunc` 등 차원 스칼라 식과 차원-only `GROUP BY`(+ `HAVING`/`ORDER BY` 집계식)도 허용한다. 같은 `target`인 cube JOIN을 허용하며, metric이 없는 JOIN 측도 참조 차원(`dims ∪ on_dims`)으로 `GROUP BY` 서브쿼리를 만들어 다대다 팬아웃(Chasm Trap)을 차단한다. **(2) Ungrouped(행 상세) 모드:** metric/집계·`GROUP BY`·`HAVING`이 모두 없는 순수 dimension·fact(+비집계 스칼라·윈도우 식) 조회다. `SELECT DISTINCT`를 지원하며, 비-`DISTINCT` 상세 행 조회는 사전 집계된 요약 뷰(`summary: true`)로 라우팅하지 않고 원천 뷰만 선택한다. 비-`DISTINCT` Ungrouped 모드의 cube JOIN은 다대다 행 폭발 방지를 위해 거절한다(`DISTINCT` 또는 Grouped 모드만 cube JOIN 허용). 질의의 모든 식에 등장하는 fact/dim/metric/segment의 `role`과 `encryption` 가드를 직접 검사한다. 컴파일·실행 결과는 단일 SELECT(읽기 CTE 허용)만. INSERT/UPDATE/DELETE/DDL/다중 문장/쓰기 CTE 거부. 행 상한·타임아웃 강제. (문서상 `execute_semantic_sql`은 `execute_sql` 이명.)
7. **타깃 방언은 emit 어댑터(D21·D25).** 시맨틱 컴파일은 **엔진 중립**(canonical ANSI `"…"` · `date_trunc` · 표준 날짜 편의 함수). `DialectProfile`이 emit에서 `date_trunc`/날짜 편의 함수 rewrite · quote adapt · 행 상한 래퍼를 적용한다. 자유 SQL 트랜스파일·`sqlparser` Display 방언 위임·**실행 bind rewrite는 하지 않는다**(ad-hoc OLAP SELECT만).
8. **메타데이터 CRUD (예외).** 임베디드 시맨틱 저장소에 한해 MCP 툴로 **CRUD**를 제공한다. 요청/응답 바디는 **JSON**. 타깃 DB 쓰기가 아니다.
9. **MCP 제공 능력.** (1) **database** 목록(`list_databases`) · **cube** 목록/상세(`list_tables`/`describe_table`) (2) 컬럼 검색(BM25/벡터/hybrid) (3) 컬럼 샘플(+code) (4) database SELECT SQL (5) **메타데이터 CRUD(JSON)** (6) **Resource:** 등록 문서 JSON Schema(`document.schema.json`) · **Agent Skills**(`skills/` 트리를 Resource로 제공). 그 외 툴·Resource는 기본 표면에 넣지 않는다. DW 에이전트에게는 메타 CRUD 도구를 주지 않는다. 이 에이전트에게 데이터베이스는 `list_databases` 항목이고, 테이블은 cube이며, 컬럼은 dimension, fact, metric이다.
10. **시크릿.** 로그·에러·툴 응답에 DSN/비밀번호를 실지 않는다. 타깃 자격 증명은 기동 설정으로만 주입(채팅으로 비밀번호 받지 않음). 메타 CRUD JSON에도 시크릿 필드를 넣지 않는다.
11. **문서 우선 · TDD.**

테이블 필터의 식은 물리 컬럼 이름을 쓴다. cube, metric, segment의 식은 그 cube의 dimension, fact 이름을 쓴다. 필터·metric·segment 식은 코딩 에이전트가 작성한 **신뢰된 SQL 조각**이며 새 DSL을 두지 않는다(D16). 서버는 `sqlparser`로 파싱한 뒤 AST를 제한한다: metric `expression`은 스칼라·집계·윈도우 식(등록된 `metric.expression`에는 읽기 전용 `SELECT` 서브쿼리 허용), segment·metric `filter`는 단일 boolean 식(등록된 `segment.filter.expression`·`metric.filter.expression`에는 `IN (SELECT …)`/`EXISTS (SELECT …)` 등 읽기 전용 서브쿼리 허용), `HAVING`은 집계·metric·차원 비교 boolean 식만. 호출자의 ad-hoc SQL(`SELECT`/`WHERE`/`HAVING`/`ORDER BY`) 내 직접 서브쿼리 및 다중 문장·쓰기·쓰기 CTE는 거부한다.

컴파일할 때 아래 필터를 모두 AND로 붙인다.

1. 선택된 view가 닿는 각 table의 `scope_filter`
2. 호출자 role(`caller_role`, 미지정 시 `SQL_MCP_DEFAULT_ROLE`)과 같은 table `permission_filters`
3. 각 table의 `default_filter`
4. 질의에 쓰인 metric의 `filter` (단일 metric이고 무필터 ad-hoc이 없으면 WHERE, 복수 metric이거나 필터 유무가 서로 다르면 `CASE WHEN filter THEN … END` 집계 내부 조건)
5. 질의에 쓰인 segment의 `filter` 및 사용자 `WHERE`의 일반 dimension/fact 조건식

table, cube, dimension, fact, metric, segment의 `role`을 생략하거나 `all`이면 모든 role이 볼 수 있다. table `permission_filters`의 `role`은 그 행 필터가 적용되는 역할이다. role 이름 배열이면 호출자는 그 중 하나의 role을 가져야 하며, 질의가 직접 참조하는 모든 계층(cube, table, 직접 조회·필터링·정렬·그룹핑·샘플링한 dimension/fact, metric, segment)에 대해 접근 권한을 검증한다(명명 metric 수식 내부의 fact는 해당 metric의 `role`을 따른다). `encryption`이 true인 dimension·fact는 **암·복호화하지 않고** 툴 응답·샘플·로그에서 원문을 내지 않는다 — 프로젝션·집계식·셀 값은 `null`(또는 `"***"`)로 마스킹하고 필터·정렬·그룹·조인 조건 사용을 거부한다(D17). AES·토큰화는 타깃 DW 책임이다.

식 안의 컬럼/멤버 이름 검사는 **이중**이다(D18). `metadata_*` 저장 시에는 연결 그래프만 검증한다(`column`·`group_by`·cube/view 존재 등; `expression`으로 만든 dimension·fact의 view 컬럼 연결 검사는 하지 않는다). 식 본문의 식별자는 **컴파일 직전** AST에서 알려진 dimension·fact·(해당 시) metric/view/물리 컬럼으로 해석 가능한지 검사하고, 실패하면 `validation`으로 거절한다. `execute_sql`은 시맨틱 컴파일을 반드시 거친다. 시맨틱 SELECT는 **(1) Grouped 모드**(명명 metric · ad-hoc 집계 · metric 파생 연산 · 윈도우 함수 **혼용 가능** + 선택 dimension/차원 스칼라 식 `GROUP BY` 및 `ROLLUP`/`CUBE`/`GROUPING SETS` + 차원-only `GROUP BY`/`HAVING`) 또는 **(2) Ungrouped 상세 모드**(dim/fact·비집계 스칼라·윈도우 식 프로젝션, 요약 뷰 우회, 비-`DISTINCT` cube JOIN 거절)로 컴파일한다(D23). 공통으로 `SELECT DISTINCT`, 일반 `WHERE`, `ORDER BY`(별칭·위치·멤버·스칼라/집계식), `LIMIT`/`OFFSET`, `params` `:name` 안전 리터럴 치환을 지원한다.

view를 join, union, minus로 만들 때 이름과 데이터와 코드가 같은 컬럼은 하나의 view 컬럼으로 묶는다. `mappings`가 둘 이상이면 가리키는 컬럼 이름이 같다.

`combine`이 `select`인 view는 view만 묶는다. Grouped 집계 질의(또는 `SELECT DISTINCT`)에서는 사용자 SQL이 참조한 컬럼을 모두 가진 source 중에서 `summary`가 true인 view를 우선 선택한다(단, 반가산성 metric인 경우 요약 view의 컬럼 집합이 질의 요구 컬럼 집합과 정확히 일치할 때만 채택한다). 반면 **비-`DISTINCT` Ungrouped 상세 행 조회(D23)**는 사전 집계된 `summary: true` view를 건너뛰고 원천(non-summary) source를 우선 선택한다. 어느 source도 컬럼을 덮지 못하면 질의를 거절한다.

`metadata_create`와 `metadata_update`는 문서 하나의 JSON Schema 검증을 통과한 뒤에, 그 문서를 반영한 등록 집합 전체의 연결을 검증한다. `metadata_delete`는 그 문서를 뺀 집합의 연결을 검증한다. 검증이 실패하면 시맨틱 DB를 바꾸지 않는다. `metadata_get`과 `metadata_list`는 저장된 문서를 반환한다.

연결은 `fk.references`, view `sources`와 `mappings`, cube `view`, dimension·fact의 `column`, metric·segment `cube`, metric `group_by`, 그리고 table/view/cube의 **`target` 일치**다. `expression`으로 만든 dimension·fact는 **저장 시** view 컬럼 연결 검사를 하지 않는다(식 식별자 검사는 D18 컴파일 단계). `aggregation`이 `group`이면 `group_by`의 각 이름이 연결된 view 컬럼에 있어야 한다. MCP 저장 계층은 타입별 libSQL `meta_*` 테이블이다(`semantic/libsql/catalog.sql`). `metadata_*` **입출력은 JSON**(`document.schema.json`)이며 저장 시 타입별 행으로 분해·조회 시 JSON으로 재조립한다. 검색 투영은 `column_index`(+`target`+`embedding`)와 FTS5(`column_index_fts`, 순위 `bm25`)다 — keyword는 FTS5이며 vector/hybrid(RRF)를 유지한다.

## 2. 컴포넌트

| 컴포넌트 | 경로 | 책임 |
|----------|------|------|
| MCP 서버 | `server/` | **일반 HTTP** · **stdio(로컬)** · 툴 · SELECT 가드 · **libSQL** 시맨틱 · 타깃 DB 클라이언트 |
| 시맨틱 정의 | `semantic/` | 등록 JSON Schema · 예시 · libSQL 카탈로그 DDL |
| 시맨틱 DB | 프로세스 내 **libSQL** | 메타·FTS/벡터. CRUD는 MCP 메타 툴 |
| 타깃 DB | 외부 DW (N개) | 실제 조회 대상. SELECT only |
| Frontend / 세션형 Streamable HTTP | — | **취소** |

```
 AI Client ──HTTP (JSON)──▶ server/ (rmcp)
 Local MCP ──stdio────────▶ server/ (rmcp)   # --stdio / SQL_MCP_TRANSPORT=stdio
                              ├─ libSQL semantic  ◀── metadata CRUD
                              └─ target clients ──SELECT──▶ Target DW #1..N
```

## 3. MCP 인터페이스 형태

### 3.1 Transport

| Transport | 상태 |
|-----------|------|
| **일반 HTTP** | **기본** — MCP 메시지를 HTTP POST 요청/응답(JSON)으로 주고받음. 세션 ID·장수 SSE 스트림 없음 |
| **stdio** | **로컬 지원** — stdin/stdout JSON-RPC. Cursor 등 자식 프로세스 MCP용. `--stdio` 또는 `SQL_MCP_TRANSPORT=stdio`. 로그는 stderr |
| Streamable HTTP (세션형) | **취소** — 장수 세션·SSE 중심 모드 제외. 구현체는 `server/DESIGN.md` |

### 3.2 Tools — 조회 (타깃 / 시맨틱 검색)

| Tool | 능력 | 입력(요약) | 출력 | 규칙 |
|------|------|------------|------|------|
| `list_databases` | database 목록(DW 표면) | — | `name`·`description`·`engine`(시크릿 제외) | 설정에서 읽기. `name` = `TargetConfig.id` = 등록 문서 `target` |
| `list_tables` | **cube**=table 목록(DW 표면) | `database?` | `{ database, name, description }`만 | 시맨틱 `kind=cube` (`database`면 `cube.target` 필터). 상세는 `describe_table` |
| `describe_table` | **cube**=table 상세(DW 표면) | `table`, `database?`, `caller_role?` | `{ database, name, description, dimensions, facts, metrics, segments }` | `database`면 cube.target 일치. `caller_role`(또는 default)로 멤버 필터 · 응답에 `role`/`kind`/`view`/물리 `column`/`cube` 키 없음 |
| `search_columns` | 멤버 검색(DW 표면) | `query`, `database?`, `mode?`, `limit?`, `caller_role?` | `{ database, table, kind, name, description, score }` | `kind`=`table`\|`dimension`\|`fact`\|`metric`\|`segment`. role로 안 보이는 항목 제외. §3.5 |
| `sample_column` | dim/fact 샘플(DW 표면) | `database`, `table`, `name`, `caller_role?`, `params?`, `limit?` | `{ database, table, name, as_code, masked, values }` | §3.4 — **cube.target == database** · 물리 `resolved`/`sql` 숨김 · role 필터 적용 |
| `execute_sql` | 시맨틱 SQL 컴파일·실행 | `database`, `sql`, `caller_role?`, `params?`, `dry_run?` | 실행: `{ columns, rows, row_count, execution_time_ms }`(+상한 시 `truncated`); `dry_run=true`면 `{ sql, dry_run: true }`만 | **시맨틱 SELECT만** · **cube.target == database** · 실행 응답에 물리 `sql` 없음 |

### 3.3 Tools — 메타데이터 CRUD (JSON)

임베디드 시맨틱 저장소만. **JSON 바디**로 주고받는다. 페이로드는 `semantic/schema/document.schema.json`의 등록 문서다. `SQL_MCP_MODE=query`(기본)이면 DW 질의 모드로 `metadata_*` **변경**을 거절한다. `SQL_MCP_MODE=author`이면 코딩 에이전트용 메타 CRUD를 허용한다.

| Tool | 동작 | 입력 | 출력 |
|------|------|------|------|
| `metadata_create` | 생성 | JSON `body` (+ 선택 `kind`) | 생성된 객체 JSON |
| `metadata_get` | 단건 조회 | `kind` + `name` | 객체 JSON / not_found |
| `metadata_list` | 목록 | 필터 JSON (`kind?`) | `{ "items": [ ... ] }` |
| `metadata_update` | 수정(전체 replace) | `kind` + `name` + JSON `body`(문서 전체) | 갱신된 객체 JSON |
| `metadata_delete` | 삭제 | `kind` + `name` | `{ "deleted": true }` |

JSON에 DSN/password 금지 — 타깃 연결 비밀은 설정 쪽 id 참조만.

### 3.4 `sample_column` · `as_code` 코드사전

- DW 인자: `database` + `table`(cube) + `name`(dimension/fact). 서버가 view mappings로 물리 테이블을 고른다(첫 mapping).
- dimension `as_code`는 **코드사전 객체** `{ table, key_column, value_column, where? }`(D9). boolean 금지. 에이전트 기입·MCP 저장만. fact·table column에는 두지 않는다.
- `execute_sql` Step 7: 집계 완료 후 SELECT에 있는 dim에 대해 사전을 `dim = key` LEFT JOIN하고 `{dim}__value`를 프로젝션한다.
- `sample_column`: dim에 `as_code`가 있으면 `{key,value}` 샘플, 없으면 스칼라 `values`. 응답은 DW 표면만(물리 `resolved`/`sql` 없음). `scope_filter` · `default_filter` · `permission_filters` 및 `role` 접근 권한을 강제한다.

### 3.5 `search_columns`

- 목표 모드: `keyword`(BM25) · `vector` · `hybrid`(RRF). 기본 `hybrid` 권장. `database` 인자가 주어지면 해당 타깃만. 응답은 DW 표면(`table`·`kind`·`name`·`description`·`score`); cube 문서 hit은 `kind=table`. `caller_role`로 멤버 필터.
- JSON 교환·검증 정본은 `semantic/schema/document.schema.json`. 저장은 타입별 `meta_*`. 검색 인덱스(`column_index` / FTS5)는 등록 JSON의 **투영**이며 규칙은 [semantic/DESIGN.md](semantic/DESIGN.md)(D12). 파이프라인 상세는 [server/spec.md](server/spec.md).
- **D14:** 임베딩은 외부 OpenAI-호환 HTTP API(`SQL_MCP_EMBEDDING_URL` + `API_KEY`)로만 생성하고 libSQL에 저장한다. CRUD 시 텍스트가 변경된 항목만 증분 임베딩한다. API가 없으면 embedder를 두지 않으며, `vector`/`hybrid` 요청은 **keyword(BM25)로 폴백**한다. 오프라인 검색 품질은 BM25 강화(한글 토큰 유지, 문자 bigram)로 올린다. 로컬 ONNX·운영용 HashEmbedder는 쓰지 않는다. 투영 범위는 [semantic/DESIGN.md](semantic/DESIGN.md) D12.

### 3.6 Resources

| URI | 이름 | MIME | 내용 |
|-----|------|------|------|
| `https://sql-mcp.local/semantic/document.schema.json` | `document.schema.json` | `application/schema+json` | 등록 문서 JSON Schema 정본(`semantic/schema/document.schema.json`). `$id`와 동일 |
| `https://sql-mcp.local/skills/{skill}/…` | `{skill}/…` (상대 경로) | 확장자별 (`text/markdown` · `application/json` · `application/schema+json`) | Agent Skills 정본(`skills/`). 서버 바이너리에 embed. `SKILL.md`·계약·예시 포함 |

코딩 에이전트는 `resources/list` · `resources/read`로 스키마와 스킬 파일을 읽는다. 스킬 정본은 제품 비종속 `skills/`이며, Cursor 로컬 발견용으로 `.cursor/skills` → `../skills` 심볼릭을 둔다. 서버는 MCP Skills extension(`io.modelcontextprotocol/skills`)을 광고하며 `skills/list` · `skills/get`을 구현한다(URI는 기존 Resource와 동일 `https://sql-mcp.local/skills/…`). `resources/directory/read`는 미구현(`directoryRead` 미광고).

### 3.7 에러

`code`: `not_select` | `timeout` | `row_limit` | `sql_parse` | `not_found` | `validation` | `forbidden` | `connection` | `internal`. 시크릿·풀 DSN 금지(DB 에러 메시지는 DSN/비밀번호 마스킹 후 전달).

## 4. 시맨틱 등록 문서

디렉터리 이름이 문서 종류다. 예시는 `semantic/examples/`에 있다. 파일명(확장자 제외)은 `name`과 같다.

```text
semantic/examples/
  tables/{name}.json
  views/{name}.json
  cubes/{name}.json
  metrics/{name}.json
  segments/{name}.json
```

참조는 `table → view → cube → metric|segment` 방향이다. cube는 view 하나에 연결된다. metric과 segment는 cube 하나에 연결된다. metric 이름은 같은 cube의 dimension, fact 이름과 겹칠 수 없다.

**이름 우선권.** 자연스러운 짧은 `name`은 **cube · dimension · fact · metric · segment**가 우선한다(이들이 `search_columns` 검색 대상 — D12). 이들과 문자열이 겹치면 **table · view · view 컬럼** 쪽 논리 이름에 접미사(`_table`, `_view`, `_col` 등)를 붙여 피한다. 물리 `db`/`schema`/`table` 및 table `columns[].name`(물리 컬럼)은 바꾸지 않는다. dimension/fact가 `column`으로 가리키는 view 컬럼과 멤버 `name`이 같은 경우는 동일 식별 공유로 허용한다.

**식별자 이원(D22).** `$defs/name`은 시맨틱·논리 식별자(`^[A-Za-z_][A-Za-z0-9_]*$`: 문서 name · target · view 컬럼 · dim/fact/metric/segment 등). `$defs/physical_ident`는 물리 경로·컬럼(공백·`%`·괄호 등 허용; 따옴표·백틱·제어문자·`;` 금지): `db`/`schema`/`table`, table `columns[].name`/`pk`/`fk.columns`, view `mappings[].column`·join `on` 컬럼, `as_code` key/value 컬럼. view 컬럼 `name`과 dim/fact `column`은 시맨틱 `name`만 쓴다 — 물리명이 패턴 밖이면 view에서 별칭을 두고 `mappings.column`에 물리를 남긴다. emit은 `quote_ident`로 인용한다.

필드 타입의 정본은 `semantic/schema/document.schema.json` 하나다. `$id`는 `https://sql-mcp.local/semantic/document.schema.json`이며 HTTP 다운로드 주소가 아니다(MCP Resource URI와 같다 — §3.6).

| 문서 | 필수 필드 |
| --- | --- |
| table | `kind`, `name`, `target`, `db`, `schema`, `table`, `columns`, `pk`, `fk`, `permission_filters`, `scope_filter`, `default_filter` |
| view | `kind`, `name`, `target`, `combine`, `sources`, `columns`. join은 `joins`, select는 `prefer`. `sources[]` = `{ kind, table, alias? }` (`table`=카탈로그 문서명, `alias` 생략 시=`table`; role-play는 동일 `table`+다른 `alias`) |
| cube | `kind`, `name`, `target`, `view`, `description`, `dimensions`, `facts` (+ 선택 `role`) |
| metric | `kind`, `name`, `description`, `cube`, `expression`, `aggregation`, `filter` |
| segment | `kind`, `name`, `description`, `cube`, `filter` |

table, cube `role`의 기본값은 `all`이다. `columns`는 `name`, `type`, `nullable`이다. `pk`는 그 컬럼 이름이다. `fk.columns`는 이 테이블 컬럼이고 `fk.references`는 다른 논리 테이블과 그 컬럼이다.

view `combine`은 `join`, `union`, `minus`, `select` 중 하나다. `combine`이 없으면 view가 아니다. `join`, `union`, `minus`의 source는 테이블이고, `select`의 source는 view다. union과 minus는 첫 source가 기준이고 각 view 컬럼이 모든 source를 가리킨다. join의 `type`은 `inner`, `left`, `right`, `full`이다.

dimension `type`은 `code`, `date`, `time`, `datetime`, `string` 중 하나다(취급 방식). fact `type`은 `amount`, `fx`, `quantity` 중 하나다(금액·환율·일반 수량). 둘 다 **`description` 필드**(검색 투영)와 `encryption`(D17 마스킹)을 가진다. `description`은 **메타 사용**용(검색·동의어·업무 의미)이며, 생성 출처(`from table.column`, `Auto cube…` 등)를 넣지 않는다. 사용 서술이 없으면 빈 문자열이다. dimension만 선택 `as_code`(D9 코드사전)를 가진다. `role`의 기본값은 `all`이다. 값은 view 컬럼 `column`이거나, view 컬럼 이름을 쓰는 스칼라 `expression`이다. 둘 중 하나만 둔다.

metric `expression`은 연결된 cube의 dimension/fact 연산이다. `aggregation`은 `group`, `rollup`, `cube`, `grouping_sets` 중 하나이고, 질의의 그룹 기준을 그 방식으로 묶는다. `group`이면 `group_by`에 GROUP BY로 쓰는 view 컬럼을 둔다. cube·dimension·fact·metric·segment의 `name`·`description`은 **메타 사용** 표면이다(`search_columns` D12); `description`은 비울 수 있다. metric과 segment `role`의 기본값은 `all`이다. segment `filter`는 비울 수 없다.

예시는 `semantic/examples/`에 있다. libSQL 카탈로그 DDL은 `semantic/libsql/catalog.sql`이다.

## 5. 서버 내부 경계

```
HTTP ▶ rmcp tools
          ├─ metadata_*  ──▶ libSQL semantic (in-memory snapshot + single write tx)
          ├─ search_*    ──▶ FTS5(bm25) / vector / hybrid (target-filtered)
          └─ list/describe/sample/execute ──▶ target DB (by id, with timeout)
```

### 5.1 타깃 DialectProfile (형태, D21)

컴파일러와 타깃 클라이언트 **사이**의 emit 계약. 내부 모듈 배치는 [server/DESIGN.md](server/DESIGN.md).

| 능력 | 역할 | 엔진 예 |
|------|------|---------|
| `quote_ident` | 식별자 인용(샘플·타깃 직접 SQL) | Postgres/SQLite/Oracle `"…"` · MySQL `` `…` `` |
| `rewrite_date_trunc` | canonical `date_trunc` + 날짜 편의 함수 → 엔진 식 | MySQL `DATE_FORMAT`/`DATE` · Oracle `TRUNC` · SQLite `datetime(…, 'start of …')` · PG identity; D25: `NOW`/`DATEDIFF`/`YEAR`·`EXTRACT` 등 |
| `adapt_compiled_sql` | canonical `"…"` → 엔진 quote | MySQL backticks · 나머지 identity |
| `wrap_row_limit` | 행 상한 래퍼 | `LIMIT n` · Oracle `FETCH FIRST n ROWS ONLY` |
| `emit_physical_sql` | rewrite + quote adapt | `pipeline` Step 8 |

**D25 canonical 날짜 표면** (metric/dimension/filter 식): `CURRENT_DATE` · `CURRENT_TIMESTAMP` · `NOW()` · `date_trunc('year'|'month'|'day'|'hour', x)` · `YEAR`/`MONTH`/`DAY` · `DATEDIFF(end, start)`(일수) · `EXTRACT(YEAR|MONTH|DAY|HOUR FROM x)`. emit: Oracle `NOW`→`CURRENT_TIMESTAMP` · `DATEDIFF`→PG 날짜 차·SQLite `julianday`·Oracle `TRUNC` 차 · `YEAR`/`MONTH`/`DAY`→PG/Oracle `EXTRACT` · SQLite `EXTRACT`→`strftime`.

`compile`은 canonical만 낸다. `:name` 바인드 파라미터는 `params` 제공 시 이스케이프된 리터럴로 치환하며, 드라이버 prepared bind rewrite는 하지 않는다.

## 6. 설정 형태

| 변수 / 설정 | 의미 |
|-------------|------|
| `SQL_MCP_SEMANTIC_PATH` | 임베디드 시맨틱 DB 파일 경로 |
| 타깃 연결 목록 | 설정 파일/환경(이름·URL/시크릿) — **복수** |
| `SQL_MCP_MAX_ROWS` | `execute_sql` 행 상한 |
| `SQL_MCP_SAMPLE_LIMIT` | 샘플 기본 개수 |
| `SQL_MCP_TIMEOUT_MS` | 타깃 쿼리 타임아웃 |
| `SQL_MCP_MODE` | `query`(기본, DW 질의 · 메타 변경 금지) \| `author`(메타 CRUD 허용) |
| `SQL_MCP_DEFAULT_ROLE` | 호출자가 `caller_role`을 생략했을 때 기본 적용할 역할 |
