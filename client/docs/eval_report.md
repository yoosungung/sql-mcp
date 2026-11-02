# BIRD Mini-Dev Eval Report (student_club only · gemma4:e2b-mlx)

- **일시**: `2026-10-29T06:28:00Z` · scored `2026-10-29T06:28:25Z`
- **범위**: target `student_club` · BIRD Mini-Dev **48문항** (SMALL_DBS 나머지 제외)
- **모델**: `ollama:gemma4:e2b-mlx` · MCP `http://127.0.0.1:8090/mcp` · concurrency 4
- **에이전트 입력**: 유저 메시지는 BIRD `question`만 (`evidence`·`db_id` 없음). DB는 `list_databases`로 선택.
- **카탈로그**: `list_tables` = `cube_join_student_club` 1개 (view `join_student_club` One-Big-View). 허브 분할 큐브 없음.
- **산출**: `client/eval/eval.json` · `client/eval/score_report.json` · `client/eval/eval_sql.json` · 문항별 [`eval.md`](eval.md)

게이트(고정): 그레인 FAIL → 0 · 지식 FAIL → 0. 분류기: `semantic-from-eval` methodology.

---

## 1. Executive Summary

| 지표 | 이번 |
|------|------|
| EX (unordered set) | **15/48 (31.25%)** |
| 실패 | 33 |
| 잘못된 시멘틱 정의 (그레인) | **6** |
| agent가 실패 | **25** |
| sql-mcp 버그 | **0** |
| 기타 | **2** |

**체크포인트:** 그레인 0 **미달**(6) · 지식 0 **미달**(25). sql-mcp 버그는 없음. 소요 ~25.3분 (concurrency 4).

---

## 2. 성적 분해

### 2.1 난이도별 EX

| difficulty | EX | n | 정확도 |
|------------|----|---|--------|
| simple | 9 | 21 | 42.9% |
| moderate | 6 | 22 | 27.3% |
| challenging | 0 | 5 | 0.0% |

### 2.2 실패 사유 (`score_eval_doc`)

| 건수 | bucket | 사유 |
|------|--------|------|
| 14 | dw_agent | 성공한 execute_sql 없음 (도구/재시도 실패) |
| 8 | dw_agent | execute 성공 · gold와 로직/필터/값 불일치 |
| 4 | semantic_layer | 행 수 과다 (조인 팬아웃 의심) |
| 3 | dw_agent | 프로젝션 열 개수/구성 불일치 |
| 2 | dw_agent | 필터/조인으로 결과 공집합 |
| 1 | dw_agent | `SELECT *` 금지 |
| 1 | semantic_layer | 시맨틱 식별자 미등록 |

### 2.3 EX FAIL cause (`eval_sql.json`, 33)

| cause | n | qid |
|-------|---|-----|
| agent가 실패 | 25 | 1317, 1322, 1339, 1344, 1351, 1357, 1359, 1361, 1362, 1368, 1371, 1375, 1381, 1390, 1398, 1399, 1401, 1403, 1404, 1410, 1422, 1427, 1457, 1460, 1464 |
| 잘못된 시멘틱 정의 | 6 | 1323 (extra `budget`), 1331·1432 (extra `budget`,`expense`), 1350 (extra `event`), 1387 (extra `attendance`), 1409 (extra `budget`,`event`) |
| sql-mcp 버그 | 0 | — |
| 기타 | 2 | 1411·1435 (`cube superhero`를 `student_club`에 질의) |

에이전트가 `FROM student_club`(큐브명=`cube_join_student_club`)을 쓴 건 9문항(`not found: cube/student_club`: 1317, 1339, 1344, 1357, 1359, 1381, 1390, 1410, 1457).

문항별 Gold/Agent SQL·rows는 [`eval.md`](eval.md).
