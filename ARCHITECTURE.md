# ARCHITECTURE.md

컴포넌트 *간* 계약과 인터페이스 형태. 내부 구현은 `server/DESIGN.md`(코드 도입 시)에 둔다.

## 1. 계약사항 (불변 규칙)

1. **임베디드 시맨틱 DB (libSQL).** 시맨틱 저장소는 **MCP 서버(`rmcp`/process) 안에 libSQL로 임베드**한다(별도 원격 시맨틱 서버 없음). 파일 경로 등은 기동 설정. (장차) BM25/벡터 인덱스는 이 DB에 둔다.
1a. **시맨틱 레이어 모델은 향후 정의.** 엔티티·필드·매핑·검색 인덱스 설계는 **지금 고정하지 않는다**(ROADMAP D12 / §4). 임베디드 저장소와 `metadata_*`(JSON) 통로만 계약으로 두고, 스키마는 추후 ARCHITECTURE·DESIGN에 올린다. 정의 시에도 원천 DW 카탈로그 전체 미러는 하지 않는 방향을 유지한다.
2. **타깃 DB (복수).** 실제 DW/조회 대상은 **타깃 DB**라 부른다. **복수 등록**을 허용한다. `execute_sql`·샘플 등 데이터 조회는 타깃 DB로 간다.
3. **언어.** MCP 서버는 **Rust** + [`rmcp`](https://crates.io/crates/rmcp) (MCP spec `2026-07-28`).
4. **범위.** **MCP 서버만.** Frontend·Admin UI·Admin API·**Streamable HTTP**는 **취소(범위 밖)**. Transport는 **stdio만**.
5. **타깃 SQL = SELECT only.** 타깃 DB에 대한 임의 SQL은 **단일 SELECT**(및 DESIGN 화이트리스트 읽기 CTE)만. INSERT/UPDATE/DELETE/DDL/다중 문장/쓰기 CTE 거부. 행 상한·타임아웃 강제.
6. **메타데이터 CRUD (예외).** 임베디드 시맨틱 저장소에 한해 MCP 툴로 **CRUD**를 제공한다. 요청/응답 바디는 **JSON**. 타깃 DB 쓰기가 아니다. JSON **스키마·kind는 시맨틱 레이어 정의 시(§4) 확정**.
7. **MCP 제공 능력.** (1) 타깃·테이블 목록/상세 (2) 컬럼 검색(시맨틱 정의 후 BM25/벡터/hybrid) (3) 컬럼 샘플(+code) (4) 타깃 SELECT SQL (5) **메타데이터 CRUD(JSON)**. 그 외 툴은 기본 표면에 넣지 않는다.
8. **시크릿.** 로그·에러·툴 응답에 DSN/비밀번호를 실지 않는다. 타깃 자격 증명은 기동 설정으로만 주입(채팅으로 비밀번호 받지 않음). 메타 CRUD JSON에도 시크릿 필드를 넣지 않는다.
9. **문서 우선 · TDD.**

## 2. 컴포넌트

| 컴포넌트 | 경로 | 책임 |
|----------|------|------|
| MCP 서버 | `server/` | stdio · 툴 · SELECT 가드 · **임베디드 시맨틱 DB** · 타깃 DB 클라이언트 |
| 시맨틱 DB | 프로세스 내 **libSQL** | 메타·(장차) FTS/벡터. CRUD는 MCP 메타 툴 |
| 타깃 DB | 외부 DW (N개) | 실제 조회 대상. SELECT only |
| Frontend / HTTP transport | — | **취소** |

```
 AI Client (stdio MCP)
        │
        ▼
 ┌──────────────────────────────────┐
 │  server/ (rmcp)                  │
 │   ├─ embedded semantic DB        │◀── metadata CRUD (JSON)
 │   │    (BM25 / vector)           │
 │   └─ target DB clients (1..N)    │──SELECT──▶ Target DW #1..N
 └──────────────────────────────────┘
```

## 3. MCP 인터페이스 형태

### 3.1 Transport

| Transport | 상태 |
|-----------|------|
| stdio | **유일하게 지원** |
| Streamable HTTP | **취소** |

### 3.2 Tools — 조회 (타깃 / 시맨틱 검색)

| Tool | 능력 | 입력(요약) | 출력 | 규칙 |
|------|------|------------|------|------|
| `list_targets` | 타깃 DB 목록 | — | id·이름·엔진(시크릿 제외) | 메타에서 읽기 |
| `list_tables` | 테이블 목록 | `target?`, `schema?` | 테이블 메타 | 타깃 또는 시맨틱 등록분 — DESIGN |
| `describe_table` | 테이블 상세 | `target?`, `table`, `schema?` | 컬럼·키 요약 | |
| `search_columns` | 컬럼 검색 | `query`, `target?`, `mode?`, `limit?` | 매칭 메타 | §3.2.2 — 시맨틱 레이어 정의 후 인덱스 연동 |
| `sample_column` | 컬럼 샘플 | `target`, `table`, `column`, `limit?` | 샘플·`is_code` | §3.2.1 — **타깃** |
| `execute_sql` | SQL 수행 | `target`, `sql`, `params?` | 행·메타 | **타깃 SELECT만** |

### 3.3 Tools — 메타데이터 CRUD (JSON)

임베디드 시맨틱 저장소만. **JSON 바디**로 주고받는다.  
**페이로드 스키마는 시맨틱 레이어 향후 정의(§4)까지 안정화하지 않는다** — 툴 이름·JSON 입출력 형태만 고정.

| Tool | 동작 | 입력 | 출력 |
|------|------|------|------|
| `metadata_create` | 생성 | JSON `body` (+ 선택 `kind`) | 생성된 객체 JSON |
| `metadata_get` | 단건 조회 | `id` 또는 키 JSON | 객체 JSON / not_found |
| `metadata_list` | 목록 | 필터 JSON | `{ "items": [ ... ] }` |
| `metadata_update` | 수정 | `id` + JSON `body` | 갱신된 객체 JSON |
| `metadata_delete` | 삭제 | `id` 또는 키 JSON | `{ "deleted": true }` |

JSON에 DSN/password 금지 — 타깃 연결 비밀은 설정 쪽 id 참조만.

### 3.4 `sample_column` — code

- 일반: 짧은 샘플, 긴 텍스트 truncate.
- 코드: `is_code: true`, 개행 유지, 선택 `language`. 판정은 D9.

### 3.5 `search_columns`

- 목표 모드: `keyword`(BM25) · `vector` · `hybrid`(RRF). 기본 `hybrid` 권장.
- **인덱싱할 필드·문서 단위는 시맨틱 레이어 정의(§4) 이후**에 확정. 그 전까지는 스텁/미연동 가능(ROADMAP).
- 임베딩 생성은 서버 측(D14). 벡터는 임베디드 DB에 저장.

### 3.6 Resources

M1 툴만. Resources는 나중에 재개 가능.

### 3.7 에러

`code`: `not_select` | `timeout` | `row_limit` | `sql_parse` | `not_found` | `validation` | `connection` | `internal`. 시크릿·풀 DSN 금지.

## 4. 시맨틱 레이어 (향후 정의)

**미정의.** 메트릭·디멘션·매핑·검색 문서화 단위 등은 나중에 이 절과 `server/DESIGN.md`에 채운다.  
지금은 «임베디드 저장소 + `metadata_*` JSON CRUD + (장차) 검색 인덱스» 자리만 확보한다.

## 5. 서버 내부 경계

```
stdio ▶ rmcp tools
          ├─ metadata_*  ──▶ embedded semantic DB
          ├─ search_*    ──▶ embedded FTS/vector
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
