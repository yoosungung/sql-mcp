# ROADMAP.md

수행 계획·순서·미결정 항목. 일정(일/주) 추정은 두지 않는다 — 마일스톤과 의존성으로만 나눈다.

## 목표

**시맨틱 레이어 DB**를 진실 원천으로 두고, (1) Rust MCP가 그 DB를 **SELECT·탐색**하고 (2) **Frontend**가 그 DB를 **관리**하는 시스템을 만든다.

MCP 능력: **테이블 목록/상세**, **컬럼 검색**, **컬럼 샘플(+code)**, **SELECT SQL**.  
참고 구현 수단: [`rmcp`](https://github.com/modelcontextprotocol/rust-sdk), sqlx류 가드. 시맨틱 레이어+MCP 선행 사례(OrionBelt, warehouse-mcp 등)는 모델 설계(D12) 참고용.

## 확정된 결정

| 항목 | 결정 |
|------|------|
| 데이터 원천 | **시맨틱 레이어 DB** (MCP는 여기만 사용) |
| 메타 스키마 | **얇게** — 원천 엔터프라이즈 스키마를 통째 미러하지 않음 ([ARCHITECTURE §1a](ARCHITECTURE.md)) |
| MCP 언어·SDK | Rust / `rmcp` (`2026-07-28`) |
| MCP 능력 | `list_tables`, `describe_table`, `search_columns`, `sample_column`, `execute_sql`(**SELECT만**) |
| MCP 쓰기 | 없음 |
| Frontend | **필수** — 시맨틱 레이어 DB 관리 UI (`frontend/`) |
| Resources | M1에서 없음 (툴만) |
| MCP transport | stdio (M1) |
| 개발 방식 | TDD, 문서 우선 ([AGENTS.md](AGENTS.md)) |

## 미결정

| ID | 질문 | 옵션 / 비고 |
|----|------|-------------|
| D1 | 시맨틱 DB 엔진 | **A)** SQLite **B)** PostgreSQL **C)** 둘 다. 권장: 로컬은 A, 공유/프로드는 B |
| D2 | SQL 크레이트 (MCP) | **A)** sqlx **B)** 네이티브. 권장: A |
| D5 | Streamable HTTP | M2+ 여부 |
| D7 | 배포 | 바이너리 / 컨테이너 / k8s — 필요 시만 |
| D9 | code 판정 | 휴리스틱만 vs `as_code` 플래그 |
| D10 | Admin API 위치 | **A)** 별도 `api/`(Rust) **B)** `server`에 MCP+HTTP 공존 **C)** Frontend BFF만. 권장: A(쓰기와 MCP 분리) |
| D11 | Frontend 스택 | 예: React/Vite, Next 등 — 팀 선호 확인 |
| D12 | 시맨틱 모델 필드 | **방향 확정: 얇은 메타.** 남는 선택 — 최소 테이블 집합(연결/테이블/컬럼/매핑)만 vs 메트릭·조인까지. YAML 병행 여부 |
| D13 | 물리 웨어하우스 | 시맨틱만 질의 vs 레이어가 물리 SQL 컴파일·실행까지 하는지 |

## 마일스톤

### M0 — 문서·계약

- [x] 문서 레이아웃 · ARCHITECTURE · ROADMAP · README
- [x] MCP 툴 표면 · SELECT-only
- [x] 시맨틱 레이어 DB + Frontend 관리 역할 분리
- [ ] D1·D2·D10·D11·D12 합의 (스키마·스택 없이 코드 금지에 가깝게)

**완료 기준:** 공유 DB·역할 분리·첫 엔진·Frontend/API 방향이 문서에 고정됨.

### M1 — 시맨틱 DB + MCP 읽기 (stdio)

의존: D1·D2 (최소). D12가 비어 있으면 **최소 스키마**(테이블/컬럼 메타를 담을 수 있는 수준)를 ARCHITECTURE에 한 절 추가한 뒤 진행.

1. 시맨틱 DB 마이그레이션/시드 (최소)
2. `server/` + `DESIGN.md` + Commands — `rmcp` stdio
3. SELECT 가드 + 툴 5개 (대상 = 시맨틱 DB)
4. 테스트: 가드 · 검색 · 샘플/code · 통합
5. Inspector/Cursor 스모크

**완료 기준:** 시맨틱 DB에 대해 다섯 툴이 동작하고 비-SELECT는 거부.

### M2 — Frontend 관리 UI

의존: D10·D11·D12.

1. Admin API (D10) + `DESIGN.md`
2. `frontend/` 스켈레톤 + 시맨틱 객체 CRUD·브라우저
3. MCP와 동일 DB를 본다는 통합 스모크 (UI로 넣은 정의 → MCP `list_tables`/`search_columns`에 보임)
4. (D5) Streamable HTTP 선택적
5. CI: fmt/clippy/test(+ frontend test)

**완료 기준:** UI로 시맨틱 객체를 관리하면 MCP 읽기에 반영된다.

### M3 — 모델 확장·배포·하드닝

- (D12/D13) 메트릭·조인·물리 실행 등 확장
- (D7) 실제 배포 산출물만
- 관측·시크릿 필터·릴리스

## 구현 시 작업 규칙 (요약)

1. 계획 변경 → 이 파일
2. 계약/인터페이스 변경 → `ARCHITECTURE.md`
3. 컴포넌트 내부 → `server/DESIGN.md` / `frontend/DESIGN.md` / (있으면) `api/DESIGN.md`
4. 코드: 스켈레톤 → 테스트 → 구현
