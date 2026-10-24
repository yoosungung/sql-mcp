# ROADMAP.md

수행 계획·순서·미결정 항목. 일정(일/주) 추정은 두지 않는다 — 마일스톤과 의존성으로만 나눈다. 완료는 한 줄, 남은 일만 상세. 계약 본문은 [ARCHITECTURE.md](ARCHITECTURE.md).

## 목표

Rust MCP(`rmcp`) 서버: **임베디드 시맨틱 DB** + **복수 타깃 DB** 조회(SELECT) + **메타데이터 CRUD(JSON)**. Transport는 **일반 HTTP**(기본) + **stdio**(로컬). Frontend / Admin API / 세션형 Streamable HTTP는 취소.

## 완료

- M0: 문서·계약. MCP-only, 임베디드 libSQL, 일반 HTTP + stdio.
- M1: 타깃 SELECT·탐색·메타 JSON CRUD가 일반 HTTP로 동작.
- M2: `search_columns`(BM25/벡터/hybrid), 컴파일, CI, SQLite·BIRD `target_db/`.
- M3: release·컨테이너·`deploy/k8s/base/`, MySQL·Oracle, RBAC·타임아웃.
- M4: 타입별 `meta_*`, `execute_sql`은 시맨틱 SQL만, 요약 뷰·pruning·union.
- M5: SELECT 절 전체, 전 계층 `role`, 카탈로그 캐시·`SQL_MCP_TIMEOUT_MS`.
- 같은 `target` cube JOIN/UNION · `execute_sql.dry_run`.
- D1: 시맨틱 엔진은 libSQL.
- D2: 시맨틱 `libsql` 0.9(core), 타깃 `sqlx` 0.9, Oracle `oracledb` thin, 크레이트 분리, Rust 1.94+.
- D7: release 바이너리 + 컨테이너. Kustomize는 `deploy/k8s/base/`.
- D9: dimension `as_code`는 집계 후 `{dim}__value`.
- D12: JSON 교환 `document.schema.json`, 저장은 `meta_*`, 검색은 FTS5 + embedding.
- D13: Postgres·MySQL·SQLite·Oracle. 테스트 타깃은 BIRD Mini-Dev SQLite.
- D14: 임베딩은 외부 API만. 없으면 BM25. 로컬 ONNX 비채택.
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

## 남은 일 — Cube 관계 모델 · 팬아웃 거절 (D29)

**구현됨. 거절 플래그는 꺼져 있고 BIRD는 아직 돌리지 않았다.** 허브 정본 `semantic_plan.yaml`(팩트 테이블 1 = view+cube 1)은 유지하고, 그 옆에 Cube식 관계를 둔다. 큐브 그레인 = 허브 `table.pk`. `table.fk`가 허브 `dimensions`를 가리키면 `many_to_one`. 컴파일은 MetricFlow식 **거절**(primary–foreign 팬아웃 · 캐즘). 조회 표면은 `execute_sql` 시맨틱 SELECT(D19). **비채택:** Cube.dev 런타임 교체 · Looker 심바가 재작성(행 집합이 바뀜) · `Never JOIN` 설명문으로 거절.

전제: 에이전트는 `semantic_plan.yaml`을 직접 읽지 않는다. `generate_meta`가 허브로 만든 큐브를 `execute_sql`로 조회한다. `viewJoin.type`은 SQL 조인(`inner`/`left`/`right`/`full`)이고, `relationship`이 카디널리티다. D23 비-`DISTINCT` Ungrouped cube JOIN 거절은 유지된다. 뷰 안 행 배율 거절은 `SQL_MCP_GRAIN_GUARD`에 적힌 target에서만 돈다.

컷오버 전까지 기존 큐브 JSON·설명문은 그대로 두고, 관계와 거절은 옆에 얹는다. 계약 문서는 구현 착수 때 `ARCHITECTURE.md`·`semantic/schema/document.schema.json`·`server/DESIGN.md`를 이 절과 함께 고친다.

1. **yaml 옆에 Cube 관계.** 허브 목록은 팩트 정본으로 유지한다. 관계 초안은 허브 + `table.pk` + `table.fk`에서 만들고, 메타 디렉터리를 비우지 않는다. `viewJoin`에 SQL `type`과 별도로 `relationship`(`many_to_one` / `one_to_one` / `one_to_many`)을 둔다. 큐브 그레인은 허브 테이블 `pk`다.
2. **팩트 큐브 ↔ pk/fk.** 각 허브에서 팩트 `pk`가 그레인이다. `fk` 대상이 그 허브 `dimensions`에 있으면 `many_to_one`이다. `fk`가 다른 허브(형제 팩트)를 가리키면 뷰에 흡수하지 않고 기존 cube JOIN으로 남긴다. 브릿지(`connected`, `attendance` 등)는 자체 허브 `pk`다. 다대다는 한 조인으로 합치지 않는다.
3. **컴파일 거절.** 팬아웃(집계 측 `pk`가 조인으로 복제)과 캐즘(형제 팩트 또는 두 일대다가 한 뷰에서 만남)은 `validation`으로 거절한다. `many_to_one` 차원 조인과, 각 측을 집계한 뒤 잇는 Grouped cube JOIN은 통과한다. 합계를 기본키 기준으로 다시 쓰지 않는다. D23 Ungrouped `DISTINCT` 가드와 D28 `additive: false`는 유지한다. 테스트가 먼저다.
4. **조회 경로.** 1단계에서는 관계가 있어도 거절은 플래그 뒤이고, 에이전트는 지금처럼 등록 큐브 `execute_sql`만 쓴다. 2단계는 검증을 통과한 DB만 거절을 켠다. 큐브 선택은 `Never JOIN` 문장이 아니라 그 큐브의 dimension/fact로 한다. yaml을 에이전트 입력으로 바꾸지 않는다.
5. **컷오버 전 검증.** `hub_grain.py`와 같은 dry_run으로 컴파일된 물리 테이블이 허브(팩트 + 나열 차원) 안에 있는지 본다. 골든은 두 갈래다. 의도된 팬아웃·캐즘 SQL은 거절되고, 이미 맞던 `many_to_one` 집계의 물리 SQL은 유지된다. BIRD는 바꾼 DB만 돌린다. 그레인 FAIL이 줄지 않거나 해당 DB의 EX가 R3(`client/eval` 116/286의 그 DB 부분집합)보다 내려가면 그 DB는 끈다. 286 전체는 DB별 게이트 뒤에만 한다.

- [x] `viewJoin.relationship` + 허브 `pk` 그레인 스키마 · yaml에서 관계 초안 생성(기존 JSON 유지)
- [x] 허브별 pk/fk → `many_to_one` 매핑 · 형제 팩트·브릿지는 뷰에 흡수하지 않음
- [x] 팬아웃·캐즘 `validation` 거절 테스트 → 컴파일러. 심바가 재작성 없음
- [x] 거절 플래그 `SQL_MCP_GRAIN_GUARD`. 켠 DB 없음. 조회는 `execute_sql`(D19)
- [x] dry_run 그레인 골든(뷰 소스·컴파일 테이블 ⊆ 허브, `many_to_one` 물리 SQL 유지)
- [x] 플래그 끈 채 SMALL_DBS 286 재eval (카탈로그 `data/semantic.d29.eval.db`, EX **116/286** R3 동률, 그레인 34→30). 보고 [`client/docs/eval_report.md`](client/docs/eval_report.md)
- [ ] 거절 켠 DB별 게이트. 그레인 FAIL이 줄고 해당 DB EX ≥ R3일 때만 유지. `pk`가 비어 grain을 못 단 큐브: `throm_examination`, `hero_attribute`, `hero_power`

## 구현 시 작업 규칙

1. 계획 → 이 파일 · 계약 → `ARCHITECTURE.md` · 등록 스키마 → `semantic/` · 서버 내부 → `server/DESIGN.md`
2. 스켈레톤 → 테스트 → 구현
