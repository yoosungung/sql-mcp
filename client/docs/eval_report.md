# BIRD Mini-Dev Eval Report (sql-mcp + DW agent)

- **일시**: 2026-10-23 19:08 KST 시작 · 종료 `created_at` `2026-10-23T13:01:31Z` (2026-10-23 22:01 KST) · scored `2026-10-23T13:02:07Z`
- **범위**: SMALL_DBS 6개 · **286문항** 전체 · **라운드 3 카탈로그** (`data/semantic.hub.d28.eval.db` — 허브 + R3 검색/JOIN 철회 + 로직·필터 description)
- **모델**: `ollama:gemma4:e2b` · **MCP**: `http://127.0.0.1:8090/mcp` · concurrency **8**
- **서버**: `SQL_MCP_MODE=query` · BM25 only
- **산출**: `client/eval/eval.d28.json` · `client/eval/score_report.d28.json` · `client/eval/eval_sql.d28.json` · 문항별 [`eval.md`](eval.md)
- **직전**: 라운드 2 `eval.r2.json` (EX 110/286, 그레인 28, 지식 116). 허브 R1 `eval.hub.json` (EX 101/286). One-Big-View 베이스라인 `eval.json` (EX 112/286)

---

## 1. Executive Summary

| 지표 | One-Big-View | 허브 (R1) | R2 라우팅 | 이번 (R3 지식) | vs R2 |
|------|--------------|-----------|-----------|----------------|-------|
| EX (unordered set) | 112/286 (**39.16%**) | 101/286 (**35.31%**) | 110/286 (**38.46%**) | **116/286 (40.56%)** | **+2.10pp** (+6문항 net) |
| 실패 | 174 | 185 | 176 | 170 | −6 |
| gained / lost vs R2 | — | — | — | +32 / −26 | net +6 |
| 잘못된 시멘틱 정의 (그레인) | 61 | 33 | 28 | **34** | **+6** |
| agent가 실패 | 109 | 121 | 116 | **106** | **−10** |
| 기타 | 4 | 30 | 32 | 30 | −2 |
| sql-mcp 버그 | 0 | 1 | 0 | 0 | 0 |

**체크포인트 (change_semantic §10.3):** EX ≥ 112 **통과**(116) · 그레인 ≤ 20 **미달**(34, R2보다 악화) · 지식 ≤ 100 **미달**(106) · 기타 ungrouped JOIN ≤ 12 **미달**(19). Δ≠0이라 정지는 아님. 장기 게이트 0/0/286은 유지.

**핵심 관찰**

1. **허브 분할 이후 처음으로 One-Big-View(112)를 넘었다.** 로직/필터 description(GPT≠GLU, Phone, circuitRef, APS 등)이 thrombosis +6.0pp · california +10.0pp로 EX를 끌어올렸다.
2. **지식 116→106.** execute-OK 로직 불일치는 56→55로 거의 그대로라, 이득은 공집합·무SQL·일부 리터럴이 PASS로 바뀐 쪽이다. 같은 description을 또 늘리면 Δ≈0이 될 수 있다.
3. **그레인 28→34.** F1 잘못된 큐브(standings를 results, qualifying를 results)가 늘었다. 검색 문장이 형제 큐브로 새 실패를 만든 분류 이동이다.

다음: P1 CA/throm `Never JOIN` **게이트 실패**(아래 §5). 286 재eval 안 함. F1 standings description·Never JOIN 문구는 반복하지 않는다.

---

## 2. 성적 분해

### 2.1 DB별 EX

| db_id | EX | n | 정확도 | vs R2 | vs OBV |
|-------|----|---|--------|-------|--------|
| student_club | 33 | 48 | **68.8%** | +2.1pp | 0.0pp |
| superhero | 30 | 52 | **57.7%** | −3.8pp | −13.5pp |
| california_schools | 11 | 30 | **36.7%** | +10.0pp | +26.7pp |
| formula_1 | 20 | 66 | **30.3%** | 0.0pp | +4.5pp |
| toxicology | 10 | 40 | **25.0%** | +2.5pp | +2.5pp |
| thrombosis_prediction | 12 | 50 | **24.0%** | +6.0pp | −4.0pp |

### 2.2 난이도별 EX

| difficulty | EX | n | 정확도 | vs R2 |
|------------|----|---|--------|-------|
| simple | 46 | 83 | 55.4% | −3.6pp (49→46) |
| moderate | 52 | 135 | 38.5% | +5.9pp (44→52) |
| challenging | 18 | 68 | 26.5% | +1.5pp (17→18) |

### 2.3 실패 사유 (classify_failure)

| 건수 | 비율(실패 내) | 사유 |
|------|---------------|------|
| 55 | 32.4% | execute 성공 · gold와 로직/필터/값 불일치 |
| 40 | 23.5% | 성공한 execute_sql 없음 |
| 22 | 12.9% | 프로젝션 열 개수/구성 불일치 |
| 20 | 11.8% | 행 수 과다 (조인 팬아웃 의심) |
| 16 | 9.4% | 결과 공집합 (gold는 비공집합) |
| 6 | 3.5% | ORDER BY에 집계식 직접 사용 |
| 5 | 2.9% | 시맨틱 SQL 구문/괄호 오류 |
| 3 | 1.8% | SELECT * 금지 |
| 2 | 1.2% | 비허용 함수 (D25) |
| 1 | 0.6% | CASE END 누락 |

### 2.4 EX FAIL cause (`eval_sql.d28.json`, 170)

| cause | n |
|-------|---|
| agent가 실패 | 106 |
| 잘못된 시멘틱 정의 | 34 |
| 기타 | 30 |
| sql-mcp 버그 | 0 |

| db_id | 잘못된 시멘틱 정의 | agent가 실패 | 기타 |
|-------|-------------------|--------------|------|
| formula_1 | 17 | 27 | 2 |
| toxicology | 8 | 13 | 9 |
| california_schools | 5 | 8 | 6 |
| thrombosis_prediction | 3 | 30 | 5 |
| superhero | 1 | 20 | 1 |
| student_club | 0 | 8 | 7 |

잔여 그레인(34): F1 17건이 핵심 — standings/qualifying/랩을 `f1_race_results` 또는 형제 큐브로 고름(Q846, 902, 906, 948–951). tox는 connected/atom 과잉 JOIN. CA SAT에 `schools_frpm`. throm은 examination/patient 교차.

기타 30건 중 **ungrouped cube JOIN 19**. cube JOIN ON 누락 2.

---

## 3. 직전(R2) 대비

| 변화 | 해석 |
|------|------|
| 전체 +2.10pp (gained 32 / lost 26) | 지식 패치. One-Big-View 112를 **+4** |
| 그레인 28→34 | F1 잘못된 큐브가 지식 FAIL에서 그레인으로 이동·증가 |
| 지식 116→106 · 기타 32→30 | 로직 56→55. ungrouped JOIN 20→19 |
| 벽시계 ≈ 2h 53m · 문항 평균 288s (p50 225s, max 1425s) | R2 ≈ 2h 50m · 평균 284s |
| california +10.0pp · thrombosis +6.0pp | Phone/DOC/APS/GPT 검색이 EX로 이어짐 |
| superhero −3.8pp | colour/skin 패치가 회귀를 막지 못함 |

gained qid vs R2: 23, 26, 28, 31, 48, 230, 232, 244, 245, 253, 760, 779, 790, 872, 877, 909, 944, 977, 1002, 1162, 1164, 1169, 1185, 1189, 1198, 1229, 1256, 1317, 1339, 1350, 1392, 1401.

lost qid vs R2: 50, 62, 200, 236, 239, 249, 736, 761, 772, 788, 806, 854, 859, 931, 960, 1001, 1003, 1150, 1152, 1155, 1192, 1302, 1340, 1346, 1403, 1457.

---

## 4. 타이밍

- 문항당 평균 agent 경과 **~288s** (p50 ~225s, max ~1425s)
- 벽시계 ≈ **2h 53m** (concurrency 8)
- 평균 tool_calls ≈ **4.0**/문항

---

## 5. P1 CA/throm 80문항 게이트 (실패)

- **일시**: 2026-10-24 08:11 KST 시작 · 종료 `created_at` `2026-10-24T00:38:29Z` (2026-10-24 09:38 KST) · scored `2026-10-24T00:38:57Z`
- **범위**: `california_schools` 30 + `thrombosis_prediction` 50 · 카탈로그 `data/semantic.hub.d28.eval.db` (R3 + CA/throm `Never JOIN`)
- **산출**: `eval.d28.ca_throm.json` · `score_report.d28.ca_throm.json` · `eval_sql.d28.ca_throm.json`

| 지표 | R3 (같은 80) | 이번 | Δ |
|------|--------------|------|---|
| EX | 23/80 (28.8%) | **16/80 (20.0%)** | **−7** |
| california_schools | 11/30 | **5/30** | −6 |
| thrombosis_prediction | 12/50 | **11/50** | −1 |
| 그레인 | 8 | **10** | +2 |
| agent가 실패 | 38 | 38 | 0 |
| 기타 | 11 | 16 | +5 |

gained vs R3: 1155, 1156, 1220, 1275. lost: 23, 26, 28, 31, 47, 79, 1185, 1189, 1195, 1198, 1229.

형제 JOIN은 남음(Q24·25 `schools_frpm`+`schools_sat`, Q1187 exam+lab). CA schools-only(Q47·79·82)가 `schools_frpm`으로 가서 그레인·EX가 같이 나빠졌다. **이 레버 정지 · 286 안 함.**
