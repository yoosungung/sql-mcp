# 시맨틱 수정 계획 (초안 · 검토용)

> **상태**: Eric 검토·수정 대기. 구현·등록·재eval은 이 문서 합의 후에만.
> **근거**: `client/docs/eval_report.md` (EX 37.76%, 286문항) · `target_db/generate_meta.py` / `enrich_meta_from_qa.py` · skills `semantic-*` 계약 · OLAP star/grain (chasm·fan trap 회피).
> **범위**: **시맨틱 메타(table/view/cube/metric/segment)와 그 생성기**. 에이전트 프롬프트·EX Soft-F1·서버 방언은 [§6 범위 밖](#6-범위-밖-별도-트랙)에만 링크.

파일명 `change_sementic`은 요청 경로 그대로(오타 `sementic` 유지).

---

## 1. 목표

| 목표 | 성공 기준 (제안) |
|------|------------------|
| BIRD SMALL_DBS EX 상향 | 전체 EX **≥ 50%** (현재 37.76%). 1차 게이트: `california_schools` **≥ 25%**, `formula_1` **≥ 40%**. |
| 그레인 안전 | 집계 질의에서 Fan/Chasm으로 인한 “행 수 과다/공집합” 실패가 **시맨틱 원인**으로 재현·감소 가능한 수준으로 줄어듦. |
| 에이전트 발견성 | 질문 어휘(`excellence rate`, `eligible free`, `Virtual=F` 등) ↔ `describe_table`/metric/segment/description **1:1 매핑**이 문서에 명시. |

**비목표 (이 계획에서 약속하지 않음):** 서브쿼리·윈도우가 필요한 Challenging 문항 전수 해결, Soft-F1 도입, LLM 교체.

---

## 2. 현황 진단 (시맨틱만)

### 2.1 생성 파이프라인

```text
sqlite → generate_meta.py → meta/{table,view,cube,*_count}.json
       → enrich_meta_from_qa.py → domain metric/segment JSON
       → semantic-register / register_meta → data/semantic.db
```

- DB당 **연결 요소 1개면 `{db}_all` One-Big-View + 큐브 `{db}` 1개** (`views_from_tables`).
- 동명 컬럼은 **먼저 온 테이블이 자연명**, 이후는 `table_col` 접두 (`amount`=`income.amount`, `budget_amount`=`budget.amount`).
- Role-playing 룩업(예: superhero colour)은 이미 `*_colour_table` 분리.
- `enrich_meta_from_qa`는 **단일 큐브 파일 `{db}.json` 가정**.

### 2.2 Eval에서 드러난 시맨틱 병목

| 증상 | 시맨틱 쪽 해석 | 대표 DB |
|------|----------------|---------|
| 로직/필터 불일치·공집합 | 멤버 의미 모호(`County` vs `County_Name`), segment 미사용, 메트릭을 컬럼처럼 참조 | california_schools |
| 행 수 과다(팬아웃) | 복수 팩트·브릿지를 한 INNER JOIN 뷰에 묶은 뒤 그레인 혼합 조회 | formula_1, student_club |
| 프로젝션 불일치 | metric만 SELECT해야 할 곳에 dim+metric / 합성 문자열 | 전 DB (에이전트+계약) |
| 식별자 미발견 | metric은 있는데 에이전트가 컬럼으로 씀; description/검색 신호가 약함 | california_schools (`avg_math_score` 등) |

**참고 (서버):** `view_prune`의 `sources[0]` 무조건 앵커는 **이미 outer-join/빈 t_req일 때만**으로 완화됨. 남은 팬아웃은 **뷰에 간선이 너무 많음·그레인 혼합** 쪽이 큼 → 메타 분할이 본선.

### 2.3 DB별 제안 우선순위

| 우선 | db_id | 현재 EX | 초점 |
|------|-------|---------|------|
| P0 | california_schools | 3.3% | 명명·동의어·metric/segment 발견성, schools↔frpm↔satscores 그레인 |
| P0 | formula_1 | 28.8% | **멀티 큐브**(results / lapTimes / standings …)로 One-Big-View 해체 |
| P1 | student_club | 54.2% | event–budget 직접 경로·amount/status 모호성 정리 |
| P1 | toxicology · thrombosis | 37.5% / 30% | bridge·나이/진단 코호트 segment·metric |
| P2 | superhero | 61.5% | 유지·회귀만 (role-playing 이미 분리) |

---

## 3. 설계 원칙 (합의 포인트)

검토 시 **채택/수정/기각**을 표시해 주세요.

1. **프로세스당 큐브 (star / fact constellation)**  
   하나의 비즈니스 프로세스·그레인 = 하나의 view+cube. 공유 차원(conformed dim)만 재사용. 복수 팩트 metric을 한 SELECT에 넣지 않음(Chasm은 서버 D23 cube JOIN 규칙에 맡김).
2. **One-Big-View는 기본값에서 제외**  
   `{db}_all`은 디버그/탐색용으로만 두거나, 생성기 기본을 **허브(팩트)별 뷰**로 전환. (옵션 A/B는 §4.1)
3. **자연명은 질문 어휘·도메인 우선**  
   “먼저 등록된 테이블” 규칙보다 **의미적으로 자주 쓰이는 엔티티**에 자연명. 충돌 시 description에 `from table.column` + 동의어.
4. **도메인 KPI는 enrich가 정본**  
   비율·코호트는 ad-hoc `/`보다 **named metric/segment**. 에이전트는 metric 이름을 SELECT.
5. **생성기 자동화 + DB별 오버라이드**  
   `generate_meta` 휴리스틱 개선 + `target_db/<db>/semantic_plan.yaml`(가칭)로 큐브 분할·리네임·금지 조인을 선언. 전 DB 수작업 JSON만으로 가지 않음.

---

## 4. 작업 패키지 (제안 순서)

### 4.1 P0-A — 뷰/큐브 분할 정책 (`generate_meta`)

**문제:** `formula_1_all` 등 13+ 테이블 INNER 스패닝이 그레인을 붕괴.

**옵션 (택1 또는 단계적):**

| 옵션 | 내용 | 장점 | 단점 |
|------|------|------|------|
| **A1. 팩트 허브 멀티뷰** | PK가 타 FK의 많이 들어오는 테이블(또는 명시 목록)마다 `view`+`cube` | 그레인 명확 | enrich/에이전트가 큐브 선택 필요 |
| **A2. 주제 클러스터** | DB별 수동 클러스터(예: results+drivers+races / lapTimes+… ) | 통제 가능 | 유지비 |
| **A3. `{db}_all` 유지 + summary 서브뷰** | 큰 뷰는 `summary:true` 후보만, 질의용은 작은 뷰 | 점진적 | all이 남으면 오남용 |

**제안 기본안:** A1을 생성기 기본으로, A2로 P0 DB만 오버라이드.

**산출:** `generate_meta.py` API·테스트 · `target_db/README.md` · (필요 시) skills `semantic-table-view` 계약 한 줄.

**열린 질문:**  
- `{db}_all`을 계속 생성할까, 아예 끌까?  
- 큐브 이름 규칙: `{db}_{fact}` vs 도메인명(`race_results`)?  
- `enrich_meta_from_qa` 멀티 큐브 매핑 방식(메트릭 → cube 명시 테이블)?

---

### 4.2 P0-B — california_schools 메타 큐레이션

**문제:** EX 3.3%. schools/frpm/satscores 조인은 단순(2 join)이나 **멤버·메트릭 발견 실패**가 큼. enrich에 `excellence_rate` / `eligible_free_rate_k12` / `avg_math_score` / `exclusively_virtual`가 **이미 존재**하나 에이전트가 컬럼처럼 오용.

**작업 (시맨틱):**

1. **dimension 리네임·동의어 description**  
   - `County` vs `County_Name` — 질문 언어(“county of Los Angeles”)에 맞는 쪽을 자연명으로, 다른 쪽은 접두+description.  
   - `Virtual` — segment `exclusively_virtual`과 코드값(`F` 등)을 description에 명시.
2. **metric description에 “SELECT metric_name only” 검색 문구** + `group_by` 그레인(학교/CDS) 점검.
3. **뷰 그레인 선택 (검토):**  
   - 유지: 단일 `california_schools` 큐브.  
   - 또는: `schools_frpm` / `schools_sat` 두 큐브로 나눠 enrollment vs SAT 질의 분리.
4. 공백 물리명 ↔ sanitize 별칭 표를 cube/view description 또는 table column description에 고정.

**열린 질문:** 단일 큐브 유지 vs 2큐브 분할?

---

### 4.3 P0-C — formula_1 멀티 큐브

**문제:** races 루트 거대 뷰 → results/lapTimes/standings 교차 시 팬아웃.

**제안 큐브 초안 (수정 가능):**

| cube (가칭) | 중심 팩트 | 포함 차원 (예) |
|-------------|-----------|----------------|
| `f1_race_results` | results | races, drivers, constructors, circuits, status |
| `f1_qualifying` | qualifying | races, drivers, constructors |
| `f1_lap_times` | lapTimes | races, drivers |
| `f1_pit_stops` | pitStops | races, drivers |
| `f1_driver_standings` | driverStandings | races/seasons, drivers |
| `f1_constructor_standings` | constructorStandings | … |
| `f1_constructor_results` | constructorResults | … |

공유 차원 table은 재등록·재사용. **교차 팩트 질의**는 (허용 시) 동일 target cube JOIN + GROUP BY 규칙에만 의존.

**열린 질문:** standings를 seasons 그레인 큐브로 묶을지, race 그레인에 둘지?

---

### 4.4 P1 — student_club / toxicology / thrombosis

- **student_club:** `amount`/`budget_amount`, `status`/`event_status` 자연명·description 재배치; event↔budget 경로가 질의에 쓰이도록 뷰 분할 또는 join 목록 점검; `food_spent_total` 등 enrich metric 유지.
- **toxicology:** atom–connected–bond 브릿지 그레인별 뷰(분자 / 결합 / 원자).
- **thrombosis:** Patient↔Laboratory 그레인; 나이·진단 코호트는 segment/metric (서버 `current_date`는 §6).

---

### 4.5 P1 — 명명·충돌 규칙 변경 (`generate_meta`)

현재: 동치류/동명에서 **source 순서 우선**.

제안:

1. **우선순위 테이블** (오버라이드): 팩트 테이블 컬럼 > 차원 자연키 라벨 > 기타.  
2. 또는 **항상 `table_col`**, 자연명은 cube dimension에서만 부여(이중 작업이지만 모호성↓).  
3. description 필수 문구 템플릿: `from {table}.{column}; aka: …; do not confuse with …`.

**열린 질문:** 1 vs 2?

---

### 4.6 P2 — enrich_meta_from_qa 확장

- 멀티 큐브: metric/segment에 `cube`를 **문서마다** 명시(이미 필드 있음) + 로더가 `{db}.json` 단일에 의존하지 않게.
- california_schools·formula_1 실패 문항 evidence에서 **이름 있는 공식만** 추가 승격(질문 1건=metric 1개 금지 규칙 유지).
- `DIVIDE`는 enrich 식에 유지 가능(계약 허용). 에이전트 ad-hoc은 §6.

---

### 4.7 검증 루프

```text
generate_meta → enrich → register → (선택) 스모크 execute_sql
→ run_eval.py (해당 db 또는 전체) → score_report → eval_report 갱신
```

DB 단위 게이트를 먼저 통과한 뒤 전체 286 재eval.

---

## 5. 일정 스케치 (합의 후)

| Phase | 내용 | 예상 산출 |
|-------|------|-----------|
| 0 | 이 문서 리뷰·옵션 확정 | 본 파일 Approved 섹션 |
| 1 | P0-A 생성기 + formula_1 분할 | 메타 JSON · 단위 테스트 |
| 2 | P0-B california_schools 큐레이션 | 메타 · db-only eval |
| 3 | P1 나머지 SMALL_DBS | 메타 · 부분 eval |
| 4 | 전체 286 eval + `eval_report.md` | EX 게이트 확인 |

---

## 6. 범위 밖 (별도 트랙)

시맨틱만으로 안 닫히는 항목 — **이 계획에 섞지 말 것.** 필요 시 ROADMAP/이슈로 분리.

- 에이전트 SYSTEM(프로젝션 열 수, DIVIDE→`/`, metric vs column).
- 서버: ad-hoc `YEAR`/`current_date` emit, Soft-F1, 윈도우·서브쿼리 ad-hoc 허용.
- LLM 업그레이드.

---

## 7. Approved / 수정 메모 (Eric)

| # | 항목 | 결정 (채움) |
|---|------|-------------|
| 1 | §3 원칙 1–5 | |
| 2 | §4.1 옵션 A1/A2/A3 · `{db}_all` 유지 여부 | |
| 3 | §4.2 CA 단일 vs 2큐브 | |
| 4 | §4.3 f1 큐브 목록 | |
| 5 | §4.5 명명 규칙 1 vs 2 | |
| 6 | EX 게이트 수치 | |
| 7 | 기타 | |

---

## 8. 참조

- Eval: [client/docs/eval_report.md](../client/docs/eval_report.md)
- Meta 생성: [target_db/README.md](../target_db/README.md)
- 계약: [skills/semantic-table-view](../skills/semantic-table-view/), [skills/semantic-cube-metric-segment/contract.md](../skills/semantic-cube-metric-segment/contract.md), [skills/semantic-from-qa](../skills/semantic-from-qa/)
- 컴파일/그레인: [ARCHITECTURE.md](../ARCHITECTURE.md), [server/spec.md](../server/spec.md)
