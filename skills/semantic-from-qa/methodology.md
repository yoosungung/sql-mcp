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
- WHERE/HAVING의 **단순 비교·AND** → segment 후보 (식별자를 cube 멤버명으로 번역)
- SELECT의 **단순 집계** (`sum`/`count`/`avg` + 컬럼) → metric `expression` 초안
- CASE/IIF의 **반복 분기 키** → code dimension 또는 segment 분열 힌트

금지:

- FROM/JOIN 그래프 전체를 metric에 넣기
- 서브쿼리·CTE·window·집합 연산·다문장
- 질문마다 다른 one-shot 식(한 번만 나오는 차이·순위·복잡한 비율)을 전부 metric화
- 물리 테이블명을 cube 식 식별자로 쓰기 (view 컬럼 / dim·fact 이름만)

번역 실패(멤버·view 컬럼으로 못 옮김)면 **스킵**하거나 view grain 변경을 제안만 한다. 억지 등록 금지.

## 지식(knowledge) 우선

1. 명시 공식·용어 정의 > 참고 SQL 패턴 > 질문 문구 휴리스틱  
2. 동의어는 `description`·검색용 서술로 남기고, `name`은 짧고 안정적인 식별자 하나  
3. 지식이 “필터”인지 “KPI”인지 모호하면: 결과는 스칼라 지표 → metric, 행 집합 제한 → segment  

## 병합 규칙

- 기존 cube가 있으면 **덮어쓰지 말고** 멤버·metric·segment를 추가·수정한다.
- 같은 식·같은 필터면 이름을 하나로 통합한다 (질문 문구를 name에 넣지 말 것).
- `aggregation=group` metric은 `group_by`를 view 컬럼으로; 질문이 매번 다른 축이면 **기본 group_by는 비우거나 grain에 맞는 최소 축만**, 나머지는 질의 시점으로 둔다.
- segment `filter`는 cube **멤버 이름** 기준 boolean 한 식.
- 이름 우선권·식 제한은 cube 스킬 contract와 동일.

## Grain 분기

다음이면 단일 `*_all` view/cube에 억지로 넣지 않는다.

- 사실 테이블 grain이 질문마다 다름 (주문 vs 주문라인 vs 월별 스냅샷)
- fan-out join으로 집계가 중복 계산됨
- 지식에 서로 다른 “한 행의 의미”가 명시됨

→ 새 view(+cube) 초안을 짧게 제안하고 `semantic-table-view` / `semantic-cube-metric-segment`로 넘긴다.

## 산출 체크리스트

- [ ] 후보마다: 재사용성(다른 질문/지식에서도 의미 있음) 확인
- [ ] metric/segment 식에 물리 SQL 잔재(별칭 `T1`, 서브쿼리) 없음
- [ ] 식별자가 대상 cube의 dim/fact/view 컬럼 규칙과 맞음
- [ ] 질문 건수 ≫ 신규 metric/segment 건수 (과적합 아님)
- [ ] eval·실행 비교를 이 산출의 완료 조건으로 쓰지 않음
