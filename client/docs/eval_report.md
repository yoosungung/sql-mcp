# BIRD Mini-Dev Eval Report (sql-mcp + DW agent)

- **일시**: 2026-10-25 09:20 KST 시작 · 종료 `created_at` `2026-10-25T03:16:19Z` (2026-10-25 12:16 KST) · scored `2026-10-25T03:16:53Z`
- **범위**: SMALL_DBS 6개 · **286문항** 전체 · **fail-plan 카탈로그** (`data/semantic.failplan.eval.db` — `fail_plan.md` 46패치를 `enrich_meta_from_qa` description/examples에 반영, `SQL_MCP_GRAIN_GUARD` 끔)
- **모델**: `ollama:gemma4:e2b` · **MCP**: `http://127.0.0.1:8090/mcp` · concurrency **8** (연결 끊김 없음, 재시도 0)
- **서버**: `SQL_MCP_MODE=query` · BM25 only · clean `cargo test -p sql-mcp --locked` 통과 바이너리
- **산출**: `client/eval/eval.failplan.json` · `client/eval/score_report.failplan.json` · `client/eval/eval_sql.failplan.json` · 문항별 [`eval.md`](eval.md)
- **직전**: D29 `eval.d29.json` (EX 116/286, 그레인 30, 지식 112). 라운드 3 `eval.d28.json` (116/286, 그레인 34, 지식 106). R2 `eval.r2.json` (110/286). 허브 R1 `eval.hub.json` (101/286). One-Big-View `eval.json` (112/286)

---

## 1. Executive Summary

| 지표 | One-Big-View | 허브 (R1) | R2 | R3 지식 | D29 | 이번 (fail plan) | vs D29 |
|------|--------------|-----------|----|---------|-----|------------------|--------|
| EX (unordered set) | 112/286 (**39.16%**) | 101/286 (**35.31%**) | 110/286 (**38.46%**) | 116/286 (**40.56%**) | 116/286 (**40.56%**) | **111/286 (38.81%)** | **−1.75pp** (net −5) |
| 실패 | 174 | 185 | 176 | 170 | 170 | 175 | +5 |
| gained / lost vs D29 | — | — | — | — | — | +25 / −30 | net −5 |
| 잘못된 시멘틱 정의 (그레인) | 61 | 33 | 28 | 34 | 30 | **43** | **+13** |
| agent가 실패 | 109 | 121 | 116 | 106 | 112 | **103** | **−9** |
| 기타 | 4 | 30 | 32 | 30 | 28 | **29** | +1 |
| sql-mcp 버그 | 0 | 1 | 0 | 0 | 0 | **0** | 0 |

**체크포인트 (change_semantic §9.3 / §10.3):** EX ≥ 112 **미달**(111) · 그레인 ≤ 20 **미달**(43, D29 30에서 악화) · 지식 ≤ 100 **미달**(103, D29 112에서 개선) · 기타 ungrouped JOIN ≤ 12 **미달**(14, D29와 동일). 장기 게이트 0/0/286은 유지.

**핵심 관찰**

1. **전체 EX는 D29보다 5건 낮다.** 25개가 새로 맞고 30개가 되돌아갔다. superhero **+2**(36→38)와 formula_1 **+1**(19→20)이 student_club **−4**(31→27) · thrombosis **−3**(13→10) · california **−1**(7→6)을 메우지 못했다. toxicology는 10으로 같다.
2. **그레인 30→43.** F1 extra JOIN이 13→19, club 5→8, throm 3→6, superhero 0→2. CA는 6→5, tox는 3으로 같다. 빈 description 채움과 “Alias is the cube name, not T1.” 문장은 잘못된 큐브 선택을 줄이지 못했고, 이전에 맞던 Q736·Q772·Q978·Q1192·Q1255가 그레인 FAIL로 들어왔다.
3. **지식 112→103.** 성공 SQL이 없는 문항은 37→34. execute-OK 로직 불일치는 55→57. 기타는 28→29이고, 그중 ungrouped cube JOIN은 14로 같고 JOIN ON 누락은 5→6이다.

다음: `SQL_MCP_GRAIN_GUARD`는 아직 끈 상태. 이번 런은 그레인이 늘었으므로 DB별 가드 확장 조건(그레인↓ 그리고 해당 DB EX ≥ R3)을 충족하지 않는다. CA `Never JOIN` 설명문과 `{db}_all`은 넣지 않았다.

---

## 2. 성적 분해

### 2.1 DB별 EX

| db_id | EX | n | 정확도 | vs D29 | vs R3 |
|-------|----|---|--------|--------|-------|
| superhero | 38 | 52 | **73.1%** | +3.8pp | +15.4pp |
| student_club | 27 | 48 | **56.3%** | −8.3pp | −12.5pp |
| formula_1 | 20 | 66 | **30.3%** | +1.5pp | 0.0pp |
| toxicology | 10 | 40 | **25.0%** | 0.0pp | 0.0pp |
| thrombosis_prediction | 10 | 50 | **20.0%** | −6.0pp | −4.0pp |
| california_schools | 6 | 30 | **20.0%** | −3.3pp | −16.7pp |

### 2.2 난이도별 EX

| difficulty | EX | n | 정확도 | vs D29 |
|------------|----|---|--------|--------|
| simple | 47 | 83 | 56.6% | −1.2pp (48→47) |
| moderate | 45 | 135 | 33.3% | −2.2pp (48→45) |
| challenging | 19 | 68 | 27.9% | −1.5pp (20→19) |

### 2.3 실패 사유 (classify_failure)

| 건수 | 비율(실패 내) | 사유 |
|------|---------------|------|
| 57 | 32.6% | execute 성공 · gold와 로직/필터/값 불일치 |
| 34 | 19.4% | 성공한 execute_sql 없음 |
| 23 | 13.1% | 프로젝션 열 개수/구성 불일치 |
| 21 | 12.0% | 행 수 과다 (조인 팬아웃 의심) |
| 13 | 7.4% | 시맨틱 식별자 미등록 |
| 12 | 6.9% | 결과 공집합 (gold는 비공집합) |
| 4 | 2.3% | 시맨틱 SQL 구문/괄호 오류 |
| 4 | 2.3% | 시맨틱 제약에 막힌 뒤 근사 질의 오답 |
| 2 | 1.1% | 비허용 함수 (D25) |
| 1 | 0.6% | 비허용 함수+CASE 혼합 |
| 1 | 0.6% | 물리 alias `T1` 사용 |
| 1 | 0.6% | SELECT * 금지 |
| 1 | 0.6% | ORDER BY에 집계식 직접 사용 |
| 1 | 0.6% | 모드/절 위치에 맞지 않는 집계 |

### 2.4 EX FAIL cause (`eval_sql.failplan.json`, 175)

| cause | n |
|-------|---|
| agent가 실패 | 103 |
| 잘못된 시멘틱 정의 | 43 |
| 기타 | 29 |
| sql-mcp 버그 | 0 |

| db_id | 잘못된 시멘틱 정의 | agent가 실패 | 기타 |
|-------|-------------------|--------------|------|
| formula_1 | 19 | 25 | 2 |
| thrombosis_prediction | 6 | 23 | 11 |
| toxicology | 3 | 18 | 9 |
| california_schools | 5 | 18 | 1 |
| superhero | 2 | 11 | 1 |
| student_club | 8 | 8 | 5 |

잔여 그레인(43): F1 19(Q846, 861, 866, 881, 902, 906, 930, 940, 948, 950, 951, 959, 967, 977, 978, 981, 989, 990, 1014) · club 8(Q1322, 1338, 1340, 1371, 1389, 1390, 1401, 1404) · throm 6(Q1175, 1187, 1192, 1195, 1247, 1255) · CA 5(Q25, 27, 40, 41, 82) · tox 3(Q197, 234, 245) · superhero 2(Q736, 772).

기타 29건 중 **ungrouped cube JOIN 14** · **JOIN ON 누락 6**. throm이 기타 11로 가장 많다.

---

## 3. 직전(D29) 대비

| 변화 | 해석 |
|------|------|
| 전체 −1.75pp (gained 25 / lost 30) | fail_plan 46패치(빈 description·alias 문장·metric copy). EX는 줄고 지식만 이동 |
| 그레인 30→43 | F1 13→19 · club 5→8 · throm 3→6 · superhero 0→2. CA 6→5 |
| 지식 112→103 · 기타 28→29 | 무SQL 37→34. 로직 불일치 55→57. ungrouped JOIN 14 유지 |
| 벽시계 ≈ 2h 56m · 문항 평균 293s (p50 241s, max 1637s) | D29 본 런 ≈ 1h 53m · 평균 202s. 이번엔 재시도 없음 |
| superhero +3.8pp | 38/52. 다만 Q736·Q772는 그레인 FAIL로 빠짐 |
| student_club −8.3pp | 31→27. 그레인 5→8 |
| thrombosis −6.0pp | 13→10. 기타 6→11 |

gained qid vs D29: 45, 62, 231, 236, 260, 282, 719, 732, 765, 775, 794, 800, 859, 877, 895, 931, 1152, 1157, 1189, 1201, 1220, 1302, 1359, 1362, 1392.

lost qid vs D29: 5, 32, 79, 208, 212, 230, 239, 736, 751, 772, 791, 933, 978, 1002, 1149, 1155, 1156, 1169, 1192, 1232, 1255, 1256, 1281, 1339, 1357, 1371, 1376, 1381, 1401, 1457.

---

## 4. 타이밍

- 문항당 평균 agent 경과 **~293s** (p50 ~241s, max ~1637s)
- 벽시계 ≈ **2h 56m** (concurrency 8, `elapsed_ms` 10537375). 연결 오류 0
- 평균 tool_calls ≈ **4.2**/문항
