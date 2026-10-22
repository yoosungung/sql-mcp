# BIRD Mini-Dev Eval Report (sql-mcp + DW agent)

- **일시**: 2026-10-20 (UTC start `12:18` · `created_at`/`ended` `15:19`, `scored_at` 15:19)
- **범위**: SMALL_DBS 6개 · **286문항** 전체
- **모델**: `ollama:gemma4:e2b` · **MCP**: `http://127.0.0.1:8090/mcp` · concurrency **8**
- **서버**: `target/debug/sql-mcp` (`cargo clean -p sql-mcp` 후 재빌드) · `data/semantic.db`(직전과 동일 스냅샷)
- **이번 변경**: `IdenticalSqlFailureMiddleware` Max Retries=3 (동일 `execute_sql` 검증 실패 즉시 종료) · D26 서브쿼리 허용이 바이너리에 반영됨
- **산출**: `client/eval/eval.json` · `client/eval/score_report.json` · 문항별 [`eval.md`](eval.md)
- **직전 스냅샷**: `client/eval/archive/*2026-10-20T0037-ex37.76*` (EX 37.76%)

---

## 1. Executive Summary

| 지표 | 직전 | 이번 | 변화 |
|------|------|------|------|
| EX (unordered set) | 108/286 (**37.76%**) | 117/286 (**40.91%**) | **+3.15pp** (+9문항 net) |
| 실패 | 178 | 169 | −9 |
| gained / lost vs prior | — | +28 / −19 | net +9 |

**핵심 관찰**

1. **전체 EX 고점 갱신(37.76%→40.91%).** 10-18과 직전 런의 37.76%를 넘김. 메타는 동일.
2. **D26 서브쿼리가 실제로 통과한다.** 스테일 거절 문자열은 바이너리에 없음. **Q1457 EX True** — `WHERE cost > (SELECT AVG(cost) FROM student_club)` 가 컴파일·실행됨(직전은 동일 SQL 반복 후 AVG 위치 오류).
3. **Max Retries가 루프를 끊는다.** 동일 실패 SQL로 `jump_to=end` **7문항**. `execute_sql` 최대 **8**(직전 Q1457형 18회·qid=994 tool_calls 26 대비). 벽시계 **4h 26m → 3h 1m**.
4. **실패 버킷:** `dw_agent` 153(160→) · `semantic_layer` 16(18→) · `sql_mcp_code` 0. `california_schools` **16.7%**(+10.0pp, 게이트 25% 미달) · `toxicology` 22.5%(−7.5pp).

---

## 2. 성적 분해

### 2.1 DB별 EX

| db_id | EX | n | 정확도 | vs prior |
|-------|----|---|--------|----------|
| superhero | 37 | 52 | **71.2%** | +3.8pp |
| student_club | 32 | 48 | **66.7%** | 0.0pp |
| thrombosis_prediction | 19 | 50 | **38.0%** | +10.0pp |
| formula_1 | 15 | 66 | **22.7%** | +3.0pp |
| toxicology | 9 | 40 | **22.5%** | −7.5pp |
| california_schools | 5 | 30 | **16.7%** | +10.0pp |

### 2.2 난이도별 EX

| difficulty | EX | n | 정확도 | vs prior |
|------------|----|---|--------|----------|
| simple | 42 | 83 | 50.6% | −2.4pp (44→42) |
| moderate | 54 | 135 | 40.0% | +4.4pp (48→54) |
| challenging | 21 | 68 | 30.9% | +7.4pp (16→21) |

### 2.3 실패 사유 (classify_failure)

| 건수 | 비율(실패 내) | 사유 |
|------|---------------|------|
| 70 | 41.4% | execute 성공 · gold와 로직/필터/값 불일치 |
| 33 | 19.5% | 프로젝션 열 개수/구성 불일치 |
| 23 | 13.6% | 결과 공집합 (gold는 비공집합) |
| 18 | 10.7% | 성공한 execute_sql 없음 |
| 16 | 9.5% | 행 수 과다 (조인 팬아웃 의심) |
| 3 | 1.8% | 비허용 함수 (D25 날짜 편의·DIVIDE·date_trunc) |
| 3 | 1.8% | 시맨틱 SQL 구문/괄호 오류 |
| 3 | 1.8% | 물리 테이블 alias(`T1` 등) — FROM cube name만 허용 |

---

## 3. 실패 구조 (Root Cause)

### ① 에이전트 로직/식별자 착오 (70 + 공집합 23)

- 최대 군집은 여전히 필터/멤버/조인 로직. DB별 로직 불일치: formula_1 22 · toxicology 13 · thrombosis 12 · california_schools 11.
- 공집합이 16→**23**. **Q1435**는 그대로: `execute_sql` 후 `sample_column(event_type)`을 호출하고도 `type = 'Game'` 공집합을 답으로 확정.

### ② 도구/재시도 고갈 (18건) + Max Retries 7건

- no-pred 19→**18**. 동일 SQL 실패 3회 중단 **7문항**(qid 1185, 869, 880, 954, 994, 800, 819). 중단 후 분류는 구문/T1 alias/DIVIDE/no-pred.
- 최대 tool_calls **15**(formula_1 qid=877). 직전 최대 31(california_schools qid=36).

### ③ 프로젝션 EX 엄격성 (33건, 34→33)

- 열 구성 감점은 유지. Soft-F1 미도입 유지.

### ④ 시맨틱 조인/그레인 (semantic_layer 16 + 팬아웃 16)

- semantic_layer 18→16. 팬아웃 15→16: toxicology 6 · california_schools 3 · thrombosis 3 · formula_1 2 · superhero 2.

### ⑤ D26 서브쿼리 (Q1457 해소)

- 직전 바이너리의 `subqueries are not allowed in semantic expressions`는 **재현되지 않음**. Q1457은 서브쿼리 AVG 필터로 EX True.

---

## 4. 직전 대비 해석

| 변화 | 해석 |
|------|------|
| 전체 +3.15pp (gained 28 / lost 19) | D26 재빌드 + Max Retries + LLM 분산. 단일 런 |
| 벽시계 4h26m→3h01m · 문항 평균 441s→300s | 동일 SQL 루프 절단이 시간에 가장 잘 맞음 |
| thrombosis +10.0pp · california_schools +10.0pp | 가장 큰 DB 이득. schools는 게이트 25%에 아직 미달(5/30) |
| toxicology −7.5pp (12→9) | lost 3문항 전부 여기. 시맨틱 분할 이슈와 겹침 |
| Q1457 EX True | clean rebuild의 직접 효과 |

참고: BIRD Mini-Dev SQLite 공개 baseline은 GPT-4 EX ~47.8% / Soft-F1 ~52%대([bird-bench/mini_dev](https://github.com/bird-bench/mini_dev)). 본 하네스는 Soft-F1 미집계·시맨틱-only 제약 · SMALL_DBS 286문항.

---

## 5. 권고 (우선순위)

1. **formula_1 멀티 큐브** — EX 22.7%로 올라도 One-Big-View 한계는 유지 ([change_semantic.md](change_semantic.md) §4.1).
2. **california_schools 발견성** — 16.7% → 게이트 ≥25%까지 metric/segment 정렬.
3. **toxicology 회귀** — 30.0%→22.5%. 팬아웃 6건이 시맨틱 그레인 후보.
4. **멤버명 vs 추측 rename** — Q1435 동일. `sample_column` 성공이 잘못된 식별자를 고착.
5. **T1 alias / DIVIDE** — Max Retries가 같은 잘못된 SQL을 3회에서 끊지만, 고치지는 않음.

---

## 6. 타이밍

- 문항당 평균 agent 경과 **~300s** (p50 ~188s, max ~1921s, california_schools qid=26)
- 벽시계 ≈ **3h 1m** (concurrency 8, `running_for_ms` 10,854,519)
- 평균 tool_calls ≈ **4.0**/문항 (최대 15, formula_1 qid=877)
- `sample_column` 호출 문항 **63**/286
- Max Retries 중단 **7**/286
