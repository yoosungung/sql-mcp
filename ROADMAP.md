# ROADMAP.md

수행 계획·순서·미결정 항목. 일정(일/주) 추정은 두지 않는다 — 마일스톤과 의존성으로만 나눈다.

## 목표

**시맨틱 레이어 DB**를 진실 원천으로 두고, (1) Rust MCP가 그 DB를 **SELECT·탐색**하고 (2) **Frontend**가 그 DB를 **관리**하는 시스템을 만든다.

MCP 능력: **테이블 목록/상세**, **컬럼 검색(BM25·벡터·hybrid)**, **컬럼 샘플(+code)**, **SELECT SQL**.  
참고 구현 수단: [`rmcp`](https://github.com/modelcontextprotocol/rust-sdk), sqlx/rusqlite. 시맨틱 레이어 선행 사례는 모델 설계(D12) 참고용.

## 확정된 결정

| 항목 | 결정 |
|------|------|
| 데이터 원천 | **시맨틱 레이어 DB** (MCP는 여기만 사용) |
| 메타 스키마 | **얇게** — 원천 엔터프라이즈 스키마를 통째 미러하지 않음 ([ARCHITECTURE §1a](ARCHITECTURE.md)) |
| 컬럼 검색 | BM25 + 벡터(+임베딩) + hybrid(RRF) 지원 목표 ([ARCHITECTURE §3.2.2](ARCHITECTURE.md)) |
| MCP 언어·SDK | Rust / `rmcp` (`2026-07-28`) |
| MCP 능력 | `list_tables`, `describe_table`, `search_columns`, `sample_column`, `execute_sql`(**SELECT만**) |
| MCP 쓰기 | 없음 |
| Frontend | **필수** — 시맨틱 레이어 DB 관리 UI (`frontend/`) |
| Resources | M1에서 없음 (툴만) |
| MCP transport | stdio (M1) |
| 개발 방식 | TDD, 문서 우선 ([AGENTS.md](AGENTS.md)) |

## 검색 DB 권장 (의견)

메타가 작다는 전제에서:

| 순위 | 조합 | 쉬운 점 | 아쉬운 점 |
|------|------|---------|-----------|
| **1 (제일 쉬움·MVP)** | **SQLite + FTS5(BM25) + sqlite-vec** | 파일 하나, 확장만 얹으면 hybrid 가능, 얇은 메타와 잘 맞음 | Frontend·MCP 동시 쓰기 시 잠금; sqlx보다 rusqlite가 확장에 편함 |
| **2 (프로덕트·동시성)** | **Postgres + ParadeDB `pg_search` + pgvector** | SQL 한곳에서 BM25·벡터·hybrid(RRF) 문서화됨, Frontend와 공유 수월 | 확장/이미지 운영 비용, 로컬 기동이 무거움 |
| 3 | Postgres + pgvector만 | 벡터는 쉬움(Neon/Supabase 등) | 진짜 BM25는 별도(ParadeDB 또는 앱 측 FTS) |
| 비권장(초기) | DuckDB FTS+VSS | 분석엔 좋음 | VSS 실험적, 다중 작성자 UI와 궁합 약함 |

**임베딩:** DB 선택이 아니라 별축. **당장 제일 쉬운 건 API 임베딩(D14-A)** 이고, 벡터 컬럼만 시맨틱 DB에 두면 된다. 나중에 로컬 ONNX로 바꿔도 스키마는 동일하다.

**종합 추천:** 지금은 **A(SQLite FTS5+vec)로 `search_columns` hybrid를 먼저 증명**하고, Frontend·다중 접속이 필요해지면 **B(Postgres+ParadeDB)** 로 옮기거나, 처음부터 공유 DB가 필요하면 B로 시작.

## 미결정

| ID | 질문 | 옵션 / 비고 |
|----|------|-------------|
| D1 | 시맨틱 DB 엔진 (검색 포함) | **A)** SQLite+FTS5+sqlite-vec **B)** Postgres+ParadeDB+pgvector **C)** Postgres+pgvector만 **D)** libSQL/Turso(네이티브 벡터+FTS) — 권장 후보 |
| D2 | SQL 크레이트 (MCP) | **A)** sqlx **B)** 네이티브(rusqlite 등). SQLite 확장이면 B가 수월 |
| D14 | 임베딩 공급 | **A)** 외부 API(가장 쉬움) **B)** 로컬 ONNX **C)** 둘 다 |
| D5 | Streamable HTTP | M2+ 여부 |
| D7 | 배포 | 바이너리 / 컨테이너 / k8s — 필요 시만 |
| D9 | code 판정 | 휴리스틱만 vs `as_code` 플래그 |
| D10 | Admin API 위치 | **A)** 별도 `api/` **B)** `server` 공존 **C)** Frontend BFF. 권장: A |
| D11 | Frontend 스택 | React/Vite, Next 등 |
| D12 | 시맨틱 모델 필드 | 얇은 메타 방향 확정. 최소 테이블 집합 vs 메트릭·조인 · YAML 병행 |
| D13 | 물리 웨어하우스 | 시맨틱만 질의 vs 물리 SQL 컴파일·실행 |

## 마일스톤

### M0 — 문서·계약

- [x] 문서 레이아웃 · ARCHITECTURE · ROADMAP · README
- [x] MCP 툴 표면 · SELECT-only
- [x] 시맨틱 레이어 DB + Frontend 관리 역할 분리
- [x] 얇은 메타 · 검색(BM25/벡터) 목표
- [ ] D1·D2·D10·D11·D12·D14 합의

**완료 기준:** 공유 DB·역할 분리·검색 엔진·Frontend/API 방향이 문서에 고정됨.

### M1 — 시맨틱 DB + MCP 읽기 (stdio)

의존: D1·D2·D14(벡터 쓸 경우). D12 최소 스키마를 ARCHITECTURE에 고정한 뒤 진행.

1. 시맨틱 DB 마이그레이션/시드 (메타 + FTS/벡터 인덱스)
2. `server/` + `DESIGN.md` + Commands — `rmcp` stdio
3. SELECT 가드 + 툴 5개 (`search_columns`에 keyword/vector/hybrid)
4. 테스트: 가드 · BM25/벡터/hybrid · 샘플/code · 통합
5. Inspector/Cursor 스모크

**완료 기준:** 시맨틱 DB에 대해 다섯 툴이 동작하고, 컬럼 검색이 BM25·(설정 시) 벡터로 동작하며 비-SELECT는 거부.

### M2 — Frontend 관리 UI

의존: D10·D11·D12.

1. Admin API (D10) + `DESIGN.md`
2. `frontend/` — 시맨틱 CRUD·브라우저 · (가능하면) 검색 미리보기
3. UI로 넣은 정의 → MCP 검색/목록에 반영되는 통합 스모크
4. (D5) Streamable HTTP 선택적
5. CI

**완료 기준:** UI 관리 결과가 MCP 읽기·검색에 반영된다.

### M3 — 모델 확장·배포·하드닝

- (D12/D13) 확장 · (D7) 배포 · 관측·릴리스

## 구현 시 작업 규칙 (요약)

1. 계획 변경 → 이 파일
2. 계약/인터페이스 변경 → `ARCHITECTURE.md`
3. 컴포넌트 내부 → `server/DESIGN.md` / `frontend/DESIGN.md` / (있으면) `api/DESIGN.md`
4. 코드: 스켈레톤 → 테스트 → 구현
