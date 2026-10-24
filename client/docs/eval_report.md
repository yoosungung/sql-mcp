# BIRD Mini-Dev Eval Report (sql-mcp + DW agent)

- **일시**: 2026-10-24 19:11 KST 시작 · 종료 `created_at` `2026-10-24T12:44:55Z` (2026-10-24 21:44 KST) · scored `2026-10-24T12:45:11Z`
- **범위**: SMALL_DBS 6개 · **286문항** 전체 · **D29 카탈로그** (`data/semantic.d29.eval.db` — `generate_meta`+`enrich` 허브 뷰 `relationship`/`grain`, `SQL_MCP_GRAIN_GUARD` 끔)
- **모델**: `ollama:gemma4:e2b` · **MCP**: `http://127.0.0.1:8090/mcp` · concurrency **8** (Ollama 중단 후 tox/CA 43문항은 conc 2→1로 재실행)
- **서버**: `SQL_MCP_MODE=query` · BM25 only · clean `cargo test -p sql-mcp --locked` 통과 바이너리
- **산출**: `client/eval/eval.d29.json` · `client/eval/score_report.d29.json` · `client/eval/eval_sql.d29.json` · 문항별 [`eval.md`](eval.md)
- **직전**: 라운드 3 `eval.d28.json` (EX 116/286, 그레인 34, 지식 106). R2 `eval.r2.json` (110/286). 허브 R1 `eval.hub.json` (101/286). One-Big-View `eval.json` (112/286)

---

## 1. Executive Summary

| 지표 | One-Big-View | 허브 (R1) | R2 | R3 지식 | 이번 (D29) | vs R3 |
|------|--------------|-----------|----|---------|------------|-------|
| EX (unordered set) | 112/286 (**39.16%**) | 101/286 (**35.31%**) | 110/286 (**38.46%**) | 116/286 (**40.56%**) | **116/286 (40.56%)** | **0.00pp** (net 0) |
| 실패 | 174 | 185 | 176 | 170 | 170 | 0 |
| gained / lost vs R3 | — | — | — | — | +30 / −30 | net 0 |
| 잘못된 시멘틱 정의 (그레인) | 61 | 33 | 28 | 34 | **30** | **−4** |
| agent가 실패 | 109 | 121 | 116 | 106 | **112** | **+6** |
| 기타 | 4 | 30 | 32 | 30 | **28** | −2 |
| sql-mcp 버그 | 0 | 1 | 0 | 0 | **0** | 0 |

**체크포인트 (change_semantic §10.3):** EX ≥ 112 **통과**(116, R3와 동률) · 그레인 ≤ 20 **미달**(30, R3 34에서 개선) · 지식 ≤ 100 **미달**(112, R3보다 악화) · 기타 ungrouped JOIN ≤ 12 **미달**(14, R3 19에서 개선). Δ(EX)=0이지만 cause는 이동함. 장기 게이트 0/0/286은 유지.

**핵심 관찰**

1. **전체 EX는 R3와 같다.** 문항은 30개씩 갈렸다. superhero **+6**(30→36)이 california **−4**(11→7) · club **−2** · F1 **−1**을 상쇄하고 thrombosis **+1**.
2. **그레인 34→30.** F1 extra JOIN이 17→13. tox 8→3. club은 0→5(이벤트 질의에 형제 큐브). D29 `relationship`/`grain` 등록은 허브 게이트(`hub_grain.py` ok mcp)를 통과했지만, 에이전트 큐브 선택은 그대로다. 거절 플래그는 끄고 돌렸다.
3. **지식 106→112.** execute-OK 로직 불일치는 55로 동일. superhero 검색이 EX로 이어진 반면 CA SAT/FRPM 라우팅과 club이 되돌렸다.

다음: `SQL_MCP_GRAIN_GUARD`는 아직 끈 상태. 켠 뒤에는 ROADMAP대로 DB별 게이트(그레인↓ 그리고 해당 DB EX ≥ R3) 뒤에만 확장. CA `Never JOIN` 설명문·F1 standings description 반복은 하지 않는다.

---

## 2. 성적 분해

### 2.1 DB별 EX

| db_id | EX | n | 정확도 | vs R3 | vs OBV |
|-------|----|---|--------|-------|--------|
| superhero | 36 | 52 | **69.2%** | +11.5pp | −1.9pp |
| student_club | 31 | 48 | **64.6%** | −4.2pp | −4.2pp |
| formula_1 | 19 | 66 | **28.8%** | −1.5pp | +3.0pp |
| thrombosis_prediction | 13 | 50 | **26.0%** | +2.0pp | −2.0pp |
| toxicology | 10 | 40 | **25.0%** | 0.0pp | +2.5pp |
| california_schools | 7 | 30 | **23.3%** | −13.4pp | +13.3pp |

### 2.2 난이도별 EX

| difficulty | EX | n | 정확도 | vs R3 |
|------------|----|---|--------|-------|
| simple | 48 | 83 | 57.8% | +2.4pp (46→48) |
| moderate | 48 | 135 | 35.6% | −3.0pp (52→48) |
| challenging | 20 | 68 | 29.4% | +2.9pp (18→20) |

### 2.3 실패 사유 (classify_failure)

| 건수 | 비율(실패 내) | 사유 |
|------|---------------|------|
| 55 | 32.4% | execute 성공 · gold와 로직/필터/값 불일치 |
| 37 | 21.8% | 성공한 execute_sql 없음 |
| 23 | 13.5% | 프로젝션 열 개수/구성 불일치 |
| 16 | 9.4% | 행 수 과다 (조인 팬아웃 의심) |
| 12 | 7.1% | 결과 공집합 (gold는 비공집합) |
| 11 | 6.5% | 시맨틱 식별자 미등록 |
| 5 | 2.9% | 시맨틱 제약에 막힌 뒤 근사 질의 오답 |
| 4 | 2.4% | 물리 alias `T1` 사용 |
| 2 | 1.2% | 시맨틱 SQL 구문/괄호 오류 |
| 2 | 1.2% | 비허용 함수 (D25) |
| 1 | 0.6% | SELECT * 금지 |
| 1 | 0.6% | ORDER BY에 집계식 직접 사용 |
| 1 | 0.6% | 모드/절 위치에 맞지 않는 집계 |

### 2.4 EX FAIL cause (`eval_sql.d29.json`, 170)

| cause | n |
|-------|---|
| agent가 실패 | 112 |
| 잘못된 시멘틱 정의 | 30 |
| 기타 | 28 |
| sql-mcp 버그 | 0 |

| db_id | 잘못된 시멘틱 정의 | agent가 실패 | 기타 |
|-------|-------------------|--------------|------|
| formula_1 | 13 | 32 | 2 |
| student_club | 5 | 8 | 4 |
| california_schools | 6 | 14 | 3 |
| thrombosis_prediction | 3 | 28 | 6 |
| toxicology | 3 | 14 | 13 |
| superhero | 0 | 16 | 0 |

잔여 그레인(30): F1 13(Q866, 872, 879, 902, 906, 930, 948, 950, 951, 959, 989, 990, 1014) · CA 6(Q17, 25, 27, 36, 41, 82) · club 5(Q1338, 1340, 1362, 1389, 1390) · tox 3(Q243, 248, 253) · throm 3(Q1152, 1153, 1187).

기타 28건 중 **ungrouped cube JOIN 14** · **JOIN ON 누락 5**. tox가 기타 13으로 가장 많다.

---

## 3. 직전(R3) 대비

| 변화 | 해석 |
|------|------|
| 전체 0.00pp (gained 30 / lost 30) | D29 관계·그레인 메타. EX 동률, cause만 이동 |
| 그레인 34→30 | F1 17→13 · tox 8→3. club 0→5가 일부를 되돌림 |
| 지식 106→112 · 기타 30→28 | 로직 55 유지. ungrouped JOIN 19→14 |
| 벽시계 ≈ 1h 53m(+재시도) · 문항 평균 202s (p50 195s, max 607s) | R3 ≈ 2h 53m · 평균 288s |
| superhero +11.5pp | 히어로 큐브 분할(attribute/power/superhero)이 EX로 이어짐 |
| california −13.4pp | SAT/FRPM 라우팅 회귀 (R3 11→7). P1 Never JOIN과 같은 방향 |

gained qid vs R3: 32, 212, 213, 220, 239, 268, 736, 751, 761, 769, 772, 788, 791, 796, 806, 854, 960, 978, 1149, 1155, 1156, 1192, 1208, 1232, 1255, 1281, 1346, 1376, 1381, 1457.

lost qid vs R3: 23, 26, 28, 31, 48, 242, 244, 245, 253, 260, 732, 794, 819, 872, 877, 944, 977, 1157, 1162, 1185, 1189, 1195, 1201, 1252, 1317, 1359, 1362, 1392, 1404, 1432.

---

## 4. 타이밍

- 문항당 평균 agent 경과 **~202s** (p50 ~195s, max ~607s)
- 본 런 벽시계 ≈ **1h 53m** (concurrency 8). Ollama가 tox/CA 구간에서 내려가 43문항을 재실행(+~25m, conc 2) + Q273 1건
- 평균 tool_calls ≈ **4.0**/문항
