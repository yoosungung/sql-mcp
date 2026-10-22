# 시맨틱 수정 계획

> **상태**: **결정됨** (Eric 2026-10-22). 생성기 구현: `target_db/<db>/semantic_plan.yaml` + `generate_meta.py` 허브 분할 + `enrich_meta_from_qa.py` description/metric/segment. 미정 항목 없음.
> **근거**: [`eval_sql.json`](../eval/eval_sql.json) EX FAIL 169건 — **잘못된 시멘틱 정의 59** (뷰/큐브 그레인) + **agent가 실패 110** (지식·식별자·식). 런은 [`eval_report.md`](eval_report.md) (EX 117/286 = 40.91%).
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

**비목표:** Soft-F1, LLM 교체, 에이전트 SYSTEM 개조. 일회성 고유명(사람 이름) segment 하드코딩. 비율 KPI를 fact 컬럼으로 노출하지 않음(metric `examples`만).

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
6. **지식은 시맨틱에 둔다 (D26).** 용어·코드는 dim/fact `description`, 연/나이는 dimension `expression`, **비율·KPI는 metric**, 코호트는 segment. metric/segment는 SQL 컬럼이 아니라 `describe_table.examples` 스니펫. 질문 1건 = metric 1개는 금지.

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
| gold 비율·DIVIDE 공식 | 24 | 반복 KPI → **metric만** (`examples` 스니펫. fact 컬럼으로 올리지 않음) |
| `YEAR`/`SUBTRACT`/`age` | 11+ | **dimension `expression`**: `date_trunc('year', …)`, `EXTRACT`, `DATEDIFF` |
| 코호트 (`label='+'`, `Magnet=1`, `Thrombosis=2`) | 반복 | **segment** `filter` |
| full name = 두 컬럼 | club/f1 | `first_name`/`last_name`/`forename`/`surname` description. concat dim 만들지 않음(EX 열 수) |
| 미생성·T1·DIVIDE 파서 | 15+6 | cube/dim description + examples에 **유효 시맨틱 SELECT** 스니펫. 물리 alias 금지 명시 |

**승격 규칙** ([methodology.md](../../skills/semantic-from-qa/methodology.md)): 이름 있는 공식·여러 질문에 재등장하는 코호트만. `Angela Sanders` 같은 일회성 리터럴은 segment 금지 — `full name → first_name AND last_name`만 description.

| 산출 | 언제 |
|------|------|
| dim/fact `description`만 | 동의어, 코드(`DOC=31`, `bond_type='-'`, `publisher_name`), “full name은 두 필터”, lat/lng vs location |
| dimension `expression` | 연도·나이가 **그룹/필터 축**일 때 (`birth_year`, `open_year`) |
| metric | 비율·합·KPI (`excellence_rate`, `eligible_free_rate_k12`, `pct_carcinogenic`, 나이 비율). **컬럼이 아님** |
| segment | 재사용 코호트 (`carcinogenic`, `magnet_k8`, `severe_thrombosis`, `marvel_publisher`) |

`enrich_meta_from_qa.py`가 evidence의 **이름 붙은 공식만** metric화하던 것을 확장: (1) `refers to` 맵 → 해당 dim/fact description, (2) 반복 코호트 → segment, (3) 이미 있는 metric 이름은 description만 보강(덮어쓰기 금지).

#### DB별 (qid는 대표)

- **thrombosis 27** (no_sql 9): `Thrombosis` description — `2`=severe, `1`=most serious. `Birthday`/`Description`/`Examination Date`에 연도 차원. 나이 = `DATEDIFF` 또는 `EXTRACT(YEAR FROM …)` expression dim. 이상 코호트 segment: `plt_abnormal` (`PLT<=100 OR PLT>=400`), `igm_abnormal`, `gpt_high`. Q1201류 `YEAR(BIRTHDAY)`는 §4.5와 같이 `Birthday` 식별자 고정.
- **formula_1 27**: `circuitRef` vs `name` vs `location` vs `(lat,lng)` description. 드라이버 full name = `forename`,`surname` 두 열. `q3`/`lapTimes.time` LIKE 패턴을 description에. `dob` → `driver_birth_year` dim. 완주 = `results.time IS NOT NULL` segment. examples에 T1 없는 SELECT.
- **toxicology 17**: `label` `'+'`/`'-'` → segment `carcinogenic` / `non_carcinogenic`. `bond_type` `'-'`/`'='`/`'#'` description. 원소 코드 `'cl'`/`'h'`/`'f'`. 반복 `DIVIDE(SUM(…), COUNT(…))` → metric (`pct_double_bond`, `pct_carcinogenic`). `atom_id` 4번째 원자 패턴은 재사용이면 expression dim.
- **california_schools 15**: `excellence_rate` / `eligible_free_rate_k12` / `eligible_frpm_rate_k12` / 등록자 합은 **metric examples만**(컬럼 추측 금지). `DOC`/`SOC`/`Magnet`/`EdOpsCode` description·segment. `OpenDate` → `open_year` dim. 주소 = Street, City, State, Zip **네 열**.
- **superhero 15**: `attribute_name` (`Intelligence`/`strength`/`durability`) description + 대소문자. 역할 색 `hair_colour` vs `eye_colour` vs `colour`(Q800 `color` 금지). `publisher_name` 리터럴. `alignment` Bad = self-interest. dumbest/strongest는 `attribute_value` + `attribute_name` 필터를 examples로.
- **student_club 9**: `type` vs `event_type` vs `category`. `approved` True/False. full name 두 컬럼. `source='Dues'`/`Fundraising`은 income 큐브(§4.4) description.

**결정:** 비율은 **metric `examples`만**. fact/dim 컬럼으로 올리지 않음.

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
| 6 | 비율 | **metric examples만** (fact 컬럼 아님) |
| 7 | 게이트 | 그레인 59→**0**, 지식 110→**0** (100%) |

---

## 8. 참조

- SQL 비교: [eval_sql.json](../eval/eval_sql.json) · 요약 [eval_report.md](eval_report.md)
- 분류: [client/DESIGN.md](../DESIGN.md) (`cause` = gold vs `agent_db_sql` 테이블)
- Meta 생성: [target_db/README.md](../../target_db/README.md)
- 계약: [semantic-from-eval](../../skills/semantic-from-eval/) (일반화 루프) · [semantic-table-view](../../skills/semantic-table-view/) · [semantic-cube-metric-segment/contract.md](../../skills/semantic-cube-metric-segment/contract.md) · [semantic-from-qa](../../skills/semantic-from-qa/)
- 컴파일/그레인: [ARCHITECTURE.md](../../ARCHITECTURE.md) D23, [server/spec.md](../../server/spec.md)
