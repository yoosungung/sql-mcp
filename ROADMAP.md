# ROADMAP.md

수행 계획·순서·미결정 항목. 일정(일/주) 추정은 두지 않는다 — 마일스톤과 의존성으로만 나눈다.

## 목표

Rust(`rmcp`)로 **안전한 SQL MCP 서버**를 만든다. AI 클라이언트가 스키마를 탐색하고 read-only 질의를 실행할 수 있게 한다.

참고한 외부 패턴(경쟁·선행 사례):

- 최소 툴 표면: DBHub류 (`execute_sql` + `search_objects`)
- Rust+sqlx+가드: steward-mcp / dbmcp / sqlx-mcp
- 공식 SDK: [modelcontextprotocol/rust-sdk](https://github.com/modelcontextprotocol/rust-sdk) (`rmcp`)

우리 차별점(초안): **계약이 문서에 고정된 read-only-first**, 시크릿 비노출, TDD, 이 저장소 문서 레이아웃 준수.

## 확정된 결정

| 항목 | 결정 |
|------|------|
| 서버 언어 | Rust |
| MCP SDK | `rmcp` (spec `2026-07-28`) |
| 기본 안전 | read-only, row limit, timeout |
| 기본 transport | stdio |
| 개발 방식 | TDD, 문서 우선 ([AGENTS.md](AGENTS.md)) |

## 미결정 (다음 대화에서 고를 것)

| ID | 질문 | 옵션 / 비고 |
|----|------|-------------|
| D1 | 첫 엔진 | **A)** SQLite만 → **B)** PostgreSQL → **C)** Postgres+SQLite. 권장: A로 가드·툴 고정 후 B |
| D2 | SQL 크레이트 | **A)** sqlx **B)** 엔진별 네이티브. 권장: A |
| D3 | 다중 연결 | M1은 단일 `DATABASE_URL`만? 다중은 M2? |
| D4 | Resources | M1에 URI 리소스 넣을지, 툴만으로 갈지 |
| D5 | Streamable HTTP | M2에 넣을지, 로컬 stdio만 유지할지 |
| D6 | 쓰기 opt-in | 아예 영구 read-only vs 플래그로 mutation 툴 노출 |
| D7 | 배포 형태 | cargo install 바이너리 / 컨테이너 / k8s (`deploy/`) — 빈 stub 금지이므로 실제 필요 시만 |
| D8 | 프론트엔드 | 없음(권장) vs 최소 상태/설정 UI |

## 마일스톤

### M0 — 문서·저장소 골격

- [x] `AGENTS.md` 문서 레이아웃
- [x] `ARCHITECTURE.md` 계약·인터페이스 초안
- [x] 이 로드맵
- [ ] `README.md` 소개(구현 전 quickstart는 “아직 없음” 명시)
- [ ] 미결정 D1–D2 확정

**완료 기준:** 계약·계획이 저장소에 있고, 첫 엔진·sqlx 여부가 합의됨.

### M1 — 최소 동작 MCP (stdio)

순서: `server/` 스켈레톤 → `server/DESIGN.md` + `## Commands` → 실패하는 테스트 → 구현.

1. Cargo 워크스페이스/`server` 바이너리, `rmcp` stdio 서버
2. 설정: URL + readonly/max_rows/timeout
3. 가드: SQL 파서(또는 동등)로 read-only 강제 + LIMIT/상한
4. 툴: `search_objects`, `execute_sql`
5. 엔진: D1에서 고른 것 1개
6. 테스트: 가드 유닛 + SQLite(또는 테스트컨테이너) 통합
7. Cursor/Inspector로 stdio 스모크

**완료 기준:** 클라이언트가 스키마를 찾고 SELECT를 실행하며, 쓰기는 거부된다.

### M2 — 엔진·운영 확장

- 추가 엔진(Postgres/MySQL 등, D1 후속)
- (D3) 다중 연결 + `list_connections`
- (D4) Resources 또는 명시적 스킵
- (D5) Streamable HTTP
- `explain_sql` opt-in
- CI: `.github/workflows/ci.yml` (fmt, clippy, test) — 워크플로 추가 시 실제 통과 가능하게

**완료 기준:** 최소 2엔진, CI green, 원격 transport는 결정대로 on/off.

### M3 — 배포·하드닝

- (D6) 쓰기 정책 최종화
- (D7) 실제 배포 산출물만 추가 (`deploy/SETUP.md` 등)
- 관측: tracing, 구조화 로그(시크릿 필터)
- 릴리스: 버전 태그, 변경 로그

## 구현 시 작업 규칙 (요약)

1. 계획 변경 → 이 파일
2. 계약/인터페이스 변경 → `ARCHITECTURE.md`
3. 서버 내부 → `server/DESIGN.md`
4. 코드: 스켈레톤 → 테스트 → 구현
