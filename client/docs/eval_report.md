# BIRD Mini-Dev Eval Report (sql-mcp + DW agent)

- **일시**: 2026-10-18 (UTC start `12:27` · `created_at`/`ended` `16:16`, `scored_at` 16:16)
- **범위**: SMALL_DBS 6개 · **286문항** 전체
- **모델**: `ollama:gemma4:e2b` · **MCP**: `http://127.0.0.1:8090/mcp` · concurrency **8**
- **서버**: 재빌드된 `target/debug/sql-mcp` · `data/semantic.db`(직전과 동일 스냅샷)
- **산출**: `client/eval/eval.json` · `client/eval/score_report.json`
- **직전 스냅샷**: `client/eval/archive/*2026-10-18T0742-ex37.76*` (EX 37.76%)

---

## 1. Executive Summary

| 지표 | 직전 | 이번 | 변화 |
|------|------|------|------|
| EX (unordered set) | 108/286 (**37.76%**) | 100/286 (**34.97%**) | **−2.79pp** (−8문항 net) |
| 실패 | 178 | 186 | +8 |
| gained / lost vs prior | — | +25 / −33 | net −8 |

**핵심 관찰**

1. **전체 EX는 소폭 하락(37.76%→34.97%).** 동일 시맨틱 DB·동일 모델에서 서버 바이너리만 갱신한 재측정으로, LLM 분산 + `formula_1` 붕괴(−12.1pp)가 주도.
2. **실행 성공·값 오답 구조는 유지.** 실패 186건 중 `pred_rows` 있음 128건(69%), 성공 `execute_sql` 없음 58건(31%). “틀린 로직/필터/프로젝션”이 여전히 주원인.
3. **DB 편차 지속.** `superhero` 61.5%(유지) · `student_club` 52.1% vs `formula_1` **16.7%** · `california_schools` 10.0%(직전 3.3%에서 소폭 회복).
4. **실패 버킷:** `dw_agent` 168 · `semantic_layer` 18 · `sql_mcp_code` 0. 팬아웃·공집합 신호가 직전보다 다소 증가.

---

## 2. 성적 분해

### 2.1 DB별 EX

| db_id | EX | n | 정확도 | vs prior |
|-------|----|---|--------|----------|
| superhero | 32 | 52 | **61.5%** | +0.0pp |
| student_club | 25 | 48 | **52.1%** | −2.1pp |
| thrombosis_prediction | 17 | 50 | **34.0%** | +4.0pp |
| toxicology | 12 | 40 | **30.0%** | −7.5pp |
| formula_1 | 11 | 66 | **16.7%** | −12.1pp |
| california_schools | 3 | 30 | **10.0%** | +6.7pp |

### 2.2 난이도별 EX

| difficulty | EX | n | 정확도 | vs prior |
|------------|----|---|--------|----------|
| simple | 39 | 83 | 47.0% | −7.2pp (45→39) |
| moderate | 45 | 135 | 33.3% | −2.2pp (48→45) |
| challenging | 16 | 68 | 23.5% | +1.5pp (15→16) |

### 2.3 실패 사유 (classify_failure)

| 건수 | 비율(실패 내) | 사유 |
|------|---------------|------|
| 61 | 32.8% | execute 성공 · gold와 로직/필터/값 불일치 |
| 45 | 24.2% | 성공한 execute_sql 없음 |
| 30 | 16.1% | 프로젝션 열 개수/구성 불일치 |
| 20 | 10.8% | 결과 공집합 (gold는 비공집합) |
| 17 | 9.1% | 행 수 과다 (조인 팬아웃 의심) |
| 7 | 3.8% | 물리 함수(DIVIDE/YEAR/strftime) |
| ≤3 | — | 구문·`SELECT *`·unknown member 등 |

---

## 3. 실패 구조 (Root Cause)

### ① 에이전트 로직/식별자 착오 (~61 + 공집합 20)

- Evidence·물리 스키마 직역, 필터 값·멤버명 착오가 최대 군집.
- DB별 로직 불일치: thrombosis 15 · toxicology 12 · formula_1 11 · california_schools 10.
- **california_schools**는 EX 자체는 올랐으나(1→3) 로직 오답이 여전히 절대 다수.

### ② 도구/재시도 고갈 (45건, 직전 34→45)

- `formula_1`에서 no-pred **30/58**(전체 no-pred의 절반 이상) — 중형 스키마에서 재시도 소진이 EX −12.1pp와 맞물림.
- 최대 tool_calls **32**(thrombosis qid=1243, ~47분).

### ③ 프로젝션 EX 엄격성 (30건)

- Gold 스칼라 vs dim+집계, 합성 문자열 vs 분리 열 등 — Soft-F1 미도입 상태에서 EX 감점 유지.

### ④ 시맨틱 조인/그레인 (semantic_layer 18 + 팬아웃 17)

- 직전 대비 `semantic_layer` 13→18, 팬아웃 12→17.
- 팬아웃 집중: toxicology 7 · thrombosis/formula_1 각 3. One-Big-View 그레인 혼합 리스크가 재확인됨 ([change_sementic.md](change_sementic.md) P0와 정합).

### ⑤ 날짜·물리 함수 (7건)

- `DIVIDE`/`YEAR`/`strftime` 버킷은 직전과 동일 규모 — 방언 힌트·allow-list 이슈는 미해소.

---

## 4. 직전 대비 해석

| 변화 | 해석 |
|------|------|
| 전체 −2.79pp (gained 25 / lost 33) | 동일 메타·모델에서 **분산이 큼**; 단일 런을 추세 단정에 쓰지 말 것 |
| formula_1 −12.1pp · no-pred 집중 | 서버 재빌드만으로는 중형 DB 안정화가 안 됨 → 멀티 큐브 분할(P0)이 본선 |
| california_schools +6.7pp · thrombosis +4.0pp | 소표본/분산 가능; 게이트(schools ≥25%)에는 여전히 미달 |
| toxicology −7.5pp · 팬아웃↑ | bridge/그레인 메타 손질 필요(P1) |
| superhero 61.5% 유지 | 회귀 없음 — 역할분리 룩업 유지 가치 |

참고: BIRD Mini-Dev SQLite 공개 baseline은 GPT-4 EX ~47.8% / Soft-F1 ~52%대([bird-bench/mini_dev](https://github.com/bird-bench/mini_dev)). 본 하네스는 Soft-F1 미집계·시맨틱-only 제약으로 직접 비교는 불가하나, 소형 로컬 모델(gemma4:e2b) 기준 35%대는 로직/프로젝션·표현력 병목이 남은 상태로 읽힘.

---

## 5. 권고 (우선순위)

1. **formula_1 멀티 큐브** — One-Big-View 해체; no-pred·EX 붕괴의 1차 타깃 ([change_sementic.md](change_sementic.md) §4.1).
2. **california_schools 발견성** — excellence/eligible rate 등 질문 어휘 ↔ metric/segment 정렬; 게이트 ≥25%까지 메타 집중.
3. **프로젝션 계약 + Soft-F1 병기** — EX만으로는 열 구성 감점이 과대; 보조 지표로 진행 추적.
4. **그레인/팬아웃** — toxicology·thrombosis bridge·코호트 segment; 뷰 간선 축소.
5. **재현** — `python run_eval.py --url http://127.0.0.1:8090/mcp --concurrency 8` → `score_eval_doc` → 본 문서 갱신. 추세 판단 시 **2회 이상** 권장.

---

## 6. 타이밍

- 문항당 평균 agent 경과 **~382s** (p50 ~218s, max ~2844s)
- 벽시계 ≈ **3h 49m** (concurrency 8, `elapsed_ms` 13,742,959)
- 평균 tool_calls ≈ **4.8**/문항 (최대 32, thrombosis_prediction qid=1243)
