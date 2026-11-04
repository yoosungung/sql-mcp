# ROADMAP.md

수행 계획·순서·미결정 항목. 일정(일/주) 추정은 두지 않는다 — 마일스톤과 의존성으로만 나눈다. 완료는 한 줄, 남은 일만 상세. 계약 본문은 [ARCHITECTURE.md](ARCHITECTURE.md).

BIRD DW agent·eval 계획·게이트는 [client/ROADMAP.md](client/ROADMAP.md) (별도 repo 후보).

## 목표

Rust MCP(`rmcp`) 서버: **임베디드 시맨틱 DB** + **복수 타깃 DB** 조회(SELECT) + **작성 HTTP `/api`(메타 JSON · 물리 카탈로그)**. Transport는 **일반 HTTP**(기본) + **stdio**(로컬 조회). 세션형 Streamable HTTP는 취소.

## 완료

- M0: 문서·계약. MCP-only, 임베디드 libSQL, 일반 HTTP + stdio.
- M1: 타깃 SELECT·탐색·메타 JSON CRUD가 일반 HTTP로 동작.
- M2: `search_columns`(BM25/벡터/hybrid), 컴파일, CI, SQLite 타깃.
- M3: release·컨테이너·`deploy/k8s/base/`, MySQL·Oracle, RBAC·타임아웃.
- M4: 타입별 `meta_*`, `execute_sql`은 시맨틱 SQL만, 요약 뷰·pruning·union.
- M5: SELECT 절 전체, 전 계층 `role`, 카탈로그 캐시·`SQL_MCP_TIMEOUT_MS`.
- 같은 `target` cube JOIN/UNION · `execute_sql.dry_run`.
- D1: 시맨틱 엔진은 libSQL.
- D2: 시맨틱 `libsql` 0.9(core), 타깃 `sqlx` 0.9, Oracle `oracledb` thin, 크레이트 분리, Rust 1.94+.
- D7: release 바이너리 + 컨테이너. Kustomize는 `deploy/k8s/base/`.
- D9: dimension `as_code`는 집계 후 `{dim}__value`.
- D12: JSON 교환 `document.schema.json`, 저장은 `meta_*`, 검색은 FTS5 + embedding.
- D13: Postgres·MySQL·SQLite·Oracle. 로컬 SQLite 스모크(예제 타깃은 client 패키지).
- D14: 임베딩은 외부 API만. 없거나 기동 시 연결 실패면 BM25. 로컬 ONNX 비채택.
- D15: `metadata_update`는 전체 replace.
- D16: metric·segment 식은 새 DSL 없이 SQL 조각 + AST 제한.
- D17: `encryption`은 응답 마스킹.
- D18: 저장은 연결 그래프, 컴파일은 AST 식별자.
- D19: `execute_sql`은 시맨틱 SELECT만.
- D20: table/view/cube `target` 일치. 같은 target JOIN만.
- D21: DialectProfile emit. 자유 트랜스파일·실행 bind rewrite 비채택.
- D22: `name` / `physical_ident` 분리.
- D23: Grouped / Ungrouped. 비-`DISTINCT` Ungrouped cube JOIN 거절.
- D24: metric `group_by`는 기본 grain. 질의 축이 있으면 override.
- D25: `CURRENT_DATE` / `NOW` / `DATEDIFF` / `EXTRACT`.
- D26: metric/segment는 `describe_table.examples`. `execute_sql`은 dim/fact ad-hoc.
- D27: 리터럴-only → existence dim. `SELECT *` 거절.
- D28: expression은 스칼라. `additive: false`면 `SUM`/`AVG`/`COUNT(col)` 거절.
- 제거: D5 세션형 Streamable HTTP, D10/D11 Frontend. stdio는 로컬용으로 복원.
- semantic-ui: 루트 Vite React 작성 화면. `GET /ui`가 `semantic-ui/dist`를 서빙. 내용 [`semantic-ui/DESIGN.md`](semantic-ui/DESIGN.md).
- D30: 메타 CRUD·물리 카탈로그는 HTTP `/api`. MCP는 조회만. `SQL_MCP_MODE` 폐기.

## 남은 일 — Cube 관계 모델 · 팬아웃 거절 (D29)

**구현됨. 거절 플래그는 기본 끔.** 큐브 그레인 = 허브 `table.pk`. `table.fk`가 허브 `dimensions`를 가리키면 `many_to_one`. 컴파일은 MetricFlow식 **거절**(primary–foreign 팬아웃 · 캐즘). 조회 표면은 `execute_sql` 시맨틱 SELECT(D19). **비채택:** Cube.dev 런타임 교체 · Looker 심바가 재작성 · `Never JOIN` 설명문으로 거절.

`viewJoin.type`은 SQL 조인(`inner`/`left`/`right`/`full`)이고, `relationship`이 카디널리티다. D23 비-`DISTINCT` Ungrouped cube JOIN 거절은 유지. 뷰 안 행 배율 거절은 `SQL_MCP_GRAIN_GUARD`에 적힌 target에서만.

1. **관계 스키마.** `viewJoin.relationship` · 허브 `pk` 그레인. 형제 팩트·브릿지는 뷰에 흡수하지 않음.
2. **컴파일 거절.** 팬아웃·캐즘 → `validation`. `many_to_one`·Grouped cube JOIN은 통과. 합계 재작성 없음.
3. **조회 경로.** 거절은 플래그. 검증 통과 DB만 켠다.

- [x] `viewJoin.relationship` + 허브 `pk` 그레인 스키마
- [x] 팬아웃·캐즘 `validation` 거절 테스트 → 컴파일러
- [x] 거절 플래그 `SQL_MCP_GRAIN_GUARD`. 켠 DB 없음
- [ ] 거절 켠 타깃별 게이트 — BIRD EX·그레인 검증은 [client/ROADMAP.md](client/ROADMAP.md)

## semantic-ui (작성 화면) — 구현됨

D10/D11 조회 Admin API 취소는 유지. D30이 작성 HTTP `/api`. 조회 계약(`execute_sql` 등)은 바꾸지 않는다. 정본은 meta JSON. `semantic-map.md`는 생성 투영이라 이 화면에서 고치지 않는다. UI 설계 [`semantic-ui/DESIGN.md`](semantic-ui/DESIGN.md).

- [x] 루트 `semantic-ui/` Vite React. `server/` 컴파일은 그대로
- [x] 타깃 셀렉터 · Physical join/union/minus · Cubes · `/api` CRUD · `GET /ui/`

## 남은 일 — semantic-ui · 스키마 화면 + API 오류

정본 [`semantic/schema/document.schema.json`](semantic/schema/document.schema.json). **화면이 스키마 필드**를 편집. **클라 JSON Schema 가드·Save 비활성은 없다.** 거절은 `POST`/`PUT /api/meta` 400을 배너에. 내용 [`semantic-ui/DESIGN.md`](semantic-ui/DESIGN.md).

- [x] `/api` 오류 배너 · Columns/Map members · cube Detail · Rename/Delete 규칙

## 구현 시 작업 규칙

1. 계획 → 이 파일 · 계약 → `ARCHITECTURE.md` · 등록 스키마 → `semantic/` · 서버 내부 → `server/DESIGN.md` · BIRD agent/eval → `client/ROADMAP.md`
2. 스켈레톤 → 테스트 → 구현
