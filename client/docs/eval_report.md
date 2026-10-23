# BIRD Mini-Dev Eval Report (sql-mcp + DW agent)

- **일시**: 2026-10-23 14:03 KST 시작 · 종료 `created_at` `2026-10-23T07:53:44Z` (2026-10-23 16:53 KST) · scored `2026-10-23T07:54:14Z`
- **범위**: SMALL_DBS 6개 · **286문항** 전체 · **라운드 2 카탈로그** (`data/semantic.hub.r2.db` — 허브 분할 + 큐브 Copy SELECT 라우팅)
- **모델**: `ollama:gemma4:e2b` · **MCP**: `http://127.0.0.1:8090/mcp` · concurrency **8**
- **서버**: `SQL_MCP_MODE=query` · BM25 only
- **산출**: `client/eval/eval.r2.json` · `client/eval/score_report.r2.json` · `client/eval/eval_sql.r2.json` · 문항별 [`eval.md`](eval.md)
- **직전**: 허브 런 `eval.hub.json` (EX 101/286, 그레인 33, 지식 121). One-Big-View 베이스라인 `eval.json` (EX 112/286)

---

## 1. Executive Summary

| 지표 | One-Big-View | 허브 (R1) | 이번 (R2 라우팅) | vs R1 |
|------|--------------|-----------|------------------|-------|
| EX (unordered set) | 112/286 (**39.16%**) | 101/286 (**35.31%**) | 110/286 (**38.46%**) | **+3.15pp** (+9문항 net) |
| 실패 | 174 | 185 | 176 | −9 |
| gained / lost vs R1 | — | — | +35 / −26 | net +9 |
| 잘못된 시멘틱 정의 (그레인) | 61 | 33 | **28** | **−5** |
| agent가 실패 | 109 | 121 | 116 | −5 |
| 기타 | 4 | 30 | 32 | +2 |
| sql-mcp 버그 | 0 | 1 | 0 | −1 |

**체크포인트 (change_semantic §9.3):** EX ≥ 112 **미달**(110) · 그레인 ≤ 20 **미달**(28) · 지식 ≤ 100 **미달**(116). Δ≠0(EX +9)이라 정지는 아님. 장기 게이트 0/0/286은 유지.

**핵심 관찰**

1. **라우팅 패치는 EX를 허브 런에서 회복시켰고 One-Big-View에는 2문항 못 미쳤다.** Copy SELECT가 superhero 61.5%(+5.8pp) · california 26.7%(+10.0pp) · formula_1 30.3%(+3.0pp)에 반영. F1 standings를 여전히 `f1_race_results`/`f1_constructor_results`로 고르는 그레인이 12건.
2. **그레인 33→28, 지식 121→116.** 남은 그레인의 다수는 잘못된 큐브 또는 cube JOIN 과잉(tox connected, club 교차, CA SAT에 FRPM).
3. **기타 32건 중 20건은 여전히 ungrouped cube JOIN.** D23 레시피가 description에 있는데 에이전트가 GROUP BY/`DISTINCT`를 빼는 실패가 늘었다.

다음: [`change_semantic.md` §10](change_semantic.md) P0 로직/필터는 `semantic.hub.d28.db`에 등록됨 — **재eval 286**. 그다음 P1 tox/club 한 큐브.

---

## 2. 성적 분해

### 2.1 DB별 EX

| db_id | EX | n | 정확도 | vs R1 | vs OBV |
|-------|----|---|--------|-------|--------|
| student_club | 32 | 48 | **66.7%** | +4.2pp | −2.1pp |
| superhero | 32 | 52 | **61.5%** | +5.8pp | −7.7pp |
| formula_1 | 20 | 66 | **30.3%** | +3.0pp | +4.5pp |
| california_schools | 8 | 30 | **26.7%** | +10.0pp | +16.7pp |
| toxicology | 9 | 40 | **22.5%** | 0.0pp | 0.0pp |
| thrombosis_prediction | 9 | 50 | **18.0%** | −2.0pp | −10.0pp |

### 2.2 난이도별 EX

| difficulty | EX | n | 정확도 | vs R1 |
|------------|----|---|--------|-------|
| simple | 49 | 83 | 59.0% | +2.4pp (47→49) |
| moderate | 44 | 135 | 32.6% | +3.0pp (40→44) |
| challenging | 17 | 68 | 25.0% | +4.4pp (14→17) |

### 2.3 실패 사유 (classify_failure)

| 건수 | 비율(실패 내) | 사유 |
|------|---------------|------|
| 56 | 31.8% | execute 성공 · gold와 로직/필터/값 불일치 |
| 41 | 23.3% | 성공한 execute_sql 없음 |
| 26 | 14.8% | 행 수 과다 (조인 팬아웃 의심) |
| 25 | 14.2% | 프로젝션 열 개수/구성 불일치 |
| 13 | 7.4% | 결과 공집합 (gold는 비공집합) |
| 7 | 4.0% | 시맨틱 SQL 구문/괄호 오류 |
| 5 | 2.8% | ORDER BY에 집계식 직접 사용 |
| 2 | 1.1% | 비허용 함수 (D25) |
| 1 | 0.6% | 비허용 함수+CASE 혼합 |

### 2.4 EX FAIL cause (`eval_sql.r2.json`, 176)

| cause | n |
|-------|---|
| agent가 실패 | 116 |
| 잘못된 시멘틱 정의 | 28 |
| 기타 | 32 |
| sql-mcp 버그 | 0 |

| db_id | 잘못된 시멘틱 정의 | agent가 실패 | 기타 |
|-------|-------------------|--------------|------|
| formula_1 | 12 | 30 | 4 |
| california_schools | 5 | 13 | 4 |
| toxicology | 4 | 17 | 10 |
| student_club | 3 | 10 | 3 |
| thrombosis_prediction | 3 | 29 | 9 |
| superhero | 1 | 17 | 2 |

잔여 그레인(28): F1은 standings 질의에 `f1_race_results`/`f1_constructor_results`(Q896–906, 948–951). tox는 connected 과잉 JOIN. CA는 SAT에 `schools_frpm`. club은 expense/attendance 교차. Q1185 `T` 미등록은 gold ``T-CHO`` 부분 문자열.

---

## 3. 직전(R1 허브) 대비

| 변화 | 해석 |
|------|------|
| 전체 +3.15pp (gained 35 / lost 26) | Copy SELECT 라우팅. One-Big-View 112에는 −2 |
| 그레인 33→28 | 잘못된 큐브가 줄었으나 F1 standings 잔여 |
| 지식 121→116 · 기타 30→32 | ungrouped JOIN 12→20. JOIN 레시피는 아직 약함 |
| 벽시계 ≈ 2h 50m · 문항 평균 284s (p50 204s, max 1492s) | R1 3h 15m · 평균 322s |
| california +10.0pp · superhero +5.8pp | 라우팅 이득이 가장 큼 |
| thrombosis −2.0pp | 지식 패치가 EX로 이어지지 않음 |

gained qid vs R1: 11, 37, 50, 62, 79, 195, 236, 242, 249, 730, 732, 761, 772, 792, 819, 824, 850, 865, 869, 904, 931, 971, 1003, 1152, 1155, 1195, 1252, 1302, 1394, 1398, 1403, 1404, 1432, 1457, 1464.

lost qid vs R1: 23, 82, 213, 232, 268, 282, 719, 751, 760, 791, 884, 895, 909, 912, 930, 1164, 1189, 1198, 1208, 1220, 1229, 1317, 1339, 1350, 1381, 1401.

---

## 4. 타이밍

- 문항당 평균 agent 경과 **~284s** (p50 ~204s, max ~1492s)
- 벽시계 ≈ **2h 50m** (concurrency 8)
- 평균 tool_calls ≈ **4.0**/문항
