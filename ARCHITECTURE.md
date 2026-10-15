# ARCHITECTURE.md

컴포넌트 *간* 계약과 인터페이스 형태. 내부 구현은 `server/DESIGN.md`(코드 도입 시)에 둔다.

## 1. 계약사항 (불변 규칙)

1. **언어.** MCP 서버는 **Rust**로 구현한다. 공식 SDK는 [`rmcp`](https://crates.io/crates/rmcp) (MCP spec `2026-07-28`, 하위 호환 유지).
2. **역할.** 이 저장소는 AI 클라이언트가 SQL 데이터베이스를 **안전하게** 탐색·질의할 수 있게 하는 MCP 서버다. DB 자격 증명은 채팅/툴 인자로 받지 않는다 — 기동 시 설정(환경변수·설정 파일·시크릿 스토어)으로만 주입한다.
3. **기본 안전.** 기본 모드는 **read-only**. 쓰기(INSERT/UPDATE/DELETE/DDL)는 명시적 opt-in 없이는 거부한다. 행 수 상한·쿼리 타임아웃을 강제한다.
4. **최소 툴 표면.** 기본 노출 툴은 소수(토큰 효율). 스키마 탐색과 질의 실행을 분리한다. 부가 툴은 설정으로 opt-in.
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

### 3.2 Tools (초안)

기본(항상 on):

| Tool | 입력(요약) | 출력 | 규칙 |
|------|------------|------|------|
| `search_objects` | `query?`, `schema?`, `kind?` (`table`/`view`/`column`/…) | 객체 목록·요약 | progressive disclosure — 인자 없으면 DB/스키마 목록, 좁히면 컬럼·PK/FK |
| `execute_sql` | `sql`, `connection?`, `params?` | 행(JSON)·메타(`row_count`, `truncated`) | read-only 가드 + row limit + timeout |

Opt-in (설정으로 켠 경우만 등록):

| Tool | 용도 |
|------|------|
| `explain_sql` | 실행 계획만 반환 |
| `list_connections` | 등록된 연결 이름·엔진·read-only 여부(시크릿 제외) |

쓰기 툴은 **별도 플래그** 없이는 등록하지 않는다(툴 목록에도 안 보임).

### 3.3 Resources (초안, opt-in 가능)

| URI | 내용 |
|-----|------|
| `sql://connections` | 연결 목록(메타만) |
| `sql://{conn}/schema` | 스키마 요약 |
| `sql://{conn}/tables/{table}` | 테이블 컬럼·키 |

리소스 vs `search_objects` 중복은 ROADMAP 미결정 항목으로 둔다. 초기에는 툴만으로도 M1 완료 가능.

### 3.4 결과·에러 스키마 (공통)

성공(툴 텍스트/구조화 콘텐츠):

```json
{
  "columns": ["id", "name"],
  "rows": [{"id": 1, "name": "a"}],
  "row_count": 1,
  "truncated": false,
  "connection": "default"
}
```

에러: MCP 툴 에러로 반환. `code`는 안정 문자열 — `readonly_violation` | `timeout` | `row_limit` | `sql_parse` | `connection` | `internal`. 메시지에 시크릿·풀 DSN 금지.

## 4. 서버 내부 경계 (형태만)

코드 도입 시 `server/DESIGN.md`에 상세화. 모듈 방향만 고정:

```
Client ──stdio/HTTP──▶ MCP layer (rmcp)
                           │
                           ▼
                      Guard (AST/read-only, limit, timeout)
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
| `SQL_MCP_READONLY` | 기본 `true` |
| `SQL_MCP_MAX_ROWS` | 기본 상한(예: 1000) |
| `SQL_MCP_TIMEOUT_MS` | 쿼리 타임아웃 |

다중 연결은 설정 파일(TOML/JSON) — 형식은 ROADMAP 미결정.
