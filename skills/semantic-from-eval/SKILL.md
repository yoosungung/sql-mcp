---
name: semantic-from-eval
description: >-
  Iteratively scores NL→SQL against gold rows, classifies remaining FAILs
  (grain vs agent knowledge vs compiler bug) with a frozen classifier, and
  patches sql-mcp semantic JSON until gates hit or progress stalls. Use when
  evaluating a semantic layer, looping EX/row-set gaps, splitting cubes that
  fan/chasm, or turning eval evidence into descriptions/metrics/segments.
---

# Semantic refine · from eval

**방법론 스킬.** 벤치마크·특정 스키마에 묶지 않는다. 입력이 내부 QA, 티켓, 벤치든 같은 절차다.

선행: 등록(또는 초안) `table`/`view`/`cube`. 없으면 `semantic-table-view` → `semantic-cube-metric-segment`.

분류·그레인·지식 규칙: [methodology.md](methodology.md).  
멤버 승격 상세는 [../semantic-from-qa/methodology.md](../semantic-from-qa/methodology.md).  
문서 형태: [../semantic-cube-metric-segment/contract.md](../semantic-cube-metric-segment/contract.md).

**범위 = 시맨틱 메타.** LLM 교체·에이전트 SYSTEM·sql-mcp 파서 수정은 이 스킬 밖.

**한 번이 아니라 라운드.** 분류기·게이트·문항 집합을 고정한 채 잔여 FAIL만 패치한다. 라운드마다 cause 카운트를 남기고, 게이트 도달·진전 없음·sql-mcp만 남음에서 멈춘다. 상세: [methodology.md](methodology.md) §반복.

## Inputs

한 문항 = 아래 역할. 필드명은 달라도 된다.

| 역할 | 필수 | 내용 |
|------|------|------|
| 질문 | 예 | 자연어 |
| 지식 | 권장 | 용어·공식·코드값 (`evidence`) |
| gold SQL | 채점 시 | **물리** SELECT (시맨틱 `execute_sql`에 넣지 않음) |
| gold rows | 채점 시 | 타깃 DW에서 gold SQL 실행 (순서 무시 집합) |
| agent SQL | FAIL 분석 시 | 시맨틱 SELECT (`FROM` cube) |
| compiled SQL | FAIL 분석 시 | `execute_sql` **dry_run**이 낸 물리 SQL |
| compile error | 해당 시 | dry_run / parse / unknown ident |

채점 하네스가 있으면 그걸 쓴다. 없으면 같은 역할의 필드를 수동·스크립트로 채운다. 특정 벤치/레포 경로를 전제로 두지 않는다.

## Workflow

첫 라운드만 범위를 잠근다. 이후는 **같은 분류기**로 3–7을 반복한다.

```
Progress:
- [ ] 1. 범위 고정: target · 문항 집합 · 게이트 · 분류기 (이후 바꾸지 않음)
- [ ] 2. 채점 (순서 무시 row-set). 이전 스냅샷이 있으면 이어서
반복:
- [ ] 3. 잔여 FAIL만 분류 (methodology.md). cause_counts 기록
- [ ] 4. 그레인 > 0 → 허브 패치 (이미 맞는 cube는 건드리지 않음)
- [ ] 5. 지식 > 0 → description / expression / metric / segment (멱등)
- [ ] 6. 등록 (요청 시 semantic-register). JSON 전체 wipe 금지
- [ ] 7. 그레인: 같은 agent SQL dry_run. 지식: 재eval (회귀 포함)
- [ ] 8. 정지? 게이트 충족 / Δ=0 / sql-mcp·기타만 남음 → 보고 후 종료
        아니면 3으로
```

### 1. Scope

고칠 것: table/view/cube/dimension/fact/metric/segment JSON.  
**비목표:** Soft-F1, LLM, SYSTEM 프롬프트, 일회성 고유명 segment, 그룹 비율을 가산 fact로 올리기.

게이트를 숫자로 적는다. 예: 그레인 FAIL → 0, 지식 FAIL → 0. **라운드 사이에 분류 규칙을 바꿔 숫자를 맞추지 않는다.**

### 2. Score

마지막 **성공** `execute_sql` rows vs gold rows. 기본은 **순서 없는 집합**(중복 정책은 하네스와 동일하게 고정).  
gold SQL은 물리 타깃에서만 실행한다.

### 3. Classify

문항마다 원인 **하나**. 규칙: [methodology.md](methodology.md) §분류.

| cause | 언제 |
|-------|------|
| `잘못된 시멘틱 정의` | 컴파일 물리 SQL이 gold에 **없는 물리 테이블**을 JOIN · gold 식별자 미등록 |
| `agent가 실패` | 테이블 집합은 맞거나 SQL 미생성 · 로직/필터/프로젝션 불일치 |
| `sql-mcp 버그` | 유효해 보이는 시맨틱 SQL을 파서·컴파일러가 거절 |
| `기타` | 위 세 갈래 밖 |

CTE·인라인 별칭은 물리 테이블이 아니다. sql-mcp 버그는 이 루프에서 고치지 않고 목록만 남긴다.

### 4. Grain

한 측정 사건(팩트 허브) = view+cube 하나. 형제 팩트를 한 뷰에 묶지 않는다. 공유 차원만 재사용. 브릿지 테이블은 흡수하지 않고 자체 그레인.

허브 목록을 로컬 계획으로 문서화한다(`hubs: [{table, cube, dimensions}]` 형태면 충분). FK 연결 요소 1개라고 형제 팩트를 한 view에 묶지 않는다.  
교차 팩트 질의는 **cube JOIN**(D23)에 맡긴다.

초안 작성: `semantic-table-view` / `semantic-cube-metric-segment`.

### 5. Knowledge

프롬프트가 아니라 **시맨틱 표면**. `search_columns`는 dim/fact만 인덱스하고, metric/segment는 `describe_table.examples`다(D26).

| 신호 | 산출 |
|------|------|
| 동의어·코드값·혼동 금지 | dim/fact `description` (생성 출처 문구 금지) |
| 연·나이 등 그룹/필터 축 | dimension `expression`(스칼라) |
| 행 그레인 비율 (`ORDER BY`/`SELECT` 컬럼) | fact `expression` + `additive: false` |
| 그룹 `SUM(a)/SUM(b)` KPI | **metric** (`examples`만. fact와 **다른 이름**) |
| 에이전트가 `WHERE name`을 쓰는 코호트 | flag dim (`IIF`/`CASE`). segment는 examples 보조 |
| 사람 이름 등 일회성 리터럴 | description만 (segment 금지) |

질문 1건 = metric/segment 1개 금지. **멱등:** 빈 `description`만 채움, 같은 이름 metric/segment는 expression/filter를 덮어쓰지 않음, 이미 있는 expression dim은 스킵.

### 6–7. Register · gates

등록 순서·검증: `semantic-register`. 초안 디렉터리 전체를 지우고 재생성하지 않는다(지식 JSON이 사라짐). 바뀐 문서만 create/update.

1. **그레인 게이트** — **같은** agent SQL을 새 메타로 dry_run. 에이전트를 다시 돌리지 않는다.
2. **지식 게이트** — 에이전트 재eval(문항 집합 전체 — 회귀 확인). description/`examples`가 SQL을 바꾸므로 생략 금지.

매 라운드 `cause_counts`를 이전과 비교한다. 패치를 냈는데 Δ=0이면 같은 수정을 반복하지 말고 원인(분류 vs 메타 vs 에이전트)을 보고 정지한다.

잔여 FAIL은 cause별로 보고하고, sql-mcp 버그만 서버 트랙으로 넘긴다.

## Do not

- 에이전트 SYSTEM·모델·재시도 한도를 이 스킬에서 고치지 않는다.
- gold 물리 SQL을 `execute_sql`에 넣거나 metric/segment에 복붙하지 않는다.
- One-Big-View로 형제 팩트를 다시 묶지 않는다.
- 그룹 비율을 가산 fact로 올리지 않는다(`SUM(rate)` 함정). 행 비율만 `additive: false` fact.
- 일회성 고유명을 segment로 하드코딩하지 않는다.
- 특정 벤치 경로·DB 목록을 전제로 두지 않는다.
- 사용자 요청 전 `/api/meta` 등록 금지.
- 라운드마다 메타를 wipe·전량 재생성하거나, 이미 PASS인 문항만 다시 고치지 않는다.
- 정지 조건 없이 동일 패치를 반복하지 않는다.
