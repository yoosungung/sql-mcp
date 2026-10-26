# Q&A · knowledge → semantic 마이닝

이 파일은 `semantic-from-qa` 상세 정본이다. JSON 필드 계약은 [../semantic-cube-metric-segment/contract.md](../semantic-cube-metric-segment/contract.md).

## 목표

질문·지식에서 **반복·이름 붙일 수 있는** 비즈니스 단위만 cube에 남긴다. 일회성 리포트 SQL을 시맨틱 레이어에 복사하지 않는다.

## 신호 → 산출물

| 신호 | 승격 조건 | 산출 |
|------|-----------|------|
| 지식의 계산식·KPI 이름 (`매출 = …`, `전환율 = …`) | 이름이 있고 식이 cube 멤버로 표현 가능 | `metric` (+ 필요 시 fact/`expression` dim) |
| 지식·질문의 cohort (`유료 고객`, `EUR 결제`, `SME`) | 동일 조건이 여러 질문/지식에 재등장하거나 용어로 고정 | `segment` (`filter.expression`) |
| GROUP BY·“by X”· dig 축 | view 컬럼(또는 파생 스칼라)으로 안정적 | `dimension` |
| 금액·수량·환율 등 측정값 | 집계 대상이 반복 | `fact` (`amount`/`quantity`/`fx`) |
| 코드값 + 표시명 | 사전 테이블이 시맨틱에 있음 | dimension `type: code` + `as_code` |
| 날짜 잘림·연/월 grain | `date_trunc` 등으로 스칼라화 가능 | dimension `expression` (view 컬럼 기준) |

## 참고 SQL 사용법

허용:

- 조인·필터에 쓰인 **물리 컬럼** → 기존 view 컬럼/`mappings`와 대조해 누락 dim·fact 후보
- WHERE/HAVING의 **비교·AND·재사용 가능한 서브쿼리 필터(`IN (SELECT …)`/`EXISTS (SELECT …)`)** → segment 후보 (식별자를 cube 멤버명으로 번역)
- SELECT의 **그룹 집계·비율·윈도우** (`SUM(a)/SUM(b)`, `OVER (...)`) → metric `expression` 초안(D26 examples)
- 행 그레인 비율(`col_a / col_b`를 SELECT/ORDER BY) → fact `expression` + `additive: false`(D28)
- CASE/IIF의 **반복 분기 키** → flag/code dimension. 에이전트가 `WHERE name`을 쓰면 segment 이름 대신 flag dim

금지:

- FROM/JOIN 그래프 전체를 metric에 넣기
- 다문장·쓰기 CTE
- 질문마다 다른 one-shot 리터럴 식을 전부 metric화
- 물리 테이블 별칭(`T1`, `T2`)을 cube 외부 식 식별자로 쓰기 (view 컬럼 / dim·fact 이름만)

번역 실패(멤버·view 컬럼으로 못 옮김)면 **스킵**하거나 view grain 변경을 제안만 한다. 억지 등록 금지.

## 지식(knowledge) 우선

1. 명시 공식·용어 정의 > 참고 SQL 패턴 > 질문 문구 휴리스틱  
2. 동의어는 `description`·검색용 서술로 남기고, `name`은 짧고 안정적인 식별자 하나  
3. 지식이 “필터”인지 “KPI”인지 모호하면: 결과는 스칼라 지표 → metric, 행 집합 제한 → segment  

## 병합 규칙

- 기존 cube가 있으면 **덮어쓰지 말고** 멤버·metric·segment를 추가·수정한다.
- 같은 식·같은 필터면 이름을 하나로 통합한다 (질문 문구를 name에 넣지 말 것).
- `aggregation=group` metric의 `group_by`는 명명 metric을 컴파일할 때의 **기본 grain**이다(D24, 허용목록 아님). `execute_sql`은 metric 이름을 식별자로 쓰지 않으므로(D26), 질의 축은 ad-hoc `GROUP BY`/차원 프로젝션으로 쓴다. 메타에는 기본으로 쓰고 싶은 축만 넣는다.
- segment `filter`는 cube **멤버 이름** 기준 boolean 한 식(필요 시 `IN (SELECT …)`/`EXISTS (SELECT …)` 포함 가능).
- 이름 우선권·식 제한은 cube 스킬 contract와 동일.

## Grain 분기

컴파일러는 `combine: "join"` 뷰에서 질의가 참조한 테이블들($T_{req}$)만 최단 경로(Steiner)로 연결하고 미사용 테이블(루트 포함)을 자동 프루닝한다. 직접 FK와 조인 키 동치류가 맞으면 prune이 동작한다. **형제 팩트가 한 뷰에 있으면** 에이전트가 그 cube만 FROM 해도 팬/캐즘이 남는다 — eval로 드러나면 [semantic-from-eval](../semantic-from-eval/methodology.md)에서 허브 분할. 그 전에 다음이면 view 보강 또는 별도 view/cube 분리를 제안한다:

- `view.joins`에 직접 FK 간선이 빠져 우회 조인(Chasm/Fan Trap)이 발생하는 경우 → `view.joins`에 누락된 FK 간선 및 조인 키 양방향 매핑 보강 제안
- 동일 소규모 룩업 테이블을 여러 역할 FK(`ship_to_address_id`, `bill_to_address_id` 등)로 참조해 동시 필터링이 막히는 경우 → 역할별 룩업 `table` 문서 분리 제안
- 사전 집계 요약 테이블과 원천 상세 테이블을 함께 라우팅해야 하는 경우 → 요약은 별도 join view로 두고 cube `choice`(`summary: true`) + `prefer: "summary"` 제안

→ view/table 수정이나 신규 생성이 필요하면 짧게 제안하고 `semantic-table-view` / `semantic-cube-metric-segment`로 넘긴다.

## 산출 체크리스트

- [ ] 후보마다: 재사용성(다른 질문/지식에서도 의미 있음) 확인
- [ ] metric/segment 외부 식에 물리 SQL 별칭 잔재(`T1.` 등) 없음
- [ ] 식별자가 대상 cube의 dim/fact/view 컬럼 규칙과 맞음
- [ ] `aggregation=group` metric의 `group_by`가 기본으로 쓸 리포트 축만 담음(차원 전부 나열하지 않음)
- [ ] 질문 건수 ≫ 신규 metric/segment 건수 (과적합 아님)
- [ ] eval·실행 비교를 이 산출의 완료 조건으로 쓰지 않음 (`semantic-from-eval`)
