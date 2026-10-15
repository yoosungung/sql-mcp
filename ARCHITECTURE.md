# ARCHITECTURE.md

컴포넌트 *간* 계약과 인터페이스 형태. 내부 구현은 `server/DESIGN.md`(코드 도입 시)에 둔다.

## 1. 계약사항 (불변 규칙)

1. **언어.** MCP 서버는 **Rust**로 구현한다. 공식 SDK는 [`rmcp`](https://crates.io/crates/rmcp) (MCP spec `2026-07-28`, 하위 호환 유지).
2. **역할.** 이 저장소는 AI 클라이언트가 SQL 데이터베이스를 **안전하게** 탐색·질의할 수 있게 하는 MCP 서버다. DB 자격 증명은 채팅/툴 인자로 받지 않는다 — 기동 시 설정(환경변수·설정 파일·시크릿 스토어)으로만 주입한다.
3. **SELECT only.** 임의 SQL 실행은 **단일 SELECT**(및 엔진이 SELECT와 동등하게 취급하는 읽기 CTE 등 — DESIGN에서 화이트리스트)만 허용한다. INSERT/UPDATE/DELETE/DDL/다중 문장/쓰기 CTE는 거부한다. 행 수 상한·쿼리 타임아웃을 강제한다.
4. **제공 능력은 셋뿐.** (1) 테이블 목록·상세 (2) 컬럼 샘플 조회 — 값이 코드인 경우 코드로 취급 (3) SELECT SQL 수행. 그 외 툴은 기본 표면에 넣지 않는다.
5. **시크릿.** 로그·에러·툴 응답에 DSN/비밀번호를 실지 않는다.
6. **문서 우선.** 계약·계획·내부 설계 변경은 해당 문서를 코드와 함께(또는 먼저) 갱신한다. 개발은 TDD(스켈레톤 → 테스트 → 구현).

## 2. 컴포넌트

| 컴포넌트 | 경로 | 책임 |
|----------|------|------|
| MCP 서버 | `server/` | stdio(및 이후 결정 시 Streamable HTTP) 트랜스포트, 툴/리소스, SQL 가드, DB 어댑터 |
| (보류) 프론트엔드 | — | 관리 UI 등은 M2 이후 재검토. 당분간 없음 |
| (보류) 배포 | — | 바이너리/컨테이너 배포는 M1 이후. 빈 stub 금지 |

## 3. MCP 인터페이스 형태

### 3.1 Transport

| Transport | 우선순위 | 비고 |
|-----------|----------|------|
| stdio | M0–M1 필수 | Cursor / Claude Desktop 등 로컬 클라이언트 |
| Streamable HTTP | 미결정 | 원격·다중 클라이언트 시. `rmcp` HTTP 서버 기능 사용 |

### 3.2 Tools (확정 표면)

| Tool | 능력 | 입력(요약) | 출력 | 규칙 |
|------|------|------------|------|------|
| `list_tables` | 테이블 목록 | `schema?`, `connection?` | 테이블명·종류(table/view)·대략 행수(가능 시) | 메타데이터만 |
| `describe_table` | 테이블 상세 | `table`, `schema?`, `connection?` | 컬럼(이름·타입·null·default)·PK/FK/인덱스 요약 | 한 테이블 단위 |
| `sample_column` | 컬럼 샘플 | `table`, `column`, `schema?`, `limit?`, `connection?` | 샘플 값 목록·`is_code`·(코드면) 보존된 개행/펜스 가능한 텍스트 | 아래 §3.2.1 |
| `execute_sql` | SQL 수행 | `sql`, `connection?`, `params?` | 행(JSON)·메타(`row_count`, `truncated`) | **SELECT만** + row limit + timeout |

쓰기·DDL·임의 스크립트 툴은 제공하지 않는다.

#### 3.2.1 `sample_column` — code 처리

- 일반 값: 짧은 문자열/숫자 샘플. 긴 텍스트는 길이 상한으로 truncate + `truncated: true`.
- **코드인 경우** (`is_code: true`): 개행을 유지한 샘플을 반환하고, 응답에 코드로 소비하라는 신호를 둔다(구조화 필드 `is_code` + 필요 시 `language` 힌트).
- “코드” 판정(초안, DESIGN에서 확정): 컬럼 타입/이름 힌트(예: `text`/`json` + 이름에 `code`/`script`/`sql`/`query`) 또는 값 휴리스틱(다줄·중괄호/들여쓰기 비율). 클라이언트가 `as_code: true`로 강제할 수 있게 할지는 ROADMAP D9.

### 3.3 Resources

M1에서는 **툴만** 제공한다. MCP Resources URI는 두지 않는다(테이블/컬럼은 위 툴로 충분). 이후 필요 시 ROADMAP에서 재개.

### 3.4 결과·에러 스키마 (공통)

`execute_sql` 성공 예:

```json
{
  "columns": ["id", "name"],
  "rows": [{"id": 1, "name": "a"}],
  "row_count": 1,
  "truncated": false,
  "connection": "default"
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

에러: MCP 툴 에러로 반환. `code`는 안정 문자열 — `not_select` | `timeout` | `row_limit` | `sql_parse` | `not_found` | `connection` | `internal`. 메시지에 시크릿·풀 DSN 금지.

## 4. 서버 내부 경계 (형태만)

코드 도입 시 `server/DESIGN.md`에 상세화. 모듈 방향만 고정:

```
Client ──stdio/HTTP──▶ MCP layer (rmcp)
                           │
                           ▼
                      Guard (AST/SELECT-only, limit, timeout)
                           │
                           ▼
                      Engine adapter (sqlx 등)
                           │
                           ▼
                      PostgreSQL / MySQL / SQLite (엔진별)
```

- **Config:** 기동 시 로드. 연결 N개, 기본 연결, `readonly`, `max_rows`, `timeout_ms`.
- **DB 접근:** async + 커넥션 풀. TLS는 rustls 우선(네이티브 의존 최소화 목표 — DESIGN에서 확정).

## 5. 설정 형태 (초안)

환경변수 우선 예시(이름은 구현 시 DESIGN에서 확정):

| 변수 | 의미 |
|------|------|
| `DATABASE_URL` 또는 엔진별 URL | 단일 연결 빠른 경로 |
| `SQL_MCP_MAX_ROWS` | `execute_sql` 행 상한(예: 1000) |
| `SQL_MCP_SAMPLE_LIMIT` | `sample_column` 기본 샘플 수 |
| `SQL_MCP_TIMEOUT_MS` | 쿼리 타임아웃 |

다중 연결은 설정 파일(TOML/JSON) — 형식은 ROADMAP 미결정.
