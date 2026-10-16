---
name: semantic-cube-metric
description: >-
  Designs semantic cube, metric, and segment registration JSON for sql-mcp on
  top of an existing view. Use when defining dimensions, facts, KPIs, or cohort
  filters after table/view docs exist.
---

# Semantic cube · metric · segment

선행: 대상 `view`(및 table)가 이미 초안·등록되어 있어야 한다. 없으면 `semantic-table-view` 먼저.

규칙: [contract.md](contract.md). 예시: [examples/](examples/).

## Workflow

```
Progress:
- [ ] 1. view 컬럼 목록 확보 (초안 파일 또는 metadata_get)
- [ ] 2. cube: dimensions + facts (column XOR expression)
- [ ] 3. metric: expression · aggregation · group_by · filter
- [ ] 4. segment: 비어 있지 않은 boolean filter
- [ ] 5. 이름 충돌 검사 후 semantic-register
```

### 1. Bound view

`cube.view`는 view 하나. view `columns[].name`이 dimension/fact `column` 및 expression 식별자의 기준이다. 이미 등록됐으면 `metadata_get`으로 가져온다(물리 DW 조회용이 아님).

### 2–4. Draft

[contract.md](contract.md) 해당 절. 형태:

- [examples/cubes/sales.json](examples/cubes/sales.json)
- [examples/metrics/revenue.json](examples/metrics/revenue.json)
- [examples/segments/paid.json](examples/segments/paid.json)

파생 예: `order_year` = `date_trunc('year', order_date)`, `krw_amount` = `sale_amount * fx_rate`.

### 5. Hand off

등록 순서: cube → metric | segment. 검증·CRUD는 **`semantic-register`**.

## Do not

- table/view를 여기서 만들지 않는다.
- 타깃 DW 물리 탐색에 MCP를 쓰지 않는다.
- 외부 레포 경로를 정본으로 삼지 않는다 — 이 스킬 폴더만.
