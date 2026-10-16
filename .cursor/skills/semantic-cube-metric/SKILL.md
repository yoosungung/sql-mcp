---
name: semantic-cube-metric
description: >-
  Designs semantic cube, metric, and segment registration JSON for sql-mcp on
  top of an existing view. Use when defining dimensions/facts, KPIs, cohort
  filters, or editing files under semantic/examples/cubes, metrics, or
  segments after table/view docs exist.
---

# Semantic cube · metric · segment

선행: 대상 `view`(및 그 table)가 이미 초안·등록되어 있어야 한다. 없으면 `semantic-table-view` 먼저.

정본: `ARCHITECTURE.md` §1·§4, `semantic/schema/document.schema.json`. 예: `semantic/examples/cubes/sales.json`, `metrics/revenue.json`, `segments/paid.json`.

## Workflow

```
Progress:
- [ ] 1. metadata_get / 파일로 view 컬럼 목록 확보
- [ ] 2. cube: dimensions + facts (column XOR expression)
- [ ] 3. metric: expression · aggregation · group_by · filter
- [ ] 4. segment: 비어 있지 않은 boolean filter
- [ ] 5. 이름 충돌 검사 후 semantic-register
```

### 1. Bound view

`cube.view`는 view **하나**. view `columns[].name`이 dimension/fact `column` 및 expression 식별자의 기준이다.

### 2. Draft `cube`

필수: `kind`, `name`, `view`, `description`, `dimensions`, `facts`.

**dimension**

- `type`: `code` | `date` | `time` | `datetime` | `string`
- 값: `column` **또는** `expression`(view 컬럼 이름 쓰는 스칼라) — 둘 중 하나만
- `encryption` (true면 응답·샘플 마스킹, D17), `role` 기본 `all`

**fact**

- `amount` / `fx` boolean
- 역시 `column` XOR `expression`
- `encryption`, `role` (배열 가능, 예: `["finance"]`)

파생 예: `order_year` = `date_trunc('year', order_date)`, `krw_amount` = `sale_amount * fx_rate`.

### 3. Draft `metric`

필수: `kind`, `name`, `cube`, `expression`, `aggregation`, `filter`.

- `expression`: 연결된 cube의 **view 컬럼** 연산(예: `sum(sale_amount)`). 신뢰된 SQL 조각; 서브쿼리·쓰기 금지.
- `aggregation`: `group` | `rollup` | `cube` | `grouping_sets`
- `group`이면 `group_by`에 GROUP BY용 **view 컬럼** 이름
- `filter`: boolean 식 객체 또는 `null`
- `name`은 같은 cube의 dimension·fact 이름과 **겹치면 안 됨**
- metric/segment 식의 식별자는 cube의 dimension·fact 이름(컴파일 시). 저장 시에는 연결 그래프 위주(D18)

### 4. Draft `segment`

필수: `kind`, `name`, `cube`, `filter`. `filter`는 비울 수 없음. 식은 cube 멤버 이름(예: `status_code = 'PAID'`).

### 5. Hand off

등록 순서: cube → metric | segment. 검증·`metadata_*`는 **`semantic-register`**.

## Do not

- table/view를 여기서 만들지 않는다.
- 새 DSL을 발명하지 않는다 — SQL 조각 + schema enum만.
- 장문 규칙을 복제하지 않는다 → [reference.md](reference.md).
