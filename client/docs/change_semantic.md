# 시맨틱 수정 계획

> **상태**: D29 카탈로그로 286 재eval. EX **116/286** (R3 동률) · 그레인 **30**(R3 34) · 지식 **112**(R3 106). `SQL_MCP_GRAIN_GUARD` 끔. `Never JOIN` 문구 추가 금지. 정본 카탈로그 `data/semantic.d29.eval.db`. F1 description 반복 금지. 장기 게이트 유지.
> **근거(계획 베이스라인)**: [`eval_sql.json`](../eval/eval_sql.json) EX FAIL — **잘못된 시멘틱 정의 59** + **agent가 실패 110**. 최신 런은 [`eval_report.md`](eval_report.md) (EX 116/286, [`eval_sql.d29.json`](../eval/eval_sql.d29.json)).
> **범위**: **시맨틱 메타(table/view/cube/dimension/fact/metric/segment)와 생성기**. sql-mcp 버그 0건·LLM 교체는 [§6](#6-범위-밖-별도-트랙).

분류 규칙: 그레인 FAIL = 컴파일 물리 SQL이 gold에 **없는 테이블을 JOIN**(또는 gold 식별자 미등록). 그 외 EX FAIL은 **agent가 실패**. CTE 별칭(`avg_diff`, `rankedschools`, `topdriver`)은 물리 팩트 조인만 본선.

---

## 1. 목표

| 목표 | 성공 기준 |
|------|-----------|
| 그레인 FAIL | `eval_sql.json` **잘못된 시멘틱 정의 59 → 0** (같은 분류기) |
| 지식 FAIL | **agent가 실패 110 → 0** (`run_eval` 후 같은 분류기) |
| 전체 | 위 169건이 모두 PASS가 되면 EX **286/286**. DB별 잔여 0 |
| 그레인 | 한 측정 사건(팩트 허브) = 한 cube. `{db}_all` 없음 |
| 발견성 | 질문·evidence 어휘 ↔ dim/fact `description` 또는 metric/segment `examples` |

**비목표:** Soft-F1, LLM 교체, 에이전트 SYSTEM 개조. 일회성 고유명(사람 이름) segment 하드코딩. 그룹 비율 KPI를 **가산** fact로 노출하지 않음(행 비율만 `additive: false` fact, 그룹 `SUM/SUM`은 metric).

---

## 2. 59건 분해

| db_id | n | 증상 (컴파일이 gold에 없는 테이블) | 대표 qid |
|-------|---|--------------------------------------|----------|
| formula_1 | 24 | `lapTimes`↔`results`/`qualifying`, `constructorResults`↔`constructorStandings`, `driverStandings`, `seasons`, `circuits` | 847, 881, 896, 928, 948–951, 959, 990 |
| toxicology | 14 | `molecule`가 atom/bond 질의에 부착; gold `connected` 브릿지가 빠지고 `bond`/`atom`으로 대체 | 197, 206, 239, 243, 268 |
| california_schools | 10 | SAT/schools 질의에 `frpm`; FRPM+SAT에 `schools`; CTE `avg_diff`/`rankedschools` | 24, 27, 40, 41, 79, 82 |
| student_club | 7 | event 질의에 `income`/`expense`/`member`; income 단건에 event–attendance | 1323, 1344, 1359, 1376, 1435 |
| thrombosis_prediction | 4 | Lab 질의에 `Examination`(또는 반대); Q1201 `BIRTHDAY` 미등록 | 1201, 1239, 1242, 1267 |
| superhero | 0 | 해당 없음 (회귀만) | — |

**한 줄 원인:** `generate_meta`가 연결 요소 1개면 `{db}_all` **One-Big-View + 큐브 1개**. 에이전트가 그 큐브만 FROM 하므로 컴파일러가 형제 팩트·브릿지를 INNER JOIN에 넣는다(Fan/Chasm). D23 그레인 가드는 **cube JOIN**용이지, 뷰 안에 이미 묶인 팩트를 나누지 않는다.

---

## 3. 설계 원칙 (합의 포인트)

**채택** (Eric 2026-10-22).

1. **프로세스당 큐브 (star).** 그레인 하나 = view+cube 하나. 공유 차원만 재사용. 복수 팩트 metric을 한 SELECT에 넣지 않음 — 교차 팩트는 동일 `target` cube JOIN(D23)에 맡김.
2. **`{db}_all` 삭제.** 생성하지 않음(디버그용도 없음). 허브는 [§4.6](#46-p1--generate_meta-허브-분할) 고정 목록.
3. **자연명은 질문 어휘 우선.** “먼저 온 테이블”보다 자주 쓰는 엔티티. `description`은 검색·동의어·코드값·금지 패턴(생성 출처 문구 금지).
4. **브릿지 테이블은 그레인.** `connected`(독성), `attendance`(클럽)는 허브에 흡수하지 않고 필요 큐브의 명시 조인으로만.
5. **생성기 + DB 오버라이드.** `generate_meta`가 §4.6 허브 목록으로 view+cube를 만든다. `semantic_plan.yaml`은 그 목록의 정본.
6. **지식은 시맨틱에 둔다 (D26·D28).** 용어·코드는 dim/fact `description`, 연/나이는 dimension `expression`, **행 비율은 비가산 fact**, **그룹 `SUM(a)/SUM(b)`는 metric**, 코호트 WHERE는 flag dim(segment는 examples 보조). metric/segment는 SQL 컬럼이 아니라 `describe_table.examples` 스니펫. 질문 1건 = 멤버 1개는 금지. fact와 metric은 같은 이름을 쓰지 않는다.

---

## 4. 작업 패키지

### 4.1 P0 — formula_1 (24)

**문제:** races를 루트로 한 거대 뷰가 results / lapTimes / qualifying / *Standings / constructorResults를 한 cube에 묶음. gold가 `results`만 쓰는데 `lapTimes`·`driverStandings`가 붙고(Q881, Q928), standings 질의에 `constructorResults`가 붙는다(Q948–951).

| cube (가칭) | 중심 팩트 | 포함 차원 |
|-------------|-----------|-----------|
| `f1_race_results` | results | races, drivers, constructors, circuits, status |
| `f1_qualifying` | qualifying | races, drivers, constructors |
| `f1_lap_times` | lapTimes | races, drivers |
| `f1_pit_stops` | pitStops | races, drivers |
| `f1_driver_standings` | driverStandings | races, drivers, seasons |
| `f1_constructor_standings` | constructorStandings | constructors, races/seasons |
| `f1_constructor_results` | constructorResults | constructors, races |

**결정:** 위 7큐브 채택. standings 그레인은 **레이스**(물리 PK: `raceId`+드라이버/컨스트럭터 1행 = 그 레이스 종료 시점 누적 순위). 시즌 최종 순위는 해당 시즌 마지막 `raceId` 필터. `seasons`는 `races.year`로만 붙이고 results/lap 큐브에는 넣지 않음. `circuits`는 `f1_race_results`·`f1_qualifying`만(드라이버-only 질의에 회로 금지).

### 4.2 P0 — toxicology (14)

**문제:** molecule–atom–bond–connected를 한 뷰에 INNER. atom/bond 카운트에 `molecule`이 붙어 팬아웃(Q197, Q201). gold가 `connected`로 atom–bond를 잇는데 컴파일은 `connected`를 빼고 `bond`/`molecule`을 넣음(Q206, Q239, Q243).

| cube (가칭) | 그레인 | 포함 |
|-------------|--------|------|
| `tox_molecule` | 분자 1행 | molecule |
| `tox_atom` | 원자 1행 | atom, molecule |
| `tox_bond` | 결합 1행 | bond, molecule |
| `tox_connected` | 연결 1행 | connected, atom, bond |

**결정:** 4큐브 유지(`connected`를 atom–bond에 흡수하지 않음 — 브릿지 그레인). 교차(원소×결합형)는 `tox_connected` 또는 cube JOIN. 분자 라벨만이면 `tox_molecule`.

### 4.3 P1 — california_schools (10)

**문제:** schools–frpm–satscores 단일 큐브. SAT/주소 질의에 `frpm`이 따라붙음(Q27, Q40, Q45, Q50, Q79, Q82). FRPM+SAT gold에 `schools`가 추가됨(Q24, Q25).

**결정:** 큐브 2개 — `schools_frpm`(허브 `frpm` ⋈ `schools`), `schools_sat`(허브 `satscores` ⋈ `schools`). 교차(Q24 eligibility+test takers)만 cube JOIN. 단일 큐브+prune은 쓰지 않음(`frpm`이 SAT 질의에 붙는 실패가 10건 중 6).

### 4.4 P1 — student_club (7)

**문제:** member를 허브로 event·attendance·budget·expense·income·zip을 한 뷰. `source`(income) 필터가 event 집계에 `income, member`를 넣음(Q1323). income notes 단건에 event–attendance–member(Q1344). budget 비에 expense/income 팬아웃(Q1359, Q1376).

| cube | 허브 | 포함 차원 |
|------|------|-----------|
| `club_event` | event | — |
| `club_attendance` | attendance | event, member |
| `club_budget` | budget | event |
| `club_expense` | expense | budget, event, member |
| `club_income` | income | member |
| `club_member` | member | zip_code, major |

`attendance`/`expense`는 브릿지·세부가 더 고우므로 허브에 흡수하지 않음. Q1435는 `club_event`만.

### 4.5 P2 — thrombosis_prediction (4)

- **큐브 3:** `throm_patient`(허브 Patient) / `throm_laboratory`(Laboratory ⋈ Patient) / `throm_examination`(Examination ⋈ Patient).
- **Q1201:** 식별자 `Birthday` 고정. 출생 연도는 dimension `expression`(`date_trunc`/`EXTRACT`). 비율 KPI는 metric.

### 4.6 P1 — `generate_meta` 허브 분할

현재: FK 연결 요소 1개면 `{db}_all`. **삭제.** 허브는 아래 고정(자동 탐지 없음). 차원은 여러 허브 뷰에 재사용. metric/segment는 문서마다 `cube` 명시.

| db | 허브 (팩트, 큐브 1개씩) | 공유 차원 |
|----|-------------------------|-----------|
| formula_1 | `results`, `qualifying`, `lapTimes`, `pitStops`, `driverStandings`, `constructorStandings`, `constructorResults` | races, drivers, constructors, circuits, seasons, status |
| toxicology | `molecule`, `atom`, `bond`, `connected` | molecule은 atom/bond의 차원이기도 함 |
| california_schools | `frpm`, `satscores` | schools |
| student_club | `event`, `attendance`, `budget`, `expense`, `income`, `member` | zip_code, major; event/member는 여러 허브에 차원으로 재사용 |
| thrombosis_prediction | `Patient`, `Laboratory`, `Examination` | Patient는 lab/exam 큐브의 차원 |
| superhero | `superhero`, `hero_attribute`, `hero_power` | alignment, gender, publisher, race, colour(역할 alias), attribute, superpower |

정본: `target_db/<db>/semantic_plan.yaml` `hubs: [...]`. `generate_meta`는 이 목록만 view+cube로 만들고 `{db}_all`을 쓰지 않는다.

### 4.7 P1 — agent가 실패 110건 (지식 등록)

컴파일 테이블 집합은 gold와 맞거나(85건 로직/필터/프로젝션) SQL을 못 남긴 경우(15)다. 프롬프트가 아니라 **시맨틱 표면**을 고친다. `search_columns`는 dim/fact만 인덱스하고, metric/segment는 `examples`로만 보인다(D26).

| 신호 (eval_sql) | n (대략) | 시맨틱 조치 |
|-----------------|----------|-------------|
| evidence `X refers to Y` / 코드값 | 101 | dim/fact `description`에 동의어·리터럴·혼동 금지 |
| gold 비율·DIVIDE 공식 | 24 | 행 그레인 → **비가산 fact**. 그룹 `SUM/SUM` → **metric**(다른 이름, examples) |
| `YEAR`/`SUBTRACT`/`age` | 11+ | **dimension `expression`**: `date_trunc('year', …)`, `EXTRACT`, `DATEDIFF` |
| 코호트 (`label='+'`, `Magnet=1`, `Thrombosis=2`) | 반복 | **segment** `filter` |
| full name = 두 컬럼 | club/f1 | `first_name`/`last_name`/`forename`/`surname` description. concat dim 만들지 않음(EX 열 수) |
| 미생성·T1·DIVIDE 파서 | 15+6 | cube/dim description + examples에 **유효 시맨틱 SELECT** 스니펫. 물리 alias 금지 명시 |

**승격 규칙** ([methodology.md](../../skills/semantic-from-qa/methodology.md)): 이름 있는 공식·여러 질문에 재등장하는 코호트만. `Angela Sanders` 같은 일회성 리터럴은 segment 금지 — `full name → first_name AND last_name`만 description.

| 산출 | 언제 |
|------|------|
| dim/fact `description`만 | 동의어, 코드(`DOC=31`, `bond_type='-'`, `publisher_name`), “full name은 두 필터”, lat/lng vs location |
| dimension `expression` | 연도·나이가 **그룹/필터 축**일 때 (`birth_year`, `open_year`) |
| metric | 그룹 KPI (`k12_frpm_eligible_share` = SUM/SUM, `pct_carcinogenic`). **컬럼이 아님** |
| segment | 재사용 코호트 (`carcinogenic`, `magnet_k8`, `severe_thrombosis`, `marvel_publisher`) |

`enrich_meta_from_qa.py`가 evidence의 **이름 붙은 공식만** metric화하던 것을 확장: (1) `refers to` 맵 → 해당 dim/fact description, (2) 반복 코호트 → segment, (3) 이미 있는 metric 이름은 description만 보강(덮어쓰기 금지).

#### DB별 (qid는 대표)

- **thrombosis 27** (no_sql 9): `Thrombosis` description — `2`=severe, `1`=most serious. `Birthday`/`Description`/`Examination Date`에 연도 차원. 나이 = `DATEDIFF` 또는 `EXTRACT(YEAR FROM …)` expression dim. 이상 코호트 segment: `plt_abnormal` (`PLT<=100 OR PLT>=400`), `igm_abnormal`, `gpt_high`. Q1201류 `YEAR(BIRTHDAY)`는 §4.5와 같이 `Birthday` 식별자 고정.
- **formula_1 27**: `circuitRef` vs `name` vs `location` vs `(lat,lng)` description. 드라이버 full name = `forename`,`surname` 두 열. `q3`/`lapTimes.time` LIKE 패턴을 description에. `dob` → `driver_birth_year` dim. 완주 = `results.time IS NOT NULL` segment. examples에 T1 없는 SELECT.
- **toxicology 17**: `label` `'+'`/`'-'` → segment `carcinogenic` / `non_carcinogenic`. `bond_type` `'-'`/`'='`/`'#'` description. 원소 코드 `'cl'`/`'h'`/`'f'`. 반복 `DIVIDE(SUM(…), COUNT(…))` → metric (`pct_double_bond`, `pct_carcinogenic`). `atom_id` 4번째 원자 패턴은 재사용이면 expression dim.
- **california_schools 15**: 행 비율 `excellence_rate` / `eligible_free_rate_k12` / `eligible_frpm_rate_k12`는 **비가산 fact**. 그룹 share는 metric(다른 이름). `DOC`/`SOC`/`Magnet`/`EdOpsCode` description·flag/segment. `OpenDate` → `open_year` dim. 주소 = Street, City, State, Zip **네 열**.
- **superhero 15**: `attribute_name` (`Intelligence`/`strength`/`durability`) description + 대소문자. 역할 색 `hair_colour` vs `eye_colour` vs `colour`(Q800 `color` 금지). `publisher_name` 리터럴. `alignment` Bad = self-interest. dumbest/strongest는 `attribute_value` + `attribute_name` 필터를 examples로.
- **student_club 9**: `type` vs `event_type` vs `category`. `approved` True/False. full name 두 컬럼. `source='Dues'`/`Fundraising`은 income 큐브(§4.4) description.

**결정 (D28):** 그룹 비율은 **metric** / 행 비율은 **비가산 fact** (`additive: false`). fact와 metric은 이름을 겹치지 않는다.

---

## 5. 검증 루프

```text
generate_meta → enrich_meta_from_qa(description/segment/metric 확장) → register
→ A. write_eval_sql.py (기존 agent_sql + 새 메타 dry_run) → 그레인 59 게이트
→ B. run_eval.py 해당 db → eval_sql 재분류 → 지식 110 게이트
→ eval_report
```

그레인(§4.1–4.6)은 **dry_run만**으로 물리 JOIN이 줄었는지 본다. 지식(§4.7)은 description/`examples`가 에이전트 SQL을 바꾸므로 **재eval 필수**.

---

## 6. 범위 밖 (별도 트랙)

이 계획에 섞지 말 것.

- 서버 방언·Soft-F1·윈도우 ad-hoc 확대·LLM 교체·에이전트 SYSTEM 개조.
- 일회성 고유명 하드코딩(Q1312 `Angela Sanders`를 segment로).
- superhero 지식 description(§4.7)은 범위 안. `{db}_all` 삭제로 `superhero` / `hero_attribute` / `hero_power` 3큐브는 일관성 때문에 만듦(그레인 FAIL은 원래 0).

---

## 7. Approved / 수정 메모 (Eric)

| # | 항목 | 결정 |
|---|------|------|
| 1 | §3 원칙 1–6 | **수용** |
| 2 | `{db}_all` · 허브 | **삭제.** 허브는 §4.6 표(팩트 테이블 고정, yaml 정본) |
| 3 | F1 큐브 · standings | 7큐브. standings = **레이스 그레인**(시즌 최종 = 마지막 race) |
| 4 | tox | **4큐브** (`connected` 분리) |
| 5 | CA | **2큐브** `schools_frpm` / `schools_sat` |
| 6 | 비율 | **그룹 = metric / 행 = 비가산 fact** (D28) |
| 7 | 게이트 | 그레인 59→**0**, 지식 110→**0** (100%) |
| 8 | 라운드 2 | **허브 재분할 없음.** 잔여 그레인의 대다수는 잘못된 큐브 선택 → 지식+재eval. 같은 agent SQL dry_run은 게이트가 아님 |

---

## 8. 참조

- SQL 비교: [eval_sql.d28.json](../eval/eval_sql.d28.json) (R3) · [eval_sql.r2.json](../eval/eval_sql.r2.json) · [eval_sql.hub.json](../eval/eval_sql.hub.json) · [eval_sql.json](../eval/eval_sql.json) (One-Big-View) · 요약 [eval_report.md](eval_report.md)
- 분류: [client/DESIGN.md](../DESIGN.md) (`cause` = gold vs `agent_db_sql` 테이블)
- Meta 생성: [target_db/README.md](../../target_db/README.md)
- 계약: [semantic-from-eval](../../skills/semantic-from-eval/) (일반화 루프) · [semantic-table-view](../../skills/semantic-table-view/) · [semantic-cube-metric-segment/contract.md](../../skills/semantic-cube-metric-segment/contract.md) · [semantic-from-qa](../../skills/semantic-from-qa/)
- 컴파일/그레인: [ARCHITECTURE.md](../../ARCHITECTURE.md) D23, [server/spec.md](../../server/spec.md)

---

## 9. 라운드 2 — 큐브 라우팅·멤버·JOIN 예시

> **채택 방향.** 허브 목록(`semantic_plan.yaml`)은 그대로 둔다. 다음 레버는 시맨틱 표면(큐브 description · metric/segment `examples` · 누락 멤버)이다. SYSTEM/LLM/파서는 [§6](#6-범위-밖-별도-트랙) 유지.

라운드 1이 JOIN 팬아웃은 줄였고(그레인 61→33), EX는 112→101로 내려갔다. `cube_descriptions`는 이미 있는데 에이전트가 `f1_race_results`를 standings/qualifying/랩에 쓰고, cube JOIN에서 ON·집계를 빠뜨린다. 분류기는 그 결과를 **잘못된 시멘틱 정의**로 찍지만, 같은 agent SQL을 dry_run해도 FROM이 그대로라 Δ=0이다. **재eval이 게이트**다.

### 9.1 잔여 그레인 33 재분해 (분류기 불변)

| 버킷 | n | 대표 | 조치 |
|------|---|------|------|
| **잘못된 큐브** (지식) | 15 | F1 standings를 `f1_race_results`(Q896–906, 1002), constructor standings를 `f1_constructor_results`(Q948–951), qualifying를 results(Q846), 랩을 results(Q879, 959); CA SAT를 `schools_frpm`(Q27, 39, 40); tox atom/bond를 `tox_connected`(Q201) | 큐브 description에 **금지 상대** + `examples`에 `FROM`이 맞는 SELECT |
| **미등록·잘못된 큐브의 식별자** | 6 | `CHO`(Q1185) `KCT`(Q1302) `birthday`(Q1227) `Symptoms`(Q1270) `cost`(Q1390) `amount`(Q1432) | Lab 멤버 등록(CHO/KCT). Birthday 대소문자. cost/amount는 `club_expense`/`club_income`으로 라우팅(이미 멤버) |
| **차원 항상 JOIN** (진짜 그레인/prune) | 12 | constructor_results에 `races`(Q869); results에 `races`/`status`/`circuits`(Q875, 877, 977, 981); superhero `colour`(Q772); tox_connected `atom`(Q234); FRPM⋈SAT에 `schools`(Q24, 25); throm에 Examination 과잉(Q1187, 1209) | **이번 라운드 yaml 유지.** 재eval 뒤 그 문항이 여전히 팬아웃 EX면 그때 차원 trim |

F1 12건은 “7큐브가 부족해서”가 아니라 **형제 큐브를 고른 것**. drivers-only 큐브를 새로 만들지 않는다(차원이지 허브가 아님).

### 9.2 작업 패키지

**P0 — 큐브 라우팅 `examples` (F1 우선).** metric/segment `examples`에 복사 가능한 시맨틱 SELECT를 넣는다(물리 `T1` 금지, D26). description만 있는 상태는 라운드 1에서 부족했다.

| 큐브 | 쓸 때 | 쓰지 말 때 |
|------|--------|------------|
| `f1_race_results` | 완주·position·status | qualifying / lapTimes / *Standings |
| `f1_driver_standings` | 드라이버 챔피언십·누적 순위 | `results.position` |
| `f1_constructor_standings` | 컨스트럭터 챔피언십 | `constructorResults` |
| `f1_qualifying` | q1/q2/q3 | race results |
| `f1_lap_times` | 랩 타임 | race results |
| `schools_sat` / `schools_frpm` | SAT vs 급식 | 반대 큐브 단독 |
| `tox_atom` / `tox_bond` / `tox_connected` | 원자 / 결합 / 브릿지 | atom+bond 질의에 connected |
| `throm_laboratory` vs `throm_examination` | 혈액 vs 진찰 | 둘을 기본 JOIN하지 않음 |
| `hero_attribute` / `hero_power` / `superhero` | 능력치 / 파워 / 히어로 속성 | 속성 질의에 `superhero`만 |

**P0 — 누락 멤버.** `CHO`·`KCT`는 enrich/생성기에 없다 → `throm_laboratory` dim/fact로 등록. `Birthday`(not `birthday`)는 이미 있으나 에이전트가 소문자를 씀 → 멤버 별칭이 없으면 description+examples만. `Symptoms`는 examination에 있음 → lab/patient에서 쓰지 말 것. club `cost`/`amount`는 expense/income에 있음 → event/member description에 “비용은 다른 큐브”.

**P1 — cube JOIN 레시피 (기타 30).** 12건 ungrouped JOIN, 4건 ON 누락. 교차 팩트 `examples`: `FROM a JOIN b ON a.id = b.id` + `GROUP BY` 또는 `SELECT DISTINCT`. T1·CTE JOIN 금지 문구는 해당 큐브 description에만(SYSTEM 금지).

**P2 — yaml trim은 조건부.** 재eval 후 Q869/Q772/Q24류가 **같은 큐브·같은 extra JOIN으로 EX FAIL**이면 그때 `circuits`(qualifying), `colour`(superhero 허브), `races`(constructor_results)를 yaml에서 뺀다. Steiner/컴파일 미prune은 sql-mcp 트랙([§6](#6-범위-밖-별도-트랙)).

### 9.3 검증 루프

```text
enrich 테스트(새 description/examples/멤버) → generate_meta+enrich → register 증분
→ run_eval 286 (같은 모델·concurrency) → eval_sql 재분류 → eval_report
```

같은 허브-런 agent SQL dry_run은 **참고만**. 라우팅 패치의 게이트가 아니다.

| 이번 라운드 체크포인트 | 장기 게이트(불변) |
|------------------------|-------------------|
| EX **≥ 112/286** (One-Big-View 회복) | EX 286/286 |
| 그레인 **≤ 20** (잘못된 큐브가 지식으로 이동·감소) | 그레인 0 |
| 지식 **≤ 100** | 지식 0 |

정지: 체크포인트 미달인데 cause_counts Δ≈0 → 같은 examples를 반복하지 않고 보고. sql-mcp 버그(Q829 CASE END)는 목록만.

### 9.4 범위

고친다: `enrich_meta_from_qa.py` DOMAIN_SPECS, 필요 시 `generate_meta` 컬럼 승격, 증분 등록 JSON.  
안 고친다: `semantic_plan.yaml` 허브 목록(P2 전까지), 에이전트 SYSTEM, 모델, `{db}_all` 복원, 그룹 비율을 가산 fact로 올리기, 일회성 고유명 segment.

---

## 10. 라운드 3 — 검색 표면 · 지식 패치 · JOIN 철회

> **채택 방향.** 큐브 `Copy: SELECT FROM`을 더 늘리지 않는다(R2에서 F1 standings가 안 줄었다). `search_columns`가 보는 **dim/fact description**, metric `examples`의 **식 스니펫**(이름은 컬럼이 아님, D26), cube description에서 **JOIN 권유를 뺀다**. 허브 yaml·SYSTEM·LLM은 [§6](#6-범위-밖-별도-트랙).

R2는 list_tables용 Copy SELECT로 EX 101→110. 잔여 그레인 28의 핵심은 에이전트가 `describe_table(f1_race_results)`만 하고 standings 큐브를 안 연 것(Q896 `position`, Q948 `points`). cube description에 JOIN 예시를 넣자 **ungrouped cube JOIN이 12→20**으로 늘었다. Q869·Q772는 EX PASS가 되어 yaml trim(§9.2 P2)은 필요 없다.

**agent가 실패 116 + 기타 32**는 컴파일 테이블이 맞거나(92 로직/필터/프로젝션) SQL을 못 남긴 경우(14)와 검증 거절(T1 55·DIVIDE 17·metric을 컬럼으로 6·ORDER BY 집계 10)이다. §4.7 지식은 이미 등록됐는데도 식이 안 보인다 — **검색 문장 + examples 식**을 보강한다. 질문 1건 = metric 1개 금지.

### 10.1 잔여 그레인 28

| 버킷 | n | 대표 | 조치 |
|------|---|------|------|
| **동명 멤버 혼동** | ~10 | Q896 gold=`driverStandings.position`인데 `FROM f1_race_results`; Q948–951 gold=`constructorStandings.points`인데 `f1_constructor_results` | `results.position`/`points` = 한 레이스. standings의 동명 멤버 = 누적 챔피언십. 단어 championship/standings/cumulative는 standings 큐브 멤버에만 |
| **교차 큐브 과잉 JOIN** | ~10 | tox 4큐브(Q197, 201, 243); club expense+attendance(Q1339); CA SAT에 `schools_frpm`(Q27, 39, 82); throm에 examination(Q1198, 1265) | cube description에서 JOIN Copy 삭제. “한 큐브만. 교차는 해당 큐브 `examples`의 GROUP BY SELECT만” |
| **항상 JOIN / 식별자** | ~8 | Q25 FRPM⋈SAT에 `schools`; Q866 lap에 `races`; Q1185 gold ``T-CHO`` → 분류기가 `T` 미등록; Q263 `element`를 bond에서 조회 | 멤버 설명만. yaml 허브 유지 |

### 10.2 작업 패키지

**P0 — 동명 멤버 검색 문장 (F1).** `position`/`points`/`wins` description을 큐브별로 다르게. `f1_race_results.position` = finishing position this race, not championship standing. `f1_driver_standings.position` = cumulative championship rank after that race. constructor 동일. `search_columns("standings"|"championship"|"Hamilton not first")`가 race_results가 아니라 standings를 치게.

**P0 — JOIN 레시피 철회.** `schools_frpm`·`throm_laboratory`·교차 큐브 description에서 `JOIN … GROUP BY` Copy를 뺀다. 한 줄만: ungrouped cube JOIN은 거절되니 SAT-only는 `schools_sat`, 혈액만은 `throm_laboratory`. 정당한 교차 2건만 metric `description`에 grouped SELECT를 남긴다(FRPM+SAT, lab+KCT) — cube description이 아님.

**P0 — agent 지식 (116, §4.7 잔여).** 프롬프트가 아니라 멤버 description + metric/segment `examples` 식. T1·metric 식별자·DIVIDE는 examples에서 금지하고 **복사 가능한 ad-hoc 식만**. **적용됨 (d28):** 빈 description 채움 + `_CONFUSION_FORCE` overwrite (`GPT`≠`GLU`, `colour`≠`skin_colour`, `circuitRef`, `EdOpsCode`, `Diagnosis` APS, `CPK>=250`). 세그먼트 `diagnosis_aps` / `merged_schools` / `locally_funded`. 게이트는 재eval.

| 신호 (R2) | n | 시맨틱 |
|-----------|---|--------|
| 로직/필터/프로젝션 (compiled OK) | 92 | `refers to` 코드·혼동 쌍을 **빈 description만** 채움. 이미 있는 문장은 덮지 않음 |
| 성공 SQL 없음 | 14 | 해당 큐브 examples에 유효 SELECT 1개(T1 없음) |
| `AS T1` / `T1.` | 49–55 | examples·description: alias = 큐브 이름만. 물리 `T1` 금지 |
| `DIVIDE(` | 17 | metric `examples`에 `(a) / (b)`. `DIVIDE` 문구 없음 |
| metric 이름을 SELECT | 6 | `eligible_frpm_rate_k12` 등은 컬럼이 아님. Copy: `(FRPM_Count_K_12) / (Enrollment_K_12)` |
| `ORDER BY COUNT/SUM/AVG` | 10 | examples: `SELECT … AS x … ORDER BY x` |
| `YEAR(` / 잘못된 나이 식 | 6+ | `birth_year` + `EXTRACT(YEAR FROM CURRENT_DATE) - birth_year`. `YEAR()` 금지 |
| ungrouped cube JOIN (기타) | 20 | §10.2 JOIN 철회 |

DB별 (대표 qid, 이미 있는 metric/segment는 expression 덮지 않음):

- **thrombosis 29:** GPT≠GLU(Q1208). 나이 Copy `EXTRACT(YEAR FROM CURRENT_DATE) - birth_year`(Q1156, 1227). Diagnosis+Date는 lab 큐브에 둘 다(Q1153). UA 남>8/여>6.5, TG>200, T_BIL, IGG≥2000, IGA, CRP, SM vs ANA_Pattern, CENTROMEA, aCL_IgM(기타 unknown). 비율은 기존 metric 식만 examples로.
- **formula_1 30:** 로직 FAIL은 standings가 아닌 문항 — `circuitRef` vs `circuits_name` vs location, forename+surname 두 필터, `q3`/`time` LIKE. `points`/`position` 동명은 위 P0 검색 문장.
- **superhero 17:** `attribute_name` 대소문자, `colour`≠`color`, publisher 리터럴. dumbest/strongest = `MIN/MAX(attribute_value)` + `attribute_name` 필터를 **examples 식**으로(큐브 Copy SELECT 추가 아님).
- **toxicology 17:** `label`/`bond_type`/`element` 리터럴. 비율은 기존 `pct_*` **식**을 examples에. `element`는 `tox_atom`만(Q263).
- **california 13:** 행 비율은 비가산 fact `eligible_*`/`excellence_rate`(Q12, 32, 40). `Phone`은 schools, SOC/DOC 코드. 그룹 share는 metric.
- **student_club 10:** `category`(budget)≠`type`(event)(Q1422). full name 두 컬럼. `spend_to_budget_ratio`는 `(SUM(spent)) / (SUM(amount))` 식. `source`는 `club_income`.

**P1 — tox/club “한 큐브”.** `tox_connected`는 atom_id2 브릿지일 때만. atom 카운트는 `tox_atom`만(Q201). club `cost`는 `club_expense`만, attendance를 붙이지 않음(Q1339). **적용됨 (d28):** cube description overwrite — tox atom/bond는 형제 JOIN 금지, club expense/budget/event/attendance는 한 큐브 + ungrouped JOIN 거절 문구. JOIN ON 레시피 없음. 게이트는 재eval.

**P1 — CA/throm “한 큐브”.** SAT-only는 `schools_sat`(Q27·39·82), 혈액은 `throm_laboratory`(Q1187·1265). **게이트 실패 (80문항):** EX 23→16, 그레인 8→10. CA 11/30→5/30(lost 23·26·28·31·47·79), throm 12/50→11/50. `Never JOIN`이 형제 JOIN을 막지 못했고(Q24·25·1187) schools-only를 `schools_frpm`으로 밀어 EX를 깎음. 이 레버 정지. 286 안 함.

**하지 않음.** 큐브 Copy SELECT 추가, drivers-only 큐브, `{db}_all`, yaml에서 `races`/`colour`/`circuits` 삭제(R2에서 해당 EX가 회복됨), SYSTEM, ORDER BY 파서 완화(sql-mcp), 질문마다 metric.

### 10.3 검증

enrich 테스트 → generate+enrich → register → **변경 DB 80문항**. CA/throm `Never JOIN`은 **미달**(EX 16/80, 그레인 10). 286 재eval은 이 레버로 돌리지 않음. 같은 agent SQL dry_run은 게이트 아님.

| 체크포인트 | 장기 |
|------------|------|
| EX **≥ 112** | 286 |
| 그레인 **≤ 20** | 0 |
| 지식 **≤ 100** · 기타 ungrouped JOIN **≤ 12** | 지식 0 |

정지: F1 Q896/948이 같은 `FROM f1_race_results`이거나, 지식 92건 로직 FAIL의 Δ≈0이면 description을 또 늘리지 않고 보고.
