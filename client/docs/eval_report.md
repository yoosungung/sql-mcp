# BIRD Mini-Dev Eval Report (superhero only · gemma4:e2b-mlx)

- **일시**: `2026-10-28T22:19:12Z` · scored `2026-10-28T22:19:17Z`
- **범위**: target `superhero` · BIRD Mini-Dev **52문항** (SMALL_DBS 나머지 제외)
- **모델**: `ollama:gemma4:e2b-mlx` · MCP `http://127.0.0.1:8090/mcp` · concurrency 4
- **에이전트 입력**: 유저 메시지는 BIRD `question`만 (`evidence`·`db_id` 없음). DB는 `list_databases`로 선택.
- **산출**: `client/eval/eval.json` · `client/eval/score_report.json` · `client/eval/eval_sql.json` · 문항별 [`eval.md`](eval.md)

게이트(고정): 그레인 FAIL → 0 · 지식 FAIL → 0. 분류기: `semantic-from-eval` methodology.

---

## 1. Executive Summary

| 지표 | 이번 |
|------|------|
| EX (unordered set) | **21/52 (40.38%)** |
| 실패 | 31 |
| 잘못된 시멘틱 정의 (그레인) | **4** |
| agent가 실패 | **26** |
| sql-mcp 버그 | **0** |
| 기타 | **1** |

**체크포인트:** 그레인 0 **미달**(4) · 지식 0 **미달**(26). sql-mcp 버그는 없음. 소요 ~19.0분 (concurrency 4).

---

## 2. 성적 분해

### 2.1 난이도별 EX

| difficulty | EX | n | 정확도 |
|------------|----|---|--------|
| simple | 9 | 14 | 64.3% |
| moderate | 10 | 26 | 38.5% |
| challenging | 2 | 12 | 16.7% |

### 2.2 실패 사유 (`score_eval_doc`)

| 건수 | bucket | 사유 |
|------|--------|------|
| 24 | dw_agent | execute 성공 · gold와 로직/필터/값 불일치 |
| 3 | semantic_layer | 행 수 과다 (조인 팬아웃 의심) |
| 2 | dw_agent | 필터/조인으로 결과 공집합 |
| 2 | dw_agent | 프로젝션 열 개수/구성 불일치 |

### 2.3 EX FAIL cause (`eval_sql.json`, 31)

| cause | n | qid |
|-------|---|-----|
| agent가 실패 | 26 | 723, 724, 726, 728, 730, 732, 733, 738, 740, 743, 744, 747, 751, 753, 758, 775, 782, 788, 790, 791, 794, 797, 801, 819, 822, 829 |
| 잘못된 시멘틱 정의 | 4 | 745·765 (extra `superhero`), 772·773 (extra `colour`) |
| sql-mcp 버그 | 0 | — |
| 기타 | 1 | 736 (`unknown ORDER BY target: self_interest_pct`) |

문항별 Gold/Agent SQL·rows는 [`eval.md`](eval.md).
