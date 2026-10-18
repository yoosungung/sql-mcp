# BIRD Mini-Dev Eval Report (sql-mcp + DW agent)

- **일시**: 2026-10-18 (UTC `created_at` 07:42, `scored_at` 07:43)
- **범위**: SMALL_DBS 6개 · **286문항** 전체
- **모델**: `ollama:gemma4:e2b` · **MCP**: `http://127.0.0.1:8090/mcp` · concurrency **8**
- **산출**: `client/eval/eval.json` · `client/eval/score_report.json`
- **직전 스냅샷**: `client/eval/archive/*2026-10-18*` (EX 24.83%)

---

## 1. Executive Summary

| 지표 | 직전 | 이번 | 변화 |
|------|------|------|------|
| EX (unordered set) | 71/286 (**24.83%**) | 108/286 (**37.76%**) | **+12.93pp** (+37문항 net) |
| 실패 | 215 | 178 | −37 |
| gained / lost vs prior | — | +60 / −23 | net +37 |

**핵심 관찰**

1. **실행은 대부분 통과, 값은 틀림.** 실패 178건 중 `pred_rows`가 있는 오답 132건(74%), 성공 `execute_sql` 없음 46건(26%). “컴파일이 안 된다”보다 “틀린 로직/필터/프로젝션”이 주원인.
2. **DB 편차 큼.** `superhero` 61.5% · `student_club` 54.2% vs `california_schools` **3.3%**(1/30). 후자는 식별자·공백 컬럼·윈도우/서브쿼리·필터 값 착오가 집중.
3. **실패 버킷은 에이전트 우위.** `dw_agent` 164 · `semantic_layer` 13 · `sql_mcp_code` 1. (직전 분류기의 “gold JOIN → semantic_layer” 휴리스틱은 제거된 상태.)

---

## 2. 성적 분해

### 2.1 DB별 EX

| db_id | EX | n | 정확도 | vs prior |
|-------|----|---|--------|----------|
| superhero | 32 | 52 | **61.5%** | +13.5pp |
| student_club | 26 | 48 | **54.2%** | +16.7pp |
| toxicology | 15 | 40 | **37.5%** | +17.5pp |
| thrombosis_prediction | 15 | 50 | **30.0%** | +8.0pp |
| formula_1 | 19 | 66 | **28.8%** | +19.7pp |
| california_schools | 1 | 30 | **3.3%** | −6.7pp |

### 2.2 난이도별 EX

| difficulty | EX | n | 정확도 |
|------------|----|---|--------|
| simple | 45 | 83 | 54.2% |
| moderate | 48 | 135 | 35.6% |
| challenging | 15 | 68 | 22.1% |

### 2.3 실패 사유 (classify_failure)

| 건수 | 비율(실패 내) | 사유 |
|------|---------------|------|
| 63 | 35.4% | execute 성공 · gold와 로직/필터/값 불일치 |
| 36 | 20.2% | 프로젝션 열 개수/구성 불일치 |
| 34 | 19.1% | 성공한 execute_sql 없음 |
| 20 | 11.2% | 결과 공집합 (gold는 비공집합) |
| 12 | 6.7% | 행 수 과다 (조인 팬아웃 의심) |
| 7 | 3.9% | 물리 함수(DIVIDE/YEAR/strftime) |
| ≤2 | — | 구문·alias·CASE·제약 우회 등 |

---

## 3. 실패 구조 (Root Cause)

### ① 에이전트 로직/식별자 착오 (최대 군집, ~63+20건)

- Evidence·물리 스키마 직역: `DIVIDE(...)`, `AdmFName1 = 'Ricci Ulrich'`(성·이름 분리 실패), `Virtual = 'F'` vs exclusively virtual 의미 혼동.
- 시맨틱 멤버명 미확인: `avg_math_score`, `excellence_rate`, `School_Code` 등 `describe_table`에 없는 식별자로 재시도 소진.
- **california_schools**에서 특히 심함 — 긴 물리 컬럼명·공백·다테이블 조인 질의를 단일 큐브 필터로 축소하면서 값/조건이 어긋남.

### ② 프로젝션 EX 엄격성 (36건)

- Gold는 `COUNT(*)` 스칼라인데 Pred는 차원+집계, 또는 `first_name || last_name` 합성 vs 두 열.
- GROUP BY 차원을 SELECT에 올려 EX set 비교 탈락 (값은 맞아도 열 구성 불일치).

### ③ 도구/재시도 고갈 (34건)

- 서브쿼리·물리 JOIN alias(`T1`/`expense`)·unknown member 반복 → `pred_rows = None`.
- Gold에 `RANK() OVER` / 스칼라 서브쿼리(평균 초과)가 있으면 D16/D23에서 표현 불가 → 근사 실패.

### ④ 시맨틱 조인/그레인 (semantic_layer 13 + 팬아웃 12)

- 단일 `{db}` 큐브 안에서 필터만으로 다테이블 의미를 흉내 내다 행 폭증/증발.
- 직전 보고서의 One-Big-View Root 강제·스패닝 트리 이슈는 **여전히 구조적 리스크**이나, 이번 채점에서는 `dw_agent` 오분류가 줄고 팬아웃/공집합으로만 일부 노출.

### ⑤ 날짜·나이 (소수지만 고착)

- `current_timestamp` / `year()` / `DATE('now')` 차단 → thrombosis·formula_1 나이 문항에서 물리 함수 버킷(7건) 및 재시도 실패.

---

## 4. 직전 대비 해석

| 변화 | 해석 |
|------|------|
| formula_1 +19.7pp, toxicology +17.5pp, student_club +16.7pp | 서버/메타/에이전트 프롬프트·채점 하네스 개선이 중형 DB에서 체감 |
| california_schools −6.7pp | 복잡도·컬럼 명명·윈도우/서브쿼리 비중이 높아 소형 모델이 붕괴; 회귀 3문항 포함 |
| semantic_layer 150→13 | 분류기 보정 효과 + 실제 실행 성공률 상승이 맞물림 (구조 문제가 사라진 것은 아님) |
| log_max 절단 버그 | 이전 리포트 ⑥ — **이미 수정됨** (이번 `no pred`는 진짜 도구 실패) |

---

## 5. 권고 (우선순위)

1. **california_schools 메타·프롬프트** — `describe_table` 멤버·메트릭(eligible rate, excellence rate)을 질문 어휘와 정렬; 공백 물리명 ↔ 시맨틱명 매핑을 샘플에 노출.
2. **프로젝션 계약** — 에이전트 SYSTEM에 “Gold와 동일 열 개수/의미만 SELECT” 강화; 또는 Soft-F1을 보조 지표로 병기.
3. **표현력** — 나이용 `current_date`/`extract(year)` 허용(emit만 SQLite 방언), evidence `DIVIDE` → `/` 자동 힌트.
4. **조인 뷰** — Inner-only·미참조 `sources[0]` 강제 제거(직전 권고 유지)로 팬아웃/공집합 잔여분 축소.
5. **재현** — `python run_eval.py --url http://127.0.0.1:8090/mcp --concurrency 8` 후 `score_eval_doc` → 본 경로의 JSON/본 문서 갱신.

---

## 6. 타이밍

- 문항당 평균 agent 경과 **~279s** (p50 ~203s, max ~2118s)
- 벽시계 ≈ **2h 50m** (concurrency 8)
- 평균 tool_calls ≈ **4.5**/문항 (최대 39, formula_1 qid=994)
