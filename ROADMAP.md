# ROADMAP.md

수행 계획·순서·미결정 항목. 일정(일/주) 추정은 두지 않는다 — 마일스톤과 의존성으로만 나눈다.

## 목표

Rust MCP(`rmcp`) 서버: **임베디드 시맨틱 DB** + **복수 타깃 DB** 조회(SELECT) + **메타데이터 CRUD(JSON)**.  
Frontend / Admin API / **Streamable HTTP**는 **취소**.

## 확정된 결정

| 항목 | 결정 |
|------|------|
| 시맨틱 DB | **MCP 프로세스에 임베드** (원격 시맨틱 서버 없음) |
| 조회 대상 | **타깃 DB**, **복수** 허용 |
| 타깃 SQL | SELECT only |
| 메타 | MCP `metadata_*` CRUD, **JSON 바디** |
| 컬럼 검색 | 임베디드 메타 위 BM25 + 벡터 + hybrid |
| MCP 툴 | `list_targets`, `list_tables`, `describe_table`, `search_columns`, `sample_column`, `execute_sql`, `metadata_{create,get,list,update,delete}` |
| Transport | **stdio만** (Streamable HTTP 취소) |
| Frontend / Admin API | 취소 |
| 개발 | TDD, 문서 우선 |

## 검색·임베디드 DB 권장 (의견)

임베디드 전제 → **SQLite+FTS5+sqlite-vec** 또는 **libSQL/Turso(임베디드)** 가 자연스럽다. ParadeDB는 원격·무거운 편.

| 순위 | 조합 | 비고 |
|------|------|------|
| **1** | **libSQL/Turso 임베디드** 또는 **SQLite+FTS5+vec** | 프로세스 내 파일, BM25+벡터 |
| 2 | Postgres+ParadeDB | 임베디드 요구와 안 맞음 — 비권장 |

**임베딩 공급(D14):** API가 제일 쉬움. 벡터는 임베디드 DB에 저장.

## 미결정

| ID | 질문 | 옵션 / 비고 |
|----|------|-------------|
| D1 | 임베디드 시맨틱 엔진 | **A)** SQLite+FTS5+sqlite-vec **D)** libSQL/Turso 임베디드 — 둘 중 권장 |
| D2 | 타깃·시맨틱 클라이언트 | sqlx / rusqlite / libsql — 엔진에 맞춤 |
| D14 | 임베딩 | API / 로컬 ONNX / 둘 다 |
| D7 | 배포 | 바이너리 등 — 필요 시만 |
| D9 | code 판정 | 휴리스틱 vs `as_code` |
| D12 | 메타 `kind`·JSON 스키마 | target/table/column/mapping 필드 |
| D13 | 타깃 엔진 | 첫 타깃이 Postgres인지 등 |
| D15 | `metadata_update` | JSON merge-patch vs 전체 replace |

(제거: D5 Streamable HTTP, D10/D11 Frontend)

## 마일스톤

### M0 — 문서·계약

- [x] MCP-only · 임베디드 시맨틱 · 복수 타깃 · 메타 CRUD(JSON) · stdio only
- [ ] D1·D2·D12·D14 합의

### M1 — 임베디드 시맨틱 + stdio MCP

1. `server/` + 임베디드 DB 파일 + `DESIGN.md` / Commands
2. `metadata_*` CRUD (JSON) + 테스트
3. `list_targets` · 타깃 SELECT 가드 · `execute_sql` / `sample_column`
4. `search_columns` keyword (BM25); 벡터는 D14 후
5. Inspector/Cursor 스모크

**완료 기준:** 메타 JSON CRUD + 타깃 SELECT + (최소) BM25 검색이 stdio로 동작.

### M2 — 검색·타깃 확장

- vector/hybrid · (D9) code · 추가 타깃 엔진(D13) · CI

### M3 — 배포·하드닝

- (D7) · 관측 · 릴리스

## 구현 시 작업 규칙

1. 계획 → 이 파일 · 계약 → `ARCHITECTURE.md` · 내부 → `server/DESIGN.md`
2. 스켈레톤 → 테스트 → 구현
