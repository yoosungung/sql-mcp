# cube · metric · segment 계약

이 파일은 스킬 동봉 정본이다.

## 선행

대상 `view`(및 그 table)가 이미 초안·등록되어 있어야 한다. 없으면 `semantic-table-view` 먼저.

참조 방향: `table → view → cube → metric|segment`.

## 이름 우선권

자연스러운 `name`은 **cube · dimension · fact · metric · segment** 우선(검색 대상: 이름·설명, dim/fact는 type 추가). 중복 시 table · view · view 컬럼이 접미사를 받아 양보한다(물리 경로·물리 컬럼명은 유지). dim/fact가 가리키는 view 컬럼과 멤버명이 같으면 OK.

## 식·역할 공통

- 필터·metric·segment·dimension/fact `expression`은 코딩 에이전트가 작성한 **신뢰된 SQL 조각**. 새 DSL 없음.
- metric `expression`: 스칼라·집계식만. segment·filter: 단일 boolean만. 서브쿼리·다중 문장·쓰기·쓰기 CTE 거부.
- cube/metric/segment 식의 식별자: 저장 시에는 연결 그래프 위주; **컴파일 직전**에 dimension·fact·(해당 시) view/물리 컬럼으로 해석. 실패 시 `validation`.
- cube·metric·segment의 식에서 쓰는 이름은 그 cube의 **dimension / fact 이름**(metric `expression`의 피연산은 view 컬럼 기준 연산으로 작성하되, 질의·segment filter는 멤버 이름).
- `role` 생략 또는 `"all"` 또는 role 이름 배열.
- `encryption: true`인 dimension/fact는 암·복호화하지 않고 응답·샘플·로그에서 원문 마스킹(`null` 또는 `"***"`). AES 등은 타깃 DW 책임.

## cube

필수: `kind`, `name`, `view`, `description`, `dimensions`, `facts`.

`view`는 view 문서 **하나**.

**dimension**

- `type`: `code` | `date` | `time` | `datetime` | `string`
- `column`(view 컬럼) **또는** `expression`(view 컬럼명을 쓰는 스칼라) — 둘 중 하나만
- `encryption`, `role`

**fact**

- `type`: `amount` | `fx` | `quantity` (취급 방식 — dim `type`과 같은 축)
- `column` XOR `expression`
- `encryption`, `role`

`expression`으로 만든 dimension/fact는 **저장 시** view 컬럼 연결 검사를 하지 않는다(식 식별자는 컴파일 단계).

## metric

필수: `kind`, `name`, `cube`, `expression`, `aggregation`, `filter`.

| 필드 | 규칙 |
|------|------|
| `expression` | 연결된 cube의 view 컬럼 연산. 예: `sum(sale_amount)` |
| `aggregation` | `group` \| `rollup` \| `cube` \| `grouping_sets` |
| `group_by` | `aggregation=group`일 때 필수에 가깝게: GROUP BY용 **view 컬럼** 이름 배열 |
| `filter` | `{ expression }` 또는 `null` |
| `name` | 같은 cube의 dimension·fact 이름과 **겹치면 안 됨** |

## segment

필수: `kind`, `name`, `cube`, `filter`. `filter`는 비울 수 없음. 식은 cube 멤버 이름. 예: `status_code = 'PAID'`.

## Quick checks

- [ ] `cube.view` 존재; `column` ∈ view columns
- [ ] `column`과 `expression` 동시 사용 없음
- [ ] metric `name` ∉ 같은 cube dimension/fact names
- [ ] `aggregation=group` ⇒ `group_by` ⊆ view columns
- [ ] segment `filter.expression` non-empty
- [ ] 식에 서브쿼리·다중 문장·쓰기 CTE 없음
