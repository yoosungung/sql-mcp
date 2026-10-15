# ROADMAP.md

수행 계획·순서·미결정 항목. 일정(일/주) 추정은 두지 않는다 — 마일스톤과 의존성으로만 나눈다.

## 목표

**시맨틱 레이어 DB**를 원천으로, Rust MCP가 그 DB를 **SELECT·탐색**하는 서버를 만든다.  
Frontend / Admin UI / Admin API는 **취소(범위 밖)**.

MCP 능력: **테이블 목록/상세**, **컬럼 검색(BM25·벡터·hybrid)**, **컬럼 샘플(+code)**, **SELECT SQL**.

## 확정된 결정

| 항목 | 결정 |
|------|------|
| 데이터 원천 | **시맨틱 레이어 DB** (MCP는 여기만 사용) |
| 메타 스키마 | **얇게** ([ARCHITECTURE §1a](ARCHITECTURE.md)) |
| 컬럼 검색 | BM25 + 벡터(+임베딩) + hybrid(RRF) |
| MCP 언어·SDK | Rust / `rmcp` (`2026-07-28`) |
| MCP 능력 | `list_tables`, `describe_table`, `search_columns`, `sample_column`, `execute_sql`(**SELECT만**) |
| MCP 쓰기 | 없음 |
| Frontend / Admin API | **취소** — `frontend/`, `api/` 만들지 않음 |
| 메타 적재 | 마이그레이션·시드·SQL/설정 (non-MCP) |
| Resources | M1에서 없음 (툴만) |
| MCP transport | stdio (M1) |
| 개발 방식 | TDD, 문서 우선 ([AGENTS.md](AGENTS.md)) |

## 검색 DB 권장 (의견)

| 순위 | 조합 | 쉬운 점 | 아쉬운 점 |
|------|------|---------|-----------|
| **1′ (추천 후보)** | **libSQL / Turso** (네이티브 벡터 + FTS/`fts_score`) | 확장 없이 벡터·키워드, SQLite 호환 | `libsql`/`turso` 클라이언트; hybrid는 앱 RRF; FTS 버전 확인 |
| **1 (로컬 최소)** | **SQLite + FTS5 + sqlite-vec** | 파일 하나·익숙한 스택 | 확장 로딩 |
| **2 (프로덕트 SQL)** | **Postgres + ParadeDB + pgvector** | SQL 원샷 BM25·벡터·RRF | 운영 무거움 |
| 3 | Postgres + pgvector만 | 벡터만 쉬움 | BM25 부족 |
| 비권장(초기) | DuckDB FTS+VSS | 분석용 | MCP 서버 원천으로 우선순위 낮음 |

**임베딩:** 별축 — **API(D14-A)가 제일 쉬움**.

**종합 추천:** **D1=D(libSQL/Turso)** 또는 확장 감수 시 A(SQLite). 엔터프라이즈 SQL이면 B(ParadeDB).

## 미결정

| ID | 질문 | 옵션 / 비고 |
|----|------|-------------|
| D1 | 시맨틱 DB 엔진 | **A)** SQLite+FTS5+sqlite-vec **B)** Postgres+ParadeDB+pgvector **C)** Postgres+pgvector만 **D)** libSQL/Turso — 권장 후보 |
| D2 | SQL 크레이트 | **A)** sqlx **B)** rusqlite(+확장) **C)** libsql/turso 클라이언트 (D1=D) |
| D14 | 임베딩 공급 | **A)** 외부 API **B)** 로컬 ONNX **C)** 둘 다 |
| D5 | Streamable HTTP | M2+ 여부 |
| D7 | 배포 | 바이너리 / 컨테이너 / k8s — 필요 시만 |
| D9 | code 판정 | 휴리스틱만 vs `as_code` |
| D12 | 시맨틱 모델 필드 | 최소 테이블 집합 vs 메트릭·조인 · YAML 병행 |
| D13 | 물리 웨어하우스 | 시맨틱만 질의 vs 물리 SQL 컴파일·실행 |

(제거됨: D10 Admin API, D11 Frontend 스택 — Frontend 취소에 따라)

## 마일스톤

### M0 — 문서·계약

- [x] 문서 레이아웃 · ARCHITECTURE · ROADMAP · README
- [x] MCP 툴 · SELECT-only · 시맨틱 DB · 얇은 메타 · 검색(BM25/벡터)
- [x] Frontend **취소** 반영
- [ ] D1·D2·D12·D14 합의

**완료 기준:** MCP-only 범위·검색 엔진 방향이 문서에 고정됨.

### M1 — 시맨틱 DB + MCP (stdio)

의존: D1·D2·D14(벡터 시)·D12 최소 스키마.

1. 시맨틱 DB 마이그레이션/시드 (메타 + FTS/벡터)
2. `server/` + `DESIGN.md` + Commands — `rmcp` stdio
3. SELECT 가드 + 툴 5개 (`search_columns` keyword/vector/hybrid)
4. 테스트 · Inspector/Cursor 스모크

**완료 기준:** 다섯 툴 동작, 컬럼 검색 BM25·(설정 시) 벡터, 비-SELECT 거부.

### M2 — 검색·운영 강화

- (D5) Streamable HTTP 선택
- (D9) code 판정 고도화
- CI (fmt, clippy, test)
- (D12/D13) 모델 확장(필요 시만)

### M3 — 배포·하드닝

- (D7) 실제 배포 산출물만 · 관측·릴리스

## 구현 시 작업 규칙 (요약)

1. 계획 변경 → 이 파일
2. 계약/인터페이스 변경 → `ARCHITECTURE.md`
3. 서버 내부 → `server/DESIGN.md`
4. 코드: 스켈레톤 → 테스트 → 구현
