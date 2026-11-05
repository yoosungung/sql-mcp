# BIRD Mini-Dev Eval Report (superhero only · gemma4:e2b-mlx)

- **일시**: `2026-11-04T07:45:05Z` · scored `2026-11-04T08:18:18Z`
- **범위**: target `superhero` · BIRD Mini-Dev **52문항** (SMALL_DBS 나머지 제외)
- **모델**: `ollama:gemma4:e2b-mlx` · MCP `http://127.0.0.1:8090/mcp` · concurrency 4
- **에이전트 입력**: 유저 메시지는 BIRD `question`만 (`evidence`·`db_id` 없음). DB는 `list_databases`로 선택.
- **카탈로그**: `list_tables` = `superhero` 1개 (cube → view `join_superhero`). dim `samples` + description 지식 반영 후 재eval.
- **산출**: `client/eval/eval.json` · `client/eval/score_report.json` · `client/eval/eval_sql.json` · 문항별 [`eval.md`](eval.md)

게이트(고정): 그레인 FAIL → 0 · 지식 FAIL → 0. 분류기: `semantic-from-eval` methodology.

이전 베이스라인: EX **27/52 (51.92%)** (samples/description 반영 전).

---

## 1. Executive Summary

| 지표 | 이번 | 이전 |
|------|------|------|
| EX (unordered set) | **32/52 (61.54%)** | 27/52 (51.92%) |
| 실패 | 20 | 25 |
| 잘못된 시멘틱 정의 (그레인) | **2** | 2 |
| agent가 실패 | **18** | 22 |
| sql-mcp 버그 | **0** | 0 |
| 기타 | **0** | 1 |

**체크포인트:** 그레인 0 **미달**(2) · 지식 0 **미달**(18). sql-mcp 버그 없음. 소요 ~33분 (concurrency 4). ΔEX **+5**.

스모크: `agent_bird.py --db superhero --question-id 717` → superpower 목록 반환 (OK).

새로 맞춘 qid: 724, 736, 737, 740, 758, 796, 800, 806, 822. 신규 FAIL: 738, 747, 765, 769.

---

## 2. 성적 분해

### 2.1 난이도별 EX

| difficulty | EX | n | 정확도 | 이전 |
|------------|----|---|--------|------|
| simple | 9 | 14 | 64.3% | 71.4% |
| moderate | 21 | 26 | 80.8% | 57.7% |
| challenging | 2 | 12 | 16.7% | 16.7% |

### 2.2 실패 사유 (`score_eval_doc`)

| 건수 | bucket | 사유 |
|------|--------|------|
| 12 | dw_agent | execute 성공 · gold와 로직/필터/값 불일치 |
| 4 | dw_agent | 프로젝션 열 개수/구성 불일치 |
| 3 | dw_agent | 성공한 execute_sql 없음 (도구/재시도 실패) |
| 1 | semantic_layer | 행 수 과다 (조인 팬아웃 의심) |

### 2.3 EX FAIL cause (`eval_sql.json`, 20)

| cause | n | qid |
|-------|---|-----|
| agent가 실패 | 18 | 726, 728, 730, 738, 743, 744, 747, 751, 765, 769, 773, 775, 788, 790, 791, 801, 819, 829 |
| 잘못된 시멘틱 정의 | 2 | 745, 772 |
| sql-mcp 버그 | 0 | — |
| 기타 | 0 | — |

문항별 Gold/Agent SQL·rows는 [`eval.md`](eval.md).
