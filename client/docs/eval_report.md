# BIRD Mini-Dev Eval Report (superhero only)

- **일시**: `2026-10-28T05:00:56Z` · scored `2026-10-28T05:01:08Z`
- **범위**: target `superhero` · BIRD Mini-Dev **52문항** (SMALL_DBS 나머지 제외)
- **모델**: `ollama:gemma4:e2b` · MCP `http://127.0.0.1:8090/mcp` · concurrency 8
- **에이전트 입력**: 유저 메시지는 BIRD `question`만 (`evidence`·`db_id` 없음). DB는 `list_databases`로 선택.
- **산출**: `client/eval/eval.json` · `client/eval/score_report.json` · `client/eval/eval_sql.json` · 문항별 [`eval.md`](eval.md)
- **직전 동일 DB**: evidence+db 프롬프트 superhero **35/52 (67.31%)**

게이트(고정): 그레인 FAIL → 0 · 지식 FAIL → 0. 분류기: `semantic-from-eval` methodology.

---

## 1. Executive Summary

| 지표 | 이번 (question only) | 직전 (evidence+db 프롬프트) |
|------|----------------------|----------------------------|
| EX (unordered set) | **20/52 (38.46%)** | 35/52 (67.31%) |
| 실패 | 32 | 17 |
| 잘못된 시멘틱 정의 (그레인) | **3** | 5 |
| agent가 실패 | **29** | 11 |
| sql-mcp 버그 | **0** | 0 |
| 기타 | **0** | 1 |

**체크포인트:** 그레인 0 **미달**(3) · 지식 0 **미달**(29). sql-mcp 버그는 없음. evidence/DB 이름을 프롬프트에서 뺀 뒤 EX가 67.31% → 38.46%로 떨어졌다.

---

## 2. 성적 분해

### 2.1 난이도별 EX

| difficulty | EX | n | 정확도 |
|------------|----|---|--------|
| simple | 8 | 14 | 57.1% |
| moderate | 10 | 26 | 38.5% |
| challenging | 2 | 12 | 16.7% |

### 2.2 실패 사유 (`score_eval_doc`)

| 건수 | bucket | 사유 |
|------|--------|------|
| 20 | dw_agent | execute 성공 · gold와 로직/필터/값 불일치 |
| 5 | dw_agent | 프로젝션 열 개수/구성 불일치 |
| 3 | semantic_layer | 행 수 과다 (조인 팬아웃 의심) |
| 3 | dw_agent | 성공한 execute_sql 없음 |
| 1 | dw_agent | 필터/조인으로 결과 공집합 |

### 2.3 EX FAIL cause (`eval_sql.json`, 32)

| cause | n | qid |
|-------|---|-----|
| agent가 실패 | 29 | 717, 723, 726, 728, 730, 732, 733, 736, 737, 738, 739, 740, 743, 744, 745, 751, 758, 766, 769, 775, 786, 788, 790, 791, 800, 801, 819, 822, 829 |
| 잘못된 시멘틱 정의 | 3 | 772·773 (extra `colour`), 779 (extra `superpower`) |
| sql-mcp 버그 | 0 | — |
| 기타 | 0 | — |

문항별 Gold/Agent SQL·rows는 [`eval.md`](eval.md).
