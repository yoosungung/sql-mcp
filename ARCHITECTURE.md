# ARCHITECTURE.md

컴포넌트 *간* 계약과 인터페이스 형태. 내부 구현은 `server/DESIGN.md`(코드 도입 시)에 둔다.

## 1. 계약사항 (불변 규칙)

1. **시맨틱 레이어 DB.** MCP가 붙는 데이터 소스는 **시맨틱 레이어 DB**뿐이다. 에이전트는 임의 운영/웨어하우스 DB에 직접 연결하지 않는다. 물리 DB 매핑·정의는 시맨틱 레이어 안에 둔다.
1a. **얇은 메타 스키마.** 시맨틱 DB의 *내부 스키마·관리 테이블*은 엔터프라이즈 원천 스키마 규모와 무관하게 **작게** 유지한다. 원천의 테이블/컬럼이 수만 개여도, 우리가 저장하는 것은 “등록·설명·매핑·샘플 힌트” 수준의 메타다. 웨어하우스 카탈로그 전체 미러링·범용 BI 시맨틱 엔진 흉내는 하지 않는다.
2. **언어.** MCP 서버는 **Rust**로 구현한다. 공식 SDK는 [`rmcp`](https://crates.io/crates/rmcp) (MCP spec `2026-07-28`, 하위 호환 유지).
3. **범위.** 이 저장소는 **MCP 서버만** 제공한다. Frontend·Admin UI·Admin API는 **취소(범위 밖)**. 시맨틱 메타 적재는 마이그레이션·시드·SQL/설정 파일 등 저장소 밖 또는 비-MCP 경로로 한다.
4. **SELECT only (MCP).** 임의 SQL 실행은 **단일 SELECT**(및 DESIGN에서 화이트리스트한 읽기 CTE)만 허용한다. INSERT/UPDATE/DELETE/DDL/다중 문장/쓰기 CTE는 거부한다. 행 수 상한·쿼리 타임아웃을 강제한다. MCP로 시맨틱 정의를 쓰지 않는다.
5. **MCP 제공 능력.** (1) 테이블 목록·상세 (2) 컬럼 검색 (3) 컬럼 샘플 — 코드면 코드로 취급 (4) SELECT SQL. 대상은 시맨틱 레이어 DB의 객체다. 그 외 툴은 기본 표면에 넣지 않는다.
6. **시크릿.** 로그·에러·툴 응답에 DSN/비밀번호를 실지 않는다. DB 자격 증명은 기동 설정으로만 주입한다(채팅/툴 인자 금지).
7. **문서 우선.** 계약·계획·내부 설계 변경은 해당 문서를 코드와 함께(또는 먼저) 갱신한다. 개발은 TDD(스켈레톤 → 테스트 → 구현).

## 2. 컴포넌트

| 컴포넌트 | 경로 | 책임 |
|----------|------|------|
| 시맨틱 레이어 DB | (엔진·인스턴스는 설정) | 시맨틱 정의·(필요 시) curated 데이터. MCP의 읽기 원천 |
| MCP 서버 | `server/` | stdio(및 이후 HTTP) · 툴 · SELECT 가드 · 시맨틱 DB 읽기 |
| Frontend / Admin API | — | **취소.** 디렉터리·마일스톤·미결정에 두지 않음 |

```
   (메타 적재: migration / seed / SQL — non-MCP)
                    │
                    ▼
            ┌─────────────────┐
            │ Semantic Layer  │
            │      DB         │
            └────────▲────────┘
                     │ SELECT only
            ┌────────┴────────┐
 AI Client ▶│    server/      │
   (MCP)    │  (rmcp stdio)   │
            └─────────────────┘
```

## 3. MCP 인터페이스 형태

### 3.1 Transport

| Transport | 우선순위 | 비고 |
|-----------|----------|------|
| stdio | M0–M1 필수 | Cursor / Claude Desktop 등 로컬 클라이언트 |
| Streamable HTTP | 미결정 | ROADMAP D5 |

### 3.2 Tools (확정 표면)

대상 DB = **시맨틱 레이어 DB**.

| Tool | 능력 | 입력(요약) | 출력 | 규칙 |
|------|------|------------|------|------|
| `list_tables` | 테이블 목록 | `schema?`, `connection?` | 테이블명·종류·대략 행수(가능 시) | 시맨틱 메타 |
| `describe_table` | 테이블 상세 | `table`, `schema?`, `connection?` | 컬럼·PK/FK/인덱스 요약 | 한 테이블 |
| `search_columns` | 컬럼 검색 | `query`, `schema?`, `table?`, `connection?`, `limit?`, `mode?` | 매칭 컬럼 메타 | §3.2.2 |
| `sample_column` | 컬럼 샘플 | `table`, `column`, `schema?`, `limit?`, `connection?` | 샘플·`is_code` | §3.2.1 |
| `execute_sql` | SQL 수행 | `sql`, `connection?`, `params?` | 행·메타 | **SELECT만** + limit + timeout |

쓰기·DDL·시맨틱 정의 변경 툴은 제공하지 않는다.

#### 3.2.1 `sample_column` — code 처리

- 일반 값: 짧은 샘플. 긴 텍스트는 truncate + `truncated: true`.
- **코드인 경우** (`is_code: true`): 개행 유지, `is_code` + 선택적 `language`.
- 판정·`as_code` 강제 여부는 ROADMAP D9 · `server/DESIGN.md`.

#### 3.2.2 `search_columns` — 컬럼 검색

- 검색 대상은 시맨틱 메타(컬럼명·설명·테이블명 등). 값은 `sample_column`.
- **모드:** `keyword`(BM25) · `vector`(임베딩 유사도) · `hybrid`(RRF 등). 기본은 `hybrid` 권장.
- `table`/`schema`로 범위 제한. 결과 `limit`으로 토큰 폭주 방지.
- 임베딩 **생성**은 DB 밖(앱/API). DB는 벡터 저장·KNN만. 엔진은 ROADMAP D1/D14.

### 3.3 Resources

M1에서는 **툴만**. Resources는 이후 ROADMAP에서 재개.

### 3.4 결과·에러 스키마 (공통)

`execute_sql` 성공 예:

```json
{
  "columns": ["id", "name"],
  "rows": [{"id": 1, "name": "a"}],
  "row_count": 1,
  "truncated": false,
  "connection": "semantic"
}
```

`search_columns` 성공 예:

```json
{
  "query": "email",
  "matches": [
    {"schema": "public", "table": "users", "column": "email", "type": "varchar", "nullable": false}
  ],
  "match_count": 1,
  "truncated": false
}
```

`sample_column` 성공 예:

```json
{
  "table": "jobs",
  "column": "script",
  "samples": [{"value": "SELECT 1;\n", "is_code": true, "language": "sql"}],
  "sample_count": 1,
  "truncated": false
}
```

에러 `code`: `not_select` | `timeout` | `row_limit` | `sql_parse` | `not_found` | `connection` | `internal`. 시크릿·풀 DSN 금지.

## 4. 시맨틱 모델 (형태만)

필드 목록(메트릭/조인 포함 범위, YAML vs 테이블 등)은 ROADMAP **D12**. §1a에 따라 **관리 테이블 수·컬럼 수는 최소**(초안: 연결·논리 테이블·컬럼·선택적 물리 매핑).

메타 적재는 MCP 밖(마이그레이션·시드·운영 SQL).

## 5. 서버 내부 경계 (형태만)

```
AI Client ──stdio/HTTP──▶ MCP (rmcp) ──Guard(SELECT)──▶ semantic DB
```

- **Config:** `SEMANTIC_DATABASE_URL`, `max_rows`, `sample_limit`, `timeout_ms`.
- **DB 접근:** async + 풀. TLS는 rustls 우선(DESIGN에서 확정).

## 6. 설정 형태 (초안)

| 변수 | 의미 |
|------|------|
| `SEMANTIC_DATABASE_URL` | 시맨틱 레이어 DB |
| `SQL_MCP_MAX_ROWS` | `execute_sql` 행 상한 |
| `SQL_MCP_SAMPLE_LIMIT` | `sample_column` 기본 샘플 수 |
| `SQL_MCP_TIMEOUT_MS` | 쿼리 타임아웃 |
