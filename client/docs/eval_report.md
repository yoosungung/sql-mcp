# BIRD Mini-Dev Eval Report (superhero only)

- **일시**: `2026-10-28T11:53:08Z` · scored `2026-10-28T11:53:18Z`
- **범위**: target `superhero` · BIRD Mini-Dev **52문항** (SMALL_DBS 나머지 제외)
- **모델**: `ollama:gemma4:e2b` · MCP `http://127.0.0.1:8090/mcp` · concurrency 8
- **에이전트 입력**: 유저 메시지는 BIRD `question`만 (`evidence`·`db_id` 없음). DB는 `list_databases`로 선택.
- **산출**: `client/eval/eval.json` · `client/eval/score_report.json` · `client/eval/eval_sql.json` · 문항별 [`eval.md`](eval.md)
- **직전 동일 DB**: question-only superhero **20/52 (38.46%)** (2026-10-28T05:00:56Z)

게이트(고정): 그레인 FAIL → 0 · 지식 FAIL → 0. 분류기: `semantic-from-eval` methodology.

---

## 1. Executive Summary

| 지표 | 이번 | 직전 (question only) |
|------|------|----------------------|
| EX (unordered set) | **26/52 (50.0%)** | 20/52 (38.46%) |
| 실패 | 26 | 32 |
| 잘못된 시멘틱 정의 (그레인) | **2** | 3 |
| agent가 실패 | **24** | 29 |
| sql-mcp 버그 | **0** | 0 |
| 기타 | **0** | 0 |

**체크포인트:** 그레인 0 **미달**(2) · 지식 0 **미달**(24). sql-mcp 버그는 없음. EX가 38.46% → 50.0%로 올랐다 (+6).

---

## 2. 성적 분해

### 2.1 난이도별 EX

| difficulty | EX | n | 정확도 |
|------------|----|---|--------|
| simple | 9 | 14 | 64.3% |
| moderate | 15 | 26 | 57.7% |
| challenging | 2 | 12 | 16.7% |

### 2.2 실패 사유 (`score_eval_doc`)

| 건수 | bucket | 사유 |
|------|--------|------|
| 15 | dw_agent | execute 성공 · gold와 로직/필터/값 불일치 |
| 5 | dw_agent | 프로젝션 열 개수/구성 불일치 |
| 3 | dw_agent | 필터/조인으로 결과 공집합 |
| 2 | dw_agent | 성공한 execute_sql 없음 |
| 1 | semantic_layer | 행 수 과다 (조인 팬아웃 의심) |

### 2.3 EX FAIL cause (`eval_sql.json`, 26)

| cause | n | qid |
|-------|---|-----|
| agent가 실패 | 24 | 724, 726, 728, 730, 736, 740, 743, 744, 745, 747, 751, 753, 758, 765, 775, 782, 788, 790, 791, 792, 801, 819, 822, 829 |
| 잘못된 시멘틱 정의 | 2 | 772·773 (extra `colour`) |
| sql-mcp 버그 | 0 | — |
| 기타 | 0 | — |

문항별 Gold/Agent SQL·rows는 [`eval.md`](eval.md).
