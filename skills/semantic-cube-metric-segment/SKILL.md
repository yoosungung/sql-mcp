---
name: semantic-cube-metric-segment
description: >-
  Designs semantic cube, metric, and segment registration JSON for sql-mcp on
  top of an existing view. Use when defining dimensions, facts, KPIs, cohort
  filters, or segments after table/view docs exist.
---

# Semantic cube · metric · segment

선행: 대상 `view`(및 table)가 이미 초안·등록되어 있어야 한다. 없으면 `semantic-table-view` 먼저.

규칙: [contract.md](contract.md). 예시: [examples/](examples/).

## Output layout

`{outdir}/{name}.json` (table/view와 같은 디렉터리에 평탄하게). view가 많으면 휴리스틱 초안 스크립트 허용 — dimension/fact 분류·metric은 [contract.md](contract.md) 준수 후 검토. **MCP 등록은 사용자 요청 전 금지.**

## Workflow

```
Progress:
- [ ] 1. view 컬럼 목록 확보 (초안 파일 또는 metadata_get)
- [ ] 2. cube: description + dimensions/facts(각 description 필수, column XOR expression)
- [ ] 3. metric: description · expression · aggregation · group_by · filter
- [ ] 4. segment: description + 비어 있지 않은 boolean filter (선택)
- [ ] 5. 이름 충돌 검사; 등록은 요청 시에만 semantic-register
```

### 1. Bound view

`cube.view`는 view 하나. view `columns[].name`이 dimension/fact `column` 및 expression 식별자의 기준이다. 이미 등록됐으면 `metadata_get`으로 가져온다(물리 DW 조회용이 아님).

### 2–4. Draft

[contract.md](contract.md) 해당 절. 형태:

- [examples/cubes/sales.json](examples/cubes/sales.json)
- [examples/metrics/revenue.json](examples/metrics/revenue.json)
- [examples/segments/paid.json](examples/segments/paid.json)

파생·고급 식 예:
- dimension/fact `expression`: `date_trunc('year', order_date)`, `YEAR(order_date)`, `SUBSTR(order_date, 1, 4)`, `sale_amount * fx_rate`, `DIVIDE(a, b)`, `IIF(cond, t, f)`
- metric `expression` (집계·윈도우·서브쿼리 허용):
  - 기본 집계/비율: `SUM(sale_amount)`, `DIVIDE(SUM(IIF(status_code = 'PAID', 1, 0)) * 100, COUNT(*))`
  - 윈도우 함수: `RANK() OVER (PARTITION BY region_code ORDER BY SUM(sale_amount) DESC)`, `SUM(sale_amount) / SUM(SUM(sale_amount)) OVER ()`
  - 시맨틱/물리 서브쿼리: `SUM(sale_amount) / (SELECT SUM(sale_amount) FROM sales)`
- segment `filter.expression` (단순 비교 및 서브쿼리 허용):
  - 단순 조건: `status_code = 'PAID'`
  - 서브쿼리 코호트: `region_code IN (SELECT region_code FROM sales GROUP BY region_code HAVING SUM(sale_amount) > 1000)`

### 5. Hand off

질문·도메인 지식·참고 SQL로 기존 cube를 다듬을 때는 **`semantic-from-qa`**.  
등록 순서: cube → metric | segment. 검증·CRUD는 **`semantic-register`**.

## Do not

- table/view를 여기서 만들지 않는다.
- 타깃 DW 물리 탐색에 MCP를 쓰지 않는다.
- 외부 레포 경로를 정본으로 삼지 않는다 — 이 스킬 폴더만.
