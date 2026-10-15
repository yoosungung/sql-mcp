# ROADMAP.md

수행 계획·순서·미결정 항목. 일정(일/주) 추정은 두지 않는다 — 마일스톤과 의존성으로만 나눈다.

## 목표

Rust MCP(`rmcp`) 서버: **임베디드 시맨틱 DB** + **복수 타깃 DB** 조회(SELECT) + **메타데이터 CRUD(JSON)**.  
Frontend / Admin API / **stdio** / **Streamable HTTP**는 **취소**. Transport는 **일반 HTTP**.

## 확정된 결정

| 항목 | 결정 |
|------|------|
| 시맨틱 DB | **MCP 프로세스에 임베드** (원격 시맨틱 서버 없음) |
| 시맨틱 엔진 | **libSQL** (임베디드) — 네이티브 벡터·FTS, 파일 하나로 M1 CRUD·이후 검색에 적합 |
| 시맨틱 등록 문서 | `table` · `view` · `cube` · `metric` · `segment`. 스키마·예시는 `semantic/` |
| 조회 대상 | **타깃 DB**, **복수** 허용 |
| 타깃 SQL | SELECT only |
| 메타 | MCP `metadata_*` CRUD, **JSON 바디**. 검증은 `semantic/schema/document.schema.json` |
| 컬럼 검색 | BM25/벡터/hybrid **목표** — 인덱스 설계는 등록 문서와 함께 |
| MCP 툴 | `list_targets`, `list_tables`, `describe_table`, `search_columns`, `sample_column`, `execute_sql`, `metadata_{create,get,list,update,delete}` |
| Transport | **일반 HTTP만** (stdio·Streamable HTTP 취소) |
| Frontend / Admin API | 취소 |
| 개발 | TDD, 문서 우선 |

## 시맨틱=libSQL 선택 이유

임베디드 + (장차) BM25/벡터 + 지금 당장 JSON 메타 CRUD에 **libSQL이 잘 맞음**.

- 확장(`sqlite-vec`) 없이 벡터·FTS 경로가 엔진에 있음
- 프로세스 내 파일 DB → `rmcp` 안에 심기 자연스러움
- `document(kind, name, body)` 카탈로그로 CRUD 통로·이후 인덱스 추가가 가능
- 주의: hybrid는 앱 RRF. Turso 브랜드/API 이동은 있으나 M1 시맨틱은 `libsql` 핀(D2)
- 타깃 DB는 libSQL일 필요 없음(Postgres 등 별도, D13)

**임베딩 공급(D14):** API가 제일 쉬움. 벡터는 libSQL에 저장. M2에서 확정.

## 미결정

| ID | 질문 | 옵션 / 비고 |
|----|------|-------------|
| D1 | 임베디드 시맨틱 엔진 | **결정: libSQL** |
| D2 | 클라이언트 | **결정:** 시맨틱=`libsql` 0.9 (`core`만) · 타깃=`sqlx` 0.9(+postgres). **크레이트 분리**. Rust **1.94+** (`rust-toolchain.toml`) |
| D14 | 임베딩 | API / 로컬 ONNX / 둘 다 — **M2** |
| D7 | 배포 | 바이너리 등 — 필요 시만 |
| D9 | code 판정 | 휴리스틱 vs `as_code` |
| D12 | 시맨틱 등록 스키마 | **초안 있음** (`semantic/`). 검색 인덱스·스키마 단일화 등은 후속 |
| D13 | 타깃 엔진 | **결정: 첫 타깃 = Postgres** (`sqlx`). 확장은 M2+ |
| D15 | `metadata_update` | **결정: 전체 replace** (merge-patch 아님). `body`는 문서 JSON 전체 |
| — | 시맨틱 SQL에서 metric·segment 문법 | |
| — | `encryption`이 true일 때의 암호 방식 | |
| — | expression/filter 컬럼 이름 검사 시점 | 컴파일 단계 |

(제거: D5 Streamable HTTP, stdio, D10/D11 Frontend)

## 마일스톤

### M0 — 문서·계약

- [x] MCP-only · 임베디드 libSQL · 복수 타깃 · 메타 CRUD(JSON) · **일반 HTTP**
- [x] 시맨틱 등록 JSON Schema·예시·카탈로그 DDL (`semantic/`)
- [x] D2·D13·D15 합의 (D14는 M2)

### M1 — 타깃 조회 + 메타 CRUD (일반 HTTP)

1. [x] `server/` + **libSQL** 임베디드 + HTTP 리스너 + `DESIGN.md` / Commands
2. [x] `metadata_*` CRUD — `document.schema.json` 검증 + `document` 테이블 저장 · update=전체 replace(D15)
3. [x] `list_targets` · 타깃 SELECT 가드 · `list_tables` / `describe_table` / `execute_sql` / `sample_column` · 첫 엔진 Postgres(D13)
4. [x] `search_columns`는 스텁 또는 no-op 허용 — 인덱스 본구현은 M2
5. [x] HTTP 클라이언트로 스모크 (`cargo test -p sql-mcp`)

**완료 기준:** 타깃 SELECT·탐색 + 메타 JSON CRUD가 **일반 HTTP**로 동작. (Postgres 실연결 스모크는 `http_smoke` ignore 테스트·DSN 환경 선택)

### M2 — 검색·컴파일

- `search_columns` BM25/벡터/hybrid 본구현 · (D14)(D9) · CI · 타깃 엔진 확장(D13 이후)
- SELECT AST 검사와 물리 SQL 컴파일

### M3 — 배포·하드닝

- (D7) · 관측 · 릴리스

## 구현 시 작업 규칙

1. 계획 → 이 파일 · 계약 → `ARCHITECTURE.md` · 등록 스키마 → `semantic/` · 서버 내부 → `server/DESIGN.md`
2. 스켈레톤 → 테스트 → 구현
