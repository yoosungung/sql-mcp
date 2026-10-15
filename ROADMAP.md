# ROADMAP.md

수행 계획·순서·미결정 항목. 일정(일/주) 추정은 두지 않는다 — 마일스톤과 의존성으로만 나눈다.

## 목표

Rust MCP(`rmcp`) 서버: **임베디드 시맨틱 DB** + **복수 타깃 DB** 조회(SELECT) + **메타데이터 CRUD(JSON)**.  
Frontend / Admin API / **Streamable HTTP**는 **취소**.

## 확정된 결정

| 항목 | 결정 |
|------|------|
| 시맨틱 DB | **MCP 프로세스에 임베드** (원격 시맨틱 서버 없음) |
| 시맨틱 엔진 | **libSQL** (임베디드) — 네이티브 벡터·FTS, 파일 하나로 M1 CRUD·이후 검색에 적합 |
| 시맨틱 레이어 모델 | **향후 정의** (D12) — 지금은 저장소·CRUD 통로만 |
| 조회 대상 | **타깃 DB**, **복수** 허용 |
| 타깃 SQL | SELECT only |
| 메타 | MCP `metadata_*` CRUD, **JSON 바디** (스키마는 D12 이후) |
| 컬럼 검색 | BM25/벡터/hybrid **목표** — 인덱스 설계는 시맨틱 정의 후 |
| MCP 툴 | `list_targets`, `list_tables`, `describe_table`, `search_columns`, `sample_column`, `execute_sql`, `metadata_{create,get,list,update,delete}` |
| Transport | **stdio만** (Streamable HTTP 취소) |
| Frontend / Admin API | 취소 |
| 개발 | TDD, 문서 우선 |

## 시맨틱=libSQL 선택 이유

임베디드 + (장차) BM25/벡터 + 지금 당장 JSON 메타 CRUD에 **libSQL이 잘 맞음**.

- 확장(`sqlite-vec`) 없이 벡터·FTS 경로가 엔진에 있음
- 프로세스 내 파일 DB → `rmcp` 안에 심기 자연스러움
- 시맨틱 모델이 아직 비어 있어도 CRUD 통로·이후 인덱스 추가로 확장 가능
- 주의: Rust는 `libsql`/`turso` 클라이언트(D2), hybrid는 앱 RRF, 브랜드/API가 Turso 쪽으로 이동 중이라 버전 핀 필요
- 타깃 DB는 libSQL일 필요 없음(Postgres 등 별도, D13)

**임베딩 공급(D14):** API가 제일 쉬움. 벡터는 libSQL에 저장.

## 미결정

| ID | 질문 | 옵션 / 비고 |
|----|------|-------------|
| D1 | 임베디드 시맨틱 엔진 | **결정: libSQL** |
| D2 | 클라이언트 | 시맨틱=`libsql`/`turso` · 타깃=sqlx 등(엔진별). 한 크레이트로 통일할지 |
| D14 | 임베딩 | API / 로컬 ONNX / 둘 다 |
| D7 | 배포 | 바이너리 등 — 필요 시만 |
| D9 | code 판정 | 휴리스틱 vs `as_code` |
| D12 | **시맨틱 레이어 전체** | **향후.** 모델·kind·JSON 스키마·검색 인덱스. M1을 막지 않음 |
| D13 | 타깃 엔진 | 첫 타깃이 Postgres인지 등 |
| D15 | `metadata_update` | JSON merge-patch vs 전체 replace (시맨틱 정의와 함께) |

(제거: D5 Streamable HTTP, D10/D11 Frontend)

## 마일스톤

### M0 — 문서·계약

- [x] MCP-only · 임베디드 저장소 · 복수 타깃 · 메타 CRUD(JSON) · stdio only
- [x] 시맨틱 레이어 모델 = **향후 정의**로 명시
- [x] D1 = libSQL
- [ ] D2·D13·D14 합의 (D12는 후속)

### M1 — 타깃 조회 + 메타 CRUD 통로 (stdio)

1. `server/` + **libSQL** 임베디드 파일(스키마 최소/플레이스홀더) + `DESIGN.md` / Commands
2. `metadata_*` CRUD — **임의/최소 JSON** 왕복 (시맨틱 스키마 비의존)
3. `list_targets` · 타깃 SELECT 가드 · `list_tables` / `describe_table` / `execute_sql` / `sample_column`
4. `search_columns`는 스텁 또는 no-op 허용 — 시맨틱 정의(D12) 후 본구현
5. Inspector/Cursor 스모크

**완료 기준:** 타깃 SELECT·탐색 + 메타 JSON CRUD 통로가 stdio로 동작. 시맨틱 도메인 모델 완성 불필요.

### M2 — 시맨틱 레이어 정의·검색

- (D12) 시맨틱 모델·JSON 스키마 확정 후 ARCHITECTURE §4 기입
- `search_columns` BM25/벡터/hybrid 본구현 · (D14)(D9) · CI · (D13) 타깃 엔진 확장

### M3 — 배포·하드닝

- (D7) · 관측 · 릴리스

## 구현 시 작업 규칙

1. 계획 → 이 파일 · 계약 → `ARCHITECTURE.md` · 내부 → `server/DESIGN.md`
2. 스켈레톤 → 테스트 → 구현
