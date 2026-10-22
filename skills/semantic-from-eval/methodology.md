# Eval → semantic 마이닝

이 파일은 `semantic-from-eval` 상세 정본이다. JSON 필드 계약은 [../semantic-cube-metric-segment/contract.md](../semantic-cube-metric-segment/contract.md). Q&A 승격은 [../semantic-from-qa/methodology.md](../semantic-from-qa/methodology.md)와 같다.

## 분류

입력: gold 물리 SQL, dry_run 컴파일 물리 SQL, 선택적 compile error, agent 시맨틱 SQL. 필드명은 하네스마다 달라도 역할만 맞으면 된다.

물리 테이블 집합은 `FROM`/`JOIN` 식별자만. **제외:** CTE 이름, 인라인 서브쿼리 별칭, `main`, 방언 카탈로그 점 앞.

```
error 있고 agent SQL 없음     → agent가 실패   (도구/재시도, SQL 미생성)
error + gold에 있는 식별자 미등록 → 잘못된 시멘틱 정의
error + 에이전트가 만든 식별자 미등록 → agent가 실패
error + 유효 시맨틱인데 parse/internal → sql-mcp 버그
error + cube not found / SQL 구문     → agent가 실패
compiled 테이블 − gold 테이블 ≠ ∅    → 잘못된 시멘틱 정의  (그레인)
compiled 있고 테이블 집합 ⊆ gold      → agent가 실패      (로직/필터/프로젝션)
그 외                                 → 기타
```

한 문항 한 cause. 그레인 FAIL이 있으면 지식 FAIL로 내려가지 않는다(테이블이 이미 틀린 상태).

## 그레인

**원인:** 한 view가 형제 팩트(또는 브릿지)를 INNER JOIN으로 묶으면, 에이전트가 그 cube만 `FROM` 해도 컴파일러가 미사용 팩트를 넣는다. 뷰 **안**에 이미 묶인 팩트는 D23 cube-JOIN 가드가 나누지 못한다. Steiner prune은 **질의에 나온 테이블**만 남긴다 — 에이전트가 잘못된 형제 멤버를 집으면 팬/캐즘이 남는다.

**규칙:**

1. 비즈니스 프로세스(측정 사건)당 cube 하나. 팩트 테이블 = 허브.
2. 허브 뷰는 허브 + **나열한 차원**만. 다른 허브는 차원이라 명시하지 않으면 넣지 않음.
3. 브릿지(M:N, 출석·할당 등)는 허브에 흡수하지 않고 자체 cube.
4. 공유 차원은 여러 허브 뷰에 재사용. metric/segment는 문서마다 `cube` 명시.
5. 교차 팩트 KPI는 한 SELECT에 복수 팩트 metric을 넣지 않는다. 같은 `target`의 cube JOIN.
6. 형제 팩트를 한 장의 One-Big-View로 만들지 않는다(디버그 포함).

허브는 FK 그래프로 **자동 탐지하지 않는다.** 도메인 담당이 고른 팩트 목록이 정본이다.

## 지식

agent가 실패인데 컴파일 테이블은 gold와 같으면, 표면이 질문 어휘를 못 가리킨 것이다.

| 신호 | 승격 | 금지 |
|------|------|------|
| `X refers to Y` / 코드 리터럴 | 해당 dim/fact `description` | 생성 출처(`from table.col`) |
| 이름 붙은 비율·합 | metric `expression` (`(a)/(b)`, `DIVIDE` 별칭) | 같은 식을 fact 컬럼으로 추가 |
| YEAR/나이/날짜 잘림이 **축** | dimension `expression` (`EXTRACT`, `date_trunc`, `DATEDIFF`) | 물리 별칭 `T1` |
| 여러 질문에 재등장하는 코호트 | segment `filter` (cube 멤버 이름) | 질문마다 다른 사람 이름 |
| full name = 두 컬럼 | 두 dim description | concat dim (결과 열 수가 바뀜) |
| 미생성 SQL / 파서 오해 | cube `description` + 유효 시맨틱 SELECT 스니펫 | 물리 테이블명을 cube 식에 |

기존 metric 이름이 있으면 **expression을 덮어쓰지 않고** 빈 description만 보강.

`search_columns`는 dimension/fact만 찾는다. 에이전트가 KPI를 쓰려면 metric/segment가 `describe_table.examples`에 보여야 한다.

## 게이트

그레인 수정은 **dry_run만**으로 검증할 수 있다(같은 agent SQL, 새 메타).  
지식 수정은 에이전트 SQL을 바꾸므로 **재eval**이 아니면 게이트를 닫지 않는다.

완료 조건은 합의한 FAIL 카운트(보통 그레인 0 · 지식 0). Soft-F1·부분 문자열 일치는 완료 조건이 아니다.

## 반복

스킬은 **루프**다. 라운드 `n`의 입력은 라운드 `n-1`의 메타 + 잔여 FAIL(+ 재eval이면 전체 문항).

| 잠금 | 규칙 |
|------|------|
| 분류기 | 라운드 사이에 규칙·임계를 바꾸지 않음 |
| 문항 집합 | 게이트에 쓴 집합을 유지. 임의로 FAIL만 골라 성공률을 만들지 않음 |
| 멱등 | 빈 description만 채움. 동명 metric/segment/expression dim은 스킵. 허브 계획은 병합 |
| 최소 변경 | 이번 라운드 cause에 해당하는 JSON만. 무관한 cube/DB는 그대로 |
| wipe 금지 | 생성기가 메타 디렉터리를 비우면 지식 문서가 사라짐. 반복 라운드에서는 증분 패치만 |

정지 (하나라도 해당하면 종료하고 보고):

1. 그레인 FAIL ≤ 게이트 **그리고** 지식 FAIL ≤ 게이트
2. 패치를 적용했는데 `cause_counts`가 이전 라운드와 같음 (Δ=0)
3. 잔여가 `sql-mcp 버그` / `기타` / 범위 밖뿐
4. 사용자가 라운드 한도를 정했으면 그 횟수

지식 재eval은 **전체 문항**을 다시 채점한다. 그레인 dry_run은 이전 FAIL의 agent SQL만으로 충분하되, 예전에 PASS였던 문항이 새 JOIN을 얻으면 그레인으로 재분류한다.

## 스킬 분기

- 물리 카탈로그 → table/view 초안: `semantic-table-view`
- view 위 cube/metric/segment 초안: `semantic-cube-metric-segment`
- eval 없이 Q&A·지식만: `semantic-from-qa`
- CRUD: `semantic-register`
- 컴파일러/방언 버그: 이 스킬에서 우회 메타를 만들지 말고 서버 이슈로 분리
