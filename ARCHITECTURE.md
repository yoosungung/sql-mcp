# ARCHITECTURE.md

컴포넌트 *간* 계약과 인터페이스 형태. 내부 구현은 `server/DESIGN.md`(코드 도입 시)에 둔다. 등록 JSON 필드 정본은 `semantic/schema/document.schema.json`이다.

## 1. 계약사항 (불변 규칙)

1. **임베디드 시맨틱 DB (libSQL).** 시맨틱 저장소는 **MCP 서버(`rmcp`/process) 안에 libSQL로 임베드**한다(별도 원격 시맨틱 서버 없음). 파일 경로 등은 기동 설정. (장차) BM25/벡터 인덱스는 이 DB에 둔다.
2. **시맨틱 등록 문서.** 코딩 에이전트가 등록하는 JSON은 `table`, `view`, `cube`, `metric`, `segment`다. 문서 하나의 정본은 `semantic/schema/document.schema.json`이다. 코딩 에이전트는 그 스키마와 이 계약의 연결 규칙을 보고 검증한다. MCP `metadata_*`도 같은 스키마로 검증한다.
3. **타깃 DB (복수).** 실제 DW/조회 대상은 **타깃 DB**라 부른다. **복수 등록**을 허용한다. `execute_sql`·샘플 등 데이터 조회는 타깃 DB로 간다.
4. **언어.** MCP 서버는 **Rust** + [`rmcp`](https://crates.io/crates/rmcp) (MCP spec `2026-07-28`).
5. **범위.** **MCP 서버만.** Frontend·Admin UI·Admin API·**세션형 Streamable HTTP**는 **취소(범위 밖)**. Transport는 **일반 HTTP**(배포·원격)와 **stdio**(로컬 자식 프로세스). 둘 다 동일 툴 표면.
6. **타깃 SQL = 시맨틱 SELECT만.** `execute_sql` 입력은 **시맨틱 SQL**(cube FROM · metric/dimension/segment)이며, 서버가 6단계 파이프라인으로 물리 SELECT를 만든 뒤에만 타깃에 실행한다. **자유 물리 SELECT 경로는 없다.** 컴파일·실행 결과는 단일 SELECT(읽기 CTE 허용)만. INSERT/UPDATE/DELETE/DDL/다중 문장/쓰기 CTE 거부. 행 상한·타임아웃 강제. (문서상 `execute_semantic_sql`은 `execute_sql` 이명.)
7. **메타데이터 CRUD (예외).** 임베디드 시맨틱 저장소에 한해 MCP 툴로 **CRUD**를 제공한다. 요청/응답 바디는 **JSON**. 타깃 DB 쓰기가 아니다.
8. **MCP 제공 능력.** (1) 타깃·테이블 목록/상세 (2) 컬럼 검색(BM25/벡터/hybrid) (3) 컬럼 샘플(+code) (4) 타깃 SELECT SQL (5) **메타데이터 CRUD(JSON)** (6) **Resource:** 등록 문서 JSON Schema(`document.schema.json`). 그 외 툴·Resource는 기본 표면에 넣지 않는다. DW 에이전트에게는 메타 CRUD 도구를 주지 않는다. 이 에이전트에게 테이블은 cube이고 컬럼은 dimension, fact, metric이다.
9. **시크릿.** 로그·에러·툴 응답에 DSN/비밀번호를 실지 않는다. 타깃 자격 증명은 기동 설정으로만 주입(채팅으로 비밀번호 받지 않음). 메타 CRUD JSON에도 시크릿 필드를 넣지 않는다.
10. **문서 우선 · TDD.**

테이블 필터의 식은 물리 컬럼 이름을 쓴다. cube, metric, segment의 식은 그 cube의 dimension, fact 이름을 쓴다. 필터·metric·segment 식은 코딩 에이전트가 작성한 **신뢰된 SQL 조각**이며 새 DSL을 두지 않는다(D16). 서버는 `sqlparser`로 파싱한 뒤 AST를 제한한다: metric `expression`은 스칼라·집계식만, segment·filter는 단일 boolean 식만. 서브쿼리·다중 문장·쓰기·쓰기 CTE는 거부한다.

컴파일할 때 아래 필터를 모두 AND로 붙인다.

1. 선택된 view가 닿는 각 table의 `scope_filter`
2. 호출자 role과 같은 table `permission_filters`
3. 각 table의 `default_filter`
4. 질의에 쓰인 metric의 `filter`
5. 질의에 쓰인 segment의 `filter`

table, dimension, fact, metric, segment의 `role`을 생략하거나 `all`이면 모든 role이 볼 수 있다. table `permission_filters`의 `role`은 그 행 필터가 적용되는 역할이다. role 이름 배열이면 호출자는 그 중 하나의 role을 가져야 한다. `encryption`이 true인 dimension·fact는 **암·복호화하지 않고** 툴 응답·샘플·로그에서 원문을 내지 않는다 — 셀 값은 `null`(또는 `"***"`)로 마스킹한다(D17). AES·토큰화는 타깃 DW 책임이다.

식 안의 컬럼/멤버 이름 검사는 **이중**이다(D18). `metadata_*` 저장 시에는 연결 그래프만 검증한다(`column`·`group_by`·cube/view 존재 등; `expression`으로 만든 dimension·fact의 view 컬럼 연결 검사는 하지 않는다). 식 본문의 식별자는 **컴파일 직전** AST에서 알려진 dimension·fact·(해당 시) view/물리 컬럼으로 해석 가능한지 검사하고, 실패하면 `validation`으로 거절한다. `execute_sql`은 시맨틱 컴파일을 반드시 거친다.

view를 join, union, minus로 만들 때 이름과 데이터와 코드가 같은 컬럼은 하나의 view 컬럼으로 묶는다. `mappings`가 둘 이상이면 가리키는 컬럼 이름이 같다.

`combine`이 `select`인 view는 view만 묶는다. 사용자 SQL이 참조한 컬럼을 모두 가진 source 중에서 `summary`가 true인 view를 우선 선택한다. 요약 view로 풀 수 없으면 나머지 source를 쓴다. 어느 source도 컬럼을 덮지 못하면 질의를 거절한다.

`metadata_create`와 `metadata_update`는 문서 하나의 JSON Schema 검증을 통과한 뒤에, 그 문서를 반영한 등록 집합 전체의 연결을 검증한다. `metadata_delete`는 그 문서를 뺀 집합의 연결을 검증한다. 검증이 실패하면 시맨틱 DB를 바꾸지 않는다. `metadata_get`과 `metadata_list`는 저장된 문서를 반환한다.

연결은 `fk.references`, view `sources`와 `mappings`, cube `view`, dimension·fact의 `column`, metric·segment `cube`, metric `group_by`다. `expression`으로 만든 dimension·fact는 **저장 시** view 컬럼 연결 검사를 하지 않는다(식 식별자 검사는 D18 컴파일 단계). `aggregation`이 `group`이면 `group_by`의 각 이름이 연결된 view 컬럼에 있어야 한다. MCP 저장 계층은 타입별 libSQL `meta_*` 테이블이다(`semantic/libsql/catalog.sql`). `metadata_*` **입출력은 JSON**(`document.schema.json`)이며 저장 시 타입별 행으로 분해·조회 시 JSON으로 재조립한다. 검색 투영은 `column_index`(+embedding)와 FTS5(`column_index_fts`, 순위 `bm25`)다 — keyword는 FTS5이며 vector/hybrid(RRF)를 유지한다.

## 2. 컴포넌트

| 컴포넌트 | 경로 | 책임 |
|----------|------|------|
| MCP 서버 | `server/` | **일반 HTTP** · **stdio(로컬)** · 툴 · SELECT 가드 · **libSQL** 시맨틱 · 타깃 DB 클라이언트 |
| 시맨틱 정의 | `semantic/` | 등록 JSON Schema · 예시 · libSQL 카탈로그 DDL |
| 시맨틱 DB | 프로세스 내 **libSQL** | 메타·(장차) FTS/벡터. CRUD는 MCP 메타 툴 |
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
| `list_targets` | 타깃 DB 목록 | — | id·이름·엔진(시크릿 제외) | 메타에서 읽기 |
| `list_tables` | 테이블 목록 | `target?`, `schema?` | 테이블 메타 | 타깃 또는 시맨틱 등록분 — DESIGN |
| `describe_table` | 테이블 상세 | `target?`, `table`, `schema?` | 컬럼·키 요약 | |
| `search_columns` | 컬럼 검색 | `query`, `target?`, `mode?`, `limit?` | 매칭 메타 | §3.2.2 |
| `sample_column` | 컬럼 샘플 | `target`, `table`, `column`, `limit?` | 샘플·`is_code` | §3.2.1 — **타깃** |
| `execute_sql` | 시맨틱 SQL 컴파일·실행 | `target`, `sql`, `caller_role?`, `params?` | 행·메타·`sql`(물리) | **시맨틱 SELECT만** → 물리 SELECT |

### 3.3 Tools — 메타데이터 CRUD (JSON)

임베디드 시맨틱 저장소만. **JSON 바디**로 주고받는다. 페이로드는 `semantic/schema/document.schema.json`의 등록 문서다.

| Tool | 동작 | 입력 | 출력 |
|------|------|------|------|
| `metadata_create` | 생성 | JSON `body` (+ 선택 `kind`) | 생성된 객체 JSON |
| `metadata_get` | 단건 조회 | `kind` + `name` | 객체 JSON / not_found |
| `metadata_list` | 목록 | 필터 JSON (`kind?`) | `{ "items": [ ... ] }` |
| `metadata_update` | 수정(전체 replace) | `kind` + `name` + JSON `body`(문서 전체) | 갱신된 객체 JSON |
| `metadata_delete` | 삭제 | `kind` + `name` | `{ "deleted": true }` |

JSON에 DSN/password 금지 — 타깃 연결 비밀은 설정 쪽 id 참조만.

### 3.4 `sample_column` — code

- 일반: 짧은 샘플, 긴 텍스트 truncate.
- 코드: `is_code: true`, 개행 유지, 선택 `language`. 런타임 판정은 D9 휴리스틱(서버가 메타를 쓰지 않음).
- 등록 문서의 `as_code`는 **코딩 에이전트가 기입**한다. MCP `metadata_*`는 스키마에 맞는 JSON을 **저장만** 하며 `as_code`를 추론하거나 샘플 결과로 덮어쓰지 않는다(D9).

### 3.5 `search_columns`

- 목표 모드: `keyword`(BM25) · `vector` · `hybrid`(RRF). 기본 `hybrid` 권장.
- JSON 교환·검증 정본은 `semantic/schema/document.schema.json`. 저장은 타입별 `meta_*`. 검색 인덱스(`column_index` / FTS5)는 등록 JSON의 **투영**이며 규칙은 [semantic/DESIGN.md](semantic/DESIGN.md)(D12). 파이프라인 상세는 [server/spec.md](server/spec.md).
- **D14:** 임베딩은 외부 OpenAI-호환 HTTP API(`SQL_MCP_EMBEDDING_URL` + `API_KEY`)로만 생성하고 libSQL에 저장한다. API가 없으면 embedder를 두지 않으며, `vector`/`hybrid` 요청은 **keyword(BM25)로 폴백**한다. 오프라인 검색 품질은 BM25 강화(한글 토큰 유지, 문자 bigram)로 올린다. 로컬 ONNX·운영용 HashEmbedder는 쓰지 않는다. 투영 범위는 [semantic/DESIGN.md](semantic/DESIGN.md) D12.

### 3.6 Resources

| URI | 이름 | MIME | 내용 |
|-----|------|------|------|
| `https://sql-mcp.local/semantic/document.schema.json` | `document.schema.json` | `application/schema+json` | 등록 문서 JSON Schema 정본(`semantic/schema/document.schema.json`). `$id`와 동일 |

코딩 에이전트는 `resources/list` · `resources/read`로 스키마를 읽고 `metadata_*` 페이로드를 작성한다. 그 외 Resource는 두지 않는다.

### 3.7 에러

`code`: `not_select` | `timeout` | `row_limit` | `sql_parse` | `not_found` | `validation` | `connection` | `internal`. 시크릿·풀 DSN 금지.

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

필드 타입의 정본은 `semantic/schema/document.schema.json` 하나다. `$id`는 `https://sql-mcp.local/semantic/document.schema.json`이며 HTTP 다운로드 주소가 아니다(MCP Resource URI와 같다 — §3.6).

| 문서 | 필수 필드 |
| --- | --- |
| table | `kind`, `name`, `db`, `schema`, `table`, `columns`, `pk`, `fk`, `permission_filters`, `scope_filter`, `default_filter` |
| view | `kind`, `name`, `combine`, `sources`, `columns`. join은 `joins`, select는 `prefer` |
| cube | `kind`, `name`, `view`, `description`, `dimensions`, `facts` |
| metric | `kind`, `name`, `cube`, `expression`, `aggregation`, `filter` |
| segment | `kind`, `name`, `cube`, `filter` |

table `role`의 기본값은 `all`이다. `columns`는 `name`, `type`, `nullable`과 선택 `as_code`(D9: 에이전트 기입·서버는 저장만)이다. `pk`는 그 컬럼 이름이다. `fk.columns`는 이 테이블 컬럼이고 `fk.references`는 다른 논리 테이블과 그 컬럼이다.

view `combine`은 `join`, `union`, `minus`, `select` 중 하나다. `combine`이 없으면 view가 아니다. `join`, `union`, `minus`의 source는 테이블이고, `select`의 source는 view다. union과 minus는 첫 source가 기준이고 각 view 컬럼이 모든 source를 가리킨다. join의 `type`은 `inner`, `left`, `right`, `full`이다.

dimension `type`은 `code`, `date`, `time`, `datetime`, `string` 중 하나다(취급 방식). fact `type`은 `amount`, `fx`, `quantity` 중 하나다(금액·환율·일반 수량). 둘 다 `encryption`(D17 마스킹)을 가진다. `role`의 기본값은 `all`이다. 값은 view 컬럼 `column`이거나, view 컬럼 이름을 쓰는 스칼라 `expression`이다. 둘 중 하나만 둔다.

metric `expression`은 연결된 cube의 view 컬럼 연산이다. `aggregation`은 `group`, `rollup`, `cube`, `grouping_sets` 중 하나이고, 질의의 그룹 기준을 그 방식으로 묶는다. `group`이면 `group_by`에 GROUP BY로 쓰는 view 컬럼을 둔다. metric과 segment `role`의 기본값은 `all`이다. segment `filter`는 비울 수 없다.

예시는 `semantic/examples/`에 있다. libSQL 카탈로그 DDL은 `semantic/libsql/catalog.sql`이다.

## 5. 서버 내부 경계

```
HTTP ▶ rmcp tools
          ├─ metadata_*  ──▶ libSQL semantic
          ├─ search_*    ──▶ (장차) FTS/vector
          └─ list/describe/sample/execute ──▶ target DB (by id)
```

## 6. 설정 형태 (초안)

| 변수 / 설정 | 의미 |
|-------------|------|
| `SQL_MCP_SEMANTIC_PATH` | 임베디드 시맨틱 DB 파일 경로 |
| 타깃 연결 목록 | 설정 파일/환경(이름·URL/시크릿) — **복수** |
| `SQL_MCP_MAX_ROWS` | `execute_sql` 행 상한 |
| `SQL_MCP_SAMPLE_LIMIT` | 샘플 기본 개수 |
| `SQL_MCP_TIMEOUT_MS` | 타깃 쿼리 타임아웃 |
